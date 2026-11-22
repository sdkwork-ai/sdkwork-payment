//! Server-side short-id mapping for anonymous cashier links.
//!
//! The cashier QR/URL carries only `s1.<22 base64url chars>` (128-bit short id).
//! The real signed capability token is minted exactly as before and stored here
//! **encrypted** (AES-256-GCM, key derived from the deployment secret + tenant +
//! epoch); the order/IAM services resolve the short id, decrypt the token, and run
//! the existing signature verification. This keeps the `/cashier/{segment}` URL
//! shape while cutting the QR payload from ~460 to ~64 characters.
//!
//! One active link per `(tenant, order)` keeps QR re-issues idempotent: the same
//! short id (and therefore the same token id / payment idempotency key) is reused.

use sdkwork_contract_service::CommerceServiceError;
use sdkwork_utils_rust::{
    cashier_short_link_segment, decrypt_cashier_short_link_token,
    encrypt_cashier_short_link_token, generate_cashier_short_link_id, now,
};
use sqlx::{PgPool, Row};

use crate::cashier_token::{
    mint_cashier_token_with_secret, resolve_cashier_token_mint_material, CashierTokenSubject,
    CASHIER_TOKEN_TTL_SECONDS,
};
use crate::shared::store_error;

/// Maximum allocation attempts when concurrent mints race on the active-order
/// unique index. Each retry re-checks for the winner before allocating again.
const MAX_MINT_ATTEMPTS: usize = 3;

/// Mints (or reuses) the short-link segment for an order.
///
/// Returns `Ok(None)` when no signing material is available — no configured
/// KEK, no stored secret, a stored secret the deployed KEK cannot open, or a
/// tenant that disabled the anonymous cashier. The caller keeps the legacy
/// order-scoped link, which the anonymous H5 exchange cannot convert into a
/// capability.
pub async fn mint_cashier_short_link_postgres(
    pool: &PgPool,
    subject: &CashierTokenSubject<'_>,
) -> Result<Option<String>, CommerceServiceError> {
    // Resolve the live signing material *before* reusing an active short link.
    // The resolve fails closed when the tenant disabled the anonymous cashier or
    // the key provider is unavailable; reusing first would keep emitting a dead
    // `s1.` link (the resolver rejects it) even though the switch is off.
    let Some((secret, key_epoch)) =
        resolve_cashier_token_mint_material(pool, subject.tenant_id).await?
    else {
        return Ok(None);
    };
    if let Some(existing) =
        load_active_short_link_postgres(pool, subject.tenant_id, subject.order_id).await?
    {
        return Ok(Some(cashier_short_link_segment(&existing)));
    }
    let subject = CashierTokenSubject {
        tenant_id: subject.tenant_id,
        order_id: subject.order_id,
        out_trade_no: subject.out_trade_no,
        amount: subject.amount,
        currency_code: subject.currency_code,
        scene: subject.scene,
        key_epoch,
    };
    let token_id = uuid::Uuid::new_v4().to_string();
    let issued_at = now().timestamp();
    let token = mint_cashier_token_with_secret(&secret, &subject, issued_at, &token_id)?;
    let ciphertext =
        encrypt_cashier_short_link_token(&secret, subject.tenant_id, subject.key_epoch, &token)
            .map_err(|error| {
                CommerceServiceError::storage(format!(
                    "cashier short link encryption failed: {error:?}"
                ))
            })?;

    // Free the `(tenant, order)` partial unique slot left by an expired row
    // before allocating; otherwise the INSERT would conflict forever with an
    // `active` row whose `expires_at` has already passed.
    sqlx::query(
        r#"
        UPDATE commerce_payment_cashier_link
        SET status = 'expired', updated_at = NOW()
        WHERE tenant_id = CAST($1 AS TEXT)
          AND order_id = CAST($2 AS TEXT)
          AND status = 'active'
          AND expires_at <= NOW()
        "#,
    )
    .bind(subject.tenant_id)
    .bind(subject.order_id)
    .execute(pool)
    .await
    .map_err(|error| store_error("failed to expire cashier short link", error))?;

    for _ in 0..MAX_MINT_ATTEMPTS {
        let short_id = generate_cashier_short_link_id();
        let inserted = sqlx::query(
            r#"
            INSERT INTO commerce_payment_cashier_link
                (short_id, tenant_id, order_id, token_id, key_epoch, token_ciphertext, expires_at)
            VALUES
                (CAST($1 AS TEXT), CAST($2 AS TEXT), CAST($3 AS TEXT), CAST($4 AS TEXT), $5, $6,
                 NOW() + (CAST($7 AS BIGINT) * INTERVAL '1 second'))
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(&short_id)
        .bind(subject.tenant_id)
        .bind(subject.order_id)
        .bind(&token_id)
        .bind(subject.key_epoch)
        .bind(&ciphertext)
        .bind(CASHIER_TOKEN_TTL_SECONDS)
        .execute(pool)
        .await
        .map_err(|error| store_error("failed to insert cashier short link", error))?;
        if inserted.rows_affected() > 0 {
            return Ok(Some(cashier_short_link_segment(&short_id)));
        }
        // Another mint won the `(tenant, order)` active row (or the random id
        // collided). Reuse the winner when it exists.
        if let Some(existing) =
            load_active_short_link_postgres(pool, subject.tenant_id, subject.order_id).await?
        {
            return Ok(Some(cashier_short_link_segment(&existing)));
        }
    }
    Err(CommerceServiceError::storage(
        "failed to allocate a cashier short link",
    ))
}

/// Resolves a short id to the stored signed token, or `None` when unknown,
/// revoked, expired, or not decryptable.
pub async fn resolve_cashier_short_link_token_postgres(
    pool: &PgPool,
    short_id: &str,
) -> Result<Option<String>, CommerceServiceError> {
    let row = sqlx::query(
        r#"
        SELECT tenant_id, key_epoch, token_ciphertext
        FROM commerce_payment_cashier_link
        WHERE short_id = CAST($1 AS TEXT)
          AND status = 'active'
          AND expires_at > NOW()
        LIMIT 1
        "#,
    )
    .bind(short_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| store_error("failed to resolve cashier short link", error))?;
    let Some(row) = row else {
        return Ok(None);
    };
    let tenant_id = row.try_get::<String, _>("tenant_id").unwrap_or_default();
    let key_epoch = row.try_get::<i64, _>("key_epoch").unwrap_or(1);
    let ciphertext = row
        .try_get::<String, _>("token_ciphertext")
        .unwrap_or_default();
    if tenant_id.trim().is_empty() || ciphertext.is_empty() {
        return Ok(None);
    }
    let Some(secrets) =
        crate::cashier_token_setting::load_cashier_token_signing_secrets_postgres(
            pool, &tenant_id,
        )
        .await
        .ok()
        .flatten()
    else {
        return Ok(None);
    };
    if !secrets.enabled || secrets.current_secret.is_empty() {
        return Ok(None);
    }
    let mut candidates = vec![secrets.current_secret];
    if let Some(previous) = secrets.previous_secret {
        candidates.push(previous);
    }
    for secret in candidates {
        if let Ok(token) =
            decrypt_cashier_short_link_token(&secret, &tenant_id, key_epoch, &ciphertext)
        {
            return Ok(Some(token));
        }
    }
    Ok(None)
}

/// Revokes every active short link for an order (e.g. order cancelled/paid).
pub async fn revoke_cashier_short_links_for_order_postgres(
    pool: &PgPool,
    tenant_id: &str,
    order_id: &str,
) -> Result<u64, CommerceServiceError> {
    let result = sqlx::query(
        r#"
        UPDATE commerce_payment_cashier_link
        SET status = 'revoked', updated_at = NOW()
        WHERE tenant_id = CAST($1 AS TEXT)
          AND order_id = CAST($2 AS TEXT)
          AND status = 'active'
        "#,
    )
    .bind(tenant_id)
    .bind(order_id)
    .execute(pool)
    .await
    .map_err(|error| store_error("failed to revoke cashier short links", error))?;
    Ok(result.rows_affected())
}

/// Deletes expired short links older than a day (audit window).
pub async fn delete_expired_cashier_short_links_postgres(
    pool: &PgPool,
) -> Result<u64, CommerceServiceError> {
    let result = sqlx::query(
        r#"
        DELETE FROM commerce_payment_cashier_link
        WHERE expires_at < NOW() - INTERVAL '1 day'
        "#,
    )
    .execute(pool)
    .await
    .map_err(|error| store_error("failed to clean cashier short links", error))?;
    Ok(result.rows_affected())
}

async fn load_active_short_link_postgres(
    pool: &PgPool,
    tenant_id: &str,
    order_id: &str,
) -> Result<Option<String>, CommerceServiceError> {
    let row = sqlx::query(
        r#"
        SELECT short_id
        FROM commerce_payment_cashier_link
        WHERE tenant_id = CAST($1 AS TEXT)
          AND order_id = CAST($2 AS TEXT)
          AND status = 'active'
          AND expires_at > NOW()
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(tenant_id)
    .bind(order_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| store_error("failed to load cashier short link", error))?;
    Ok(row.map(|row| row.try_get::<String, _>("short_id").unwrap_or_default()))
}
