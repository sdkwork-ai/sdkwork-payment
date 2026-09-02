use std::time::Duration;

use axum::extract::{Extension, Path, Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use sdkwork_contract_service::{CommerceMoney, CommerceServiceError};
use sdkwork_iam_context_service::IamAppContext;
use sdkwork_payment_providers::{
    create_provider_refund, provider_registry_for_account, query_provider_refund,
    ProviderAccountBinding, ProviderCredentialBundle, ProviderRefundSubmissionState,
};
use sdkwork_payment_repository_sqlx::{
    ensure_provider_account_matches, load_payment_attempt_provider_context_by_id_postgres,
    load_provider_account_for_existing_payment_postgres, PaymentProviderAccountRecord,
    PostgresCommerceRefundStore,
};
use sdkwork_payment_service::{CreateOwnerRefundCommand, RefundView};
use sdkwork_utils_rust::OffsetListPageParams;
use sdkwork_web_core::WebRequestContext;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{postgres::PgRow, PgPool, Row};

use crate::api_response::{
    map_service_error, not_found, success_command_accepted, success_created_item, success_item,
    success_list, unauthorized, validation,
};
use crate::command_headers::{
    validate_write_payload, AppWriteCommandHeaders, WriteCommandHeaderError,
};
use crate::subject::{backend_runtime_subject_from_extension, AppRuntimeSubject};

const REFUND_PROVIDER_SUBMIT_ATTEMPTS: u32 = 3;
const REFUND_REASON_CODES: &[&str] = &[
    "customer_request",
    "duplicate",
    "fraud",
    "service_failure",
    "other",
];

#[derive(Clone)]
enum BackendRefundPool {
    Postgres(PgPool),
}

#[derive(Clone)]
struct BackendRefundState {
    pool: BackendRefundPool,
    credentials: ProviderCredentialBundle,
}

#[derive(Debug, Deserialize)]
struct BackendRefundListParams {
    status: Option<String>,
    order_id: Option<String>,
    payment_intent_id: Option<String>,
    q: Option<String>,
    page: Option<i64>,
    page_size: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateBackendRefundBody {
    payment_intent_id: String,
    amount: Option<String>,
    reason_code: String,
    confirm_payment_intent_no: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RetryBackendRefundBody {
    confirm_refund_no: String,
    expected_status: String,
}

#[derive(Clone, Debug)]
struct RefundPaymentContext {
    payment_intent_no: String,
    owner_user_id: String,
    order_id: String,
    payment_attempt_id: String,
    currency_code: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BackendRefundView {
    id: String,
    refund_no: String,
    order_id: String,
    payment_intent_id: String,
    payment_attempt_id: String,
    provider_code: String,
    provider_account_id: Option<String>,
    amount: String,
    currency_code: String,
    status: String,
    reason_code: Option<String>,
    requested_by_type: String,
    requested_by: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug)]
struct BackendRefundListPage {
    items: Vec<BackendRefundView>,
    total_items: i64,
}

pub fn backend_payment_refund_router_with_postgres_pool(pool: PgPool) -> Router {
    build_router(BackendRefundState {
        pool: BackendRefundPool::Postgres(pool),
        credentials: ProviderCredentialBundle::from_env(),
    })
}

fn build_router(state: BackendRefundState) -> Router {
    Router::new()
        .route(
            "/backend/v3/api/payments/refunds",
            get(list_refunds).post(create_refund),
        )
        .route(
            "/backend/v3/api/payments/refunds/{refundId}",
            get(retrieve_refund),
        )
        .route(
            "/backend/v3/api/payments/refunds/{refundId}/retry",
            post(retry_refund),
        )
        .with_state(state)
}

async fn list_refunds(
    State(state): State<BackendRefundState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    Query(params): Query<BackendRefundListParams>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match backend_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized(ctx, message),
    };
    let page_params = OffsetListPageParams::parse(params.page, params.page_size);
    match state.pool.list_refunds(&subject, params, page_params).await {
        Ok(page) => success_list(ctx, page.items, page.total_items, page_params),
        Err(error) => map_service_error(ctx, error),
    }
}

async fn retrieve_refund(
    State(state): State<BackendRefundState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    Path(refund_id): Path<String>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match backend_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized(ctx, message),
    };
    match state.pool.retrieve_refund(&subject, &refund_id).await {
        Ok(Some(refund)) => success_item(ctx, refund),
        Ok(None) => not_found(ctx, "refund was not found"),
        Err(error) => map_service_error(ctx, error),
    }
}

async fn create_refund(
    State(state): State<BackendRefundState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    headers: HeaderMap,
    Json(body): Json<CreateBackendRefundBody>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match backend_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized(ctx, message),
    };
    let write_headers = match validate_backend_write_payload(
        ctx,
        &headers,
        "refunds.create",
        &body,
        "payment-refund",
    ) {
        Ok(headers) => headers,
        Err(response) => return response,
    };
    let reason_code = match normalize_reason_code(&body.reason_code) {
        Ok(value) => value,
        Err(error) => return validation(ctx, error.message()),
    };
    let payment = match state
        .pool
        .load_refund_payment_context(&subject, &body.payment_intent_id)
        .await
    {
        Ok(Some(payment)) => payment,
        Ok(None) => return not_found(ctx, "refundable payment intent was not found"),
        Err(error) => return map_service_error(ctx, error),
    };
    if body.confirm_payment_intent_no.trim() != payment.payment_intent_no {
        return validation(ctx, "payment intent confirmation does not match");
    }
    let command = match CreateOwnerRefundCommand::new_with_currency(
        &subject.tenant_id,
        subject.organization_id.as_deref(),
        &payment.owner_user_id,
        &payment.order_id,
        Some(&payment.payment_attempt_id),
        body.amount.as_deref(),
        Some(&payment.currency_code),
        Some(&reason_code),
        &write_headers.request_no,
        &write_headers.idempotency_key,
    )
    .and_then(|command| command.requested_by_operator(&subject.user_id))
    {
        Ok(command) => command,
        Err(error) => return map_service_error(ctx, error),
    };
    let refund = match state.pool.create_refund(command).await {
        Ok(refund) => refund,
        Err(error) => return map_service_error(ctx, error),
    };
    if refund_requires_provider_submission(&refund.status) {
        let refund = if refund_requires_processing_claim(&refund.status) {
            match state
                .pool
                .mark_processing(
                    &subject,
                    &refund.refund_id,
                    &write_headers,
                    "operator",
                    Some(&subject.user_id),
                )
                .await
            {
                Ok(refund) => refund,
                Err(error) => return map_service_error(ctx, error),
            }
        } else {
            refund.clone()
        };
        match submit_provider_refund_with_retry(&state, &subject, &refund, Some(reason_code)).await
        {
            Ok(ProviderRefundSubmissionState::Processing) => {}
            Ok(ProviderRefundSubmissionState::Succeeded) => {
                if let Err(error) = state
                    .pool
                    .mark_succeeded(
                        &subject,
                        &refund.refund_id,
                        &write_headers,
                        "operator",
                        Some(&subject.user_id),
                    )
                    .await
                {
                    return map_service_error(ctx, error);
                }
            }
            Ok(ProviderRefundSubmissionState::Failed) => {
                if let Err(error) = state
                    .pool
                    .mark_failed(
                        &subject,
                        &refund.refund_id,
                        &write_headers,
                        "operator",
                        Some(&subject.user_id),
                    )
                    .await
                {
                    return map_service_error(ctx, error);
                }
            }
            Err(error) => {
                if !refund_submission_retryable(&error) {
                    let _ = state
                        .pool
                        .mark_failed(
                            &subject,
                            &refund.refund_id,
                            &write_headers,
                            "operator",
                            Some(&subject.user_id),
                        )
                        .await;
                }
                return map_service_error(ctx, error);
            }
        }
    }
    match state
        .pool
        .retrieve_refund(&subject, &refund.refund_id)
        .await
    {
        Ok(Some(refund)) => success_created_item(ctx, refund),
        Ok(None) => not_found(ctx, "created refund was not found"),
        Err(error) => map_service_error(ctx, error),
    }
}

async fn retry_refund(
    State(state): State<BackendRefundState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    headers: HeaderMap,
    Path(refund_id): Path<String>,
    Json(body): Json<RetryBackendRefundBody>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match backend_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized(ctx, message),
    };
    let write_headers = match validate_backend_write_payload(
        ctx,
        &headers,
        "refunds.retry",
        &body,
        "payment-refund-retry",
    ) {
        Ok(headers) => headers,
        Err(response) => return response,
    };
    let Some(refund) = (match state.pool.retrieve_refund(&subject, &refund_id).await {
        Ok(refund) => refund,
        Err(error) => return map_service_error(ctx, error),
    }) else {
        return not_found(ctx, "refund was not found");
    };
    let expected_status = body.expected_status.trim();
    if !matches!(expected_status, "failed" | "processing") || refund.status != expected_status {
        return validation(
            ctx,
            "only a failed or processing refund with matching expectedStatus can be retried",
        );
    }
    if body.confirm_refund_no.trim() != refund.refund_no {
        return validation(ctx, "refund confirmation does not match");
    }
    let submission = if refund.status == "failed" {
        match state
            .pool
            .mark_processing(
                &subject,
                &refund.id,
                &write_headers,
                "operator",
                Some(&subject.user_id),
            )
            .await
        {
            Ok(refund) => refund,
            Err(error) => return map_service_error(ctx, error),
        }
    } else {
        RefundView {
            refund_id: refund.id.clone(),
            refund_no: refund.refund_no.clone(),
            order_id: refund.order_id.clone(),
            payment_attempt_id: refund.payment_attempt_id.clone(),
            amount: match CommerceMoney::new(&refund.amount) {
                Ok(amount) => amount,
                Err(error) => return validation(ctx, error),
            },
            currency_code: refund.currency_code.clone(),
            status: refund.status.clone(),
            reason_code: refund.reason_code.clone(),
        }
    };
    match submit_provider_refund_with_retry(
        &state,
        &subject,
        &submission,
        submission.reason_code.clone(),
    )
    .await
    {
        Ok(ProviderRefundSubmissionState::Processing) => {}
        Ok(ProviderRefundSubmissionState::Succeeded) => {
            if let Err(error) = state
                .pool
                .mark_succeeded(
                    &subject,
                    &refund.id,
                    &write_headers,
                    "operator",
                    Some(&subject.user_id),
                )
                .await
            {
                return map_service_error(ctx, error);
            }
        }
        Ok(ProviderRefundSubmissionState::Failed) => {
            if let Err(error) = state
                .pool
                .mark_failed(
                    &subject,
                    &refund.id,
                    &write_headers,
                    "operator",
                    Some(&subject.user_id),
                )
                .await
            {
                return map_service_error(ctx, error);
            }
        }
        Err(error) => {
            if !refund_submission_retryable(&error) {
                let _ = state
                    .pool
                    .mark_failed(
                        &subject,
                        &refund.id,
                        &write_headers,
                        "operator",
                        Some(&subject.user_id),
                    )
                    .await;
            }
            return map_service_error(ctx, error);
        }
    }
    success_command_accepted(ctx, Some(refund.id))
}

impl BackendRefundPool {
    async fn list_refunds(
        &self,
        subject: &AppRuntimeSubject,
        params: BackendRefundListParams,
        page: OffsetListPageParams,
    ) -> Result<BackendRefundListPage, CommerceServiceError> {
        let q = params
            .q
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| format!("%{}%", value.to_ascii_lowercase()));
        match self {
            Self::Postgres(pool) => {
                let rows = sqlx::query(
                    r#"
                    SELECT r.id, r.refund_no, r.order_id, r.payment_attempt_id,
                           a.payment_intent_id, a.provider_code, a.provider_account_id,
                           CAST(r.amount AS TEXT) AS amount, r.currency_code, r.status,
                           r.refund_reason_code, r.requested_by_type, r.requested_by,
                           CAST(r.created_at AS TEXT) AS created_at,
                           CAST(r.updated_at AS TEXT) AS updated_at,
                           COUNT(*) OVER() AS total_count
                    FROM commerce_refund r
                    INNER JOIN commerce_payment_attempt a ON a.id = r.payment_attempt_id
                    WHERE r.tenant_id = CAST($1 AS TEXT)
                      AND ((r.organization_id = CAST($2 AS TEXT)) OR (r.organization_id IS NULL AND $3 IS NULL) OR (r.organization_id = '0' AND $3 IS NULL))
                      AND ($4::text IS NULL OR LOWER(r.status) = LOWER($4::text))
                      AND ($5::text IS NULL OR r.order_id = CAST($5 AS TEXT))
                      AND ($6::text IS NULL OR a.payment_intent_id = CAST($6 AS TEXT))
                      AND ($7::text IS NULL OR LOWER(r.refund_no) LIKE $7 OR LOWER(r.order_id) LIKE $7)
                      AND r.deleted_at IS NULL
                    ORDER BY r.created_at DESC, r.id DESC
                    LIMIT $8 OFFSET $9
                    "#,
                )
                .bind(&subject.tenant_id)
                .bind(subject.organization_id.as_deref())
                .bind(subject.organization_id.as_deref())
                .bind(params.status.as_deref())
                .bind(params.order_id.as_deref())
                .bind(params.payment_intent_id.as_deref())
                .bind(q.as_deref())
                .bind(page.page_size)
                .bind(page.offset)
                .fetch_all(pool)
                .await
                .map_err(|error| CommerceServiceError::storage(format!("failed to list refunds: {error}")))?;
                Ok(BackendRefundListPage {
                    total_items: postgres_total(&rows),
                    items: rows.iter().map(map_postgres_refund).collect(),
                })
            }
        }
    }

    async fn retrieve_refund(
        &self,
        subject: &AppRuntimeSubject,
        refund_id: &str,
    ) -> Result<Option<BackendRefundView>, CommerceServiceError> {
        match self {
            Self::Postgres(pool) => sqlx::query(
                r#"
                SELECT r.id, r.refund_no, r.order_id, r.payment_attempt_id,
                       a.payment_intent_id, a.provider_code, a.provider_account_id,
                       CAST(r.amount AS TEXT) AS amount, r.currency_code, r.status,
                       r.refund_reason_code, r.requested_by_type, r.requested_by,
                       CAST(r.created_at AS TEXT) AS created_at,
                       CAST(r.updated_at AS TEXT) AS updated_at
                FROM commerce_refund r
                INNER JOIN commerce_payment_attempt a ON a.id = r.payment_attempt_id
                WHERE r.id = CAST($1 AS TEXT) AND r.tenant_id = CAST($2 AS TEXT)
                  AND ((r.organization_id = CAST($3 AS TEXT)) OR (r.organization_id IS NULL AND $4 IS NULL) OR (r.organization_id = '0' AND $4 IS NULL))
                  AND r.deleted_at IS NULL LIMIT 1
                "#,
            )
            .bind(refund_id)
            .bind(&subject.tenant_id)
            .bind(subject.organization_id.as_deref())
            .bind(subject.organization_id.as_deref())
            .fetch_optional(pool)
            .await
            .map(|row| row.as_ref().map(map_postgres_refund))
            .map_err(|error| CommerceServiceError::storage(format!("failed to retrieve refund: {error}"))),
        }
    }

    async fn load_refund_payment_context(
        &self,
        subject: &AppRuntimeSubject,
        payment_intent_id: &str,
    ) -> Result<Option<RefundPaymentContext>, CommerceServiceError> {
        match self {
            Self::Postgres(pool) => sqlx::query(
                r#"
                SELECT i.id, i.payment_intent_no, i.owner_user_id, i.order_id, i.currency_code,
                       a.id AS payment_attempt_id
                FROM commerce_payment_intent i
                INNER JOIN commerce_payment_attempt a ON a.payment_intent_id = i.id
                WHERE i.id = CAST($1 AS TEXT) AND i.tenant_id = CAST($2 AS TEXT)
                  AND ((i.organization_id = CAST($3 AS TEXT)) OR (i.organization_id IS NULL AND $4 IS NULL) OR (i.organization_id = '0' AND $4 IS NULL))
                  AND LOWER(i.status) IN ('succeeded', 'refunding', 'refunded')
                  AND LOWER(a.status) = 'succeeded'
                  AND i.deleted_at IS NULL AND a.deleted_at IS NULL
                ORDER BY a.created_at DESC, a.id DESC LIMIT 1
                "#,
            )
            .bind(payment_intent_id)
            .bind(&subject.tenant_id)
            .bind(subject.organization_id.as_deref())
            .bind(subject.organization_id.as_deref())
            .fetch_optional(pool)
            .await
            .map(|row| row.as_ref().map(map_postgres_payment_context))
            .map_err(|error| CommerceServiceError::storage(format!("failed to load refund payment context: {error}"))),
        }
    }

    async fn create_refund(
        &self,
        command: CreateOwnerRefundCommand,
    ) -> Result<RefundView, CommerceServiceError> {
        match self {
            Self::Postgres(pool) => {
                PostgresCommerceRefundStore::new(pool.clone())
                    .create_owner_refund(command)
                    .await
            }
        }
    }

    async fn mark_processing(
        &self,
        subject: &AppRuntimeSubject,
        refund_id: &str,
        headers: &AppWriteCommandHeaders,
        actor_type: &str,
        actor_id: Option<&str>,
    ) -> Result<RefundView, CommerceServiceError> {
        match self {
            Self::Postgres(pool) => {
                PostgresCommerceRefundStore::new(pool.clone())
                    .mark_owner_refund_provider_submission_processing(
                        &subject.tenant_id,
                        subject.organization_id.as_deref(),
                        refund_id,
                        actor_type,
                        actor_id,
                        &headers.request_no,
                        &headers.idempotency_key,
                    )
                    .await
            }
        }
    }

    async fn mark_failed(
        &self,
        subject: &AppRuntimeSubject,
        refund_id: &str,
        headers: &AppWriteCommandHeaders,
        actor_type: &str,
        actor_id: Option<&str>,
    ) -> Result<RefundView, CommerceServiceError> {
        match self {
            Self::Postgres(pool) => {
                PostgresCommerceRefundStore::new(pool.clone())
                    .mark_owner_refund_provider_submission_failed(
                        &subject.tenant_id,
                        subject.organization_id.as_deref(),
                        refund_id,
                        actor_type,
                        actor_id,
                        &headers.request_no,
                        &headers.idempotency_key,
                    )
                    .await
            }
        }
    }

    async fn mark_succeeded(
        &self,
        subject: &AppRuntimeSubject,
        refund_id: &str,
        headers: &AppWriteCommandHeaders,
        actor_type: &str,
        actor_id: Option<&str>,
    ) -> Result<RefundView, CommerceServiceError> {
        match self {
            Self::Postgres(pool) => {
                PostgresCommerceRefundStore::new(pool.clone())
                    .mark_owner_refund_provider_submission_succeeded(
                        &subject.tenant_id,
                        subject.organization_id.as_deref(),
                        refund_id,
                        actor_type,
                        actor_id,
                        &headers.request_no,
                        &headers.idempotency_key,
                    )
                    .await
            }
        }
    }
}

async fn submit_provider_refund_with_retry(
    state: &BackendRefundState,
    subject: &AppRuntimeSubject,
    refund: &impl RefundSubmission,
    reason_code: Option<String>,
) -> Result<ProviderRefundSubmissionState, CommerceServiceError> {
    let mut last_error = None;
    for attempt in 0..REFUND_PROVIDER_SUBMIT_ATTEMPTS {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(150 * (1 << (attempt - 1)))).await;
        }
        match submit_provider_refund(state, subject, refund, reason_code.clone()).await {
            Ok(state) => return Ok(state),
            Err(error) if refund_submission_retryable(&error) => last_error = Some(error),
            Err(error) => return Err(error),
        }
    }
    Err(last_error.expect("refund submission attempted at least once"))
}

async fn submit_provider_refund(
    state: &BackendRefundState,
    subject: &AppRuntimeSubject,
    refund: &impl RefundSubmission,
    reason_code: Option<String>,
) -> Result<ProviderRefundSubmissionState, CommerceServiceError> {
    match &state.pool {
        BackendRefundPool::Postgres(pool) => {
            let Some(attempt) = load_payment_attempt_provider_context_by_id_postgres(
                pool,
                refund.payment_attempt_id(),
            )
            .await?
            else {
                return Err(CommerceServiceError::not_found(
                    "payment attempt provider context was not found",
                ));
            };
            let provider_account_id = attempt.provider_account_id.as_deref().ok_or_else(|| {
                CommerceServiceError::conflict(
                    "original payment does not identify a provider account",
                )
            })?;
            ensure_postgres_account_refund_capability(pool, subject, provider_account_id).await?;
            let account = load_provider_account_for_existing_payment_postgres(
                pool,
                &subject.tenant_id,
                subject.organization_id.as_deref(),
                provider_account_id,
            )
            .await?
            .ok_or_else(|| {
                CommerceServiceError::conflict("original payment account is unavailable")
            })?;
            submit_with_account(
                &state.credentials,
                account,
                attempt.provider_code,
                attempt.out_trade_no,
                attempt.provider_transaction_id,
                attempt.amount,
                refund,
                reason_code,
            )
            .await
        }
    }
}

async fn submit_with_account(
    credentials: &ProviderCredentialBundle,
    account: PaymentProviderAccountRecord,
    provider_code: String,
    out_trade_no: String,
    provider_transaction_id: Option<String>,
    total_amount: String,
    refund: &impl RefundSubmission,
    reason_code: Option<String>,
) -> Result<ProviderRefundSubmissionState, CommerceServiceError> {
    ensure_provider_account_matches(Some(&account), &provider_code)?;
    let registry =
        provider_registry_for_account(credentials, Some(provider_account_binding(&account)));
    let refund_amount =
        CommerceMoney::new(refund.amount()).map_err(CommerceServiceError::storage)?;
    let total_amount = CommerceMoney::new(&total_amount).map_err(CommerceServiceError::storage)?;
    if refund.status() == "processing" {
        if let Some(state) = query_provider_refund(
            &registry,
            &provider_code,
            &out_trade_no,
            provider_transaction_id.as_deref(),
            refund.refund_no(),
        )
        .await?
        {
            return Ok(state);
        }
    }
    create_provider_refund(
        &registry,
        &provider_code,
        &out_trade_no,
        provider_transaction_id.as_deref(),
        refund.refund_no(),
        &refund_amount,
        &total_amount,
        reason_code,
    )
    .await
}

trait RefundSubmission {
    fn amount(&self) -> &str;
    fn payment_attempt_id(&self) -> &str;
    fn refund_no(&self) -> &str;
    fn status(&self) -> &str;
}

impl RefundSubmission for RefundView {
    fn amount(&self) -> &str {
        self.amount.as_str()
    }
    fn payment_attempt_id(&self) -> &str {
        &self.payment_attempt_id
    }
    fn refund_no(&self) -> &str {
        &self.refund_no
    }
    fn status(&self) -> &str {
        &self.status
    }
}

impl RefundSubmission for BackendRefundView {
    fn amount(&self) -> &str {
        &self.amount
    }
    fn payment_attempt_id(&self) -> &str {
        &self.payment_attempt_id
    }
    fn refund_no(&self) -> &str {
        &self.refund_no
    }
    fn status(&self) -> &str {
        &self.status
    }
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

async fn ensure_postgres_account_refund_capability(
    pool: &PgPool,
    subject: &AppRuntimeSubject,
    provider_account_id: &str,
) -> Result<(), CommerceServiceError> {
    let value = sqlx::query_scalar::<_, Value>(
        "SELECT capabilities FROM commerce_payment_provider_account WHERE id = $1 AND tenant_id = $2 AND (organization_id = $3 OR organization_id = '0' OR organization_id = '0') AND status IN ('active','inactive','deprecated') AND deleted_at IS NULL LIMIT 1",
    )
    .bind(provider_account_id)
    .bind(&subject.tenant_id)
    .bind(subject.organization_id.as_deref())
    .fetch_optional(pool)
    .await
    .map_err(|error| CommerceServiceError::storage(format!("failed to validate payment account refund capability: {error}")))?;
    ensure_refund_capability(value.as_ref())
}

fn ensure_refund_capability(value: Option<&Value>) -> Result<(), CommerceServiceError> {
    if value
        .and_then(|value| value.get("refund"))
        .and_then(Value::as_bool)
        == Some(true)
    {
        Ok(())
    } else {
        Err(CommerceServiceError::conflict(
            "original payment account does not enable refund capability",
        ))
    }
}

fn normalize_reason_code(value: &str) -> Result<String, CommerceServiceError> {
    let value = value.trim().to_ascii_lowercase();
    if REFUND_REASON_CODES.contains(&value.as_str()) {
        Ok(value)
    } else {
        Err(CommerceServiceError::validation(
            "refund reason must be customer_request, duplicate, fraud, service_failure, or other",
        ))
    }
}

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

#[allow(clippy::result_large_err)]
fn validate_backend_write_payload(
    ctx: Option<&WebRequestContext>,
    headers: &HeaderMap,
    scope: &str,
    body: &impl Serialize,
    request_no_prefix: &str,
) -> Result<AppWriteCommandHeaders, Response> {
    validate_write_payload(headers, scope, body, |idempotency_key| {
        format!("{request_no_prefix}-{idempotency_key}")
    })
    .map_err(|error| match error {
        WriteCommandHeaderError::InvalidHeader(message) => validation(ctx, message),
    })
}

fn map_postgres_refund(row: &PgRow) -> BackendRefundView {
    BackendRefundView {
        id: postgres_string(row, "id"),
        refund_no: postgres_string(row, "refund_no"),
        order_id: postgres_string(row, "order_id"),
        payment_intent_id: postgres_string(row, "payment_intent_id"),
        payment_attempt_id: postgres_string(row, "payment_attempt_id"),
        provider_code: postgres_string(row, "provider_code"),
        provider_account_id: postgres_optional_string(row, "provider_account_id"),
        amount: postgres_string(row, "amount"),
        currency_code: postgres_string(row, "currency_code"),
        status: postgres_string(row, "status"),
        reason_code: postgres_optional_string(row, "refund_reason_code"),
        requested_by_type: postgres_string(row, "requested_by_type"),
        requested_by: postgres_optional_string(row, "requested_by"),
        created_at: postgres_string(row, "created_at"),
        updated_at: postgres_string(row, "updated_at"),
    }
}

fn map_postgres_payment_context(row: &PgRow) -> RefundPaymentContext {
    RefundPaymentContext {
        payment_intent_no: postgres_string(row, "payment_intent_no"),
        owner_user_id: postgres_string(row, "owner_user_id"),
        order_id: postgres_string(row, "order_id"),
        payment_attempt_id: postgres_string(row, "payment_attempt_id"),
        currency_code: postgres_string(row, "currency_code"),
    }
}

fn postgres_total(rows: &[PgRow]) -> i64 {
    rows.first()
        .and_then(|row| row.try_get::<i64, _>("total_count").ok())
        .unwrap_or(0)
}

fn postgres_string(row: &PgRow, name: &str) -> String {
    postgres_optional_string(row, name).unwrap_or_default()
}

fn postgres_optional_string(row: &PgRow, name: &str) -> Option<String> {
    row.try_get::<Option<String>, _>(name)
        .ok()
        .flatten()
        .or_else(|| row.try_get::<String, _>(name).ok())
}
