//! Cashier capability-token minting for owner-order checkout.
//!
//! The checkout enrichment builds the `cashierUrl` / `qrCodePayload` the payer
//! scans. Emitting a token-signed URL there is what lets the H5 cashier open
//! **without a login session**: the URL proves "this order may be paid" instead
//! of "this user is authenticated".
//!
//! The signing secret is managed by the payment center and stored, sealed, in
//! `commerce_payment_cashier_token_setting` (see `cashier_token_setting`). The
//! deployment value `SDKWORK_COMMERCE_CASHIER_TOKEN_SECRET` is only the
//! key-encryption key for that stored secret, never the payment
//! provider-credential master key. Verification lives in the order service and
//! later in IAM; sharing the credential master key would make every verifier a
//! PSP-credential decryptor.
//!
//! Rollout model: the token's **presence in the URL is the feature flag**. When
//! neither a stored secret nor a deployment secret exists minting returns
//! `Ok(None)` and the caller keeps the legacy login-required cashier link. A
//! secret that is present but invalid fails loudly instead of silently
//! degrading.

use sdkwork_contract_service::CommerceServiceError;
use sdkwork_utils_rust::{
    build_commerce_cashier_token_url, build_commerce_cashier_token_url_with_base,
    cashier_token_secret_from_env, derive_cashier_token_key_with_epoch, encode_cashier_token,
    now, CashierTokenClaims, CashierTokenIssue,
};

/// Cashier capability lifetime, matching the H5 cashier state machine TTL.
///
/// A payer who leaves the QR on screen longer than this must re-create the
/// payment, which mints a fresh token; that mirrors the existing 15-minute
/// cashier countdown instead of inventing a second expiry model.
pub const CASHIER_TOKEN_TTL_SECONDS: i64 = 15 * 60;

/// Everything the token binds, taken from the persisted payment attempt.
pub struct CashierTokenSubject<'a> {
    pub tenant_id: &'a str,
    pub order_id: &'a str,
    pub out_trade_no: &'a str,
    pub amount: &'a str,
    pub currency_code: &'a str,
    pub scene: &'a str,
    /// Tenant key epoch resolved from the payment settings store.
    pub key_epoch: i64,
}

/// Mints a cashier token with explicit key material (testable, no process state).
pub fn mint_cashier_token_with_secret(
    secret: &[u8],
    subject: &CashierTokenSubject<'_>,
    issued_at: i64,
    token_id: &str,
) -> Result<String, CommerceServiceError> {
    let signing_key =
        derive_cashier_token_key_with_epoch(secret, subject.tenant_id, subject.key_epoch).map_err(
            |error| {
                CommerceServiceError::storage(format!(
                    "cashier token signing key is unavailable: {error}"
                ))
            },
        )?;
    let claims = CashierTokenClaims::issue(
        &CashierTokenIssue {
            tenant_id: subject.tenant_id,
            order_id: subject.order_id,
            out_trade_no: subject.out_trade_no,
            amount: subject.amount,
            currency_code: subject.currency_code,
            scene: subject.scene,
            ttl_seconds: CASHIER_TOKEN_TTL_SECONDS,
            key_epoch: subject.key_epoch,
            token_id,
        },
        issued_at,
    );
    encode_cashier_token(&claims, &signing_key).map_err(|error| {
        CommerceServiceError::storage(format!("cashier token minting failed: {error}"))
    })
}

/// Mints a cashier token with the deployment secret.
///
/// Returns `Ok(None)` when the deployment has not configured a secret, so the
/// caller keeps the login-required cashier entry.
pub fn mint_cashier_token(
    subject: &CashierTokenSubject<'_>,
) -> Result<Option<String>, CommerceServiceError> {
    let Some(secret) = cashier_token_secret_from_env().map_err(|error| {
        CommerceServiceError::storage(format!("cashier token secret is invalid: {error}"))
    })?
    else {
        return Ok(None);
    };
    let token_id = uuid::Uuid::new_v4().to_string();
    let issued_at = now().timestamp();
    mint_cashier_token_with_secret(&secret, subject, issued_at, &token_id).map(Some)
}

/// Resolves the signing material new tokens must use for `tenant_id`.
///
/// Prefers the payment-center managed secret stored in
/// `commerce_payment_cashier_token_setting` (importing the deployment secret on
/// first use) and falls back to the deployment secret for installs that have
/// never rotated. `None` means the anonymous cashier is not enabled.
pub async fn resolve_cashier_token_mint_material(
    pool: &sqlx::PgPool,
    tenant_id: &str,
) -> Result<Option<(Vec<u8>, i64)>, CommerceServiceError> {
    if let Some(resolved) =
        crate::cashier_token_setting::ensure_cashier_token_signing_secrets_postgres(pool, tenant_id)
            .await?
    {
        return Ok(Some((resolved.current_secret, resolved.key_epoch)));
    }
    let Some(secret) = cashier_token_secret_from_env().map_err(|error| {
        CommerceServiceError::storage(format!("cashier token secret is invalid: {error}"))
    })?
    else {
        return Ok(None);
    };
    let key_epoch =
        crate::cashier_token_setting::load_cashier_token_epoch_postgres(pool, tenant_id)
            .await
            .unwrap_or(crate::cashier_token_setting::DEFAULT_CASHIER_TOKEN_EPOCH);
    Ok(Some((secret, key_epoch)))
}

/// Verifies a capability token against the tenant's stored signing secret.
///
/// Tries the active secret, then the previous secret during its rotation grace
/// window, then the deployment secret for installs with no stored secret. The
/// tenant and epoch are read from the payload only to select the key; the
/// signature must still validate under that key.
pub async fn verify_cashier_token_from_store(
    pool: &sqlx::PgPool,
    token: &str,
    now_unix: i64,
) -> Option<CashierTokenClaims> {
    let tenant_id = sdkwork_utils_rust::peek_cashier_token_tenant(token)?;
    let epoch = sdkwork_utils_rust::peek_cashier_token_epoch(token);
    let candidates: Vec<Vec<u8>> =
        match crate::cashier_token_setting::load_cashier_token_signing_secrets_postgres(
            pool, &tenant_id,
        )
        .await
        {
            Ok(Some(resolved)) => {
                let mut secrets = vec![resolved.current_secret];
                if let Some(previous) = resolved.previous_secret {
                    secrets.push(previous);
                }
                secrets
            }
            Ok(None) => cashier_token_secret_from_env()
                .ok()
                .flatten()
                .into_iter()
                .collect(),
            // A stored secret that cannot be opened must fail closed; dropping
            // back to the deployment secret would bypass a rotation.
            Err(_) => return None,
        };
    for secret in candidates {
        let Ok(key) = derive_cashier_token_key_with_epoch(&secret, &tenant_id, epoch) else {
            continue;
        };
        if let Ok(claims) = sdkwork_utils_rust::verify_cashier_token(
            token,
            &key,
            now_unix,
            sdkwork_utils_rust::CASHIER_TOKEN_AUDIENCE,
        ) {
            return Some(claims);
        }
    }
    None
}

/// Builds the cashier URL for `token`, honouring the operator-configured H5 origin.
///
/// Returns `None` only when the token is blank, which cannot happen for a token
/// this module minted; callers treat `None` as "keep whatever URL already exists".
pub fn cashier_url_for_token(token: &str, h5_base_url: Option<&str>) -> Option<String> {
    build_commerce_cashier_token_url_with_base(token, h5_base_url)
        .or_else(|| build_commerce_cashier_token_url(token))
}

/// Stamps a capability-token cashier link onto an outcome that skipped provider
/// enrichment.
///
/// Provider enrichment is what normally mints the token, but a deployment without
/// PSP credentials (or a provider outage) falls back to the pre-enrichment outcome.
/// Leaving that outcome on the legacy link would silently turn the scanned QR back
/// into a login-required page, so the capability is applied here as well.
///
/// Returns the outcome unchanged when no capability can be minted (no secret, no
/// currency on the attempt); the caller must never invent an amount or currency.
pub async fn apply_cashier_capability_to_outcome(
    pool: &sqlx::PgPool,
    tenant_id: &str,
    mut outcome: sdkwork_payment_service::PayOwnerOrderOutcome,
) -> sdkwork_payment_service::PayOwnerOrderOutcome {
    let Some(currency_code) = load_attempt_currency_code(pool, tenant_id, &outcome.payment_id).await
    else {
        return outcome;
    };
    let scene = outcome
        .payment_params
        .get("cashierScene")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or("checkout")
        .to_owned();
    let Ok(Some((secret, key_epoch))) = resolve_cashier_token_mint_material(pool, tenant_id).await
    else {
        return outcome;
    };
    let subject = CashierTokenSubject {
        tenant_id,
        order_id: &outcome.order_id,
        out_trade_no: &outcome.out_trade_no,
        amount: outcome.amount.as_str(),
        currency_code: &currency_code,
        scene: &scene,
        key_epoch,
    };
    let token_id = uuid::Uuid::new_v4().to_string();
    let Ok(token) =
        mint_cashier_token_with_secret(&secret, &subject, now().timestamp(), &token_id)
    else {
        return outcome;
    };
    let Some(cashier_url) = cashier_url_for_token(&token, None) else {
        return outcome;
    };
    outcome
        .payment_params
        .insert("cashierUrl".to_owned(), cashier_url.clone());
    outcome
        .payment_params
        .insert("qrCodePayload".to_owned(), cashier_url);
    outcome
}

/// Reads the currency bound to a payment attempt.
async fn load_attempt_currency_code(
    pool: &sqlx::PgPool,
    tenant_id: &str,
    payment_id: &str,
) -> Option<String> {
    use sqlx::Row;
    let row = sqlx::query(
        "SELECT currency_code FROM commerce_payment_attempt \
         WHERE tenant_id = CAST($1 AS TEXT) AND id = CAST($2 AS TEXT) LIMIT 1",
    )
    .bind(tenant_id)
    .bind(payment_id)
    .fetch_optional(pool)
    .await
    .ok()??;
    row.try_get::<Option<String>, _>("currency_code")
        .ok()
        .flatten()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdkwork_utils_rust::derive_cashier_token_key;

    const SECRET: &[u8] = b"sdkwork-cashier-token-test-secret-0001";

    fn subject<'a>() -> CashierTokenSubject<'a> {
        CashierTokenSubject {
            tenant_id: "100001",
            order_id: "ORD-1",
            out_trade_no: "OT-1",
            amount: "6990",
            currency_code: "CNY",
            scene: "checkout",
            key_epoch: 1,
        }
    }

    #[test]
    fn minted_token_round_trips_through_the_same_secret() {
        let token = mint_cashier_token_with_secret(SECRET, &subject(), 1_700_000_000, "jti-1")
            .expect("mint");
        assert!(token.starts_with("v1."));

        let key = derive_cashier_token_key(SECRET, "100001").expect("signing key");
        let claims = sdkwork_utils_rust::verify_cashier_token(
            &token,
            &key,
            1_700_000_100,
            sdkwork_utils_rust::CASHIER_TOKEN_AUDIENCE,
        )
        .expect("verify");
        assert_eq!(claims.order_id, "ORD-1");
        assert_eq!(claims.out_trade_no, "OT-1");
        assert_eq!(claims.amount, "6990");
        assert_eq!(claims.currency_code, "CNY");
        assert_eq!(claims.scene, "checkout");
        assert_eq!(claims.token_id, "jti-1");
        assert_eq!(
            claims.expires_at - claims.issued_at,
            CASHIER_TOKEN_TTL_SECONDS
        );
    }

    #[test]
    fn another_tenant_key_cannot_verify_the_token() {
        let token = mint_cashier_token_with_secret(SECRET, &subject(), 1_700_000_000, "jti-1")
            .expect("mint");
        let foreign = derive_cashier_token_key(SECRET, "200002").expect("foreign key");
        assert!(
            sdkwork_utils_rust::verify_cashier_token(
                &token,
                &foreign,
                1_700_000_100,
                sdkwork_utils_rust::CASHIER_TOKEN_AUDIENCE,
            )
            .is_err()
        );
    }

    #[test]
    fn short_secret_is_rejected_instead_of_signing() {
        assert!(mint_cashier_token_with_secret(b"too-short", &subject(), 1_700_000_000, "jti-1")
            .is_err());
    }

    #[test]
    fn cashier_url_honours_the_configured_h5_origin() {
        let token = "v1.payload.sig";
        assert_eq!(
            cashier_url_for_token(token, Some("https://h5.example.com/")).as_deref(),
            Some("https://h5.example.com/cashier/v1.payload.sig")
        );
        let fallback = cashier_url_for_token(token, None).expect("fallback url");
        assert!(fallback.ends_with("/cashier/v1.payload.sig"));
    }

    #[test]
    fn cashier_url_never_leaks_the_order_key() {
        let url =
            cashier_url_for_token("v1.payload.sig", Some("https://h5.example.com")).expect("url");
        assert!(!url.contains("ORD-1"));
        assert!(!url.contains("outTradeNo="));
        assert!(!url.contains("scene="));
    }
}
