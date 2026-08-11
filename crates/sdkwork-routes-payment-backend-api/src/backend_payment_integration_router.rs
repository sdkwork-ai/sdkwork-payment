use std::sync::Arc;
use std::time::Instant;

use axum::extract::{Extension, Path, Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use sdkwork_iam_context_service::IamAppContext;
use sdkwork_payment_providers::{
    payment_credential_cipher, provider_registry_for_account, resolve_secret_ref,
    CredentialCipherScope, EncryptedPaymentCredential, EnvPaymentCredentialResolver,
    PaymentProviderRegistry, PaymentQueryPaymentIntentRequest, PaymentVerifyWebhookRequest,
    ProviderAccountBinding, ProviderCredentialBundle,
};
use sdkwork_payment_repository_sqlx::{
    enrich_owner_payment_attempt_postgres, webhook_status::map_provider_payment_status,
    IngestProviderWebhookCommand, OwnerOrderPaymentEnrichmentContext,
    payment_attempt_context::PaymentWebhookAttemptIdentity,
    PostgresCommercePaymentIntentStore,
    postgres_webhook_ingestion::{
        apply_webhook_payment_status_postgres, ingest_provider_webhook_postgres,
    },
};
use sdkwork_payment_service::{
    CreateOwnerPaymentAttemptCommand, CreateOwnerPaymentAttemptOutcome,
    CreateOwnerPaymentIntentCommand, PaymentIntentView,
};
use sdkwork_payment_service_host::PaymentServiceHost;
use sdkwork_utils_rust::OffsetListPageParams;
use sdkwork_web_core::WebRequestContext;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{postgres::PgRow, PgPool, Row};

use crate::api_response::{
    conflict, map_service_error, not_found, success_created_item,
    success_item, success_list, success_no_content, unauthorized, validation,
};
use crate::command_headers::{validate_write_payload, WriteCommandHeaderError};
use crate::subject::{backend_runtime_subject_from_extension, AppRuntimeSubject};

#[derive(Clone)]
enum IntegrationPool {
    Postgres(PgPool),
}

#[derive(Clone)]
struct IntegrationState {
    pool: IntegrationPool,
}

#[derive(Clone)]
struct ProviderAccountRecord {
    id: String,
    provider_code: String,
    merchant_id: Option<String>,
    environment: String,
    secret_ref: String,
    webhook_secret_ref: Option<String>,
    certificate_ref: Option<String>,
    primary_secret: Option<String>,
    webhook_secret: Option<String>,
    certificate: Option<String>,
    metadata: Value,
}

impl ProviderAccountRecord {
    fn binding(&self, environment: String) -> ProviderAccountBinding {
        ProviderAccountBinding {
            provider_code: self.provider_code.clone(),
            merchant_id: self.merchant_id.clone(),
            environment,
            secret_ref: self.secret_ref.clone(),
            webhook_secret_ref: self.webhook_secret_ref.clone(),
            certificate_ref: self.certificate_ref.clone(),
            primary_secret: self.primary_secret.clone(),
            webhook_secret: self.webhook_secret.clone(),
            certificate: self.certificate.clone(),
            metadata: self.metadata.clone(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderAccountTestBody {
    environment: Option<String>,
    #[serde(default)]
    dry_run: bool,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CredentialRotateBody {
    primary_secret: String,
    webhook_secret: Option<String>,
    certificate: Option<String>,
    #[serde(default)]
    invalidate_previous: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateSubMerchantBody {
    provider_account_id: String,
    sub_merchant_no: String,
    sub_merchant_name: Option<String>,
    sub_app_id: Option<String>,
    sub_mch_id: Option<String>,
    stripe_connected_account_id: Option<String>,
    provider_code: String,
    status: Option<String>,
    metadata: Option<Value>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateSubMerchantBody {
    sub_merchant_name: Option<String>,
    sub_app_id: Option<String>,
    sub_mch_id: Option<String>,
    stripe_connected_account_id: Option<String>,
    status: Option<String>,
    metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubMerchantListQuery {
    #[serde(default)]
    page: Option<i64>,
    #[serde(default, rename = "page_size")]
    page_size: Option<i64>,
    provider_account_id: Option<String>,
    status: Option<String>,
    q: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateCertificateBody {
    certificate_no: String,
    provider_code: String,
    certificate_type: String,
    certificate: String,
    metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CertificateListQuery {
    #[serde(default)]
    page: Option<i64>,
    #[serde(default, rename = "page_size")]
    page_size: Option<i64>,
    provider_code: Option<String>,
    certificate_type: Option<String>,
    expiring_within_days: Option<i64>,
    q: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SandboxTriggerBody {
    provider_account_id: String,
    event_type: String,
    amount: Option<String>,
    currency_code: Option<String>,
    out_trade_no: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateTestPaymentBody {
    method_key: String,
    amount: Option<String>,
    currency_code: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TestPaymentResult {
    payment_id: String,
    payment_intent_id: String,
    payment_intent_no: String,
    attempt_id: String,
    out_trade_no: String,
    method_key: String,
    provider_code: String,
    amount: String,
    currency_code: String,
    status: String,
    /// Scan-to-pay QR code URL (`wechat_native` code_url / `alipay_qr` qr_code).
    qr_code_url: Option<String>,
    /// Web cashier redirect URL (`alipay_wap`/`alipay_pc` cashier link).
    pay_url: Option<String>,
    /// Full Alipay cashier form HTML for browser render + auto submit.
    pay_form: Option<String>,
    /// Stripe PaymentIntent client secret for Stripe.js card collection.
    client_secret: Option<String>,
    /// Stripe publishable key for Stripe.js (from deployment env or the
    /// provider account metadata).
    publishable_key: Option<String>,
    expires_at: Option<String>,
    created_at: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CheckAttemptStatusBody {
    payment_intent_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CheckAttemptStatusResult {
    payment_intent_id: String,
    attempt_id: String,
    /// Raw provider status from the PSP query (e.g. WeChat `SUCCESS`,
    /// Alipay `TRADE_SUCCESS`), `None` when the attempt was already terminal.
    provider_status: Option<String>,
    local_status: String,
    paid: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct WebhookSignatureTestBody {
    provider_account_id: String,
    payload: String,
    signature: String,
    timestamp: Option<String>,
    signature_header: Option<String>,
}

pub fn build_backend_payment_integration_router(host: Arc<PaymentServiceHost>) -> Router {
    let pool = host
        .database_pool()
        .as_postgres()
        .expect("payment backend integration routes require an authoritative PostgreSQL pool")
        .clone();
    let pool = IntegrationPool::Postgres(pool);
    build_router(IntegrationState { pool })
}

pub fn backend_payment_integration_router_with_postgres_pool(pool: PgPool) -> Router {
    build_router(IntegrationState {
        pool: IntegrationPool::Postgres(pool),
    })
}

fn build_router(state: IntegrationState) -> Router {
    Router::new()
        .route(
            "/backend/v3/api/payments/provider_accounts/{providerAccountId}/test",
            post(test_provider_account),
        )
        .route(
            "/backend/v3/api/payments/provider_accounts/{providerAccountId}/credentials/rotate",
            post(rotate_provider_credentials),
        )
        .route(
            "/backend/v3/api/payments/sub_merchants",
            get(list_sub_merchants).post(create_sub_merchant),
        )
        .route(
            "/backend/v3/api/payments/sub_merchants/{subMerchantId}",
            get(retrieve_sub_merchant)
                .patch(update_sub_merchant)
                .delete(delete_sub_merchant),
        )
        .route(
            "/backend/v3/api/payments/certificates",
            get(list_certificates).post(create_certificate),
        )
        .route(
            "/backend/v3/api/payments/certificates/{certificateId}",
            get(retrieve_certificate).delete(delete_certificate),
        )
        .route(
            "/backend/v3/api/payments/dev/sandbox_trigger",
            post(trigger_sandbox_event),
        )
        .route(
            "/backend/v3/api/payments/dev/test_payments",
            post(create_test_payment),
        )
        .route(
            "/backend/v3/api/payments/dev/check_attempt_status",
            post(check_attempt_status),
        )
        .route(
            "/backend/v3/api/payments/dev/webhook_signature_test",
            post(test_webhook_signature),
        )
        .with_state(state)
}

async fn test_provider_account(
    State(state): State<IntegrationState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    headers: HeaderMap,
    Path(provider_account_id): Path<String>,
    body: Option<Json<ProviderAccountTestBody>>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match require_subject(runtime_context, ctx) {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    let body = body
        .map(|Json(value)| value)
        .unwrap_or(ProviderAccountTestBody {
            environment: None,
            dry_run: false,
        });
    if let Err(response) = validate_command(ctx, &headers, "provider-account-test", &body) {
        return response;
    }
    let account = match load_provider_account(&state.pool, &subject, &provider_account_id).await {
        Ok(Some(account)) => account,
        Ok(None) => return not_found(ctx, "payment provider account was not found"),
        Err(error) => return map_service_error(ctx, error),
    };
    let environment = body
        .environment
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&account.environment)
        .to_owned();
    if !matches!(
        environment.as_str(),
        "development" | "sandbox" | "production"
    ) {
        return validation(
            ctx,
            "environment must be development, sandbox, or production",
        );
    }

    let started = Instant::now();
    let binding = account.binding(environment.clone());
    let credentials = EnvPaymentCredentialResolver::load();
    let registry = provider_registry_for_account(&credentials, Some(binding));
    let adapter = registry.resolve(&account.provider_code);
    let readiness_issues =
        provider_account_readiness_issues(&account, &credentials, adapter.is_some());
    let credentials_resolved = readiness_issues.is_empty();
    let (ok, diagnostic) = if body.dry_run {
        (
            credentials_resolved,
            if credentials_resolved {
                "Credential references resolved and the provider adapter initialized.".to_owned()
            } else {
                readiness_issues.join(" ")
            },
        )
    } else if !readiness_issues.is_empty() {
        (false, readiness_issues.join(" "))
    } else {
        (
            false,
            "Credential references resolved, but this provider adapter does not expose a non-mutating remote connectivity probe; use dryRun for credential validation.".to_owned(),
        )
    };
    let tested_at = now_string();
    if let Err(error) = update_provider_test_status(
        &state.pool,
        &subject,
        &account.id,
        &tested_at,
        if ok { "success" } else { "failure" },
    )
    .await
    {
        return map_service_error(ctx, error);
    }
    success_item(
        ctx,
        json!({
            "ok": ok,
            "providerCode": account.provider_code,
            "environment": environment,
            "pspResponseTimeMs": started.elapsed().as_millis() as u64,
            "diagnostic": diagnostic,
            "testedAt": tested_at,
        }),
    )
}

fn provider_account_readiness_issues(
    account: &ProviderAccountRecord,
    credentials: &ProviderCredentialBundle,
    adapter_initialized: bool,
) -> Vec<String> {
    let mut issues = Vec::new();
    if account.primary_secret.is_none() && resolve_secret_ref(&account.secret_ref).is_none() {
        issues.push("primary provider credential is not configured".to_owned());
    }
    match account.provider_code.to_ascii_lowercase().as_str() {
        "wechat_pay" => {
            if account
                .metadata
                .get("appId")
                .and_then(Value::as_str)
                .is_none_or(|value| value.trim().is_empty())
            {
                issues.push("metadata.appId is required for WeChat Pay".to_owned());
            }
            if account
                .metadata
                .get("merchantSerialNo")
                .and_then(Value::as_str)
                .is_none_or(|value| value.trim().is_empty())
            {
                issues.push("metadata.merchantSerialNo is required for WeChat Pay".to_owned());
            }
            if account.merchant_id.as_deref().is_none_or(str::is_empty) {
                issues.push("merchantId is required for WeChat Pay".to_owned());
            }
            if account.webhook_secret.is_none()
                && account
                    .webhook_secret_ref
                    .as_deref()
                    .and_then(resolve_secret_ref)
                    .is_none()
            {
                issues.push("WeChat API v3 key is not configured".to_owned());
            }
            if account.certificate.is_none()
                && account
                    .certificate_ref
                    .as_deref()
                    .and_then(resolve_secret_ref)
                    .is_none()
            {
                issues.push("WeChat platform certificate is not configured".to_owned());
            }
            if account
                .metadata
                .get("notifyUrl")
                .and_then(Value::as_str)
                .is_none_or(|value| value.trim().is_empty())
                && credentials.provider_notify_url("wechat_pay").is_none()
            {
                issues.push("metadata.notifyUrl is required for WeChat Pay".to_owned());
            }
        }
        "alipay" => {
            if account.merchant_id.as_deref().is_none_or(str::is_empty) {
                issues.push("merchantId (Alipay appId) is required".to_owned());
            }
            if account.certificate.is_none()
                && account
                    .certificate_ref
                    .as_deref()
                    .and_then(resolve_secret_ref)
                    .is_none()
            {
                issues.push("Alipay public key is not configured".to_owned());
            }
        }
        "stripe" => {
            if account.webhook_secret.is_none()
                && account
                    .webhook_secret_ref
                    .as_deref()
                    .and_then(resolve_secret_ref)
                    .is_none()
            {
                issues.push("Stripe webhook signing secret is not configured".to_owned());
            }
        }
        "sandbox" => return issues,
        _ => issues.push(format!(
            "unsupported provider code {}",
            account.provider_code
        )),
    }
    if !adapter_initialized && account.provider_code != "sandbox" {
        issues.push("provider adapter could not initialize".to_owned());
    }
    issues
}

async fn rotate_provider_credentials(
    State(state): State<IntegrationState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    headers: HeaderMap,
    Path(provider_account_id): Path<String>,
    Json(body): Json<CredentialRotateBody>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match require_subject(runtime_context, ctx) {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    if let Err(response) = validate_command(ctx, &headers, "provider-credential-rotate", &body) {
        return response;
    }
    let primary_secret = match required_text(&body.primary_secret, "primarySecret", ctx) {
        Ok(value) => value,
        Err(response) => return response,
    };
    match rotate_credentials(
        &state.pool,
        &subject,
        &provider_account_id,
        primary_secret,
        normalized(body.webhook_secret),
        normalized(body.certificate),
        body.invalidate_previous,
    )
    .await
    {
        Ok(Some(item)) => success_item(ctx, item),
        Ok(None) => not_found(ctx, "payment provider account was not found"),
        Err(error) => map_service_error(ctx, error),
    }
}

async fn list_sub_merchants(
    State(state): State<IntegrationState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    Query(query): Query<SubMerchantListQuery>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match require_subject(runtime_context, ctx) {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    let page = OffsetListPageParams::parse(query.page, query.page_size);
    match query_sub_merchants(&state.pool, &subject, &query, page).await {
        Ok((items, total)) => success_list(ctx, items, total, page),
        Err(error) => map_service_error(ctx, error),
    }
}

async fn create_sub_merchant(
    State(state): State<IntegrationState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    headers: HeaderMap,
    Json(body): Json<CreateSubMerchantBody>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match require_subject(runtime_context, ctx) {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    let write = match validate_command(ctx, &headers, "sub-merchant-create", &body) {
        Ok(write) => write,
        Err(response) => return response,
    };
    if let Err(response) = validate_sub_merchant_provider(&body.provider_code, ctx) {
        return response;
    }
    let provider_account_id =
        match required_text(&body.provider_account_id, "providerAccountId", ctx) {
            Ok(value) => value,
            Err(response) => return response,
        };
    let sub_merchant_no = match required_text(&body.sub_merchant_no, "subMerchantNo", ctx) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let id = stable_id("sub-merchant", &write.idempotency_key);
    match insert_sub_merchant(
        &state.pool,
        &subject,
        &id,
        &provider_account_id,
        &sub_merchant_no,
        &body,
    )
    .await
    {
        Ok(Some(item)) => success_created_item(ctx, item),
        Ok(None) => not_found(ctx, "partner provider account was not found"),
        Err(error) => map_service_error(ctx, error),
    }
}

async fn retrieve_sub_merchant(
    State(state): State<IntegrationState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    Path(id): Path<String>,
) -> Response {
    retrieve_resource(
        state,
        runtime_context,
        request_context,
        id,
        ResourceKind::SubMerchant,
    )
    .await
}

async fn update_sub_merchant(
    State(state): State<IntegrationState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<UpdateSubMerchantBody>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match require_subject(runtime_context, ctx) {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    if let Err(response) = validate_command(ctx, &headers, "sub-merchant-update", &body) {
        return response;
    }
    match patch_sub_merchant(&state.pool, &subject, &id, &body).await {
        Ok(Some(item)) => success_item(ctx, item),
        Ok(None) => not_found(ctx, "payment sub-merchant was not found"),
        Err(error) => map_service_error(ctx, error),
    }
}

async fn delete_sub_merchant(
    State(state): State<IntegrationState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    Path(id): Path<String>,
) -> Response {
    delete_resource(
        state,
        runtime_context,
        request_context,
        id,
        ResourceKind::SubMerchant,
    )
    .await
}

async fn list_certificates(
    State(state): State<IntegrationState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    Query(query): Query<CertificateListQuery>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match require_subject(runtime_context, ctx) {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    let page = OffsetListPageParams::parse(query.page, query.page_size);
    match query_certificates(&state.pool, &subject, &query, page).await {
        Ok((items, total)) => success_list(ctx, items, total, page),
        Err(error) => map_service_error(ctx, error),
    }
}

async fn create_certificate(
    State(state): State<IntegrationState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    headers: HeaderMap,
    Json(body): Json<CreateCertificateBody>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match require_subject(runtime_context, ctx) {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    let write = match validate_command(ctx, &headers, "certificate-create", &body) {
        Ok(write) => write,
        Err(response) => return response,
    };
    if let Err(response) = validate_certificate_type(&body.certificate_type, ctx) {
        return response;
    }
    let certificate_no = match required_text(&body.certificate_no, "certificateNo", ctx) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let certificate = match required_text(&body.certificate, "certificate", ctx) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let id = stable_id("certificate", &write.idempotency_key);
    let encrypted = match payment_credential_cipher().and_then(|cipher| {
        cipher.encrypt(
            CredentialCipherScope {
                tenant_id: &subject.tenant_id,
                provider_account_id: &id,
                credential_kind: "certificate_inventory",
            },
            &certificate,
        )
    }) {
        Ok(value) => value,
        Err(_) => return map_service_error(ctx, storage("certificate encryption failed")),
    };
    match insert_certificate(
        &state.pool,
        &subject,
        &id,
        &certificate_no,
        &encrypted,
        &body,
    )
    .await
    {
        Ok(item) => success_created_item(ctx, item),
        Err(error) => map_service_error(ctx, error),
    }
}

async fn retrieve_certificate(
    State(state): State<IntegrationState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    Path(id): Path<String>,
) -> Response {
    retrieve_resource(
        state,
        runtime_context,
        request_context,
        id,
        ResourceKind::Certificate,
    )
    .await
}

async fn delete_certificate(
    State(state): State<IntegrationState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    Path(id): Path<String>,
) -> Response {
    delete_resource(
        state,
        runtime_context,
        request_context,
        id,
        ResourceKind::Certificate,
    )
    .await
}

async fn trigger_sandbox_event(
    State(state): State<IntegrationState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    headers: HeaderMap,
    Json(body): Json<SandboxTriggerBody>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match require_subject(runtime_context, ctx) {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    let write = match validate_command(ctx, &headers, "sandbox-trigger", &body) {
        Ok(write) => write,
        Err(response) => return response,
    };
    let account =
        match load_provider_account(&state.pool, &subject, &body.provider_account_id).await {
            Ok(Some(account)) => account,
            Ok(None) => return not_found(ctx, "payment provider account was not found"),
            Err(error) => return map_service_error(ctx, error),
        };
    if !matches!(account.environment.as_str(), "development" | "sandbox") {
        return conflict(
            ctx,
            "sandbox events require a development or sandbox provider account",
        );
    }
    let operation_id = stable_id("sandbox-operation", &write.idempotency_key);
    let event_id = stable_id("sandbox-event", &write.idempotency_key);
    let payload = json!({
        "id": event_id,
        "type": body.event_type,
        "providerAccountId": account.id,
        "providerCode": account.provider_code,
        "amount": body.amount,
        "currencyCode": body.currency_code,
        "outTradeNo": body.out_trade_no,
        "sandbox": true,
    });
    // The sandbox callback is ingested synchronously through the same path
    // as a real PSP webhook (out-trade-no resolution → status machine → event
    // record), so the simulated payment takes effect immediately instead of
    // waiting for a queue consumer that does not exist.
    let IntegrationPool::Postgres(pool) = &state.pool else {
        return validation(ctx, "payment dev endpoints require the postgres integration pool");
    };
    let ingest_command = IngestProviderWebhookCommand {
        provider_code: account.provider_code.clone(),
        provider_event_id: event_id.clone(),
        event_type: Some(body.event_type.clone()),
        out_trade_no: body.out_trade_no.clone(),
        payment_status: Some("succeeded".to_owned()),
        payload,
        tenant_id: Some(subject.tenant_id.clone()),
        organization_id: subject.organization_id.clone(),
    };
    let outcome = match ingest_provider_webhook_postgres(pool, ingest_command).await {
        Ok(outcome) => outcome,
        Err(error) => return map_service_error(ctx, error),
    };
    success_item(
        ctx,
        SandboxTriggerResult {
            operation_id,
            event_id,
            webhook_event_id: outcome.webhook_event_id,
            payment_attempt_id: outcome.payment_attempt_id,
            applied_status: outcome.applied_status,
        },
    )
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SandboxTriggerResult {
    operation_id: String,
    event_id: String,
    webhook_event_id: String,
    payment_attempt_id: Option<String>,
    applied_status: Option<String>,
}

// ===========================================================================
// One-cent test payment (dev config)
// ===========================================================================

/// Payment method keys that can drive the one-cent test payment. Two classes:
/// scan-to-pay QR methods (`wechat_native` → WeChat `/v3/pay/transactions/native`
/// `code_url`, `alipay_qr` → Alipay `alipay.trade.precreate` `qr_code`) and
/// web-redirect methods (`alipay_wap`/`alipay_pc` → Alipay H5/PC cashier
/// `redirect_url`/`payForm`). Other products return SDK invocation payloads,
/// require payer identifiers (openid/buyer_id), or have no checkout at all
/// (sandbox).
const TEST_PAYMENT_METHOD_KEYS: [&str; 10] = [
    "wechat_native",
    "alipay_qr",
    "alipay_wap",
    "alipay_pc",
    // Stripe card/wallet methods use the client-secret confirm flow: the
    // admin enters card details in the dialog (Stripe.js) and pays directly.
    "stripe_card",
    "stripe_apple_pay",
    "stripe_google_pay",
    "stripe_alipay",
    "stripe_wechat_pay",
    // The local sandbox has no real PSP checkout; the dialog simulates the
    // payment success via the sandbox webhook trigger (no credentials needed).
    "sandbox_test",
];
const TEST_PAYMENT_ORDER_SUBJECT: &str = "One-cent test payment";
/// Test order lifetime in minutes. Order-expiration enforcement rejects
/// payment for orders with missing/expired boundaries, and the provider
/// checkout window is `min(order expiry, provider checkout TTL)` (900s),
/// so the test order is created with a 15-minute future expiry that yields
/// the full QR window.
const TEST_PAYMENT_ORDER_TTL_MINUTES: i64 = 15;
/// Upper bound (minor units) for one-cent test payments: the dev endpoint
/// must not create large real PSP orders even for operators — defense in
/// depth behind the `commerce.payments.dev.test_payments` permission.
const TEST_PAYMENT_AMOUNT_MAX_MINOR_UNITS: i64 = 10_000;

struct TestPaymentMethodRecord {
    method_key: String,
    status: String,
}

/// Provider-enriched intent store for the test payment flow. Mirrors the
/// app-api `ProviderEnriched*PaymentIntents` wrappers so the attempt checkout
/// drives the real provider adapter (WeChat native code_url, Alipay QR, ...).

struct PostgresTestPaymentStore {
    inner: Arc<PostgresCommercePaymentIntentStore>,
    pool: PgPool,
    registry: Arc<PaymentProviderRegistry>,
    credentials: ProviderCredentialBundle,
}

impl PostgresTestPaymentStore {
    async fn create_owner_payment_intent(
        &self,
        command: CreateOwnerPaymentIntentCommand,
    ) -> Result<PaymentIntentView, sdkwork_contract_service::CommerceServiceError> {
        self.inner.create_owner_payment_intent(command).await
    }

    async fn create_owner_payment_attempt(
        &self,
        command: CreateOwnerPaymentAttemptCommand,
    ) -> Result<CreateOwnerPaymentAttemptOutcome, sdkwork_contract_service::CommerceServiceError>
    {
        let registry = self.registry.clone();
        let credentials = self.credentials.clone();
        let pool = self.pool.clone();
        let inner = self.inner.clone();
        let tenant_id = command.tenant_id.clone();
        let organization_id = command.organization_id.clone();
        let outcome = inner.create_owner_payment_attempt(command).await?;
        let order_id = outcome.order_id.clone();
        enrich_owner_payment_attempt_postgres(
            &pool,
            OwnerOrderPaymentEnrichmentContext {
                deployment_registry: &registry,
                credentials: &credentials,
                tenant_id: &tenant_id,
                organization_id: organization_id.as_deref(),
                order_id: &order_id,
                payment_scene: None,
            },
            outcome,
        )
        .await
    }
}

async fn create_test_payment(
    State(state): State<IntegrationState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    headers: HeaderMap,
    Json(body): Json<CreateTestPaymentBody>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match require_subject(runtime_context, ctx) {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    let write = match validate_command(ctx, &headers, "test-payment", &body) {
        Ok(write) => write,
        Err(response) => return response,
    };
    let method_key = match required_text(&body.method_key, "methodKey", ctx) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let amount = normalized(body.amount).unwrap_or_else(|| "0.01".to_owned());
    if !is_money_amount(&amount) {
        return validation(ctx, "amount must match ^[0-9]+(\\.[0-9]{1,2})?$");
    }
    match decimal_amount_minor_units(&amount) {
        Some(minor) if minor > TEST_PAYMENT_AMOUNT_MAX_MINOR_UNITS => {
            return validation(
                ctx,
                "amount must not exceed 10000 minor units (100.00); the one-cent test payment is a small-amount verification only",
            );
        }
        Some(_) => {}
        None => return validation(ctx, "amount overflows the supported range"),
    }
    let currency_code = normalized(body.currency_code).unwrap_or_else(|| "CNY".to_owned());
    if currency_code.len() != 3 {
        return validation(ctx, "currencyCode must be a 3-letter ISO currency code");
    }

    let method = match load_payment_method_for_test(&state.pool, &subject, &method_key).await {
        Ok(Some(method)) => method,
        Ok(None) => return not_found(
            ctx,
            format!(
                "payment method {method_key} was not found for this tenant/organization; create it in the payment methods workspace and enable it"
            ),
        ),
        Err(error) => return map_service_error(ctx, error),
    };
    if method.status != "active" {
        return conflict(
            ctx,
            format!(
                "payment method {method_key} is not active; enable it in the payment methods workspace first"
            ),
        );
    }
    if !TEST_PAYMENT_METHOD_KEYS.contains(&method.method_key.as_str()) {
        return validation(
            ctx,
            format!(
                "payment method {method_key} does not support one-cent test payments; only these methods can: wechat_native, alipay_qr (scan-to-pay QR), alipay_wap, alipay_pc (web cashier), stripe_* (card), sandbox_test (local simulation)"
            ),
        );
    }

    // The test payment must hang off a payable order (the payment executor
    // reads the amount from the order), so an internal test order is created
    // first. `ON CONFLICT DO NOTHING` keeps idempotent retries stable.
    let order_id = stable_id("test-payment-order", &write.idempotency_key);
    let order_no = format!("TP-{}", &order_id[order_id.len().saturating_sub(24)..]);
    let now = now_string();
    // The test order must carry a future expiry: order-expiration enforcement
    // rejects intent/attempt creation for orders without a payable boundary.
    let order_expires_at = now_plus_minutes(TEST_PAYMENT_ORDER_TTL_MINUTES);
    if let Err(error) = insert_test_order(
        &state.pool,
        &subject,
        &order_id,
        &order_no,
        &amount,
        &currency_code,
        &now,
        &order_expires_at,
    )
    .await
    {
        return map_service_error(ctx, error);
    }

    let checkout =
        match create_test_payment_checkout(&state.pool, &subject, &order_id, &method_key, &write)
            .await
        {
            Ok(outcome) => outcome,
            Err(error) => {
                // PSP credential/config and upstream transport failures are
                // diagnosable setup problems for a dev test payment, not
                // internal errors: surface the concrete message as a 4xx
                // instead of a generic 50001 so the operator can act on it
                // (e.g., configure a provider account + channel, or fix the
                // upstream call).
                if matches!(error.code(), "provider-unavailable" | "transport") {
                    let message = error.message();
                    let guidance = if message.contains("is not configured") {
                        "create an active provider account and channel for this method (or set the provider credentials), then retry"
                    } else {
                        "check the provider credentials, merchant configuration, network access, and the callback (notify) URL configuration, then retry"
                    };
                    return validation(
                        ctx,
                        format!("payment provider checkout failed: {message}; {guidance}"),
                    );
                }
                // Storage failures (e.g. a missing table column in this
                // deployment) must be visible for a dev endpoint instead of a
                // generic 50001: report the concrete SQL problem.
                if error.code() == "storage" {
                    return validation(
                        ctx,
                        format!(
                            "payment test order storage failed: {} (verify the payment/order schema of this deployment)",
                            error.message()
                        ),
                    );
                }
                // Order-boundary conflicts (e.g. "order is not pending
                // payment") get the stored order status and expiry appended;
                // method-availability conflicts get the method/channel/account
                // state appended — so a schema or configuration issue is
                // visible instead of opaque.
                if error.code() == "conflict" {
                    let order_diagnostic =
                        load_test_order_diagnostic(&state.pool, &subject, &order_id).await;
                    let method_diagnostic =
                        load_test_payment_method_diagnostic(&state.pool, &subject, &method_key)
                            .await;
                    let diagnostic = [order_diagnostic, method_diagnostic]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join("; ");
                    if !diagnostic.is_empty() {
                        return conflict(ctx, format!("{} ({diagnostic})", error.message()));
                    }
                }
                return map_service_error(ctx, error);
            }
        };
    let (intent, attempt) = checkout;

    let qr_code_url = attempt
        .payment_params
        .get("qrCodeUrl")
        .cloned()
        .or_else(|| attempt.payment_params.get("cashierUrl").cloned())
        .or_else(|| attempt.payment_params.get("paymentUrl").cloned());
    success_item(
        ctx,
        TestPaymentResult {
            payment_id: attempt.attempt_id.clone(),
            payment_intent_id: attempt.payment_intent_id.clone(),
            payment_intent_no: intent.payment_intent_no,
            attempt_id: attempt.attempt_id,
            out_trade_no: attempt.out_trade_no,
            method_key: attempt.payment_method,
            provider_code: attempt.provider_code,
            amount: attempt.amount.as_str().to_owned(),
            currency_code,
            status: attempt.status,
            qr_code_url,
            pay_url: attempt.payment_params.get("payUrl").cloned(),
            pay_form: attempt.payment_params.get("payForm").cloned(),
            client_secret: attempt.payment_params.get("clientSecret").cloned(),
            publishable_key: std::env::var("STRIPE_PUBLISHABLE_KEY")
                .ok()
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty()),
            expires_at: attempt.payment_params.get("expiresAt").cloned(),
            created_at: now,
        },
    )
}

async fn create_test_payment_checkout(
    pool: &IntegrationPool,
    subject: &AppRuntimeSubject,
    order_id: &str,
    method_key: &str,
    write: &crate::command_headers::AppWriteCommandHeaders,
) -> Result<
    (PaymentIntentView, CreateOwnerPaymentAttemptOutcome),
    sdkwork_contract_service::CommerceServiceError,
> {
    let credentials = ProviderCredentialBundle::from_env();
    let registry = Arc::new(PaymentProviderRegistry::from_credentials(
        credentials.clone(),
    ));
    let intent_command = CreateOwnerPaymentIntentCommand::new(
        &subject.tenant_id,
        subject.organization_id.as_deref(),
        &subject.user_id,
        order_id,
        method_key,
        &write.request_no,
        &format!("test-payment-intent-{}", write.idempotency_key),
    )?;
    let build_attempt = |payment_intent_id: &str| {
        CreateOwnerPaymentAttemptCommand::new(
            &subject.tenant_id,
            subject.organization_id.as_deref(),
            &subject.user_id,
            payment_intent_id,
            &write.request_no,
            &format!("test-payment-attempt-{}", write.idempotency_key),
        )
    };
    match pool {
        IntegrationPool::Postgres(pool) => {
            let store = PostgresTestPaymentStore {
                inner: Arc::new(PostgresCommercePaymentIntentStore::new(pool.clone())),
                pool: pool.clone(),
                registry,
                credentials,
            };
            let intent = store.create_owner_payment_intent(intent_command).await?;
            let attempt = store
                .create_owner_payment_attempt(build_attempt(&intent.payment_intent_id)?)
                .await?;
            Ok((intent, attempt))
        }
    }
}

/// Queries the PSP for the current payment state of a test-payment attempt and
/// applies the result through the same status machine the webhook path uses.
/// This closes the loop for sandbox/real PSP payments whose async notify did
/// not arrive (e.g. Alipay sandbox without a configured callback): the admin
/// scans/pays, then confirms via this endpoint instead of waiting forever.
async fn check_attempt_status(
    State(state): State<IntegrationState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    headers: HeaderMap,
    Json(body): Json<CheckAttemptStatusBody>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match require_subject(runtime_context, ctx) {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    let write = match validate_command(ctx, &headers, "check-attempt-status", &body) {
        Ok(write) => write,
        Err(response) => return response,
    };
    let payment_intent_id = match required_text(&body.payment_intent_id, "paymentIntentId", ctx) {
        Ok(value) => value,
        Err(response) => return response,
    };

    let IntegrationPool::Postgres(pool) = &state.pool else {
        return validation(ctx, "payment dev endpoints require the postgres integration pool");
    };
    let organization_id = subject
        .organization_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("0");
    let attempt = match load_test_attempt_for_check(pool, &subject, &payment_intent_id).await {
        Ok(Some(attempt)) => attempt,
        Ok(None) => {
            return not_found(ctx, "payment attempt was not found for this payment intent");
        }
        Err(error) => return map_service_error(ctx, error),
    };

    let terminal = matches!(
        attempt.status.as_str(),
        "succeeded" | "failed" | "closed" | "canceled" | "cancelled"
    );
    if terminal {
        return success_item(
            ctx,
            CheckAttemptStatusResult {
                payment_intent_id: payment_intent_id.clone(),
                attempt_id: attempt.attempt_id.clone(),
                provider_status: None,
                local_status: attempt.status.clone(),
                paid: attempt.status == "succeeded",
            },
        );
    }

    // Resolve the provider account bound to the attempt's channel so the PSP
    // query uses the same credentials as the original checkout.
    let account = match load_provider_account_for_attempt(pool, &subject, &attempt).await {
        Ok(account) => account,
        Err(error) => return map_service_error(ctx, error),
    };
    let registry = provider_registry_for_account(
        &EnvPaymentCredentialResolver::load(),
        account.as_ref().map(|account| account.binding(account.environment.clone())),
    );
    let Some(adapter) = registry.resolve(&attempt.provider_code) else {
        return validation(
            ctx,
            format!(
                "payment provider {} is not configured for this attempt; enable the provider account and retry",
                attempt.provider_code
            ),
        );
    };
    // Stripe queries by the native PaymentIntent id (`pi_...`); WeChat and
    // Alipay query by the merchant out-trade-no.
    let query_reference = if attempt.provider_code == "stripe" {
        attempt
            .provider_transaction_id
            .clone()
            .unwrap_or_else(|| attempt.out_trade_no.clone())
    } else {
        attempt.out_trade_no.clone()
    };
    let query_outcome = match adapter
        .query_payment_intent(PaymentQueryPaymentIntentRequest {
            payment_intent_id: Some(query_reference),
            metadata: json!({}),
        })
        .await
    {
        Ok(outcome) => outcome,
        Err(error) => {
            let message =
                sdkwork_contract_service::CommerceServiceError::from(error).message().to_string();
            return validation(
                ctx,
                format!(
                    "payment provider query failed: {message}; check the provider credentials and merchant configuration"
                ),
            );
        }
    };
    let raw_status = query_outcome.raw_status.clone();
    let Some(raw_status) = raw_status
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
    else {
        return success_item(
            ctx,
            CheckAttemptStatusResult {
                payment_intent_id,
                attempt_id: attempt.attempt_id.clone(),
                provider_status: raw_status,
                local_status: attempt.status.clone(),
                paid: false,
            },
        );
    };
    let target_status = map_provider_payment_status(&attempt.provider_code, &raw_status);
    let mut local_status = attempt.status.clone();
    if target_status.is_some() && target_status != Some(local_status.as_str()) {
        let identity = PaymentWebhookAttemptIdentity {
            payment_attempt_id: attempt.attempt_id.clone(),
            payment_intent_id: attempt.payment_intent_id.clone(),
            provider_code: attempt.provider_code.clone(),
            out_trade_no: attempt.out_trade_no.clone(),
            attempt_status: attempt.status.clone(),
            tenant_id: attempt.tenant_id.clone(),
            organization_id: attempt.organization_id.clone(),
            owner_user_id: attempt.owner_user_id.clone(),
            order_id: attempt.order_id.clone(),
        };
        let mut tx = match pool.begin().await {
            Ok(tx) => tx,
            Err(error) => return map_service_error(ctx, storage(error.to_string())),
        };
        let applied = match apply_webhook_payment_status_postgres(
            &mut tx,
            &identity,
            Some(&raw_status),
            &now_string(),
        )
        .await
        {
            Ok(applied) => applied,
            Err(error) => {
                // A terminal local state (e.g. already closed) is a normal
                // outcome, not a failure: report the current state.
                let _ = tx.rollback().await;
                return success_item(
                    ctx,
                    CheckAttemptStatusResult {
                        payment_intent_id,
                        attempt_id: attempt.attempt_id.clone(),
                        provider_status: Some(raw_status),
                        local_status: local_status.clone(),
                        paid: local_status == "succeeded",
                    },
                );
            }
        };
        if let Err(error) = tx.commit().await {
            return map_service_error(ctx, storage(error.to_string()));
        }
        if let Some(applied) = applied {
            local_status = applied;
        }
    }
    success_item(
        ctx,
        CheckAttemptStatusResult {
            payment_intent_id,
            attempt_id: attempt.attempt_id,
            provider_status: Some(raw_status),
            local_status: local_status.clone(),
            paid: local_status == "succeeded",
        },
    )
}

struct TestAttemptForCheck {
    attempt_id: String,
    payment_intent_id: String,
    provider_code: String,
    out_trade_no: String,
    provider_transaction_id: Option<String>,
    status: String,
    channel_id: Option<String>,
    tenant_id: String,
    organization_id: Option<String>,
    owner_user_id: String,
    order_id: String,
}

async fn load_test_attempt_for_check(
    pool: &PgPool,
    subject: &AppRuntimeSubject,
    payment_intent_id: &str,
) -> Result<Option<TestAttemptForCheck>, sdkwork_contract_service::CommerceServiceError> {
    let organization_id = subject
        .organization_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("0");
    let row = sqlx::query(
        "SELECT id, payment_intent_id, provider_code, out_trade_no,                 COALESCE(callback_payload->>'providerTransactionId', callback_payload->>'provider_transaction_id') AS provider_transaction_id,                 status, channel_id, tenant_id, organization_id, owner_user_id, order_id \
         FROM commerce_payment_attempt \
         WHERE tenant_id = CAST($1 AS TEXT) \
           AND ((organization_id = CAST($2 AS TEXT)) OR (organization_id IS NULL AND $2::text IS NULL) OR (organization_id = '0' AND $2::text IS NULL)) \
           AND owner_user_id = CAST($3 AS TEXT) \
           AND payment_intent_id = CAST($4 AS TEXT) AND deleted_at IS NULL \
         ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(&subject.tenant_id)
    .bind(organization_id)
    .bind(&subject.user_id)
    .bind(payment_intent_id)
    .fetch_optional(pool)
    .await
    .map_err(storage)?;
    Ok(row.map(|row| TestAttemptForCheck {
        attempt_id: row.try_get("id").unwrap_or_default(),
        payment_intent_id: row.try_get("payment_intent_id").unwrap_or_default(),
        provider_code: row.try_get("provider_code").unwrap_or_default(),
        out_trade_no: row.try_get("out_trade_no").unwrap_or_default(),
        provider_transaction_id: row.try_get("provider_transaction_id").ok().flatten(),
        status: row.try_get("status").unwrap_or_default(),
        channel_id: row.try_get("channel_id").ok().flatten(),
        tenant_id: row.try_get("tenant_id").unwrap_or_default(),
        organization_id: row.try_get("organization_id").ok().flatten(),
        owner_user_id: row.try_get("owner_user_id").unwrap_or_default(),
        order_id: row.try_get("order_id").unwrap_or_default(),
    }))
}

async fn load_provider_account_for_attempt(
    pool: &PgPool,
    subject: &AppRuntimeSubject,
    attempt: &TestAttemptForCheck,
) -> Result<Option<ProviderAccountRecord>, sdkwork_contract_service::CommerceServiceError>
{
    let Some(channel_id) = attempt.channel_id.as_deref() else {
        return Ok(None);
    };
    let organization_id = subject
        .organization_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("0");
    let row = sqlx::query(
        "SELECT a.id, a.provider_code, a.merchant_id, a.environment, a.secret_ref, \
                a.webhook_secret_ref, a.certificate_ref, a.primary_secret, a.webhook_secret, \
                a.certificate, a.metadata, a.status \
         FROM commerce_payment_channel c \
         INNER JOIN commerce_payment_provider_account a ON a.id = c.provider_account_id AND a.deleted_at IS NULL \
         WHERE c.id = CAST($1 AS TEXT) AND c.tenant_id = CAST($2 AS TEXT) AND c.deleted_at IS NULL \
         LIMIT 1",
    )
    .bind(channel_id)
    .bind(&attempt.tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(storage)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let status: String = row.try_get("status").unwrap_or_default();
    if status != "active" {
        return Ok(None);
    }
    Ok(Some(ProviderAccountRecord {
        id: row.try_get("id").unwrap_or_default(),
        provider_code: row.try_get("provider_code").unwrap_or_default(),
        merchant_id: row.try_get("merchant_id").ok().flatten(),
        environment: row.try_get("environment").unwrap_or_default(),
        secret_ref: row.try_get("secret_ref").unwrap_or_default(),
        webhook_secret_ref: row.try_get("webhook_secret_ref").ok().flatten(),
        certificate_ref: row.try_get("certificate_ref").ok().flatten(),
        primary_secret: row.try_get("primary_secret").ok().flatten(),
        webhook_secret: row.try_get("webhook_secret").ok().flatten(),
        certificate: row.try_get("certificate").ok().flatten(),
        metadata: row.try_get("metadata").ok().flatten().unwrap_or_else(|| json!({})),
    }))
}

async fn load_payment_method_for_test(
    pool: &IntegrationPool,
    subject: &AppRuntimeSubject,
    method_key: &str,
) -> Result<Option<TestPaymentMethodRecord>, sdkwork_contract_service::CommerceServiceError> {
    match pool {
        IntegrationPool::Postgres(pool) => {
            let row = sqlx::query(
                "SELECT method_key, provider_code, status FROM commerce_payment_method WHERE tenant_id = CAST($1 AS TEXT) AND (organization_id = CAST($2 AS TEXT) OR organization_id = '0') AND method_key = CAST($3 AS TEXT) AND deleted_at IS NULL LIMIT 1",
            )
            .bind(&subject.tenant_id)
            .bind(&subject.organization_id)
            .bind(method_key)
            .fetch_optional(pool)
            .await
            .map_err(storage)?;
            Ok(row.map(|row| TestPaymentMethodRecord {
                method_key: row.try_get("method_key").unwrap_or_default(),
                status: row.try_get("status").unwrap_or_default(),
            }))
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn insert_test_order(
    pool: &IntegrationPool,
    subject: &AppRuntimeSubject,
    order_id: &str,
    order_no: &str,
    amount: &str,
    currency_code: &str,
    now: &str,
    expires_at: &str,
) -> Result<(), sdkwork_contract_service::CommerceServiceError> {
    let breakdown_id = stable_id("test-payment-breakdown", order_id);
    // Platform rows persist the sentinel organization scope (`"0"`) so
    // tenant (personal) sessions never write NULL into the NOT NULL
    // `organization_id` column.
    let organization_id = subject
        .organization_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("0");
    match pool {
        IntegrationPool::Postgres(pool) => {
            sqlx::query(
                "INSERT INTO commerce_order (id, tenant_id, organization_id, owner_user_id, order_no, status, subject, currency_code, expired_at, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, 'pending_payment', $6, $7, CAST($8 AS TIMESTAMPTZ), CAST($9 AS TIMESTAMPTZ), CAST($10 AS TIMESTAMPTZ)) ON CONFLICT (id) DO UPDATE SET expired_at = EXCLUDED.expired_at, updated_at = EXCLUDED.updated_at",
            )
            .bind(order_id)
            .bind(&subject.tenant_id)
            .bind(organization_id)
            .bind(&subject.user_id)
            .bind(order_no)
            .bind(TEST_PAYMENT_ORDER_SUBJECT)
            .bind(currency_code)
            .bind(expires_at)
            .bind(now)
            .bind(now)
            .execute(pool)
            .await
            .map_err(storage)?;
            sqlx::query(
                "INSERT INTO commerce_order_amount_breakdown (id, tenant_id, organization_id, order_id, allocation_type, original_amount, discount_amount, payable_amount, currency_code, created_at) VALUES ($1, $2, $3, $4, 'order_total', $5, '0', $6, $7, CAST($8 AS TIMESTAMPTZ)) ON CONFLICT (id) DO NOTHING",
            )
            .bind(&breakdown_id)
            .bind(&subject.tenant_id)
            .bind(organization_id)
            .bind(order_id)
            .bind(amount)
            .bind(amount)
            .bind(currency_code)
            .bind(now)
            .execute(pool)
            .await
            .map_err(storage)?;
            Ok(())
        }
    }
}

/// Loads the stored order status and expiry for the test payment order so a
/// boundary conflict (e.g. "order is not pending payment") can be reported
/// with the actual stored values. Returns `None` when the order cannot be
/// read (then the original error is returned unchanged).
async fn load_test_order_diagnostic(
    pool: &IntegrationPool,
    subject: &AppRuntimeSubject,
    order_id: &str,
) -> Option<String> {
    let IntegrationPool::Postgres(pool) = pool else {
        return None;
    };
    let row = sqlx::query(
        "SELECT status, to_char(CAST(expired_at AS TIMESTAMPTZ) AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS expired_at FROM commerce_order WHERE tenant_id = CAST($1 AS TEXT) AND organization_id = CAST($2 AS TEXT) AND id = CAST($3 AS TEXT) LIMIT 1",
    )
    .bind(&subject.tenant_id)
    .bind(subject.organization_id.as_deref().unwrap_or("0"))
    .bind(order_id)
    .fetch_optional(pool)
    .await
    .ok()?;
    let status = row
        .as_ref()
        .and_then(|row| row.try_get::<String, _>("status").ok())
        .unwrap_or_default();
    let expired_at = row
        .as_ref()
        .and_then(|row| row.try_get::<Option<String>, _>("expired_at").ok())
        .flatten()
        .unwrap_or_else(|| "null".to_owned());
    Some(format!("order status={status}, expires_at={expired_at}"))
}

/// Loads the payment method availability state (method/channel/account) for
/// the test payment so an intent-creation conflict like "payment method is
/// unavailable" can report exactly which configuration link is missing.
async fn load_test_payment_method_diagnostic(
    pool: &IntegrationPool,
    subject: &AppRuntimeSubject,
    method_key: &str,
) -> Option<String> {
    let IntegrationPool::Postgres(pool) = pool else {
        return None;
    };
    let rows = sqlx::query(
        "SELECT m.status AS method_status, m.organization_id AS method_org, \
                c.id AS channel_id, c.status AS channel_status, \
                c.provider_account_id, a.status AS account_status \
         FROM commerce_payment_method m \
         LEFT JOIN commerce_payment_channel c \
           ON c.tenant_id = m.tenant_id \
          AND (c.method_id = m.id OR (c.method_id IS NULL AND LOWER(c.provider_code) = LOWER(m.provider_code))) \
          AND c.deleted_at IS NULL \
         LEFT JOIN commerce_payment_provider_account a \
           ON a.id = c.provider_account_id AND a.deleted_at IS NULL \
         WHERE m.tenant_id = CAST($1 AS TEXT) \
           AND (m.organization_id = CAST($2 AS TEXT) OR m.organization_id = '0') \
           AND m.method_key = CAST($3 AS TEXT) AND m.deleted_at IS NULL \
         ORDER BY m.id, c.id",
    )
    .bind(&subject.tenant_id)
    .bind(subject.organization_id.as_deref().unwrap_or("0"))
    .bind(method_key)
    .fetch_all(pool)
    .await
    .ok()?;
    if rows.is_empty() {
        return Some("method not found".to_owned());
    }
    let method_status = rows
        .first()
        .and_then(|row| row.try_get::<String, _>("method_status").ok())
        .unwrap_or_default();
    let method_org = rows
        .first()
        .and_then(|row| row.try_get::<Option<String>, _>("method_org").ok())
        .flatten()
        .unwrap_or_else(|| "null".to_owned());
    let channels = rows
        .iter()
        .filter_map(|row| {
            let channel_id = row.try_get::<Option<String>, _>("channel_id").ok().flatten()?;
            let channel_status = row
                .try_get::<String, _>("channel_status")
                .unwrap_or_default();
            let account_status = row
                .try_get::<Option<String>, _>("account_status")
                .ok()
                .flatten()
                .unwrap_or_else(|| "none".to_owned());
            let account_id = row
                .try_get::<Option<String>, _>("provider_account_id")
                .ok()
                .flatten()
                .unwrap_or_else(|| "-".to_owned());
            let guidance = match (channel_status.as_str(), account_status.as_str()) {
                ("active", "inactive") => format!(
                    " account {account_id} is inactive: enable it in Provider Accounts (edit → Test → Activate) with real or PSP sandbox credentials"
                ),
                ("active", "none") => format!(
                    " channel {channel_id} has no provider account bound: edit the channel and bind an active provider account"
                ),
                ("inactive", _) => format!(
                    " channel {channel_id} is inactive: enable it in Payment Channels"
                ),
                _ => String::new(),
            };
            Some(format!(
                "{channel_id}[{channel_status},account:{account_id}/{account_status}]{guidance}"
            ))
        })
        .collect::<Vec<_>>();
    let channels_text = if channels.is_empty() {
        "no channel".to_owned()
    } else {
        channels.join(",")
    };
    Some(format!(
        "method={method_key} status={method_status} org={method_org}; channels: {channels_text}"
    ))
}

/// Converts a decimal amount string ("12.50", "12", "0.01") to integer
/// smallest units (1250, 1200, 1) using string arithmetic only.
fn decimal_amount_minor_units(value: &str) -> Option<i64> {
    let (yuan_part, cents_part) = value.split_once('.').unwrap_or((value, ""));
    let yuan: i64 = yuan_part.parse().ok()?;
    let cents: i64 = match cents_part {
        "" => 0,
        single if single.len() == 1 => format!("{single}0").parse().ok()?,
        two => two[..2].parse().ok()?,
    };
    yuan.checked_mul(100)?.checked_add(cents)
}

/// Validates a money string against `^[0-9]+(\.[0-9]{1,2})?$` without regex.
fn is_money_amount(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes.len() > 32 {
        return false;
    }
    let mut index = 0;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    if index == 0 {
        return false;
    }
    if index < bytes.len() {
        if bytes[index] != b'.' {
            return false;
        }
        index += 1;
        let mut fraction_digits = 0;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            fraction_digits += 1;
            index += 1;
        }
        if fraction_digits == 0 || fraction_digits > 2 || index != bytes.len() {
            return false;
        }
    }
    true
}

async fn test_webhook_signature(
    State(state): State<IntegrationState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    headers: HeaderMap,
    Json(body): Json<WebhookSignatureTestBody>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match require_subject(runtime_context, ctx) {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    if let Err(response) = validate_command(ctx, &headers, "webhook-signature-test", &body) {
        return response;
    }
    let account =
        match load_provider_account(&state.pool, &subject, &body.provider_account_id).await {
            Ok(Some(account)) => account,
            Ok(None) => return not_found(ctx, "payment provider account was not found"),
            Err(error) => return map_service_error(ctx, error),
        };
    let registry = provider_registry_for_account(
        &EnvPaymentCredentialResolver::load(),
        Some(account.binding(account.environment.clone())),
    );
    let Some(adapter) = registry.resolve(&account.provider_code) else {
        return validation(
            ctx,
            "provider credentials could not initialize the payment adapter",
        );
    };
    let signature_header = body.signature_header.clone().unwrap_or_else(|| {
        match account.provider_code.as_str() {
            "stripe" => "stripe-signature",
            "wechat_pay" => "wechatpay-signature",
            _ => "signature",
        }
        .to_owned()
    });
    let mut provider_headers = vec![(signature_header, body.signature.clone())];
    if let Some(timestamp) = body.timestamp.clone() {
        provider_headers.push(("wechatpay-timestamp".to_owned(), timestamp));
    }
    if account.provider_code == "wechat_pay" {
        provider_headers.push((
            "wechatpay-nonce".to_owned(),
            "sdkwork-signature-test".to_owned(),
        ));
    }
    let payload = if account.provider_code == "alipay" && !body.payload.contains("sign=") {
        format!(
            "{}&sign={}",
            body.payload.trim_end_matches('&'),
            body.signature
        )
    } else {
        body.payload.clone()
    };
    let result = adapter
        .verify_webhook(PaymentVerifyWebhookRequest {
            headers: provider_headers,
            body: payload.into_bytes(),
            metadata: json!({"signatureTest": true}),
        })
        .await;
    let (ok, diagnostic) = match result {
        Ok(outcome) => (
            outcome.verified,
            if outcome.verified {
                "Webhook signature verified successfully."
            } else {
                "Webhook signature did not verify."
            },
        ),
        Err(_) => (
            false,
            "Webhook signature verification could not be completed.",
        ),
    };
    success_item(
        ctx,
        json!({
            "ok": ok,
            "providerCode": account.provider_code,
            "algorithm": provider_algorithm(&account.provider_code),
            "diagnostic": diagnostic,
            "testedAt": now_string(),
        }),
    )
}

#[derive(Clone, Copy)]
enum ResourceKind {
    SubMerchant,
    Certificate,
}

async fn retrieve_resource(
    state: IntegrationState,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    id: String,
    kind: ResourceKind,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match require_subject(runtime_context, ctx) {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    match load_resource(&state.pool, &subject, &id, kind).await {
        Ok(Some(item)) => success_item(ctx, item),
        Ok(None) => not_found(ctx, "payment resource was not found"),
        Err(error) => map_service_error(ctx, error),
    }
}

async fn delete_resource(
    state: IntegrationState,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    id: String,
    kind: ResourceKind,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match require_subject(runtime_context, ctx) {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    match soft_delete_resource(&state.pool, &subject, &id, kind).await {
        Ok(true) => success_no_content(ctx),
        Ok(false) => not_found(ctx, "payment resource was not found"),
        Err(error) => map_service_error(ctx, error),
    }
}

#[allow(clippy::result_large_err)]
fn require_subject(
    runtime_context: Option<Extension<IamAppContext>>,
    ctx: Option<&WebRequestContext>,
) -> Result<AppRuntimeSubject, Response> {
    backend_runtime_subject_from_extension(runtime_context)
        .map_err(|message| unauthorized(ctx, message))
}

#[allow(clippy::result_large_err)]
fn validate_command<T: Serialize>(
    ctx: Option<&WebRequestContext>,
    headers: &HeaderMap,
    scope: &str,
    body: &T,
) -> Result<crate::command_headers::AppWriteCommandHeaders, Response> {
    validate_write_payload(headers, scope, body, |key| format!("{scope}-{key}"))
        .map_err(|error| command_header_error(ctx, error))
}

fn command_header_error(
    ctx: Option<&WebRequestContext>,
    error: WriteCommandHeaderError,
) -> Response {
    match error {
        WriteCommandHeaderError::InvalidHeader(message) => validation(ctx, message),
    }
}

#[allow(clippy::result_large_err)]
fn required_text(
    value: &str,
    field: &str,
    ctx: Option<&WebRequestContext>,
) -> Result<String, Response> {
    let value = value.trim();
    if value.is_empty() {
        Err(validation(ctx, format!("{field} is required")))
    } else {
        Ok(value.to_owned())
    }
}

fn normalized(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[allow(clippy::result_large_err)]
fn validate_sub_merchant_provider(
    value: &str,
    ctx: Option<&WebRequestContext>,
) -> Result<(), Response> {
    if matches!(value, "stripe" | "alipay" | "wechat_pay") {
        Ok(())
    } else {
        Err(validation(
            ctx,
            "providerCode must be stripe, alipay, or wechat_pay",
        ))
    }
}

#[allow(clippy::result_large_err)]
fn validate_certificate_type(value: &str, ctx: Option<&WebRequestContext>) -> Result<(), Response> {
    if matches!(
        value,
        "merchant_private_key" | "provider_public_key" | "platform_certificate" | "webhook_secret"
    ) {
        Ok(())
    } else {
        Err(validation(ctx, "certificateType is invalid"))
    }
}

fn stable_id(prefix: &str, value: &str) -> String {
    let normalized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    format!("{prefix}-{normalized}")
}

fn now_string() -> String {
    sqlx::types::chrono::Utc::now().to_rfc3339()
}

fn now_plus_minutes(minutes: i64) -> String {
    sdkwork_utils_rust::format_datetime(
        sdkwork_utils_rust::add_minutes(sdkwork_utils_rust::now(), minutes),
        None,
    )
}

fn provider_algorithm(provider_code: &str) -> &'static str {
    match provider_code {
        "stripe" => "HMAC-SHA256",
        "alipay" | "wechat_pay" => "RSA-SHA256",
        _ => "unknown",
    }
}

fn certificate_kind(certificate_type: &str) -> &'static str {
    match certificate_type {
        "merchant_private_key" => "private",
        "platform_certificate" => "platform",
        "webhook_secret" => "root",
        _ => "public",
    }
}

fn certificate_type(kind: &str) -> &'static str {
    match kind {
        "private" => "merchant_private_key",
        "platform" => "platform_certificate",
        "root" => "webhook_secret",
        _ => "provider_public_key",
    }
}

fn parse_json(value: Option<String>) -> Value {
    value
        .and_then(|value| serde_json::from_str(&value).ok())
        .unwrap_or_else(|| json!({}))
}

fn storage(error: impl std::fmt::Display) -> sdkwork_contract_service::CommerceServiceError {
    sdkwork_contract_service::CommerceServiceError::storage(error.to_string())
}

async fn load_provider_account(
    pool: &IntegrationPool,
    subject: &AppRuntimeSubject,
    id: &str,
) -> Result<Option<ProviderAccountRecord>, sdkwork_contract_service::CommerceServiceError> {
    match pool {
        IntegrationPool::Postgres(pool) => {
            let row = sqlx::query(
                "SELECT id, provider_code, merchant_id, environment, secret_ref, webhook_secret_ref, certificate_ref, CAST(metadata AS TEXT) AS metadata FROM commerce_payment_provider_account WHERE id = CAST($1 AS TEXT) AND tenant_id = CAST($2 AS TEXT) AND ((organization_id = CAST($3 AS TEXT)) OR (organization_id IS NULL AND $3 IS NULL) OR (organization_id = '0' AND $3 IS NULL)) AND deleted_at IS NULL LIMIT 1",
            )
            .bind(id)
            .bind(&subject.tenant_id)
            .bind(&subject.organization_id)
            .fetch_optional(pool)
            .await
            .map_err(storage)?;
            match row {
                Some(row) => {
                    let mut account = map_provider_account_pg(row);
                    if uses_database_credentials(&account) {
                        let credentials =
                            sdkwork_payment_repository_sqlx::load_provider_credentials_postgres(
                                pool,
                                &subject.tenant_id,
                                subject.organization_id.as_deref(),
                                id,
                            )
                            .await?;
                        account.primary_secret = credentials.primary_secret;
                        account.webhook_secret = credentials.webhook_secret;
                        account.certificate = credentials.certificate;
                    }
                    Ok(Some(account))
                }
                None => Ok(None),
            }
        }
    }
}

fn map_provider_account_pg(row: PgRow) -> ProviderAccountRecord {
    ProviderAccountRecord {
        id: row.try_get("id").unwrap_or_default(),
        provider_code: row.try_get("provider_code").unwrap_or_default(),
        merchant_id: row.try_get("merchant_id").ok().flatten(),
        environment: row.try_get("environment").unwrap_or_default(),
        secret_ref: row.try_get("secret_ref").unwrap_or_default(),
        webhook_secret_ref: row.try_get("webhook_secret_ref").ok().flatten(),
        certificate_ref: row.try_get("certificate_ref").ok().flatten(),
        primary_secret: None,
        webhook_secret: None,
        certificate: None,
        metadata: parse_json(row.try_get("metadata").ok()),
    }
}

fn uses_database_credentials(account: &ProviderAccountRecord) -> bool {
    account.secret_ref.starts_with("database:")
        || account
            .webhook_secret_ref
            .as_deref()
            .is_some_and(|value| value.starts_with("database:"))
        || account
            .certificate_ref
            .as_deref()
            .is_some_and(|value| value.starts_with("database:"))
}

async fn update_provider_test_status(
    pool: &IntegrationPool,
    subject: &AppRuntimeSubject,
    id: &str,
    tested_at: &str,
    status: &str,
) -> Result<(), sdkwork_contract_service::CommerceServiceError> {
    match pool {
        IntegrationPool::Postgres(pool) => {
            sqlx::query("UPDATE commerce_payment_provider_account SET last_tested_at = CAST($1 AS TIMESTAMPTZ), last_test_status = $2, updated_at = CAST($1 AS TIMESTAMPTZ) WHERE id = CAST($3 AS TEXT) AND tenant_id = CAST($4 AS TEXT) AND ((organization_id = CAST($5 AS TEXT)) OR (organization_id IS NULL AND $5 IS NULL) OR (organization_id = '0' AND $5 IS NULL)) AND deleted_at IS NULL")
                .bind(tested_at).bind(status).bind(id).bind(&subject.tenant_id).bind(&subject.organization_id)
                .execute(pool).await.map_err(storage)?;
        }
    }
    Ok(())
}

async fn rotate_credentials(
    pool: &IntegrationPool,
    subject: &AppRuntimeSubject,
    id: &str,
    primary_secret: String,
    webhook_secret: Option<String>,
    certificate: Option<String>,
    invalidate_previous: bool,
) -> Result<Option<Value>, sdkwork_contract_service::CommerceServiceError> {
    let rotated_at = now_string();
    let metadata_patch = json!({"previousCredentialsInvalidated": invalidate_previous, "credentialsRotatedAt": rotated_at});
    match pool {
        IntegrationPool::Postgres(pool) => {
            sdkwork_payment_repository_sqlx::rotate_provider_credentials_postgres(
                pool,
                &subject.tenant_id,
                subject.organization_id.as_deref(),
                id,
                sdkwork_payment_repository_sqlx::ProviderCredentialWrite {
                    primary_secret: Some(primary_secret),
                    webhook_secret,
                    certificate,
                },
            )
            .await?;
            sqlx::query("UPDATE commerce_payment_provider_account SET metadata = COALESCE(metadata, '{}'::jsonb) || CAST($1 AS JSONB), last_tested_at = NULL, last_test_status = NULL, updated_at = CAST($2 AS TIMESTAMPTZ) WHERE id = CAST($3 AS TEXT) AND tenant_id = CAST($4 AS TEXT) AND ((organization_id = CAST($5 AS TEXT)) OR (organization_id IS NULL AND $5 IS NULL) OR (organization_id = '0' AND $5 IS NULL)) AND deleted_at IS NULL")
                .bind(metadata_patch.to_string()).bind(&rotated_at).bind(id).bind(&subject.tenant_id).bind(&subject.organization_id)
                .execute(pool).await.map_err(storage)?;
        }
    }
    load_provider_account(pool, subject, id)
        .await
        .map(|item| item.map(provider_account_json))
}

fn provider_account_json(account: ProviderAccountRecord) -> Value {
    json!({
        "id": account.id,
        "providerCode": account.provider_code,
        "merchantId": account.merchant_id,
        "environment": account.environment,
        "hasPrimarySecret": account.primary_secret.is_some() || !account.secret_ref.trim().is_empty(),
        "hasWebhookSecret": account.webhook_secret.is_some() || account.webhook_secret_ref.as_deref().is_some_and(|value| !value.trim().is_empty()),
        "hasCertificate": account.certificate.is_some() || account.certificate_ref.as_deref().is_some_and(|value| !value.trim().is_empty()),
        "credentialStorage": if account.secret_ref.starts_with("database:") { "database_encrypted" } else { "legacy_reference" },
        "metadata": account.metadata,
    })
}

async fn query_sub_merchants(
    pool: &IntegrationPool,
    subject: &AppRuntimeSubject,
    query: &SubMerchantListQuery,
    page: OffsetListPageParams,
) -> Result<(Vec<Value>, i64), sdkwork_contract_service::CommerceServiceError> {
    match pool {
        IntegrationPool::Postgres(pool) => {
            let rows = sqlx::query("SELECT sm.id, sm.provider_account_id, sm.external_sub_merchant_id, sm.sub_appid, sm.sub_mch_id, sm.display_name, sm.status, CAST(sm.metadata AS TEXT) AS metadata, CAST(sm.created_at AS TEXT) AS created_at, CAST(sm.updated_at AS TEXT) AS updated_at, pa.provider_code, COUNT(*) OVER() AS total_count FROM commerce_payment_sub_merchant sm JOIN commerce_payment_provider_account pa ON pa.id = sm.provider_account_id AND pa.tenant_id = sm.tenant_id WHERE sm.tenant_id = CAST($1 AS TEXT) AND sm.organization_id = CAST($2 AS TEXT) AND sm.deleted_at IS NULL AND ($3 IS NULL OR sm.provider_account_id = CAST($3 AS TEXT)) AND ($4 IS NULL OR sm.status = CAST($4 AS TEXT)) AND ($5 IS NULL OR sm.external_sub_merchant_id ILIKE '%' || CAST($5 AS TEXT) || '%' OR COALESCE(sm.display_name, '') ILIKE '%' || CAST($5 AS TEXT) || '%') ORDER BY sm.updated_at DESC, sm.id DESC LIMIT $6 OFFSET $7")
                .bind(&subject.tenant_id).bind(&subject.organization_id).bind(&query.provider_account_id).bind(&query.status).bind(&query.q).bind(page.page_size).bind(page.offset)
                .fetch_all(pool).await.map_err(storage)?;
            let total = pg_total(&rows);
            Ok((rows.into_iter().map(map_sub_merchant_pg).collect(), total))
        }
    }
}

fn map_sub_merchant_pg(row: PgRow) -> Value {
    map_sub_merchant(&row, true)
}

fn map_sub_merchant<R: Row>(row: &R, _postgres: bool) -> Value
where
    for<'c> &'c str: sqlx::ColumnIndex<R>,
    String: for<'r> sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
{
    let text = |name: &str| row.try_get::<String, _>(name).unwrap_or_default();
    let optional = |name: &str| row.try_get::<Option<String>, _>(name).ok().flatten();
    let provider = text("provider_code");
    let external = text("external_sub_merchant_id");
    json!({
        "id": text("id"), "providerAccountId": text("provider_account_id"),
        "subMerchantNo": external,
        "subMerchantName": optional("display_name"), "subAppId": optional("sub_appid"),
        "subMchId": optional("sub_mch_id"),
        "stripeConnectedAccountId": if provider == "stripe" { Some(text("external_sub_merchant_id")) } else { None },
        "providerCode": provider, "status": text("status"),
        "metadata": parse_json(optional("metadata")), "createdAt": text("created_at"), "updatedAt": text("updated_at")
    })
}

async fn insert_sub_merchant(
    pool: &IntegrationPool,
    subject: &AppRuntimeSubject,
    id: &str,
    provider_account_id: &str,
    sub_merchant_no: &str,
    body: &CreateSubMerchantBody,
) -> Result<Option<Value>, sdkwork_contract_service::CommerceServiceError> {
    let account = load_provider_account(pool, subject, provider_account_id).await?;
    let Some(account) = account else {
        return Ok(None);
    };
    if account.provider_code != body.provider_code {
        return Err(sdkwork_contract_service::CommerceServiceError::validation(
            "providerCode must match the parent provider account",
        ));
    }
    let external_id = body
        .stripe_connected_account_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(sub_merchant_no);
    let now = now_string();
    let metadata = body
        .metadata
        .clone()
        .unwrap_or_else(|| json!({}))
        .to_string();
    let status = body.status.as_deref().unwrap_or("active");
    // Platform rows persist the sentinel organization scope (`"0"`) so
    // tenant (personal) sessions never write NULL into the NOT NULL
    // `organization_id` column.
    let organization_id = subject
        .organization_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("0");
    match pool {
        IntegrationPool::Postgres(pool) => {
            sqlx::query("INSERT INTO commerce_payment_sub_merchant (id, tenant_id, organization_id, provider_account_id, external_sub_merchant_id, sub_appid, sub_mch_id, display_name, status, metadata, created_at, updated_at) VALUES (CAST($1 AS TEXT), CAST($2 AS TEXT), CAST($3 AS TEXT), CAST($4 AS TEXT), $5, $6, $7, $8, $9, CAST($10 AS JSONB), CAST($11 AS TIMESTAMPTZ), CAST($11 AS TIMESTAMPTZ)) ON CONFLICT(id) DO NOTHING")
                .bind(id).bind(&subject.tenant_id).bind(organization_id).bind(provider_account_id).bind(external_id).bind(&body.sub_app_id).bind(&body.sub_mch_id).bind(&body.sub_merchant_name).bind(status).bind(&metadata).bind(&now)
                .execute(pool).await.map_err(storage)?;
        }
    }
    let item = load_resource(pool, subject, id, ResourceKind::SubMerchant).await?;
    let Some(item) = item else {
        return Ok(None);
    };
    let replay_matches = item["providerAccountId"] == provider_account_id
        && item["subMerchantNo"] == external_id
        && item["providerCode"] == body.provider_code
        && item["status"] == status;
    if !replay_matches {
        return Err(sdkwork_contract_service::CommerceServiceError::conflict(
            "Idempotency-Key was already used with a different sub-merchant payload",
        ));
    }
    Ok(Some(item))
}

async fn patch_sub_merchant(
    pool: &IntegrationPool,
    subject: &AppRuntimeSubject,
    id: &str,
    body: &UpdateSubMerchantBody,
) -> Result<Option<Value>, sdkwork_contract_service::CommerceServiceError> {
    let now = now_string();
    let metadata = body.metadata.as_ref().map(Value::to_string);
    match pool {
        IntegrationPool::Postgres(pool) => {
            let result = sqlx::query("UPDATE commerce_payment_sub_merchant SET display_name = COALESCE($1, display_name), sub_appid = COALESCE($2, sub_appid), sub_mch_id = COALESCE($3, sub_mch_id), external_sub_merchant_id = COALESCE($4, external_sub_merchant_id), status = COALESCE($5, status), metadata = COALESCE(CAST($6 AS JSONB), metadata), version = version + 1, updated_at = CAST($7 AS TIMESTAMPTZ) WHERE id = CAST($8 AS TEXT) AND tenant_id = CAST($9 AS TEXT) AND organization_id = CAST($10 AS TEXT) AND deleted_at IS NULL")
                .bind(&body.sub_merchant_name).bind(&body.sub_app_id).bind(&body.sub_mch_id).bind(&body.stripe_connected_account_id).bind(&body.status).bind(&metadata).bind(&now).bind(id).bind(&subject.tenant_id).bind(&subject.organization_id)
                .execute(pool).await.map_err(storage)?;
            if result.rows_affected() == 0 {
                return Ok(None);
            }
        }
    }
    load_resource(pool, subject, id, ResourceKind::SubMerchant).await
}

async fn query_certificates(
    pool: &IntegrationPool,
    subject: &AppRuntimeSubject,
    query: &CertificateListQuery,
    page: OffsetListPageParams,
) -> Result<(Vec<Value>, i64), sdkwork_contract_service::CommerceServiceError> {
    let kind = query.certificate_type.as_deref().map(certificate_kind);
    match pool {
        IntegrationPool::Postgres(pool) => {
            let rows = sqlx::query("SELECT id, certificate_no, provider_code, kind, content_ref, fingerprint_sha256, CAST(valid_until AS TEXT) AS valid_until, issuer_cn, subject_cn, status, CAST(metadata AS TEXT) AS metadata, CAST(created_at AS TEXT) AS created_at, CAST(updated_at AS TEXT) AS updated_at, COUNT(*) OVER() AS total_count FROM commerce_payment_certificate WHERE tenant_id = CAST($1 AS TEXT) AND organization_id = CAST($2 AS TEXT) AND deleted_at IS NULL AND ($3 IS NULL OR provider_code = CAST($3 AS TEXT)) AND ($4 IS NULL OR kind = CAST($4 AS TEXT)) AND ($5 IS NULL OR certificate_no ILIKE '%' || CAST($5 AS TEXT) || '%' OR COALESCE(subject_cn, '') ILIKE '%' || CAST($5 AS TEXT) || '%') AND ($6 IS NULL OR valid_until IS NULL OR valid_until <= NOW() + (CAST($6 AS TEXT) || ' days')::interval) ORDER BY updated_at DESC, id DESC LIMIT $7 OFFSET $8")
                .bind(&subject.tenant_id).bind(&subject.organization_id).bind(&query.provider_code).bind(kind).bind(&query.q).bind(query.expiring_within_days).bind(page.page_size).bind(page.offset)
                .fetch_all(pool).await.map_err(storage)?;
            let total = pg_total(&rows);
            Ok((rows.into_iter().map(map_certificate_pg).collect(), total))
        }
    }
}

fn map_certificate_pg(row: PgRow) -> Value {
    map_certificate(&row)
}

fn map_certificate<R: Row>(row: &R) -> Value
where
    for<'c> &'c str: sqlx::ColumnIndex<R>,
    String: for<'r> sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
{
    let text = |name: &str| row.try_get::<String, _>(name).unwrap_or_default();
    let optional = |name: &str| row.try_get::<Option<String>, _>(name).ok().flatten();
    let kind = text("kind");
    let status = text("status");
    json!({
        "id": text("id"), "certificateNo": text("certificate_no"), "providerCode": text("provider_code"),
        "certificateType": certificate_type(&kind),
        "hasContent": text("content_ref").starts_with("database:"),
        "credentialStorage": if text("content_ref").starts_with("database:") { "database_encrypted" } else { "legacy_reference" },
        "fingerprint": optional("fingerprint_sha256"), "expiresAt": optional("valid_until"),
        "issuer": optional("issuer_cn"), "subject": optional("subject_cn"),
        "status": if status == "pending" { "pending_rotation" } else { status.as_str() },
        "metadata": parse_json(optional("metadata")), "createdAt": text("created_at"), "updatedAt": text("updated_at")
    })
}

async fn insert_certificate(
    pool: &IntegrationPool,
    subject: &AppRuntimeSubject,
    id: &str,
    certificate_no: &str,
    encrypted: &EncryptedPaymentCredential,
    body: &CreateCertificateBody,
) -> Result<Value, sdkwork_contract_service::CommerceServiceError> {
    let now = now_string();
    let metadata = body
        .metadata
        .clone()
        .unwrap_or_else(|| json!({}))
        .to_string();
    let kind = certificate_kind(&body.certificate_type);
    // Platform rows persist the sentinel organization scope (`"0"`) so
    // tenant (personal) sessions never write NULL into the NOT NULL
    // `organization_id` column.
    let organization_id = subject
        .organization_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("0");
    match pool {
        IntegrationPool::Postgres(pool) => {
            sqlx::query("INSERT INTO commerce_payment_certificate (id, tenant_id, organization_id, certificate_no, provider_code, kind, fingerprint_sha256, content_ref, ciphertext, encryption_key_id, encryption_algorithm, status, metadata, created_at, updated_at) VALUES (CAST($1 AS TEXT), CAST($2 AS TEXT), CAST($3 AS TEXT), $4, $5, $6, $7, 'database:certificate_inventory', $8, $9, $10, 'active', CAST($11 AS JSONB), CAST($12 AS TIMESTAMPTZ), CAST($12 AS TIMESTAMPTZ)) ON CONFLICT(id) DO NOTHING")
                .bind(id).bind(&subject.tenant_id).bind(organization_id).bind(certificate_no).bind(&body.provider_code).bind(kind).bind(&encrypted.fingerprint_sha256).bind(&encrypted.ciphertext).bind(&encrypted.encryption_key_id).bind(&encrypted.encryption_algorithm).bind(&metadata).bind(&now)
                .execute(pool).await.map_err(storage)?;
        }
    }
    let item = load_resource(pool, subject, id, ResourceKind::Certificate)
        .await?
        .ok_or_else(|| {
            sdkwork_contract_service::CommerceServiceError::storage(
                "created certificate could not be reloaded",
            )
        })?;
    let replay_matches = item["certificateNo"] == certificate_no
        && item["providerCode"] == body.provider_code
        && item["certificateType"] == body.certificate_type
        && item["fingerprint"] == encrypted.fingerprint_sha256;
    if !replay_matches {
        return Err(sdkwork_contract_service::CommerceServiceError::conflict(
            "Idempotency-Key was already used with a different certificate payload",
        ));
    }
    Ok(item)
}

async fn load_resource(
    pool: &IntegrationPool,
    subject: &AppRuntimeSubject,
    id: &str,
    kind: ResourceKind,
) -> Result<Option<Value>, sdkwork_contract_service::CommerceServiceError> {
    let (table, columns) = match kind {
        ResourceKind::SubMerchant => ("commerce_payment_sub_merchant", "id, provider_account_id, external_sub_merchant_id, sub_appid, sub_mch_id, display_name, status, metadata, created_at, updated_at, (SELECT provider_code FROM commerce_payment_provider_account pa WHERE pa.id = provider_account_id AND pa.tenant_id = commerce_payment_sub_merchant.tenant_id LIMIT 1) AS provider_code"),
        ResourceKind::Certificate => ("commerce_payment_certificate", "id, certificate_no, provider_code, kind, content_ref, fingerprint_sha256, valid_until, issuer_cn, subject_cn, status, metadata, created_at, updated_at"),
    };
    match pool {
        IntegrationPool::Postgres(pool) => {
            let columns = match kind {
                ResourceKind::SubMerchant => "id, provider_account_id, external_sub_merchant_id, sub_appid, sub_mch_id, display_name, status, CAST(metadata AS TEXT) AS metadata, CAST(created_at AS TEXT) AS created_at, CAST(updated_at AS TEXT) AS updated_at, (SELECT provider_code FROM commerce_payment_provider_account pa WHERE pa.id = provider_account_id AND pa.tenant_id = commerce_payment_sub_merchant.tenant_id LIMIT 1) AS provider_code",
                ResourceKind::Certificate => "id, certificate_no, provider_code, kind, content_ref, fingerprint_sha256, CAST(valid_until AS TEXT) AS valid_until, issuer_cn, subject_cn, status, CAST(metadata AS TEXT) AS metadata, CAST(created_at AS TEXT) AS created_at, CAST(updated_at AS TEXT) AS updated_at",
            };
            let sql = format!("SELECT {columns} FROM {table} WHERE id = CAST($1 AS TEXT) AND tenant_id = CAST($2 AS TEXT) AND organization_id = CAST($3 AS TEXT) AND deleted_at IS NULL LIMIT 1");
            let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                .bind(id)
                .bind(&subject.tenant_id)
                .bind(&subject.organization_id)
                .fetch_optional(pool)
                .await
                .map_err(storage)?;
            Ok(row.map(|row| match kind {
                ResourceKind::SubMerchant => map_sub_merchant_without_provider_pg(row),
                ResourceKind::Certificate => map_certificate_pg(row),
            }))
        }
    }
}

fn map_sub_merchant_without_provider_pg(row: PgRow) -> Value {
    map_sub_merchant_without_provider(&row)
}

fn map_sub_merchant_without_provider<R: Row>(row: &R) -> Value
where
    for<'c> &'c str: sqlx::ColumnIndex<R>,
    String: for<'r> sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
{
    let text = |name: &str| row.try_get::<String, _>(name).unwrap_or_default();
    let optional = |name: &str| row.try_get::<Option<String>, _>(name).ok().flatten();
    let provider_code = text("provider_code");
    json!({
        "id": text("id"), "providerAccountId": text("provider_account_id"),
        "subMerchantNo": text("external_sub_merchant_id"), "subMerchantName": optional("display_name"),
        "subAppId": optional("sub_appid"), "subMchId": optional("sub_mch_id"),
        "stripeConnectedAccountId": if provider_code == "stripe" { optional("external_sub_merchant_id") } else { None },
        "providerCode": provider_code, "status": text("status"),
        "metadata": parse_json(optional("metadata")), "createdAt": text("created_at"), "updatedAt": text("updated_at")
    })
}

async fn soft_delete_resource(
    pool: &IntegrationPool,
    subject: &AppRuntimeSubject,
    id: &str,
    kind: ResourceKind,
) -> Result<bool, sdkwork_contract_service::CommerceServiceError> {
    let table = match kind {
        ResourceKind::SubMerchant => "commerce_payment_sub_merchant",
        ResourceKind::Certificate => "commerce_payment_certificate",
    };
    let now = now_string();
    let affected = match pool {
        IntegrationPool::Postgres(pool) => {
            let sql = format!("UPDATE {table} SET deleted_at = CAST($1 AS TIMESTAMPTZ), updated_at = CAST($1 AS TIMESTAMPTZ), version = version + 1 WHERE id = CAST($2 AS TEXT) AND tenant_id = CAST($3 AS TEXT) AND organization_id = CAST($4 AS TEXT) AND deleted_at IS NULL");
            sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                .bind(&now)
                .bind(id)
                .bind(&subject.tenant_id)
                .bind(&subject.organization_id)
                .execute(pool)
                .await
                .map_err(storage)?
                .rows_affected()
        }
    };
    Ok(affected > 0)
}

fn pg_total(rows: &[PgRow]) -> i64 {
    rows.first()
        .and_then(|row| row.try_get("total_count").ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{
        create_test_payment_checkout, decimal_amount_minor_units, insert_test_order,
        now_plus_minutes, now_string, stable_id, AppRuntimeSubject, IntegrationPool,
        TEST_PAYMENT_ORDER_TTL_MINUTES,
    };
    use crate::command_headers::AppWriteCommandHeaders;

    #[test]
    fn decimal_amounts_convert_to_minor_units() {
        assert_eq!(decimal_amount_minor_units("0.01"), Some(1));
        assert_eq!(decimal_amount_minor_units("12.50"), Some(1250));
        assert_eq!(decimal_amount_minor_units("12"), Some(1200));
        assert_eq!(decimal_amount_minor_units("0.5"), Some(50));
        assert_eq!(decimal_amount_minor_units("100"), Some(10_000));
        assert_eq!(decimal_amount_minor_units("not-a-number"), None);
        assert_eq!(decimal_amount_minor_units("999999999999999999999999"), None);
    }

    #[tokio::test]
    async fn insert_test_order_accepts_rfc3339_timestamps_for_timestamptz_columns() {
        // Regression: `commerce_order_amount_breakdown.created_at` is
        // `TIMESTAMPTZ`; binding the RFC3339 `now` string without a cast makes
        // PostgreSQL reject the insert (storage error → 50001 on the
        // `/payments/dev/test_payments` endpoint).
        let Some(url) = std::env::var("SDKWORK_DATABASE_TEST_POSTGRES_URL").ok() else {
            eprintln!("SKIP: SDKWORK_DATABASE_TEST_POSTGRES_URL is not configured");
            return;
        };
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await
            .expect("postgres pool");
        let cleanup_pool = pool.clone();
        let pool = IntegrationPool::Postgres(pool);
        let subject = AppRuntimeSubject {
            tenant_id: "100001".to_owned(),
            organization_id: Some("0".to_owned()),
            user_id: "2".to_owned(),
        };
        let idempotency_key = format!("regression-test-payment-{}", sdkwork_utils_rust::uuid());
        let order_id = stable_id("test-payment-order", &idempotency_key);
        let order_no = format!("TP-{}", &order_id[order_id.len().saturating_sub(24)..]);
        insert_test_order(
            &pool,
            &subject,
            &order_id,
            &order_no,
            "0.01",
            "CNY",
            &now_string(),
            &now_plus_minutes(TEST_PAYMENT_ORDER_TTL_MINUTES),
        )
        .await
        .expect("test order insert must accept RFC3339 timestamps");
        // The checkout must advance past the order bootstrap: the only
        // failure allowed now is the environment-dependent channel eligibility
        // check (409), never a storage error (50001).
        let write = AppWriteCommandHeaders {
            idempotency_key,
            request_hash: "hash".to_owned(),
            request_no: "request-regression-checkout".to_owned(),
        };
        match create_test_payment_checkout(&pool, &subject, &order_id, "wechat_native", &write)
            .await
        {
            Ok(_) => eprintln!("test payment checkout succeeded"),
            Err(error) => {
                assert_eq!(
                    error.code(),
                    "conflict",
                    "checkout must not fail with a storage error after the order bootstrap: {error:?}"
                );
                eprintln!(
                    "test payment checkout stopped at environment-dependent eligibility check: {}",
                    error.message()
                );
            }
        }
        let _ = sqlx::query("DELETE FROM commerce_order_amount_breakdown WHERE order_id = $1")
            .bind(&order_id)
            .execute(&cleanup_pool)
            .await;
        let _ = sqlx::query("DELETE FROM commerce_order WHERE id = $1")
            .bind(&order_id)
            .execute(&cleanup_pool)
            .await;
    }

    #[test]
    fn test_order_expiry_is_future_rfc3339_parseable() {
        let now = sqlx::types::chrono::DateTime::parse_from_rfc3339(&now_string())
            .expect("now_string must be RFC3339")
            .with_timezone(&sqlx::types::chrono::Utc);
        let expires_at = now_plus_minutes(TEST_PAYMENT_ORDER_TTL_MINUTES);
        let parsed = sqlx::types::chrono::DateTime::parse_from_rfc3339(&expires_at)
            .expect("test order expiry must be RFC3339")
            .with_timezone(&sqlx::types::chrono::Utc);
        // Order-expiration enforcement parses the same RFC3339 form and
        // requires the boundary to be strictly in the future.
        let remaining = (parsed - now).num_seconds();
        assert!(
            remaining >= TEST_PAYMENT_ORDER_TTL_MINUTES * 60 - 5,
            "test order expiry must stay near the configured TTL, got {remaining}s"
        );
    }
}
