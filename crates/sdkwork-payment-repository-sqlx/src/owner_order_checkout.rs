//! Owner-order pay PSP enrichment after repository persistence.
//!
//! Shared by payment and order app-api routers so `orders.payments.create` and `payments.create`
//! return the same cashier parameters.
use crate::load_default_notify_domain_postgres;
use crate::provider_account::{
    ensure_provider_account_matches, load_active_provider_account_for_channel_postgres,
    load_active_provider_account_postgres, load_provider_account_for_existing_payment_postgres,
    PaymentProviderAccountRecord,
};
use chrono::{DateTime, Duration, Utc};
use sdkwork_contract_service::CommerceServiceError;
use sdkwork_payment_providers::{
    enrich_pay_owner_order_outcome, normalize_provider_code, provider_registry_for_account,
    CheckoutContext, PaymentProviderRegistry, ProviderAccountBinding, ProviderCredentialBundle,
};
use sdkwork_payment_service::{
    CancelOrderPaymentsCommand, CreateOwnerPaymentAttemptOutcome, PayOwnerOrderOutcome,
    PaymentRecordItem,
};
use sqlx::{PgPool, Postgres, Transaction};
use tokio::time::{sleep, Instant};
pub fn provider_account_binding(record: &PaymentProviderAccountRecord) -> ProviderAccountBinding {
    ProviderAccountBinding {
        provider_code: record.provider_code.clone(),
        merchant_id: record.merchant_id.clone(),
        environment: record.environment.clone(),
        secret_ref: record.secret_ref.clone(),
        webhook_secret_ref: record.webhook_secret_ref.clone(),
        certificate_ref: record.certificate_ref.clone(),
        primary_secret: record.primary_secret.clone(),
        webhook_secret: record.webhook_secret.clone(),
        certificate: record.certificate.clone(),
        metadata: record.metadata.clone(),
    }
}
use crate::owner_payment_params::owner_order_payment_params;
use crate::payment_attempt_context::{
    load_payment_attempt_provider_context_postgres, persist_attempt_enrichment_postgres,
};
const PROVIDER_CHECKOUT_TTL_SECONDS: i64 = 900;
const POSTGRES_CHECKOUT_LOCK_RETRY_MILLIS: u64 = 25;
const POSTGRES_CHECKOUT_LOCK_TIMEOUT_SECONDS: u64 = 30;
#[derive(Clone, Copy)]
pub struct OwnerOrderPaymentEnrichmentContext<'a> {
    pub deployment_registry: &'a PaymentProviderRegistry,
    pub credentials: &'a ProviderCredentialBundle,
    pub tenant_id: &'a str,
    pub organization_id: Option<&'a str>,
    pub order_id: &'a str,
    pub payment_scene: Option<&'a str>,
}
pub fn payment_record_is_checkout_eligible(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        "created" | "pending" | "processing"
    )
}
pub async fn cancel_owner_order_payments_with_provider_postgres(
    pool: &PgPool,
    deployment_registry: &PaymentProviderRegistry,
    credentials: &ProviderCredentialBundle,
    command: CancelOrderPaymentsCommand,
) -> Result<(), CommerceServiceError> {
    let lock_key = checkout_lock_key_from_parts(
        &command.tenant_id,
        command.organization_id.as_deref(),
        &command.order_id,
    );
    let lock_transaction = acquire_postgres_checkout_lock(pool, &lock_key).await?;
    let result = crate::owner_order_provider_close::cancel_owner_order_payments_with_provider_postgres_unlocked(
        pool,
        deployment_registry,
        credentials,
        command,
    )
    .await;
    let release_result = release_postgres_checkout_lock(lock_transaction).await;
    match (result, release_result) {
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Ok(()), Ok(_)) => Ok(()),
    }
}
pub async fn enrich_payment_record_checkout_postgres(
    pool: &PgPool,
    deployment_registry: &PaymentProviderRegistry,
    credentials: &ProviderCredentialBundle,
    tenant_id: &str,
    organization_id: Option<&str>,
    owner_user_id: &str,
    record: PaymentRecordItem,
) -> Result<PayOwnerOrderOutcome, CommerceServiceError> {
    let base = payment_record_to_pay_outcome(&record, None);
    if !payment_record_is_checkout_eligible(&record.status) {
        return Ok(base);
    }
    let Some(ctx) =
        load_payment_attempt_provider_context_postgres(pool, tenant_id, owner_user_id, &record.id)
            .await?
    else {
        return Ok(base);
    };
    let outcome = payment_record_to_pay_outcome(&record, Some(&ctx));
    let enriched = enrich_owner_order_payment_postgres(
        pool,
        OwnerOrderPaymentEnrichmentContext {
            deployment_registry,
            credentials,
            tenant_id,
            organization_id,
            order_id: &record.order_id,
            payment_scene: None,
        },
        outcome,
    )
    .await?;
    Ok(enriched)
}
fn payment_record_to_pay_outcome(
    record: &PaymentRecordItem,
    provider_ctx: Option<&crate::payment_attempt_context::PaymentAttemptProviderContext>,
) -> PayOwnerOrderOutcome {
    let provider_code = provider_ctx
        .map(|ctx| ctx.provider_code.clone())
        .unwrap_or_else(|| record.method.clone());
    let out_trade_no = provider_ctx
        .map(|ctx| ctx.out_trade_no.clone())
        .unwrap_or_else(|| record.order_no.clone());
    let mut payment_params = owner_order_payment_params(
        &provider_code,
        &record.order_id,
        &record.order_no,
        None,
        &out_trade_no,
    );
    if let Some(ctx) = provider_ctx {
        if let Some(channel_id) = ctx.channel_id.as_deref() {
            payment_params.insert("channelId".to_owned(), channel_id.to_owned());
        }
        if let Some(native_id) = ctx.provider_transaction_id.as_deref() {
            payment_params.insert("providerTransactionId".to_owned(), native_id.to_owned());
        }
    }
    PayOwnerOrderOutcome {
        amount: record.amount.clone(),
        order_id: record.order_id.clone(),
        out_trade_no,
        payment_id: record.id.clone(),
        payment_method: record.method.clone(),
        status: record.status.clone(),
        payment_params,
    }
}
pub async fn enrich_owner_order_payment_postgres(
    pool: &PgPool,
    context: OwnerOrderPaymentEnrichmentContext<'_>,
    outcome: PayOwnerOrderOutcome,
) -> Result<PayOwnerOrderOutcome, CommerceServiceError> {
    let lock_key = checkout_lock_key(&context);
    let lock_transaction = acquire_postgres_checkout_lock(pool, &lock_key).await?;
    let result = enrich_owner_order_payment_postgres_locked(pool, context, outcome).await;
    let release_result = release_postgres_checkout_lock(lock_transaction).await;
    match (result, release_result) {
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Ok(outcome), Ok(_)) => Ok(outcome),
    }
}
async fn enrich_owner_order_payment_postgres_locked(
    pool: &PgPool,
    context: OwnerOrderPaymentEnrichmentContext<'_>,
    outcome: PayOwnerOrderOutcome,
) -> Result<PayOwnerOrderOutcome, CommerceServiceError> {
    let owner_user_id = load_active_attempt_owner_user_id_postgres(
        pool,
        context.tenant_id,
        context.organization_id,
        context.order_id,
        &outcome.payment_id,
    )
    .await?;
    let attempt_context = load_payment_attempt_provider_context_postgres(
        pool,
        context.tenant_id,
        &owner_user_id,
        &outcome.payment_id,
    )
    .await?
    .ok_or_else(|| {
        CommerceServiceError::conflict(
            "payment attempt was superseded while checkout was being prepared",
        )
    })?;
    ensure_provider_attempt_snapshot(&attempt_context, &outcome)?;
    crate::owner_order_provider_close::close_expired_owner_order_provider_attempts_for_order_postgres(
        pool,
        context.deployment_registry,
        context.credentials,
        context.tenant_id,
        context.organization_id,
        &owner_user_id,
        context.order_id,
    )
    .await?;
    let expires_at = provider_checkout_expiration_postgres(
        pool,
        context.tenant_id,
        &owner_user_id,
        &outcome.payment_id,
    )
    .await?;
    crate::owner_order_provider_close::close_owner_order_provider_attempts_postgres(
        pool,
        context.deployment_registry,
        context.credentials,
        context.tenant_id,
        context.organization_id,
        &owner_user_id,
        context.order_id,
        Some(&outcome.payment_id),
    )
    .await?;
    let provider_code = attempt_context.provider_code.clone();
    let account =
        provider_account_for_attempt_postgres(pool, &context, &attempt_context, &provider_code)
            .await?;
    // Resolve the configured default notify domain (exact org -> platform
    // '0' -> env fallback inside provider_checkout_context) so the notify URL
    // passed to the PSP is built from the payment center configuration.
    let notify_domain_base =
        load_default_notify_domain_postgres(pool, context.tenant_id, context.organization_id)
            .await?
            .map(|domain| {
                build_notify_domain_base_url(&domain.protocol, &domain.hostname, domain.port)
            });
    let enriched = enrich_owner_order_payment_outcome(
        &context,
        account.as_ref().map(provider_account_binding),
        &provider_code,
        &attempt_context.idempotency_key,
        Some(&attempt_context.payment_metadata),
        outcome,
        expires_at.as_deref(),
        attempt_context.currency_code.as_deref(),
        notify_domain_base.as_deref(),
    )
    .await?;
    persist_attempt_enrichment_postgres(
        pool,
        context.tenant_id,
        &enriched.payment_id,
        &enriched.payment_params,
    )
    .await?;
    Ok(enriched)
}
async fn provider_account_for_attempt_postgres(
    pool: &PgPool,
    context: &OwnerOrderPaymentEnrichmentContext<'_>,
    attempt: &crate::payment_attempt_context::PaymentAttemptProviderContext,
    provider_code: &str,
) -> Result<Option<PaymentProviderAccountRecord>, CommerceServiceError> {
    let account = if let Some(provider_account_id) = attempt.provider_account_id.as_deref() {
        load_provider_account_for_existing_payment_postgres(
            pool,
            context.tenant_id,
            context.organization_id,
            provider_account_id,
        )
        .await?
        .ok_or_else(|| {
            CommerceServiceError::conflict(
                "payment attempt provider account snapshot is unavailable",
            )
        })?
        .into()
    } else if let Some(channel_id) = attempt.channel_id.as_deref() {
        load_active_provider_account_for_channel_postgres(
            pool,
            context.tenant_id,
            context.organization_id,
            channel_id,
            provider_code,
        )
        .await?
    } else {
        load_active_provider_account_postgres(
            pool,
            context.tenant_id,
            context.organization_id,
            provider_code,
        )
        .await?
    };
    ensure_provider_account_matches(account.as_ref(), provider_code)?;
    Ok(account)
}
fn checkout_lock_key(context: &OwnerOrderPaymentEnrichmentContext<'_>) -> String {
    checkout_lock_key_from_parts(context.tenant_id, context.organization_id, context.order_id)
}
fn checkout_lock_key_from_parts(
    tenant_id: &str,
    organization_id: Option<&str>,
    order_id: &str,
) -> String {
    fn component(value: &str) -> String {
        format!("{}:{value}", value.len())
    }
    let organization = organization_id
        .map(component)
        .unwrap_or_else(|| "none".to_owned());
    format!(
        "payment-checkout:v1|{}|{organization}|{}",
        component(tenant_id),
        component(order_id)
    )
}
async fn acquire_postgres_checkout_lock(
    pool: &PgPool,
    lock_key: &str,
) -> Result<Transaction<'static, Postgres>, CommerceServiceError> {
    if pool.options().get_max_connections() < 2 {
        return Err(CommerceServiceError::storage(
            "payment checkout advisory locking requires a PostgreSQL pool with at least two connections",
        ));
    }
    let deadline =
        Instant::now() + std::time::Duration::from_secs(POSTGRES_CHECKOUT_LOCK_TIMEOUT_SECONDS);
    loop {
        let mut transaction = pool.begin().await.map_err(|error| {
            crate::shared::store_error("failed to begin payment checkout lock transaction", error)
        })?;
        let acquired = sqlx::query_scalar::<_, bool>(
            "SELECT pg_try_advisory_xact_lock(hashtextextended($1, 0))",
        )
        .bind(lock_key)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| {
            crate::shared::store_error("failed to acquire payment checkout advisory lock", error)
        })?;
        if acquired {
            return Ok(transaction);
        }
        transaction.rollback().await.map_err(|error| {
            crate::shared::store_error("failed to roll back payment checkout lock attempt", error)
        })?;
        if Instant::now() >= deadline {
            return Err(CommerceServiceError::locked(
                "payment checkout is already being processed",
            ));
        }
        sleep(std::time::Duration::from_millis(
            POSTGRES_CHECKOUT_LOCK_RETRY_MILLIS,
        ))
        .await;
    }
}
async fn release_postgres_checkout_lock(
    transaction: Transaction<'static, Postgres>,
) -> Result<(), CommerceServiceError> {
    transaction.commit().await.map_err(|error| {
        crate::shared::store_error("failed to release payment checkout advisory lock", error)
    })
}
pub async fn enrich_owner_payment_attempt_postgres(
    pool: &PgPool,
    context: OwnerOrderPaymentEnrichmentContext<'_>,
    outcome: CreateOwnerPaymentAttemptOutcome,
) -> Result<CreateOwnerPaymentAttemptOutcome, CommerceServiceError> {
    let pay_outcome = attempt_outcome_to_pay_outcome(&outcome);
    let enriched = enrich_owner_order_payment_postgres(pool, context, pay_outcome).await?;
    Ok(merge_attempt_payment_params(
        outcome,
        enriched.payment_params,
    ))
}
fn attempt_outcome_to_pay_outcome(
    outcome: &CreateOwnerPaymentAttemptOutcome,
) -> PayOwnerOrderOutcome {
    let mut payment_params = outcome.payment_params.clone();
    payment_params
        .entry("providerCode".to_owned())
        .or_insert_with(|| outcome.provider_code.clone());
    PayOwnerOrderOutcome {
        amount: outcome.amount.clone(),
        order_id: outcome.order_id.clone(),
        out_trade_no: outcome.out_trade_no.clone(),
        payment_id: outcome.attempt_id.clone(),
        payment_method: outcome.payment_method.clone(),
        status: outcome.status.clone(),
        payment_params,
    }
}
fn merge_attempt_payment_params(
    mut outcome: CreateOwnerPaymentAttemptOutcome,
    payment_params: std::collections::BTreeMap<String, String>,
) -> CreateOwnerPaymentAttemptOutcome {
    outcome.payment_params = payment_params;
    outcome
}
async fn load_active_attempt_owner_user_id_postgres(
    pool: &PgPool,
    tenant_id: &str,
    organization_id: Option<&str>,
    order_id: &str,
    attempt_id: &str,
) -> Result<String, CommerceServiceError> {
    let row = sqlx::query(
        "SELECT owner_user_id FROM commerce_payment_attempt WHERE tenant_id = CAST($1 AS TEXT) AND ((organization_id = CAST($2 AS TEXT)) OR (organization_id IS NULL AND $2 IS NULL) OR (organization_id = '0' AND $2 IS NULL)) AND order_id = CAST($3 AS TEXT) AND id = CAST($4 AS TEXT) AND LOWER(COALESCE(status, '')) IN ('created', 'pending', 'processing') AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(organization_id)
    .bind(order_id)
    .bind(attempt_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| {
        crate::shared::store_error("failed to load active payment attempt owner", error)
    })?
    .ok_or_else(|| {
        CommerceServiceError::conflict(
            "payment attempt was superseded while checkout was being prepared",
        )
    })?;
    Ok(sqlx::Row::try_get::<String, _>(&row, "owner_user_id").unwrap_or_default())
}
async fn enrich_owner_order_payment_outcome(
    context: &OwnerOrderPaymentEnrichmentContext<'_>,
    account: Option<ProviderAccountBinding>,
    provider_code: &str,
    idempotency_key: &str,
    payment_metadata: Option<&serde_json::Value>,
    outcome: PayOwnerOrderOutcome,
    expires_at: Option<&str>,
    currency_code: Option<&str>,
    notify_domain_base: Option<&str>,
) -> Result<PayOwnerOrderOutcome, CommerceServiceError> {
    let registry = match account {
        Some(binding) => provider_registry_for_account(context.credentials, Some(binding)),
        None => context.deployment_registry.clone(),
    };
    let checkout_context = provider_checkout_context(
        context,
        provider_code,
        idempotency_key,
        payment_metadata,
        expires_at,
        currency_code,
        notify_domain_base,
    );
    enrich_pay_owner_order_outcome(&registry, &checkout_context, outcome).await
}
fn provider_checkout_context(
    context: &OwnerOrderPaymentEnrichmentContext<'_>,
    provider_code: &str,
    idempotency_key: &str,
    payment_metadata: Option<&serde_json::Value>,
    expires_at: Option<&str>,
    currency_code: Option<&str>,
    notify_domain_base: Option<&str>,
) -> CheckoutContext {
    // Configured default notify domain wins; the env webhook base and the
    // per-provider account metadata remain the fallbacks.
    let notify_url = notify_domain_base
        .map(|base| {
            let path = crate::notify_domain::ORDER_PAYMENT_WEBHOOK_PATH
                .replace("{providerCode}", &normalize_provider_code(provider_code));
            format!("{}{}", base.trim_end_matches('/'), path)
        })
        .or_else(|| {
            context
                .credentials
                .provider_notify_url(&normalize_provider_code(provider_code))
        });
    CheckoutContext {
        provider_code: provider_code.to_owned(),
        currency_code: currency_code
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("CNY")
            .to_owned(),
        tenant_id: context.tenant_id.to_owned(),
        order_id: context.order_id.to_owned(),
        idempotency_key: idempotency_key.to_owned(),
        expires_at: expires_at.map(str::to_owned),
        notify_url,
        payment_scene: context.payment_scene.map(str::to_owned),
        payment_metadata: payment_metadata.cloned(),
    }
}
/// `{protocol}://{hostname}{:port}` base for building notify URLs.
fn build_notify_domain_base_url(protocol: &str, hostname: &str, port: Option<i32>) -> String {
    match port {
        Some(port) => format!("{protocol}://{hostname}:{port}"),
        None => format!("{protocol}://{hostname}"),
    }
}

fn ensure_provider_attempt_snapshot(
    attempt: &crate::payment_attempt_context::PaymentAttemptProviderContext,
    outcome: &PayOwnerOrderOutcome,
) -> Result<(), CommerceServiceError> {
    if attempt.idempotency_key.trim().is_empty() {
        return Err(CommerceServiceError::storage(
            "payment attempt is missing its persisted idempotency key",
        ));
    }
    if attempt.out_trade_no != outcome.out_trade_no {
        return Err(CommerceServiceError::conflict(
            "payment attempt changed while checkout was being prepared",
        ));
    }
    Ok(())
}
fn provider_checkout_expiration(
    order_expires_at: Option<&str>,
) -> Result<String, CommerceServiceError> {
    let now = Utc::now();
    let provider_limit = now + Duration::seconds(PROVIDER_CHECKOUT_TTL_SECONDS);
    let expires_at = match order_expires_at
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(value) => DateTime::parse_from_rfc3339(value)
            .map_err(|_| CommerceServiceError::conflict("payment attempt expiry is invalid"))?
            .with_timezone(&Utc)
            .min(provider_limit),
        None => provider_limit,
    };
    if expires_at <= now {
        return Err(CommerceServiceError::conflict(
            "payment attempt has expired",
        ));
    }
    Ok(expires_at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
}
async fn provider_checkout_expiration_postgres(
    pool: &PgPool,
    tenant_id: &str,
    owner_user_id: &str,
    attempt_id: &str,
) -> Result<Option<String>, CommerceServiceError> {
    let row = sqlx::query(
        "SELECT to_char(expires_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS expires_at FROM commerce_payment_attempt WHERE tenant_id = CAST($1 AS TEXT) AND owner_user_id = CAST($2 AS TEXT) AND id = CAST($3 AS TEXT) AND LOWER(COALESCE(status, '')) IN ('created', 'pending', 'processing') AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(owner_user_id)
    .bind(attempt_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| crate::shared::store_error("failed to load payment attempt expiry", error))?
    .ok_or_else(|| {
        CommerceServiceError::conflict(
            "payment attempt changed while checkout expiry was being loaded",
        )
    })?;
    let order_expires_at =
        sqlx::Row::try_get::<Option<String>, _>(&row, "expires_at").unwrap_or_default();
    let expires_at = provider_checkout_expiration(order_expires_at.as_deref())?;
    let update = sqlx::query(
        "UPDATE commerce_payment_attempt SET expires_at = $1::timestamptz, updated_at = $2::timestamptz WHERE tenant_id = CAST($3 AS TEXT) AND id = CAST($4 AS TEXT) AND LOWER(COALESCE(status, '')) IN ('created', 'pending', 'processing') AND deleted_at IS NULL",
    )
    .bind(&expires_at)
    .bind(crate::shared::current_timestamp_string())
    .bind(tenant_id)
    .bind(attempt_id)
    .execute(pool)
    .await
    .map_err(|error| crate::shared::store_error("failed to persist payment attempt expiry", error))?;
    ensure_attempt_expiry_persisted(update.rows_affected())?;
    Ok(Some(expires_at))
}
fn ensure_attempt_expiry_persisted(rows_affected: u64) -> Result<(), CommerceServiceError> {
    if rows_affected == 1 {
        Ok(())
    } else {
        Err(CommerceServiceError::conflict(
            "payment attempt changed while checkout expiry was being persisted",
        ))
    }
}
