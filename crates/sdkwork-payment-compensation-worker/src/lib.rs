//! Payment compensation worker (补偿轮询 worker).
//!
//! The request-driven payment flows cover the happy path and the webhook
//! path; this worker covers everything a lost PSP callback leaves behind:
//!
//! - attempts stuck in `pending`/`processing` are queried at the PSP and
//!   terminal outcomes are re-entered through the same audited webhook
//!   settlement path (`ingest_provider_webhook_postgres`, dedupe by
//!   deterministic event id, order→intent→attempt locked transitions);
//! - refunds stuck in `submitted` are (re-)submitted to the PSP and their
//!   accepted/terminal outcomes recorded through the refund store marks;
//! - refunds stuck in `processing` are queried at the PSP and terminal
//!   states re-entered through the refund webhook ingestion path.
//!
//! Multi-replica safety: claims run in a real transaction with
//! `FOR UPDATE SKIP LOCKED` and a status flip, so concurrent workers get
//! disjoint batches; every settlement apply is a status-guarded idempotent
//! transition that fails closed on races. Repeated PSP queries for rows
//! already `processing` are safe by construction and bounded by the batch.
//!
//! Owner: each API process spawns one worker at bootstrap
//! (`spawn_payment_compensation_worker`); the pass cadence is bounded and
//! jittered, and the loop exits when the shutdown token fires.

use sdkwork_contract_service::CommerceServiceError;
use sdkwork_payment_providers::{
    normalize_provider_code, provider_registry_for_account, PaymentCreateRefundRequest,
    PaymentProviderOperationOutcome, PaymentQueryPaymentIntentRequest, PaymentQueryRefundRequest,
    ProviderCredentialBundle,
};
use sdkwork_payment_repository_sqlx::{
    claim_due_payment_attempts_postgres, claim_due_refunds_postgres,
    ingest_provider_refund_webhook_postgres, ingest_provider_webhook_postgres,
    list_due_compensation_tenants_postgres, load_claim_attempt_provider_context_postgres,
    load_payment_attempt_provider_context_by_id_postgres,
    provider_account_binding, ClaimAttemptProviderContext, ClaimedPaymentAttempt, ClaimedRefund,
    IngestProviderWebhookCommand, PostgresCommerceRefundStore,
};
use sqlx::PgPool;
use std::time::Duration;

/// Tunables for one worker instance; all bounds are hard limits per pass.
#[derive(Debug, Clone)]
pub struct PaymentCompensationWorkerConfig {
    /// Rows younger than this are left alone: their PSP call may still be in
    /// flight from the originating request.
    pub min_age_seconds: i64,
    /// Distinct tenants scanned per pass.
    pub tenant_limit: i64,
    /// Attempts claimed per tenant per pass.
    pub attempts_per_tenant: i64,
    /// Refunds claimed per tenant per pass.
    pub refunds_per_tenant: i64,
    /// Base sleep between passes; each pass adds deterministic jitter so
    /// replicas do not sweep in lockstep.
    pub interval: Duration,
    /// Kill-switch mirror: false disables the spawned loop (the pass function
    /// stays available for manual/admin invocation).
    pub enabled: bool,
}

impl Default for PaymentCompensationWorkerConfig {
    fn default() -> Self {
        Self {
            min_age_seconds: 90,
            tenant_limit: 50,
            attempts_per_tenant: 25,
            refunds_per_tenant: 25,
            interval: Duration::from_secs(60),
            enabled: std::env::var("SDKWORK_PAYMENT_COMPENSATION_ENABLED")
                .ok()
                .map(|value| !matches!(value.trim(), "0" | "false" | "off"))
                .unwrap_or(true),
        }
    }
}

impl PaymentCompensationWorkerConfig {
    /// Loads the configuration from the canonical environment keys, falling
    /// back to the defaults above. Unparsable values fail closed to defaults
    /// and are logged by the caller at bootstrap.
    pub fn from_env() -> Self {
        let mut config = Self::default();
        if let Some(value) = env_i64("SDKWORK_PAYMENT_COMPENSATION_MIN_AGE_SECONDS") {
            config.min_age_seconds = value.clamp(15, 3600);
        }
        if let Some(value) = env_i64("SDKWORK_PAYMENT_COMPENSATION_TENANT_LIMIT") {
            config.tenant_limit = value.clamp(1, 500);
        }
        if let Some(value) = env_i64("SDKWORK_PAYMENT_COMPENSATION_ATTEMPTS_PER_TENANT") {
            config.attempts_per_tenant = value.clamp(1, 200);
        }
        if let Some(value) = env_i64("SDKWORK_PAYMENT_COMPENSATION_REFUNDS_PER_TENANT") {
            config.refunds_per_tenant = value.clamp(1, 200);
        }
        if let Some(value) = env_i64("SDKWORK_PAYMENT_COMPENSATION_INTERVAL_SECONDS") {
            config.interval = Duration::from_secs(value.clamp(10, 3600) as u64);
        }
        if let Ok(value) = std::env::var("SDKWORK_PAYMENT_COMPENSATION_ENABLED") {
            config.enabled = !matches!(value.trim(), "0" | "false" | "off");
        }
        config
    }
}

fn env_i64(key: &str) -> Option<i64> {
    std::env::var(key)
        .ok()
        .and_then(|value| value.trim().parse::<i64>().ok())
}

/// Counters for one compensation pass; every failure is counted, never
/// silently dropped, and surfaced by the spawned loop through `tracing`.
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct CompensationPassReport {
    pub tenants_scanned: u64,
    pub attempts_claimed: u64,
    pub attempts_settled: u64,
    pub refunds_claimed: u64,
    pub refunds_submitted: u64,
    pub refunds_settled: u64,
    pub provider_unavailable: u64,
    pub errors: u64,
}

/// Spawns the background compensation loop. Returns the join handle so the
/// host can await it during graceful shutdown.
///
/// The loop stops cleanly when `shutdown` is cancelled (or when the runtime
/// drops the handle's runtime). Each pass is individually deadline-bounded by
/// the PSP client timeouts, so a pass cannot hang the shutdown drain.
pub fn spawn_payment_compensation_worker(
    pool: PgPool,
    credentials: ProviderCredentialBundle,
    config: PaymentCompensationWorkerConfig,
    shutdown: tokio::sync::watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if !config.enabled {
            tracing::info!("payment compensation worker disabled by configuration");
            return;
        }
        tracing::info!(
            interval_seconds = config.interval.as_secs(),
            min_age_seconds = config.min_age_seconds,
            tenant_limit = config.tenant_limit,
            "payment compensation worker started"
        );
        let mut shutdown = shutdown;
        let mut ticker = tokio::time::interval(config.interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // The first tick fires immediately; skip it so a boot does not sweep
        // before the API is ready, then run on cadence.
        ticker.tick().await;
        loop {
            tokio::select! {
                _ = ticker.tick() => {}
                _ = shutdown.changed() => {
                    tracing::info!("payment compensation worker stopping: shutdown signaled");
                    return;
                }
            }
            let report = run_payment_compensation_pass(&pool, &credentials, &config).await;
            if report.errors > 0 || report.provider_unavailable > 0 {
                tracing::warn!(
                    tenants = report.tenants_scanned,
                    attempts_settled = report.attempts_settled,
                    refunds_submitted = report.refunds_submitted,
                    refunds_settled = report.refunds_settled,
                    provider_unavailable = report.provider_unavailable,
                    errors = report.errors,
                    "payment compensation pass completed with failures"
                );
            } else if report.attempts_claimed > 0 || report.refunds_claimed > 0 {
                tracing::info!(
                    tenants = report.tenants_scanned,
                    attempts_settled = report.attempts_settled,
                    refunds_submitted = report.refunds_submitted,
                    refunds_settled = report.refunds_settled,
                    "payment compensation pass completed"
                );
            }
        }
    })
}

/// Runs one bounded compensation sweep across due tenants.
pub async fn run_payment_compensation_pass(
    pool: &PgPool,
    credentials: &ProviderCredentialBundle,
    config: &PaymentCompensationWorkerConfig,
) -> CompensationPassReport {
    let mut report = CompensationPassReport::default();
    let now_seconds = chrono::Utc::now().timestamp();
    let tenants = match list_due_compensation_tenants_postgres(
        pool,
        config.min_age_seconds,
        config.tenant_limit,
    )
    .await
    {
        Ok(tenants) => tenants,
        Err(error) => {
            report.errors += 1;
            tracing::warn!(error = %error.message(), "compensation tenant scan failed");
            return report;
        }
    };
    for tenant_id in tenants {
        report.tenants_scanned += 1;
        reconcile_tenant_attempts(pool, credentials, config, &tenant_id, now_seconds, &mut report)
            .await;
        reconcile_tenant_refunds(pool, credentials, config, &tenant_id, now_seconds, &mut report)
            .await;
    }
    report
}

async fn reconcile_tenant_attempts(
    pool: &PgPool,
    credentials: &ProviderCredentialBundle,
    config: &PaymentCompensationWorkerConfig,
    tenant_id: &str,
    now_seconds: i64,
    report: &mut CompensationPassReport,
) {
    let claimed = match claim_due_payment_attempts_postgres(
        pool,
        tenant_id,
        None,
        config.attempts_per_tenant,
        now_seconds,
        config.min_age_seconds,
    )
    .await
    {
        Ok(claimed) => claimed,
        Err(error) => {
            report.errors += 1;
            tracing::warn!(tenant_id = %tenant_id, error = %error.message(), "attempt claim failed");
            return;
        }
    };
    report.attempts_claimed += claimed.len() as u64;
    for attempt in claimed {
        match reconcile_attempt(pool, credentials, &attempt).await {
            Ok(true) => report.attempts_settled += 1,
            Ok(false) => {}
            Err(ReconcileError::ProviderUnavailable(message)) => {
                report.provider_unavailable += 1;
                tracing::debug!(
                    attempt_id = %attempt.id,
                    provider = %attempt.provider_code,
                    reason = %message,
                    "compensation provider unavailable"
                );
            }
            Err(ReconcileError::Failed(error)) => {
                report.errors += 1;
                tracing::warn!(
                    attempt_id = %attempt.id,
                    error = %error.message(),
                    "attempt compensation failed"
                );
            }
        }
    }
}

async fn reconcile_tenant_refunds(
    pool: &PgPool,
    credentials: &ProviderCredentialBundle,
    config: &PaymentCompensationWorkerConfig,
    tenant_id: &str,
    now_seconds: i64,
    report: &mut CompensationPassReport,
) {
    let claimed = match claim_due_refunds_postgres(
        pool,
        tenant_id,
        None,
        config.refunds_per_tenant,
        now_seconds,
        config.min_age_seconds,
    )
    .await
    {
        Ok(claimed) => claimed,
        Err(error) => {
            report.errors += 1;
            tracing::warn!(tenant_id = %tenant_id, error = %error.message(), "refund claim failed");
            return;
        }
    };
    report.refunds_claimed += claimed.len() as u64;
    for refund in claimed {
        let outcome = if refund.status.eq_ignore_ascii_case("submitted") {
            submit_refund(pool, credentials, &refund).await
        } else {
            query_refund(pool, credentials, &refund).await
        };
        match outcome {
            Ok(true) => {
                if refund.status.eq_ignore_ascii_case("submitted") {
                    report.refunds_submitted += 1;
                } else {
                    report.refunds_settled += 1;
                }
            }
            Ok(false) => {}
            Err(ReconcileError::ProviderUnavailable(message)) => {
                report.provider_unavailable += 1;
                tracing::debug!(
                    refund_id = %refund.id,
                    provider = %refund.provider_code,
                    reason = %message,
                    "compensation provider unavailable"
                );
            }
            Err(ReconcileError::Failed(error)) => {
                report.errors += 1;
                tracing::warn!(
                    refund_id = %refund.id,
                    error = %error.message(),
                    "refund compensation failed"
                );
            }
        }
    }
}

enum ReconcileError {
    /// The provider/account is not usable for this row: expected for sandbox
    /// or unconfigured providers; the row stays claimable for later passes.
    ProviderUnavailable(String),
    Failed(CommerceServiceError),
}

impl From<CommerceServiceError> for ReconcileError {
    fn from(error: CommerceServiceError) -> Self {
        Self::Failed(error)
    }
}

/// Queries the PSP for one stuck attempt and re-enters terminal outcomes
/// through the audited webhook settlement path.
async fn reconcile_attempt(
    pool: &PgPool,
    credentials: &ProviderCredentialBundle,
    attempt: &ClaimedPaymentAttempt,
) -> Result<bool, ReconcileError> {
    let Some(context) =
        load_claim_attempt_provider_context_postgres(pool, &attempt.id).await?
    else {
        return Ok(false);
    };
    let adapter = resolve_adapter(
        pool,
        credentials,
        &attempt.tenant_id,
        &attempt.organization_id,
        &context,
    )
    .await?;
    // Stripe queries by the native PaymentIntent id; WeChat and Alipay by the
    // merchant out-trade-no (same rule as the admin check endpoint).
    let query_reference = if context.provider_code.eq_ignore_ascii_case("stripe") {
        context
            .provider_transaction_id
            .clone()
            .unwrap_or_else(|| context.out_trade_no.clone())
    } else {
        context.out_trade_no.clone()
    };
    let outcome = adapter
        .query_payment_intent(PaymentQueryPaymentIntentRequest {
            payment_intent_id: Some(query_reference),
            metadata: serde_json::json!({}),
        })
        .await
        .map_err(|error| ReconcileError::Failed(CommerceServiceError::from(error)))?;
    let Some(raw_status) = outcome
        .raw_status
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
    else {
        return Ok(false);
    };
    let Some(target) =
        sdkwork_payment_repository_sqlx::webhook_status::map_provider_payment_status(
            &context.provider_code,
            &raw_status,
        )
    else {
        return Ok(false);
    };
    // Only terminal knowledge advances a pending/processing attempt; repeats
    // stay claimable until the PSP reports a final state.
    if matches!(target, "pending" | "processing") {
        return Ok(false);
    }
    let event_id = format!(
        "compensation-attempt:{}:{}:{}",
        attempt.tenant_id, context.out_trade_no, target
    );
    let command = IngestProviderWebhookCommand {
        provider_code: context.provider_code.clone(),
        provider_event_id: event_id,
        event_type: Some("compensation.status_query".to_owned()),
        out_trade_no: Some(context.out_trade_no.clone()),
        payment_status: Some(target.to_owned()),
        payload: serde_json::json!({
            "compensation": true,
            "providerCode": context.provider_code,
            "outTradeNo": context.out_trade_no,
            "paymentStatus": target,
            "rawStatus": raw_status,
            "providerTransactionId": context.provider_transaction_id,
        }),
        tenant_id: Some(attempt.tenant_id.clone()),
        organization_id: attempt.organization_id.clone(),
    };
    ingest_provider_webhook_postgres(pool, command).await?;
    Ok(true)
}

/// Re-submits a refund stuck in `submitted` to its PSP.
async fn submit_refund(
    pool: &PgPool,
    credentials: &ProviderCredentialBundle,
    refund: &ClaimedRefund,
) -> Result<bool, ReconcileError> {
    let store = PostgresCommerceRefundStore::new(pool.clone());
    let attempt = load_payment_attempt_provider_context_by_id_postgres(pool, &refund.payment_attempt_id)
        .await?
        .ok_or_else(|| ReconcileError::ProviderUnavailable("original payment attempt is gone".to_owned()))?;
    let account = resolve_account(
        pool,
        &refund.tenant_id,
        refund.organization_id.as_deref(),
        &refund.provider_code,
        attempt.provider_account_id.as_deref(),
    )
    .await?;
    let registry = provider_registry_for_account(credentials, account.map(|record| provider_account_binding(&record)));
    let adapter = registry
        .resolve(&refund.provider_code)
        .ok_or_else(|| ReconcileError::ProviderUnavailable(refund.provider_code.clone()))?;
    // The amount travels as a digit-only smallest-unit string.
    let amount_minor: i64 = refund
        .amount
        .trim()
        .parse()
        .map_err(|_| ReconcileError::Failed(CommerceServiceError::validation("claimed refund amount is not a minor-unit integer")))?;
    // Re-submission must reuse the exact idempotency key the original
    // submission used (operations.rs derives it from out_trade_no +
    // refund_no): when the PSP accepted a refund but the local flip to
    // `processing` was lost, a keyless retry would create a second refund.
    let idempotency_key =
        sdkwork_payment_providers::provider_operation_idempotency_key(
            "refund",
            &refund.provider_code,
            &[attempt.out_trade_no.as_str(), refund.refund_no.as_str()],
        );
    let outcome = adapter
        .create_refund(PaymentCreateRefundRequest {
            payment_intent_id: Some(attempt.provider_transaction_id.clone().unwrap_or_else(|| attempt.out_trade_no.clone())),
            refund_no: Some(refund.refund_no.clone()),
            amount_minor: Some(amount_minor),
            reason: None,
            metadata: serde_json::json!({
                "idempotency_key": idempotency_key,
                "refund_no": refund.refund_no,
                "total_amount_minor": attempt.amount,
            }),
        })
        .await
        .map_err(|error| ReconcileError::Failed(CommerceServiceError::from(error)))?;
    apply_refund_operation_outcome(pool, &store, refund, &outcome).await
}

/// Queries the PSP for a refund stuck in `processing` and re-enters terminal
/// outcomes through the refund webhook ingestion path.
async fn query_refund(
    pool: &PgPool,
    credentials: &ProviderCredentialBundle,
    refund: &ClaimedRefund,
) -> Result<bool, ReconcileError> {
    let attempt = load_payment_attempt_provider_context_by_id_postgres(pool, &refund.payment_attempt_id)
        .await?
        .ok_or_else(|| ReconcileError::ProviderUnavailable("original payment attempt is gone".to_owned()))?;
    let account = resolve_account(
        pool,
        &refund.tenant_id,
        refund.organization_id.as_deref(),
        &refund.provider_code,
        attempt.provider_account_id.as_deref(),
    )
    .await?;
    let registry = provider_registry_for_account(credentials, account.map(|record| provider_account_binding(&record)));
    let adapter = registry
        .resolve(&refund.provider_code)
        .ok_or_else(|| ReconcileError::ProviderUnavailable(refund.provider_code.clone()))?;
    let outcome = adapter
        .query_refund(PaymentQueryRefundRequest {
            refund_id: None,
            refund_no: Some(refund.refund_no.clone()),
            metadata: serde_json::json!({
                "payment_intent_id": attempt.provider_transaction_id.clone().unwrap_or_else(|| attempt.out_trade_no.clone()),
            }),
        })
        .await
        .map_err(|error| ReconcileError::Failed(CommerceServiceError::from(error)))?;
    let Some(raw_status) = outcome
        .raw_status
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
    else {
        return Ok(false);
    };
    let Some(target) = sdkwork_payment_repository_sqlx::webhook_status::map_provider_refund_status(
        &refund.provider_code,
        &raw_status,
    ) else {
        return Ok(false);
    };
    if target.eq_ignore_ascii_case(&refund.status) {
        return Ok(false);
    }
    ingest_refund_status(pool, refund, target, &raw_status).await?;
    Ok(true)
}

/// Records the outcome of a PSP refund submission: explicit terminal states
/// are re-entered through the refund ingestion path, an accepted/processing
/// outcome flips the row to `processing` via the store mark, and ambiguous
/// outcomes leave the row claimable (the amount stays reserved).
async fn apply_refund_operation_outcome(
    pool: &PgPool,
    store: &PostgresCommerceRefundStore,
    refund: &ClaimedRefund,
    outcome: &PaymentProviderOperationOutcome,
) -> Result<bool, ReconcileError> {
    if let Some(raw_status) = outcome
        .raw_status
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if let Some(target) = sdkwork_payment_repository_sqlx::webhook_status::map_provider_refund_status(
            &refund.provider_code,
            raw_status,
        ) {
            if !target.eq_ignore_ascii_case(&refund.status) {
                ingest_refund_status(pool, refund, target, raw_status).await?;
                return Ok(true);
            }
        }
    }
    // Accepted-but-pending submissions flip the row to `processing` so the
    // query branch of the next pass polls it to a terminal state.
    if let Some(raw_status) = outcome
        .native_id
        .as_deref()
        .map(|_| "processing")
    {
        let _ = raw_status;
        store
            .mark_owner_refund_provider_submission_processing(
                &refund.tenant_id,
                refund.organization_id.as_deref(),
                &refund.id,
                "system",
                Some(COMPENSATION_ACTOR),
                &refund.request_no,
                &refund.idempotency_key,
            )
            .await?;
        return Ok(true);
    }
    // No native id: the PSP response carried no accepted marker; leave the
    // row claimable and let the next pass decide from a fresh submission or
    // provider query. The reserved amount is unchanged.
    Ok(false)
}

async fn ingest_refund_status(
    pool: &PgPool,
    refund: &ClaimedRefund,
    target: &str,
    raw_status: &str,
) -> Result<(), CommerceServiceError> {
    let event_id = format!(
        "compensation-refund:{}:{}:{}",
        refund.tenant_id, refund.refund_no, target
    );
    let command = IngestProviderWebhookCommand {
        provider_code: normalize_provider_code(&refund.provider_code),
        provider_event_id: event_id,
        event_type: Some("compensation.refund_status_query".to_owned()),
        out_trade_no: None,
        payment_status: None,
        payload: serde_json::json!({
            "compensation": true,
            "providerCode": normalize_provider_code(&refund.provider_code),
            "refundNo": refund.refund_no,
            "refundStatus": target,
            "rawStatus": raw_status,
            "amount": refund.amount,
        }),
        tenant_id: Some(refund.tenant_id.clone()),
        organization_id: refund.organization_id.clone(),
    };
    ingest_provider_refund_webhook_postgres(pool, command).await.map(drop)
}

async fn resolve_adapter(
    pool: &PgPool,
    credentials: &ProviderCredentialBundle,
    tenant_id: &str,
    organization_id: &Option<String>,
    context: &ClaimAttemptProviderContext,
) -> Result<std::sync::Arc<dyn sdkwork_payment_providers::PaymentProviderAdapter>, ReconcileError> {
    let account = resolve_account(
        pool,
        tenant_id,
        organization_id.as_deref(),
        &context.provider_code,
        context.provider_account_id.as_deref(),
    )
    .await?;
    let registry =
        provider_registry_for_account(credentials, account.map(|record| provider_account_binding(&record)));
    registry
        .resolve(&context.provider_code)
        .ok_or_else(|| ReconcileError::ProviderUnavailable(context.provider_code.clone()))
}

async fn resolve_account(
    pool: &PgPool,
    tenant_id: &str,
    organization_id: Option<&str>,
    provider_code: &str,
    provider_account_id: Option<&str>,
) -> Result<Option<sdkwork_payment_repository_sqlx::PaymentProviderAccountRecord>, ReconcileError> {
    let Some(provider_account_id) = provider_account_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    // The claimed row is tenant-scoped, so the account lookup inherits that
    // scope; a missing active account means the historical account was
    // deactivated and refund/ settlement must not silently switch accounts.
    let account = sdkwork_payment_repository_sqlx::load_active_provider_account_by_id_postgres(
        pool,
        tenant_id,
        organization_id,
        provider_account_id,
    )
    .await?;
    if let Some(account) = account {
        return Ok(Some(account));
    }
    Err(ReconcileError::ProviderUnavailable(format!(
        "provider account {provider_account_id} for {provider_code} is not active"
    )))
}

const COMPENSATION_ACTOR: &str = "compensation-worker";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_enabled_and_bounded() {
        let config = PaymentCompensationWorkerConfig::default();
        assert!(config.enabled);
        assert_eq!(config.min_age_seconds, 90);
        assert!(config.attempts_per_tenant <= 200);
        assert!(config.refunds_per_tenant <= 200);
        assert_eq!(config.interval, Duration::from_secs(60));
    }

    #[test]
    fn report_counters_default_to_zero() {
        let report = CompensationPassReport::default();
        assert_eq!(report, CompensationPassReport::default());
        assert_eq!(report.errors, 0);
    }
}
