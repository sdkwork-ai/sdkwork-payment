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
    PaymentProviderRegistry, PaymentVerifyWebhookRequest, ProviderAccountBinding,
    ProviderCredentialBundle,
};
use sdkwork_payment_repository_sqlx::{
    enrich_owner_payment_attempt_postgres,
    OwnerOrderPaymentEnrichmentContext, PostgresCommercePaymentIntentStore,
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
    accepted_async_command, conflict, map_service_error, not_found, success_created_item,
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
    qr_code_url: Option<String>,
    expires_at: Option<String>,
    created_at: String,
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
    let mock = |value: Option<&str>| {
        value
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some_and(|value| value.starts_with("mock-") || value.starts_with("MOCK_"))
    };
    if account
        .metadata
        .get("configurationState")
        .and_then(Value::as_str)
        .is_some_and(|value| value.eq_ignore_ascii_case("mock"))
        || mock(account.merchant_id.as_deref())
        || mock(account.metadata.get("appId").and_then(Value::as_str))
        || mock(
            account
                .metadata
                .get("merchantSerialNo")
                .and_then(Value::as_str),
        )
    {
        issues.push("replace bootstrap mock identifiers".to_owned());
    }
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
    match insert_sandbox_webhook_event(
        &state.pool,
        &subject,
        &operation_id,
        &event_id,
        &account.provider_code,
        payload,
    )
    .await
    {
        Ok(()) => accepted_async_command(ctx, operation_id, "pending", None),
        Err(error) => map_service_error(ctx, error),
    }
}

// ===========================================================================
// One-cent test payment (dev config)
// ===========================================================================

/// QR-code-capable provider codes that support one-cent scan-to-pay testing.
const QR_TEST_PROVIDER_CODES: [&str; 3] = ["wechat_pay", "alipay", "sandbox"];
const TEST_PAYMENT_ORDER_SUBJECT: &str = "One-cent test payment";

struct TestPaymentMethodRecord {
    provider_code: String,
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
    ) -> Result<CreateOwnerPaymentAttemptOutcome, sdkwork_contract_service::CommerceServiceError> {
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
    let currency_code = normalized(body.currency_code).unwrap_or_else(|| "CNY".to_owned());
    if currency_code.len() != 3 {
        return validation(ctx, "currencyCode must be a 3-letter ISO currency code");
    }

    let method = match load_payment_method_for_test(&state.pool, &subject, &method_key).await {
        Ok(Some(method)) => method,
        Ok(None) => return not_found(ctx, "payment method was not found"),
        Err(error) => return map_service_error(ctx, error),
    };
    if method.status != "active" {
        return conflict(ctx, "payment method is not active");
    }
    if !QR_TEST_PROVIDER_CODES.contains(&method.provider_code.as_str()) {
        return validation(
            ctx,
            "payment method does not support QR-code scan-to-pay testing",
        );
    }

    // The test payment must hang off a payable order (the payment executor
    // reads the amount from the order), so an internal test order is created
    // first. `ON CONFLICT DO NOTHING` keeps idempotent retries stable.
    let order_id = stable_id("test-payment-order", &write.idempotency_key);
    let order_no = format!("TP-{}", &order_id[order_id.len().saturating_sub(24)..]);
    let now = now_string();
    if let Err(error) = insert_test_order(
        &state.pool,
        &subject,
        &order_id,
        &order_no,
        &amount,
        &currency_code,
        &now,
    )
    .await
    {
        return map_service_error(ctx, error);
    }

    let checkout = match create_test_payment_checkout(
        &state.pool,
        &subject,
        &order_id,
        &method_key,
        &write,
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(error) => return map_service_error(ctx, error),
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
    let registry = Arc::new(PaymentProviderRegistry::from_credentials(credentials.clone()));
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

async fn load_payment_method_for_test(
    pool: &IntegrationPool,
    subject: &AppRuntimeSubject,
    method_key: &str,
) -> Result<Option<TestPaymentMethodRecord>, sdkwork_contract_service::CommerceServiceError> {
    match pool {
        IntegrationPool::Postgres(pool) => {
            let row = sqlx::query(
                "SELECT method_key, provider_code, status FROM commerce_payment_method WHERE tenant_id = CAST($1 AS TEXT) AND ((organization_id = CAST($2 AS TEXT)) OR (organization_id IS NULL AND $2 IS NULL) OR (organization_id = '0' AND $2 IS NULL)) AND method_key = CAST($3 AS TEXT) AND deleted_at IS NULL LIMIT 1",
            )
            .bind(&subject.tenant_id)
            .bind(&subject.organization_id)
            .bind(method_key)
            .fetch_optional(pool)
            .await
            .map_err(storage)?;
            Ok(row.map(|row| TestPaymentMethodRecord {
                provider_code: row.try_get("provider_code").unwrap_or_default(),
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
) -> Result<(), sdkwork_contract_service::CommerceServiceError> {
    let breakdown_id = stable_id("test-payment-breakdown", order_id);
    match pool {
        IntegrationPool::Postgres(pool) => {
            sqlx::query(
                "INSERT INTO commerce_order (id, tenant_id, organization_id, owner_user_id, order_no, status, subject, currency_code, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, 'pending_payment', $6, $7, $8, $9) ON CONFLICT (id) DO NOTHING",
            )
            .bind(order_id)
            .bind(&subject.tenant_id)
            .bind(&subject.organization_id)
            .bind(&subject.user_id)
            .bind(order_no)
            .bind(TEST_PAYMENT_ORDER_SUBJECT)
            .bind(currency_code)
            .bind(now)
            .bind(now)
            .execute(pool)
            .await
            .map_err(storage)?;
            sqlx::query(
                "INSERT INTO commerce_order_amount_breakdown (id, tenant_id, organization_id, order_id, allocation_type, original_amount, discount_amount, payable_amount, currency_code, created_at) VALUES ($1, $2, $3, $4, 'order_total', $5, '0', $6, $7, $8) ON CONFLICT (id) DO NOTHING",
            )
            .bind(&breakdown_id)
            .bind(&subject.tenant_id)
            .bind(&subject.organization_id)
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
    match pool {
        IntegrationPool::Postgres(pool) => {
            sqlx::query("INSERT INTO commerce_payment_sub_merchant (id, tenant_id, organization_id, provider_account_id, external_sub_merchant_id, sub_appid, sub_mch_id, display_name, status, metadata, created_at, updated_at) VALUES (CAST($1 AS TEXT), CAST($2 AS TEXT), CAST($3 AS TEXT), CAST($4 AS TEXT), $5, $6, $7, $8, $9, CAST($10 AS JSONB), CAST($11 AS TIMESTAMPTZ), CAST($11 AS TIMESTAMPTZ)) ON CONFLICT(id) DO NOTHING")
                .bind(id).bind(&subject.tenant_id).bind(&subject.organization_id).bind(provider_account_id).bind(external_id).bind(&body.sub_app_id).bind(&body.sub_mch_id).bind(&body.sub_merchant_name).bind(status).bind(&metadata).bind(&now)
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
    match pool {
        IntegrationPool::Postgres(pool) => {
            sqlx::query("INSERT INTO commerce_payment_certificate (id, tenant_id, organization_id, certificate_no, provider_code, kind, fingerprint_sha256, content_ref, ciphertext, encryption_key_id, encryption_algorithm, status, metadata, created_at, updated_at) VALUES (CAST($1 AS TEXT), CAST($2 AS TEXT), CAST($3 AS TEXT), $4, $5, $6, $7, 'database:certificate_inventory', $8, $9, $10, 'active', CAST($11 AS JSONB), CAST($12 AS TIMESTAMPTZ), CAST($12 AS TIMESTAMPTZ)) ON CONFLICT(id) DO NOTHING")
                .bind(id).bind(&subject.tenant_id).bind(&subject.organization_id).bind(certificate_no).bind(&body.provider_code).bind(kind).bind(&encrypted.fingerprint_sha256).bind(&encrypted.ciphertext).bind(&encrypted.encryption_key_id).bind(&encrypted.encryption_algorithm).bind(&metadata).bind(&now)
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

async fn insert_sandbox_webhook_event(
    pool: &IntegrationPool,
    subject: &AppRuntimeSubject,
    operation_id: &str,
    event_id: &str,
    provider_code: &str,
    payload: Value,
) -> Result<(), sdkwork_contract_service::CommerceServiceError> {
    let now = now_string();
    match pool {
        IntegrationPool::Postgres(pool) => {
            sqlx::query("INSERT INTO commerce_payment_webhook_event (id, tenant_id, organization_id, event_id, event_type, provider_code, payload, status, received_at, created_at, updated_at) VALUES (CAST($1 AS TEXT), CAST($2 AS TEXT), CAST($3 AS TEXT), $4, 'sdkwork.sandbox.triggered', $5, CAST($6 AS JSONB), 'queued', CAST($7 AS TIMESTAMPTZ), CAST($7 AS TIMESTAMPTZ), CAST($7 AS TIMESTAMPTZ)) ON CONFLICT(id) DO NOTHING")
                .bind(operation_id).bind(&subject.tenant_id).bind(&subject.organization_id).bind(event_id).bind(provider_code).bind(payload.to_string()).bind(&now)
                .execute(pool).await.map_err(storage)?;
        }
    }
    let stored_payload = match pool {
        IntegrationPool::Postgres(pool) => {
            sqlx::query_scalar::<_, String>(
                "SELECT CAST(payload AS TEXT) FROM commerce_payment_webhook_event WHERE id = CAST($1 AS TEXT) AND tenant_id = CAST($2 AS TEXT) AND organization_id = CAST($3 AS TEXT) LIMIT 1",
            )
            .bind(operation_id)
            .bind(&subject.tenant_id)
            .bind(&subject.organization_id)
            .fetch_optional(pool)
            .await
            .map_err(storage)?
        }
    };
    let stored_payload = stored_payload
        .and_then(|value| serde_json::from_str::<Value>(&value).ok())
        .ok_or_else(|| {
            sdkwork_contract_service::CommerceServiceError::storage(
                "sandbox webhook event could not be reloaded",
            )
        })?;
    if stored_payload != payload {
        return Err(sdkwork_contract_service::CommerceServiceError::conflict(
            "Idempotency-Key was already used with a different sandbox event payload",
        ));
    }
    Ok(())
}


fn pg_total(rows: &[PgRow]) -> i64 {
    rows.first()
        .and_then(|row| row.try_get("total_count").ok())
        .unwrap_or(0)
}

