//! Owner-order pay PSP enrichment after repository persistence.
//!
//! Shared by payment and order app-api routers so `orders.payments.create` and `payments.create`
//! return the same cashier parameters.
use crate::load_default_notify_domain_postgres;
use crate::payment_domain_setting::resolve_h5_cashier_base_url_postgres;
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
use std::sync::{Arc, OnceLock};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
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
use crate::cashier_token::{cashier_url_for_token, mint_cashier_token, CashierTokenSubject};
use crate::owner_payment_params::owner_order_payment_params;
use crate::payment_attempt_context::{
    load_payment_attempt_provider_context_postgres, persist_attempt_enrichment_postgres,
};
const PROVIDER_CHECKOUT_TTL_SECONDS: i64 = 900;
const POSTGRES_CHECKOUT_LOCK_RETRY_MILLIS: u64 = 25;
const POSTGRES_CHECKOUT_LOCK_TIMEOUT_SECONDS: u64 = 30;
/// Longer than the advisory-lock deadline: a gate waiter is queued behind
/// checkouts that themselves finish within the lock timeout plus PSP time.
const POSTGRES_CHECKOUT_GATE_TIMEOUT_SECONDS: u64 = 90;

/// Serializes checkout-lock acquisition so the advisory-lock transaction and
/// its follow-up pool queries together can never exceed the pool capacity.
///
/// Each checkout pins one pooled connection for the advisory-lock transaction
/// while its locked work takes a second connection from the same pool. If the
/// pool ever fills with lock transactions alone, every holder waits forever
/// for a work connection. Capping concurrent checkouts at half the pool
/// capacity keeps at most `permits` lock connections plus `permits` work
/// connections outstanding — always within `max_connections`.
///
/// Sized once per process: the payment capability runs on the process-shared
/// authoritative PostgreSQL pool (`DATABASE_SPEC.md` process shared pool).
fn checkout_connection_gate(pool: &PgPool) -> Result<Arc<Semaphore>, CommerceServiceError> {
    static GATE: OnceLock<Arc<Semaphore>> = OnceLock::new();
    let max_connections = pool.options().get_max_connections();
    if max_connections < 2 {
        return Err(CommerceServiceError::storage(
            "payment checkout advisory locking requires a PostgreSQL pool with at least two connections",
        ));
    }
    let permits = usize::try_from(max_connections / 2)
        .unwrap_or(1)
        .max(1);
    Ok(GATE
        .get_or_init(|| Arc::new(Semaphore::new(permits)))
        .clone())
}

/// Acquires the checkout connection gate, bounding concurrent lock-holding
/// checkouts so the lock transaction plus its pool work cannot exhaust the
/// pool into self-deadlock. The wait itself is bounded: a caller that cannot
/// obtain a gate permit fails with a retryable locked error instead of
/// queueing indefinitely behind other checkouts.
async fn acquire_checkout_connection_gate(
    pool: &PgPool,
) -> Result<OwnedSemaphorePermit, CommerceServiceError> {
    let gate = checkout_connection_gate(pool)?;
    let wait = tokio::time::timeout(
        std::time::Duration::from_secs(POSTGRES_CHECKOUT_GATE_TIMEOUT_SECONDS),
        Arc::clone(&gate).acquire_owned(),
    )
    .await;
    match wait {
        Ok(Ok(permit)) => Ok(permit),
        Ok(Err(_)) => Err(CommerceServiceError::storage(
            "payment checkout connection gate is closed",
        )),
        Err(_) => Err(CommerceServiceError::locked(
            "payment checkout is busy: too many concurrent checkouts, retry shortly",
        )),
    }
}
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
    let _connection_gate = acquire_checkout_connection_gate(pool).await?;
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
    let _connection_gate = acquire_checkout_connection_gate(pool).await?;
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
    // Resolve the configured H5 payment domain (支付设置 → 域名设置). It is the
    // operator-chosen origin for the cashier deep-link / QR code payload and
    // the default H5 return URL handed to redirect-style PSP products.
    let h5_domain_base =
        resolve_h5_cashier_base_url_postgres(pool, context.tenant_id, context.organization_id)
            .await?;
    let cashier_scene = cashier_scene_from_payment_params(&outcome.payment_params);
    // Mint the anonymous-cashier capability before the links are built: the token
    // is what lets the H5 cashier open without a login session. Minting is skipped
    // (and the legacy login-required link is kept) when the attempt has no currency
    // to bind, or when the host has not installed a payment credential cipher.
    let cashier_token = mint_cashier_token_for_outcome(
        &context,
        &outcome,
        &cashier_scene,
        attempt_context.currency_code.as_deref(),
    )?;
    let mut checkout_metadata = attempt_context.payment_metadata.clone();
    if let Some(base) = h5_domain_base.as_deref() {
        apply_default_h5_return_url(
            &mut checkout_metadata,
            base,
            &cashier_scene,
            context.order_id,
            &outcome.out_trade_no,
            cashier_token.as_deref(),
        );
    }
    let enriched = enrich_owner_order_payment_outcome(
        &context,
        account.as_ref().map(provider_account_binding),
        &provider_code,
        &attempt_context.idempotency_key,
        Some(&checkout_metadata),
        outcome,
        expires_at.as_deref(),
        attempt_context.currency_code.as_deref(),
        notify_domain_base.as_deref(),
    )
    .await?;
    // The provider enrichment may surface its own `cashierUrl`; a minted token URL
    // wins over it, and the configured H5 domain wins over the deployment default,
    // so every generated QR/cashier link follows the operator's configuration.
    let enriched =
        apply_configured_h5_cashier_url(enriched, h5_domain_base.as_deref(), cashier_token.as_deref());
    persist_attempt_enrichment_postgres(
        pool,
        context.tenant_id,
        &enriched.payment_id,
        &enriched.payment_params,
    )
    .await?;
    Ok(enriched)
}

/// Reads the cashier scene recorded on the payment params, falling back to the
/// canonical `checkout` scene. The scene keeps the H5 cashier route consistent
/// with the one the order/account surfaces render.
fn cashier_scene_from_payment_params(
    params: &std::collections::BTreeMap<String, String>,
) -> String {
    if let Some(scene) = params
        .get("cashierScene")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        return scene.to_owned();
    }
    params
        .get("cashierUrl")
        .and_then(|url| url.split_once("scene=").map(|(_, rest)| rest))
        .and_then(|rest| rest.split('&').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("checkout")
        .to_owned()
}

/// Mints the anonymous-cashier capability for a checkout outcome.
///
/// Skipped (returns `Ok(None)`) when the attempt has no bound currency: the
/// anonymous cashier verifies the charged amount against the token, so a token
/// without a currency cannot be validated and must not be issued.
fn mint_cashier_token_for_outcome(
    context: &OwnerOrderPaymentEnrichmentContext<'_>,
    outcome: &PayOwnerOrderOutcome,
    scene: &str,
    currency_code: Option<&str>,
) -> Result<Option<String>, CommerceServiceError> {
    let Some(currency_code) = currency_code.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    mint_cashier_token(&CashierTokenSubject {
        tenant_id: context.tenant_id,
        order_id: &outcome.order_id,
        out_trade_no: &outcome.out_trade_no,
        amount: outcome.amount.as_str(),
        currency_code,
        scene,
    })
}

/// Supplies a default H5 `return_url` for redirect-style PSP products (Alipay
/// WAP/PC) when neither the caller nor the order metadata provided one. The
/// field is only a default: explicit `return_url`/`returnUrl` metadata wins.
///
/// A minted cashier token is used so the payer lands back on the *anonymous*
/// cashier after the provider redirect; the legacy link would drop them on the
/// login wall and they could not see the payment result.
fn apply_default_h5_return_url(
    metadata: &mut serde_json::Value,
    base: &str,
    scene: &str,
    order_id: &str,
    out_trade_no: &str,
    cashier_token: Option<&str>,
) {
    if !metadata.is_object() {
        *metadata = serde_json::json!({});
    }
    let absent = |key: &str| {
        metadata
            .get(key)
            .map(|value| {
                value.is_null()
                    || value
                        .as_str()
                        .is_some_and(|text| text.trim().is_empty())
            })
            .unwrap_or(true)
    };
    if absent("return_url") && absent("returnUrl") {
        let return_url = cashier_token
            .and_then(|token| cashier_url_for_token(token, Some(base)))
            .unwrap_or_else(|| {
                sdkwork_utils_rust::build_commerce_cashier_url_with_base(
                    scene,
                    order_id,
                    out_trade_no,
                    Some(base),
                )
            });
        metadata["return_url"] = serde_json::json!(return_url);
    }
}

/// Rewrites the cashier URL / QR payload for the anonymous cashier.
///
/// A minted token wins even when no H5 domain is configured: the params still
/// carry the legacy link from `owner_order_payment_params`, and leaving it in
/// place would keep the scanned QR on the login-required path. Without a token
/// the configured H5 domain still rewrites the legacy link, which is the exact
/// pre-token behaviour.
fn apply_configured_h5_cashier_url(
    mut outcome: PayOwnerOrderOutcome,
    h5_base_url: Option<&str>,
    cashier_token: Option<&str>,
) -> PayOwnerOrderOutcome {
    let configured_base = h5_base_url.map(str::trim).filter(|value| !value.is_empty());
    let scene = cashier_scene_from_payment_params(&outcome.payment_params);
    let cashier_url = cashier_token
        .and_then(|token| cashier_url_for_token(token, configured_base))
        .or_else(|| {
            configured_base.map(|base| {
                sdkwork_utils_rust::build_commerce_cashier_url_with_base(
                    &scene,
                    &outcome.order_id,
                    &outcome.out_trade_no,
                    Some(base),
                )
            })
        });
    let Some(cashier_url) = cashier_url else {
        return outcome;
    };
    outcome
        .payment_params
        .insert("cashierUrl".to_owned(), cashier_url.clone());
    outcome
        .payment_params
        .insert("qrCodePayload".to_owned(), cashier_url);
    outcome
        .payment_params
        .insert("cashierScene".to_owned(), scene);
    if let Some(base) = configured_base {
        outcome.payment_params.insert(
            "cashierBaseUrl".to_owned(),
            base.trim_end_matches('/').to_owned(),
        );
    }
    outcome
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
        "SELECT owner_user_id FROM commerce_payment_attempt WHERE tenant_id = CAST($1 AS TEXT) AND ((organization_id = CAST($2 AS TEXT)) OR (organization_id = '0' AND $2 IS NULL)) AND order_id = CAST($3 AS TEXT) AND id = CAST($4 AS TEXT) AND LOWER(COALESCE(status, '')) IN ('created', 'pending', 'processing') AND deleted_at IS NULL",
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
    let deployment_registry_fallback = account.is_none();
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
    let mut outcome =
        enrich_pay_owner_order_outcome(&registry, &checkout_context, outcome).await?;
    if deployment_registry_fallback {
        // Settlement attribution must never be implicit: when a tenant has no
        // bound provider account, checkout falls back to the deployment-level
        // PSP account. Mark the cashier parameters and log loudly so operators
        // can spot tenants charging through the platform account.
        tracing::warn!(
            provider_code = %provider_code,
            tenant_id = %context.tenant_id,
            "no tenant provider account binding: checkout is routed through the deployment-level provider credentials"
        );
        outcome
            .payment_params
            .entry("credentialSource".to_owned())
            .or_insert_with(|| "deployment-registry".to_owned());
    }
    Ok(outcome)
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
    // per-provider account metadata remain the fallbacks. A missing notify
    // URL means the PSP has nowhere to call back and settlement falls
    // entirely to compensation sweeps — that must never be silent.
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
    if notify_url.is_none() {
        tracing::warn!(
            target = "payment.checkout",
            provider_code = %normalize_provider_code(provider_code),
            tenant_id = %context.tenant_id,
            order_id = %context.order_id,
            "no notify domain row and no ORDER_PAYMENT_WEBHOOK_BASE_URL configured: provider callbacks are disabled for this checkout"
        );
    }
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
        "SELECT to_char(expires_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS expires_at, organization_id FROM commerce_payment_attempt WHERE tenant_id = CAST($1 AS TEXT) AND owner_user_id = CAST($2 AS TEXT) AND id = CAST($3 AS TEXT) AND LOWER(COALESCE(status, '')) IN ('created', 'pending', 'processing') AND deleted_at IS NULL",
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
    let attempt_organization_id =
        sqlx::Row::try_get::<String, _>(&row, "organization_id").unwrap_or_else(|_| "0".to_owned());
    let expires_at = provider_checkout_expiration(order_expires_at.as_deref())?;
    let update = sqlx::query(
        "UPDATE commerce_payment_attempt SET expires_at = $1::timestamptz, updated_at = $2::timestamptz WHERE tenant_id = CAST($3 AS TEXT) AND id = CAST($4 AS TEXT) AND organization_id = CAST($5 AS TEXT) AND LOWER(COALESCE(status, '')) IN ('created', 'pending', 'processing') AND deleted_at IS NULL",
    )
    .bind(&expires_at)
    .bind(crate::shared::current_timestamp_string())
    .bind(tenant_id)
    .bind(attempt_id)
    .bind(&attempt_organization_id)
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

#[cfg(test)]
mod cashier_link_tests {
    use super::*;
    use sdkwork_contract_service::CommerceMoney;

    const TOKEN: &str = "v1.eyJvcmRlcklkIjoiT1JELTEifQ.sig";

    fn outcome() -> PayOwnerOrderOutcome {
        let mut payment_params = std::collections::BTreeMap::new();
        payment_params.insert(
            "cashierUrl".to_owned(),
            "https://platform.sdkwork.com/cashier/ORD-1?scene=checkout&outTradeNo=OT-1"
                .to_owned(),
        );
        payment_params.insert(
            "qrCodePayload".to_owned(),
            "https://platform.sdkwork.com/cashier/ORD-1?scene=checkout&outTradeNo=OT-1"
                .to_owned(),
        );
        payment_params.insert("cashierScene".to_owned(), "checkout".to_owned());
        PayOwnerOrderOutcome {
            amount: CommerceMoney::new("6990").expect("amount"),
            order_id: "ORD-1".to_owned(),
            out_trade_no: "OT-1".to_owned(),
            payment_id: "pay-1".to_owned(),
            payment_method: "wechat_pay".to_owned(),
            status: "created".to_owned(),
            payment_params,
        }
    }

    #[test]
    fn configured_h5_domain_keeps_the_minted_token() {
        let enriched = apply_configured_h5_cashier_url(
            outcome(),
            Some("https://h5.example.com/"),
            Some(TOKEN),
        );
        let cashier_url = enriched
            .payment_params
            .get("cashierUrl")
            .expect("cashier url")
            .clone();
        assert_eq!(
            cashier_url,
            format!("https://h5.example.com/cashier/{TOKEN}")
        );
        assert_eq!(
            enriched.payment_params.get("qrCodePayload"),
            Some(&cashier_url)
        );
        assert_eq!(
            enriched.payment_params.get("cashierBaseUrl").map(String::as_str),
            Some("https://h5.example.com")
        );
        assert_eq!(
            enriched.payment_params.get("cashierScene").map(String::as_str),
            Some("checkout")
        );
    }

    #[test]
    fn a_minted_token_replaces_the_legacy_link_without_a_configured_domain() {
        let enriched = apply_configured_h5_cashier_url(outcome(), None, Some(TOKEN));
        let cashier_url = enriched
            .payment_params
            .get("cashierUrl")
            .expect("cashier url")
            .clone();
        assert!(cashier_url.ends_with(&format!("/cashier/{TOKEN}")));
        assert!(
            !cashier_url.contains("ORD-1"),
            "the anonymous cashier must not leak the order key"
        );
        assert!(!cashier_url.contains("outTradeNo="));
        assert!(
            enriched.payment_params.get("cashierBaseUrl").is_none(),
            "no configured domain means no cashierBaseUrl override"
        );
    }

    #[test]
    fn without_a_token_the_configured_domain_keeps_the_legacy_link() {
        let enriched =
            apply_configured_h5_cashier_url(outcome(), Some("https://h5.example.com"), None);
        assert_eq!(
            enriched.payment_params.get("cashierUrl").map(String::as_str),
            Some("https://h5.example.com/cashier/ORD-1?scene=checkout&outTradeNo=OT-1")
        );
    }

    #[test]
    fn without_a_token_and_without_a_domain_nothing_changes() {
        let enriched = apply_configured_h5_cashier_url(outcome(), None, None);
        assert_eq!(
            enriched.payment_params.get("cashierUrl").map(String::as_str),
            Some("https://platform.sdkwork.com/cashier/ORD-1?scene=checkout&outTradeNo=OT-1")
        );
    }

    #[test]
    fn wap_return_url_carries_the_token() {
        let mut metadata = serde_json::json!({});
        apply_default_h5_return_url(
            &mut metadata,
            "https://h5.example.com/",
            "checkout",
            "ORD-1",
            "OT-1",
            Some(TOKEN),
        );
        assert_eq!(
            metadata.get("return_url").and_then(serde_json::Value::as_str),
            Some(format!("https://h5.example.com/cashier/{TOKEN}").as_str())
        );
    }

    #[test]
    fn wap_return_url_falls_back_to_the_legacy_link_without_a_token() {
        let mut metadata = serde_json::json!({});
        apply_default_h5_return_url(
            &mut metadata,
            "https://h5.example.com",
            "checkout",
            "ORD-1",
            "OT-1",
            None,
        );
        assert_eq!(
            metadata.get("return_url").and_then(serde_json::Value::as_str),
            Some("https://h5.example.com/cashier/ORD-1?scene=checkout&outTradeNo=OT-1")
        );
    }

    #[test]
    fn an_explicit_return_url_wins_over_the_minted_token() {
        let mut metadata = serde_json::json!({ "return_url": "https://merchant.example/back" });
        apply_default_h5_return_url(
            &mut metadata,
            "https://h5.example.com",
            "checkout",
            "ORD-1",
            "OT-1",
            Some(TOKEN),
        );
        assert_eq!(
            metadata.get("return_url").and_then(serde_json::Value::as_str),
            Some("https://merchant.example/back")
        );
    }
}
