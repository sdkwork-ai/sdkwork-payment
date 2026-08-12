use crate::owner_order_checkout::provider_account_binding;
use crate::payment_attempt_context::load_payment_attempt_provider_context_postgres;
use crate::provider_account::{
    ensure_provider_account_matches, load_active_provider_account_postgres,
    load_provider_account_for_existing_payment_postgres,
};
use crate::shared::{current_timestamp_string, store_error, string_cell};
use sdkwork_contract_service::{CommercePaymentStatus, CommerceServiceError};
use sdkwork_payment_providers::{
    cancel_provider_payment, provider_registry_for_account, PaymentProviderRegistry,
    ProviderCredentialBundle,
};
use sdkwork_payment_service::CancelOrderPaymentsCommand;
use sqlx::PgPool;
#[derive(Clone, Debug, Eq, PartialEq)]
struct ActiveAttemptIdentity {
    attempt_id: String,
    payment_intent_id: String,
}
pub(crate) async fn cancel_owner_order_payments_with_provider_postgres_unlocked(
    pool: &PgPool,
    deployment_registry: &PaymentProviderRegistry,
    credentials: &ProviderCredentialBundle,
    command: CancelOrderPaymentsCommand,
) -> Result<(), CommerceServiceError> {
    close_owner_order_provider_attempts_postgres(
        pool,
        deployment_registry,
        credentials,
        &command.tenant_id,
        command.organization_id.as_deref(),
        &command.owner_user_id,
        &command.order_id,
        None,
    )
    .await?;
    crate::PostgresCommerceOwnerOrderPaymentStore::new(pool.clone())
        .cancel_order_payments(command)
        .await
}
pub async fn close_owner_order_provider_attempts_postgres(
    pool: &PgPool,
    deployment_registry: &PaymentProviderRegistry,
    credentials: &ProviderCredentialBundle,
    tenant_id: &str,
    organization_id: Option<&str>,
    owner_user_id: &str,
    order_id: &str,
    excluded_attempt_id: Option<&str>,
) -> Result<(), CommerceServiceError> {
    let rows = sqlx::query(
        r#"
        SELECT id, payment_intent_id
        FROM commerce_payment_attempt
        WHERE tenant_id = CAST($1 AS TEXT)
          AND ((organization_id = CAST($2 AS TEXT)) OR (organization_id IS NULL AND $2 IS NULL) OR (organization_id = '0' AND $2 IS NULL))
          AND owner_user_id = CAST($3 AS TEXT)
          AND order_id = CAST($4 AS TEXT)
          AND ($5::text IS NULL OR id <> CAST($5 AS TEXT))
          AND LOWER(COALESCE(status, '')) IN ('created', 'pending', 'processing')
          AND deleted_at IS NULL
        ORDER BY created_at, id
        "#,
    )
    .bind(tenant_id)
    .bind(organization_id)
    .bind(owner_user_id)
    .bind(order_id)
    .bind(excluded_attempt_id)
    .fetch_all(pool)
    .await
    .map_err(|error| store_error("failed to load active order payment attempts", error))?;
    let attempts = rows
        .iter()
        .map(|row| ActiveAttemptIdentity {
            attempt_id: string_cell(row, "id"),
            payment_intent_id: string_cell(row, "payment_intent_id"),
        })
        .collect::<Vec<_>>();
    close_attempts_postgres(
        pool,
        deployment_registry,
        credentials,
        tenant_id,
        organization_id,
        owner_user_id,
        attempts,
    )
    .await
}
pub async fn close_expired_owner_order_provider_attempts_postgres(
    pool: &PgPool,
    deployment_registry: &PaymentProviderRegistry,
    credentials: &ProviderCredentialBundle,
    tenant_id: &str,
    organization_id: Option<&str>,
    owner_user_id: &str,
) -> Result<(), CommerceServiceError> {
    close_expired_owner_order_provider_attempts_postgres_scoped(
        pool,
        deployment_registry,
        credentials,
        tenant_id,
        organization_id,
        owner_user_id,
        None,
    )
    .await
}
pub(crate) async fn close_expired_owner_order_provider_attempts_for_order_postgres(
    pool: &PgPool,
    deployment_registry: &PaymentProviderRegistry,
    credentials: &ProviderCredentialBundle,
    tenant_id: &str,
    organization_id: Option<&str>,
    owner_user_id: &str,
    order_id: &str,
) -> Result<(), CommerceServiceError> {
    close_expired_owner_order_provider_attempts_postgres_scoped(
        pool,
        deployment_registry,
        credentials,
        tenant_id,
        organization_id,
        owner_user_id,
        Some(order_id),
    )
    .await
}
async fn close_expired_owner_order_provider_attempts_postgres_scoped(
    pool: &PgPool,
    deployment_registry: &PaymentProviderRegistry,
    credentials: &ProviderCredentialBundle,
    tenant_id: &str,
    organization_id: Option<&str>,
    owner_user_id: &str,
    order_id: Option<&str>,
) -> Result<(), CommerceServiceError> {
    let rows = sqlx::query(
        r#"
        SELECT pa.id, pa.payment_intent_id
        FROM commerce_payment_attempt pa
        INNER JOIN commerce_order o
          ON o.tenant_id = pa.tenant_id
         AND o.id = pa.order_id
         AND o.owner_user_id = pa.owner_user_id
        WHERE pa.tenant_id = CAST($1 AS TEXT)
          AND ((pa.organization_id = CAST($2 AS TEXT)) OR (pa.organization_id IS NULL AND $2 IS NULL) OR (pa.organization_id = '0' AND $2 IS NULL))
          AND pa.owner_user_id = CAST($3 AS TEXT)
          AND ($4::text IS NULL OR pa.order_id = CAST($4 AS TEXT))
          AND LOWER(COALESCE(pa.status, '')) IN ('created', 'pending', 'processing')
          AND pa.deleted_at IS NULL
          AND (
            LOWER(COALESCE(o.status, '')) IN ('expired', 'closed', 'cancelled', 'canceled')
            OR (o.expired_at IS NOT NULL AND NULLIF(o.expired_at, '')::timestamptz <= CURRENT_TIMESTAMP)
          )
        ORDER BY pa.created_at, pa.id
        LIMIT 100
        "#,
    )
    .bind(tenant_id)
    .bind(organization_id)
    .bind(owner_user_id)
    .bind(order_id)
    .fetch_all(pool)
    .await
    .map_err(|error| store_error("failed to load expired order payment attempts", error))?;
    let attempts = rows
        .iter()
        .map(|row| ActiveAttemptIdentity {
            attempt_id: string_cell(row, "id"),
            payment_intent_id: string_cell(row, "payment_intent_id"),
        })
        .collect::<Vec<_>>();
    close_attempts_postgres(
        pool,
        deployment_registry,
        credentials,
        tenant_id,
        organization_id,
        owner_user_id,
        attempts,
    )
    .await
}
async fn close_attempts_postgres(
    pool: &PgPool,
    deployment_registry: &PaymentProviderRegistry,
    credentials: &ProviderCredentialBundle,
    tenant_id: &str,
    organization_id: Option<&str>,
    owner_user_id: &str,
    attempts: Vec<ActiveAttemptIdentity>,
) -> Result<(), CommerceServiceError> {
    for attempt in attempts {
        let Some(context) = load_payment_attempt_provider_context_postgres(
            pool,
            tenant_id,
            owner_user_id,
            &attempt.attempt_id,
        )
        .await?
        else {
            continue;
        };
        let registry = registry_for_existing_attempt_postgres(
            pool,
            deployment_registry,
            credentials,
            tenant_id,
            organization_id,
            &context,
        )
        .await?;
        cancel_provider_payment(
            &registry,
            &context.provider_code,
            &context.out_trade_no,
            context.provider_transaction_id.as_deref(),
        )
        .await?;
        mark_attempt_canceled_postgres(pool, tenant_id, &attempt).await?;
    }
    Ok(())
}
async fn registry_for_existing_attempt_postgres(
    pool: &PgPool,
    deployment_registry: &PaymentProviderRegistry,
    credentials: &ProviderCredentialBundle,
    tenant_id: &str,
    organization_id: Option<&str>,
    context: &crate::PaymentAttemptProviderContext,
) -> Result<PaymentProviderRegistry, CommerceServiceError> {
    if context.provider_code.trim().eq_ignore_ascii_case("sandbox") {
        return Ok(deployment_registry.clone());
    }
    let account = match context.provider_account_id.as_deref() {
        Some(account_id) => Some(
            load_provider_account_for_existing_payment_postgres(
                pool,
                tenant_id,
                organization_id,
                account_id,
            )
            .await?
            .ok_or_else(|| {
                CommerceServiceError::conflict(
                    "original payment provider account is unavailable for close",
                )
            })?,
        ),
        None if context.channel_id.is_some() => None,
        None => {
            load_active_provider_account_postgres(
                pool,
                tenant_id,
                organization_id,
                &context.provider_code,
            )
            .await?
        }
    };
    ensure_provider_account_matches(account.as_ref(), &context.provider_code)?;
    Ok(match account {
        Some(account) => {
            provider_registry_for_account(credentials, Some(provider_account_binding(&account)))
        }
        None => deployment_registry.clone(),
    })
}
async fn mark_attempt_canceled_postgres(
    pool: &PgPool,
    tenant_id: &str,
    attempt: &ActiveAttemptIdentity,
) -> Result<(), CommerceServiceError> {
    let now = current_timestamp_string();
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| store_error("failed to begin payment attempt close transaction", error))?;
    let attempt_update = sqlx::query(
        r#"
        UPDATE commerce_payment_attempt
        SET status = $1, updated_at = $2::timestamptz
        WHERE tenant_id = CAST($3 AS TEXT)
          AND id = CAST($4 AS TEXT)
          AND LOWER(COALESCE(status, '')) IN ('created', 'pending', 'processing')
          AND deleted_at IS NULL
        "#,
    )
    .bind(CommercePaymentStatus::Canceled.as_str())
    .bind(&now)
    .bind(tenant_id)
    .bind(&attempt.attempt_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| store_error("failed to close payment attempt", error))?;
    let persisted_status = if attempt_update.rows_affected() == 0 {
        sqlx::query_scalar::<_, String>(
            "SELECT status FROM commerce_payment_attempt WHERE tenant_id = CAST($1 AS TEXT) AND id = CAST($2 AS TEXT) AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(&attempt.attempt_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| store_error("failed to verify closed payment attempt", error))?
    } else {
        None
    };
    ensure_attempt_close_persisted(attempt_update.rows_affected(), persisted_status.as_deref())?;
    sqlx::query(
        r#"
        UPDATE commerce_payment_intent pi
        SET status = $1, updated_at = $2::timestamptz
        WHERE pi.tenant_id = CAST($3 AS TEXT)
          AND pi.id = CAST($4 AS TEXT)
          AND LOWER(COALESCE(pi.status, '')) IN ('created', 'pending', 'processing')
          AND pi.deleted_at IS NULL
          AND NOT EXISTS (
            SELECT 1 FROM commerce_payment_attempt pa
            WHERE pa.tenant_id = pi.tenant_id
              AND pa.payment_intent_id = pi.id
              AND LOWER(COALESCE(pa.status, '')) IN ('created', 'pending', 'processing')
              AND pa.deleted_at IS NULL
          )
        "#,
    )
    .bind(CommercePaymentStatus::Canceled.as_str())
    .bind(&now)
    .bind(tenant_id)
    .bind(&attempt.payment_intent_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| store_error("failed to close payment intent", error))?;
    tx.commit()
        .await
        .map_err(|error| store_error("failed to commit payment attempt close", error))
}
fn ensure_attempt_close_persisted(
    rows_affected: u64,
    persisted_status: Option<&str>,
) -> Result<(), CommerceServiceError> {
    match rows_affected {
        1 => Ok(()),
        0 => match persisted_status
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("canceled" | "cancelled" | "closed") => Ok(()),
            Some("succeeded" | "success" | "paid") => Err(CommerceServiceError::conflict(
                "payment attempt completed while it was being closed",
            )),
            Some(status) => Err(CommerceServiceError::storage(format!(
                "payment attempt close was not persisted from status {status}"
            ))),
            None => Err(CommerceServiceError::storage(
                "payment attempt disappeared while it was being closed",
            )),
        },
        count => Err(CommerceServiceError::storage(format!(
            "payment attempt close updated {count} rows; expected at most one"
        ))),
    }
}
