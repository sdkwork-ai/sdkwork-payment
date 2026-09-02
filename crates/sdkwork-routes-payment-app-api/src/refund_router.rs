use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Extension, Path, Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use sdkwork_contract_service::CommerceServiceError;
use sdkwork_iam_context_service::IamAppContext;
use sdkwork_payment_providers::{
    create_provider_refund, provider_registry_for_account, query_provider_refund,
    PaymentProviderRegistry, ProviderAccountBinding, ProviderCredentialBundle,
    ProviderRefundSubmissionState,
};
use sdkwork_payment_repository_sqlx::{
    ensure_provider_account_matches, load_active_provider_account_postgres,
    load_payment_attempt_provider_context_by_id_postgres,
    load_provider_account_for_existing_payment_postgres, PaymentProviderAccountRecord,
    PostgresCommerceRefundStore,
};
use sdkwork_payment_service::{
    CreateOwnerRefundCommand, RefundDetailQuery, RefundListPage, RefundListQuery, RefundView,
};
use sdkwork_utils_rust::OffsetListPageParams;
use sdkwork_web_core::WebRequestContext;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::api_response::{
    map_service_error, not_found, success_created_item, success_item, success_list, unauthorized,
    validation,
};
use crate::command_headers::validate_app_write_payload;
use crate::subject::app_runtime_subject_from_extension;

pub type CommerceRefundFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, CommerceServiceError>> + Send + 'a>>;

pub trait CommerceRefundStore: Send + Sync {
    fn create_owner_refund<'a>(
        &'a self,
        command: CreateOwnerRefundCommand,
    ) -> CommerceRefundFuture<'a, RefundView>;

    fn list_owner_refunds<'a>(
        &'a self,
        query: RefundListQuery,
    ) -> CommerceRefundFuture<'a, RefundListPage>;

    fn retrieve_owner_refund<'a>(
        &'a self,
        query: RefundDetailQuery,
    ) -> CommerceRefundFuture<'a, Option<RefundView>>;
}

#[derive(Clone)]
struct AppRefundState {
    store: Arc<dyn CommerceRefundStore>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateRefundBody {
    order_id: String,
    #[serde(rename = "paymentAttemptId", alias = "payment_attempt_id")]
    payment_attempt_id: Option<String>,
    amount: Option<String>,
    #[serde(rename = "reasonCode", alias = "reason_code")]
    reason_code: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RefundListParams {
    status: Option<String>,
    page: Option<i64>,
    page_size: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RefundResponse {
    refund_id: String,
    refund_no: String,
    order_id: String,
    payment_attempt_id: String,
    amount: String,
    currency_code: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason_code: Option<String>,
}

struct ProviderEnrichedPostgresRefundStore {
    inner: Arc<PostgresCommerceRefundStore>,
    pool: PgPool,
    credentials: ProviderCredentialBundle,
}

fn provider_account_binding(record: &PaymentProviderAccountRecord) -> ProviderAccountBinding {
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

impl CommerceRefundStore for PostgresCommerceRefundStore {
    fn create_owner_refund<'a>(
        &'a self,
        command: CreateOwnerRefundCommand,
    ) -> CommerceRefundFuture<'a, RefundView> {
        Box::pin(async move { self.create_owner_refund(command).await })
    }

    fn list_owner_refunds<'a>(
        &'a self,
        query: RefundListQuery,
    ) -> CommerceRefundFuture<'a, RefundListPage> {
        Box::pin(async move { self.list_owner_refunds(query).await })
    }

    fn retrieve_owner_refund<'a>(
        &'a self,
        query: RefundDetailQuery,
    ) -> CommerceRefundFuture<'a, Option<RefundView>> {
        Box::pin(async move { self.retrieve_owner_refund(query).await })
    }
}

async fn submit_provider_refund_postgres(
    credentials: &ProviderCredentialBundle,
    pool: &PgPool,
    tenant_id: &str,
    organization_id: Option<&str>,
    refund: &RefundView,
    reason_code: Option<String>,
) -> Result<ProviderRefundSubmissionState, CommerceServiceError> {
    let Some(ctx) =
        load_payment_attempt_provider_context_by_id_postgres(pool, &refund.payment_attempt_id)
            .await?
    else {
        return Err(CommerceServiceError::not_found(
            "payment attempt provider context was not found for refund submission",
        ));
    };
    let account = match ctx.provider_account_id.as_deref() {
        Some(provider_account_id) => load_provider_account_for_existing_payment_postgres(
            pool,
            tenant_id,
            organization_id,
            provider_account_id,
        )
        .await?
        .map(Some)
        .ok_or_else(|| {
            CommerceServiceError::conflict(
                "original payment provider account is unavailable for refund",
            )
        })?,
        None if ctx.channel_id.is_some() => None,
        None => {
            load_active_provider_account_postgres(
                pool,
                tenant_id,
                organization_id,
                &ctx.provider_code,
            )
            .await?
        }
    };
    ensure_provider_account_matches(account.as_ref(), &ctx.provider_code)?;
    let registry = provider_registry_for_account(
        credentials,
        account.map(|record| provider_account_binding(&record)),
    );
    let total_amount = sdkwork_contract_service::CommerceMoney::new(&ctx.amount)
        .map_err(CommerceServiceError::storage)?;
    if refund.status == "processing" {
        if let Some(state) = query_provider_refund(
            &registry,
            &ctx.provider_code,
            &ctx.out_trade_no,
            ctx.provider_transaction_id.as_deref(),
            &refund.refund_no,
        )
        .await?
        {
            return Ok(state);
        }
    }
    create_provider_refund(
        &registry,
        &ctx.provider_code,
        &ctx.out_trade_no,
        ctx.provider_transaction_id.as_deref(),
        &refund.refund_no,
        &refund.amount,
        &total_amount,
        reason_code,
    )
    .await
}

const REFUND_PROVIDER_SUBMIT_ATTEMPTS: u32 = 3;

fn refund_submission_retryable(error: &CommerceServiceError) -> bool {
    !matches!(
        error.code(),
        "not-found" | "validation" | "validation-failed" | "forbidden" | "conflict"
    )
}

fn refund_requires_provider_submission(status: &str) -> bool {
    matches!(status, "submitted" | "processing" | "failed")
}

fn refund_requires_processing_claim(status: &str) -> bool {
    matches!(status, "submitted" | "failed")
}

async fn submit_provider_refund_postgres_with_retry(
    credentials: &ProviderCredentialBundle,
    pool: &PgPool,
    tenant_id: &str,
    organization_id: Option<&str>,
    refund: &RefundView,
    reason_code: Option<String>,
) -> Result<ProviderRefundSubmissionState, CommerceServiceError> {
    let mut last_error = None;
    for attempt in 0..REFUND_PROVIDER_SUBMIT_ATTEMPTS {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(150 * (1 << (attempt - 1)))).await;
        }
        match submit_provider_refund_postgres(
            credentials,
            pool,
            tenant_id,
            organization_id,
            refund,
            reason_code.clone(),
        )
        .await
        {
            Ok(state) => return Ok(state),
            Err(error) if refund_submission_retryable(&error) => last_error = Some(error),
            Err(error) => return Err(error),
        }
    }
    Err(last_error.expect("refund submission attempted at least once"))
}

impl CommerceRefundStore for ProviderEnrichedPostgresRefundStore {
    fn create_owner_refund<'a>(
        &'a self,
        command: CreateOwnerRefundCommand,
    ) -> CommerceRefundFuture<'a, RefundView> {
        let inner = self.inner.clone();
        let pool = self.pool.clone();
        let credentials = self.credentials.clone();
        let tenant_id = command.tenant_id.clone();
        let organization_id = command.organization_id.clone();
        let reason_code = command.reason_code.clone();
        let requested_by = command.requested_by.clone();
        let requested_by_type = command.requested_by_type.clone();
        let request_no = command.request_no.clone();
        let idempotency_key = command.idempotency_key.clone();
        Box::pin(async move {
            let mut refund = inner.create_owner_refund(command).await?;
            if refund_requires_provider_submission(&refund.status) {
                if refund_requires_processing_claim(&refund.status) {
                    refund = inner
                        .mark_owner_refund_provider_submission_processing(
                            &tenant_id,
                            organization_id.as_deref(),
                            &refund.refund_id,
                            &requested_by_type,
                            Some(&requested_by),
                            &request_no,
                            &idempotency_key,
                        )
                        .await?;
                }
                match submit_provider_refund_postgres_with_retry(
                    &credentials,
                    &pool,
                    &tenant_id,
                    organization_id.as_deref(),
                    &refund,
                    reason_code,
                )
                .await
                {
                    Ok(ProviderRefundSubmissionState::Processing) => {}
                    Ok(ProviderRefundSubmissionState::Succeeded) => {
                        refund = inner
                            .mark_owner_refund_provider_submission_succeeded(
                                &tenant_id,
                                organization_id.as_deref(),
                                &refund.refund_id,
                                &requested_by_type,
                                Some(&requested_by),
                                &request_no,
                                &idempotency_key,
                            )
                            .await?;
                    }
                    Ok(ProviderRefundSubmissionState::Failed) => {
                        refund = inner
                            .mark_owner_refund_provider_submission_failed(
                                &tenant_id,
                                organization_id.as_deref(),
                                &refund.refund_id,
                                &requested_by_type,
                                Some(&requested_by),
                                &request_no,
                                &idempotency_key,
                            )
                            .await?;
                    }
                    Err(error) => {
                        if !refund_submission_retryable(&error) {
                            let _ = inner
                                .mark_owner_refund_provider_submission_failed(
                                    &tenant_id,
                                    organization_id.as_deref(),
                                    &refund.refund_id,
                                    &requested_by_type,
                                    Some(&requested_by),
                                    &request_no,
                                    &idempotency_key,
                                )
                                .await;
                        }
                        return Err(error);
                    }
                }
            }
            Ok(refund)
        })
    }

    fn list_owner_refunds<'a>(
        &'a self,
        query: RefundListQuery,
    ) -> CommerceRefundFuture<'a, RefundListPage> {
        let inner = self.inner.clone();
        Box::pin(async move { inner.list_owner_refunds(query).await })
    }

    fn retrieve_owner_refund<'a>(
        &'a self,
        query: RefundDetailQuery,
    ) -> CommerceRefundFuture<'a, Option<RefundView>> {
        let inner = self.inner.clone();
        Box::pin(async move { inner.retrieve_owner_refund(query).await })
    }
}

pub fn app_refund_router_with_postgres_pool(
    pool: PgPool,
    _registry: Arc<PaymentProviderRegistry>,
    credentials: ProviderCredentialBundle,
) -> Router {
    build_app_refund_router(Arc::new(ProviderEnrichedPostgresRefundStore {
        inner: Arc::new(PostgresCommerceRefundStore::new(pool.clone())),
        pool,
        credentials,
    }))
}

pub fn build_app_refund_router(store: Arc<dyn CommerceRefundStore>) -> Router {
    Router::new()
        .route("/app/v3/api/refunds", post(create_refund).get(list_refunds))
        .route("/app/v3/api/refunds/{refundId}", get(retrieve_refund))
        .with_state(AppRefundState { store })
}

async fn create_refund(
    State(state): State<AppRefundState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    headers: HeaderMap,
    Json(body): Json<CreateRefundBody>,
) -> Response {
    let ctx = request_ctx(&request_context);
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized(ctx, message),
    };
    let write_headers =
        match validate_app_write_payload(&headers, "refunds.create", &body, |idempotency_key| {
            format!("refund-{}-{}", subject.user_id, idempotency_key)
        }) {
            Ok(value) => value,
            Err(response) => return response,
        };
    let command = match CreateOwnerRefundCommand::new(
        &subject.tenant_id,
        subject.organization_id.as_deref(),
        &subject.user_id,
        &body.order_id,
        body.payment_attempt_id.as_deref(),
        body.amount.as_deref(),
        body.reason_code.as_deref(),
        &write_headers.request_no,
        &write_headers.idempotency_key,
    ) {
        Ok(command) => command,
        Err(error) => return validation(ctx, error.message()),
    };

    match state.store.create_owner_refund(command).await {
        Ok(refund) => success_created_item(ctx, map_refund(refund)),
        Err(error) => refund_system_response(ctx, "refund create failed", error),
    }
}

async fn list_refunds(
    State(state): State<AppRefundState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    Query(params): Query<RefundListParams>,
) -> Response {
    let ctx = request_ctx(&request_context);
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized(ctx, message),
    };
    let page_params = OffsetListPageParams::parse(params.page, params.page_size);
    let query = match RefundListQuery::new(
        &subject.tenant_id,
        subject.organization_id.as_deref(),
        &subject.user_id,
        params.status.as_deref(),
    ) {
        Ok(query) => query.with_paging(page_params.offset, page_params.page_size),
        Err(error) => return validation(ctx, error.message()),
    };

    match state.store.list_owner_refunds(query).await {
        Ok(page) => {
            let items: Vec<RefundResponse> = page.items.into_iter().map(map_refund).collect();
            success_list(ctx, items, page.total_items, page_params)
        }
        Err(error) => refund_system_response(ctx, "refund list is unavailable", error),
    }
}

async fn retrieve_refund(
    State(state): State<AppRefundState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    Path(refund_id): Path<String>,
) -> Response {
    let ctx = request_ctx(&request_context);
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized(ctx, message),
    };
    let query = match RefundDetailQuery::new(
        &subject.tenant_id,
        subject.organization_id.as_deref(),
        &subject.user_id,
        &refund_id,
    ) {
        Ok(query) => query,
        Err(error) => return validation(ctx, error.message()),
    };

    match state.store.retrieve_owner_refund(query).await {
        Ok(Some(refund)) => success_item(ctx, map_refund(refund)),
        Ok(None) => not_found(ctx, "refund was not found"),
        Err(error) => refund_system_response(ctx, "refund read model is unavailable", error),
    }
}

fn map_refund(value: RefundView) -> RefundResponse {
    RefundResponse {
        refund_id: value.refund_id,
        refund_no: value.refund_no,
        order_id: value.order_id,
        payment_attempt_id: value.payment_attempt_id,
        amount: value.amount.as_str().to_owned(),
        currency_code: value.currency_code,
        status: value.status,
        reason_code: value.reason_code,
    }
}

#[cfg(test)]
mod provider_submission_tests {
    use super::{
        refund_requires_processing_claim, refund_requires_provider_submission,
        refund_submission_retryable,
    };
    use sdkwork_contract_service::CommerceServiceError;

    #[test]
    fn ambiguous_refund_submission_can_be_replayed_without_reclaiming() {
        assert!(refund_requires_provider_submission("processing"));
        assert!(!refund_requires_processing_claim("processing"));
        assert!(refund_requires_provider_submission("failed"));
        assert!(refund_requires_processing_claim("failed"));
        assert!(!refund_requires_provider_submission("succeeded"));
    }

    #[test]
    fn deterministic_validation_failure_is_not_ambiguous() {
        assert!(!refund_submission_retryable(
            &CommerceServiceError::validation("invalid provider refund input")
        ));
        assert!(refund_submission_retryable(&CommerceServiceError::storage(
            "provider response was not observed"
        )));
    }
}

fn request_ctx(ext: &Option<Extension<WebRequestContext>>) -> Option<&WebRequestContext> {
    ext.as_ref().map(|Extension(value)| value)
}

fn refund_system_response(
    context: Option<&WebRequestContext>,
    _label: &str,
    error: CommerceServiceError,
) -> Response {
    map_service_error(context, error)
}
