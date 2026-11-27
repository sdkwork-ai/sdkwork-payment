use std::sync::Arc;
use std::time::Instant;

use axum::extract::{Extension, Path, Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use sdkwork_iam_context_service::IamAppContext;
use sdkwork_payment_providers::{
    cancel_provider_payment, capture_provider_payment, payment_credential_cipher,
    provider_payment_awaits_capture, provider_registry_for_account, resolve_secret_ref,
    CredentialCipherScope, EncryptedPaymentCredential, EnvPaymentCredentialResolver,
    PaymentProviderRegistry, PaymentQueryPaymentIntentRequest, PaymentVerifyWebhookRequest,
    ProviderAccountBinding, ProviderCredentialBundle,
};use sdkwork_payment_repository_sqlx::{
    enrich_owner_payment_attempt_postgres,
    payment_attempt_context::PaymentWebhookAttemptIdentity,
    postgres_webhook_ingestion::{
        apply_webhook_payment_status_postgres, ingest_provider_webhook_postgres,
    },
    webhook_status::map_provider_payment_status,
    IngestProviderWebhookCommand, OwnerOrderPaymentEnrichmentContext,
    PostgresCommercePaymentIntentStore,
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
use tracing;

use crate::api_response::{
    conflict, forbidden, map_service_error, not_found, provider_rejected,
    sanitize_provider_error_message, success_created_item, success_item, success_list,
    success_no_content, unauthorized, validation,
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
    /// Unsigned sandbox webhook ingestion is a development affordance: a
    /// production-like deployment must never accept simulated settlement
    /// events, even behind authenticated admin routes.
    sandbox_trigger_enabled: bool,
}

/// Sandbox settlement injection is a development affordance and is enabled
/// only on explicitly non-production environments (dev, test); staging, prod,
/// and unknown environments stay fail-closed.
fn sandbox_trigger_enabled_for_environment() -> bool {
    matches!(
        sdkwork_payment_service_host::payment_runtime_environment(),
        sdkwork_web_core::WebEnvironment::Dev | sdkwork_web_core::WebEnvironment::Test
    )
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
    #[serde(default)]
    provider_code: Option<String>,
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
    /// Payer identity a product addresses directly instead of "whoever scans
    /// this": `openid` for `wechat_jsapi`, `client_ip` for `wechat_h5`,
    /// `buyer_id`/`buyer_open_id` for `alipay_jsapi`. Values are validated by
    /// the provider request itself; the server only requires the key when the
    /// product declares it, so the operator sees a configuration finding
    /// instead of an opaque provider rejection.
    payer_identity: Option<PayerIdentityBody>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PayerIdentityBody {
    openid: Option<String>,
    client_ip: Option<String>,
    buyer_id: Option<String>,
    buyer_open_id: Option<String>,
}

impl PayerIdentityBody {
    fn into_value(self) -> Value {
        let mut object = serde_json::Map::new();
        for (key, value) in [
            ("openid", self.openid),
            ("client_ip", self.client_ip),
            ("buyer_id", self.buyer_id),
            ("buyer_open_id", self.buyer_open_id),
        ] {
            if let Some(value) = normalized(value) {
                object.insert(key.to_owned(), Value::String(value));
            }
        }
        Value::Object(object)
    }

    fn has(&self, metadata_key: &str) -> bool {
        let value = match metadata_key {
            "openid" => self.openid.as_deref(),
            "client_ip" => self.client_ip.as_deref(),
            "buyer_id" => self
                .buyer_id
                .as_deref()
                .or(self.buyer_open_id.as_deref()),
            _ => None,
        };
        value.is_some_and(|value| !value.trim().is_empty())
    }
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
    /// Order-bound H5 cashier deep-link built from the configured H5 payment
    /// domain (支付设置 → 域名设置); falls back to the deployment origin.
    cashier_url: Option<String>,
    /// Web cashier redirect URL (`alipay_wap`/`alipay_pc` cashier link).
    pay_url: Option<String>,
    /// Full Alipay cashier form HTML for browser render + auto submit.
    pay_form: Option<String>,
    /// Stripe PaymentIntent client secret for Stripe.js card collection.
    client_secret: Option<String>,
    /// Stripe publishable key for Stripe.js (from deployment env or the
    /// provider account metadata).
    publishable_key: Option<String>,
    /// How the cashier must launch this payment, normalized across providers
    /// and products: `qr_code` (show the QR), `redirect` (open the pay URL),
    /// `form_redirect` (auto-submit the cashier form), `card` (Stripe.js card
    /// element), `jsapi` / `app` (client invocation payload), `local_sandbox`
    /// (no PSP, settle simulated) or `unsupported` (the channel returned
    /// nothing that can be launched — a configuration finding).
    launch_kind: String,
    /// Client invocation payload (`jsapiPayload` for wx.requestPayment,
    /// `appPayload` for WeChat PayReq, `orderStr` for the Alipay App SDK).
    launch_payload: Option<String>,
    /// `nextAction` exactly as the checkout normalizer recorded it, for
    /// operator diagnosis when the launch kind is projected.
    next_action: Option<String>,
    /// True when the operator can settle this test payment from the dialog
    /// (sandbox / client-payload products) instead of paying for real.
    settle_supported: bool,
    provider_param_keys: Vec<String>,
    /// Payer inputs the product's checkout cannot be created without
    /// (`openid` / `client_ip` / `buyer_id`), declared by the provider adapter.
    required_payer_fields: Vec<TestPaymentPayerField>,
    expires_at: Option<String>,
    created_at: String,
}

/// One payer input a test payment needs before the provider checkout exists.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TestPaymentPayerField {
    method_key: String,
    /// Metadata key the provider adapter reads (`openid`, `client_ip`, `buyer_id`).
    field: String,
    label: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CheckAttemptStatusBody {
    payment_intent_id: String,
    /// Settle a wallet-style or sandbox trade the payer has not paid yet (see
    /// [`TestPaymentSettleBody`]) as part of the same request. The settlement
    /// gate is re-checked here, so the flag can never grant a capability the
    /// dedicated endpoint would refuse.
    #[serde(default)]
    settle_if_unpaid: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct TestPaymentActionBody {
    payment_intent_id: String,
}

/// Settlement override for a one-cent test payment.
///
/// A one-cent test payment must prove the WHOLE chain — checkout, settlement,
/// webhook bookkeeping, refund — for every configured service provider. Two
/// channel classes cannot finish on their own from an admin browser session:
///
/// - the local `sandbox` provider has no PSP checkout at all, and
/// - wallet products (`wechat_jsapi`, `alipay_jsapi`, `alipay_app`,
///   `wechat_app`) hand the launcher to a client that needs a payer identity
///   (openid / buyer_id) or an installed app the operator may not have.
///
/// For those, an explicit operator action (button behind a confirmation —
/// never an automatic step) injects the settlement event through the same
/// ingest pipeline a PSP callback uses, so the refund path can be verified
/// instead of the test ending at "created".
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct TestPaymentSettleBody {
    payment_intent_id: String,
    /// `success` (default) settles the payment; `failed` marks it failed so
    /// the failure branch of the status machine can be exercised too.
    #[serde(default)]
    outcome: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TestPaymentSettleResult {
    payment_intent_id: String,
    attempt_id: String,
    out_trade_no: String,
    /// Attempt status before the simulated event.
    previous_status: String,
    /// Attempt status after the simulated event was applied.
    local_status: String,
    provider_code: String,
    applied_status: Option<String>,
    webhook_event_id: String,
    event_id: String,
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
    build_router(IntegrationState {
        pool,
        sandbox_trigger_enabled: sandbox_trigger_enabled_for_environment(),
    })
}

pub fn backend_payment_integration_router_with_postgres_pool(pool: PgPool) -> Router {
    build_router(IntegrationState {
        pool: IntegrationPool::Postgres(pool),
        sandbox_trigger_enabled: sandbox_trigger_enabled_for_environment(),
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
            "/backend/v3/api/payments/dev/test_payment_products",
            get(list_test_payment_products),
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
            "/backend/v3/api/payments/dev/settle_test_payment",
            post(settle_test_payment),
        )
        .route(
            "/backend/v3/api/payments/dev/cancel_test_payment",
            post(cancel_test_payment),
        )
        .route(
            "/backend/v3/api/payments/dev/refund_test_payment",
            post(refund_test_payment),
        )
        .route(
            "/backend/v3/api/payments/dev/notification_test",
            post(test_payment_notification),
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
            // 验签凭据（官方二选一）：微信支付公钥（默认/推荐）或平台证书。
            if account.certificate.is_none()
                && account
                    .certificate_ref
                    .as_deref()
                    .and_then(resolve_secret_ref)
                    .is_none()
            {
                issues.push(
                    "WeChat verification key (platform certificate or WeChat Pay public key) is not configured"
                        .to_owned(),
                );
            }
            match metadata_text(&account.metadata, "signVerifyMode").as_deref() {
                Some("platform_certificate") => {
                    if metadata_text(&account.metadata, "platformCertificateSerialNo").is_none() {
                        issues.push(
                            "metadata.platformCertificateSerialNo is required for WeChat Pay platform certificate mode"
                                .to_owned(),
                        );
                    }
                }
                Some("wechatpay_public_key") | None => {
                    if metadata_text(&account.metadata, "wechatpayPublicKeyId").is_none() {
                        issues.push(
                            "metadata.wechatpayPublicKeyId is required for WeChat Pay public key mode"
                                .to_owned(),
                        );
                    }
                }
                Some(other) => issues.push(format!(
                    "metadata.signVerifyMode must be platform_certificate or wechatpay_public_key, got {other}"
                )),
            }
            if account
                .metadata
                .get("notifyUrl")
                .and_then(Value::as_str)
                .is_none_or(|value| value.trim().is_empty())
                && credentials.provider_notify_url("wechat_pay").is_none()
            {
                issues.push(
                    "metadata.notifyUrl (or a configured default notify domain in the payment center) is required for WeChat Pay"
                        .to_owned(),
                );
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

fn metadata_text(metadata: &Value, key: &str) -> Option<String> {
    metadata
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
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
    if !state.sandbox_trigger_enabled {
        return forbidden(
            ctx,
            "sandbox event triggers are disabled on production-like deployments",
        );
    }
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
    // IntegrationPool carries exactly the authoritative Postgres pool; the
    // irrefutable bind documents that there is no second engine.
    let IntegrationPool::Postgres(pool) = &state.pool;
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

/// Payment method keys that can drive the one-cent test payment. Every product
/// a configured service provider can sell is reachable, which is what makes
/// "configure the provider, then exercise the whole payment function" true for
/// the whole catalogue instead of a hand-picked subset:
///
/// - scan-to-pay QR: `wechat_native` (WeChat `/v3/pay/transactions/native`
///   `code_url`), `alipay_qr` (Alipay `alipay.trade.precreate` `qr_code`);
/// - web cashier navigation: `alipay_wap` / `alipay_pc` (Alipay H5/PC cashier),
///   `wechat_h5` (WeChat `/v3/pay/transactions/h5` `h5_url`), `paypal`
///   (PayPal Orders approve link);
/// - client invocation payloads: `wechat_jsapi` / `wechat_app`,
///   `alipay_app` / `alipay_jsapi` — the launcher is rendered in the dialog,
///   and settlement is supported for them because a browser session cannot
///   present the payer identity or the installed app the launcher expects;
/// - card / wallet collection: `stripe_card`, `stripe_apple_pay`,
///   `stripe_google_pay`, `stripe_alipay`, `stripe_wechat_pay` (client secret
///   confirmed with Stripe.js);
/// - the local sandbox (`sandbox_test`) and the legacy `wechat_pay` alias that
///   the WeChat adapter maps onto Native.
const TEST_PAYMENT_METHOD_KEYS: [&str; 17] = [
    "wechat_native",
    "wechat_pay",
    "wechat_jsapi",
    "wechat_h5",
    "wechat_app",
    "alipay_qr",
    "alipay_wap",
    "alipay_pc",
    "alipay_app",
    "alipay_jsapi",
    "stripe_card",
    "stripe_apple_pay",
    "stripe_google_pay",
    "stripe_alipay",
    "stripe_wechat_pay",
    "paypal",
    // The local sandbox has no real PSP checkout; settlement is simulated
    // through the sandbox webhook trigger (no credentials needed).
    "sandbox_test",
];

/// Products that hand the payer a client invocation payload instead of a
/// pay-in-browser surface. They are still full one-cent tests — the checkout is
/// created for real and the launcher is rendered in the dialog — but settlement
/// can be simulated for them, because an admin browser session cannot present
/// the payer's openid / buyer_id or the installed app the payload targets.
const TEST_PAYMENT_CLIENT_ONLY_METHOD_KEYS: [&str; 4] = [
    "wechat_jsapi",
    "wechat_app",
    "alipay_app",
    "alipay_jsapi",
];

/// True when the settle override is allowed for the method: the local sandbox
/// performs no real settlement, and a client-payload product cannot be
/// completed from the dialog. Every card / QR / redirect / hosted-checkout
/// product moves real money, so its settlement must come from the provider.
fn test_payment_settle_supported(method_key: &str) -> bool {
    let method_key = method_key.trim().to_ascii_lowercase();
    method_key == "sandbox_test" || TEST_PAYMENT_CLIENT_ONLY_METHOD_KEYS.contains(&method_key.as_str())
}

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
    // Products that address a specific payer (`wechat_jsapi` openid,
    // `wechat_h5` client IP, `alipay_jsapi` buyer id) fail here, before an
    // order/intent exists, when the identity is missing. Otherwise the
    // provider rejection would only appear after the test order was written
    // and leave a half-created test behind.
    let payer_identity = body.payer_identity.clone().unwrap_or_default();
    let missing_payer_fields: Vec<&'static sdkwork_payment_providers::PaymentProductRequirement> =
        sdkwork_payment_providers::product_payer_requirements(&method.method_key)
            .iter()
            .filter(|requirement| !payer_identity.has(requirement.metadata_key))
            .collect();
    if !missing_payer_fields.is_empty() {
        let missing = missing_payer_fields
            .iter()
            .map(|requirement| {
                format!(
                    "{} ({})",
                    requirement.metadata_key,
                    requirement.label
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        return validation(
            ctx,
            format!(
                "payment method {} addresses a specific payer and needs: {missing}. Fill the payer identity fields in the test console and retry",
                method.method_key
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

    let checkout = match create_test_payment_checkout(
        &state.pool,
        &subject,
        &order_id,
        &method_key,
        &payer_identity.clone().into_value(),
        &write,
    )
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
                let message = sanitize_provider_error_message(error.message());
                // PSP authentication failures (e.g. WeChat `SIGN_ERROR`,
                // Stripe `Invalid API Key`) arrive as `HTTP 401` from the
                // PSP — NOT from this API. Report them with the dedicated
                // 41101 code (not 40001/401xx) and an HTTP 400 (not 401),
                // with the PSP's own HTTP status stripped from the detail,
                // so the admin session boundary never treats a
                // payment-gateway rejection as a login problem.
                let guidance = if message.contains("is not configured") {
                    "create an active provider account and channel for this method (or set the provider credentials), then retry"
                } else if message.contains("SIGN_ERROR") {
                    "the payment gateway rejected the request signature: the account is using auto-filled test credentials, not a real merchant key. Replace them with the real merchant API private key / certificate in Provider Accounts (edit → save), then retry — the request itself reached the PSP"
                } else if message.contains("Unauthorized")
                    || message.contains("Invalid API Key")
                    || message.contains("invalid api key")
                {
                    "the PSP rejected the credentials (the 401 comes from the payment gateway, not from this admin API — your session is fine). Replace the account credentials with valid PSP keys in Provider Accounts, then retry"
                } else {
                    "check the provider credentials, merchant configuration, network access, and the callback (notify) URL configuration, then retry"
                };
                return provider_rejected(
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
                    load_test_payment_method_diagnostic(&state.pool, &subject, &method_key).await;
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
            method_key: attempt.payment_method.clone(),
            provider_code: attempt.provider_code.clone(),
            amount: attempt
                .amount
                .as_str()
                .parse::<i64>()
                .map(minor_units_to_decimal_string)
                .unwrap_or_else(|_| attempt.amount.as_str().to_owned()),
            currency_code,
            status: attempt.status,
            qr_code_url,
            cashier_url: attempt.payment_params.get("cashierUrl").cloned(),
            pay_url: attempt.payment_params.get("payUrl").cloned(),
            pay_form: attempt.payment_params.get("payForm").cloned(),
            client_secret: attempt.payment_params.get("clientSecret").cloned(),
            publishable_key: std::env::var("STRIPE_PUBLISHABLE_KEY")
                .ok()
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty()),
            launch_kind: test_payment_launch_kind(&attempt.provider_code, &attempt.payment_params)
                .as_str()
                .to_owned(),
            launch_payload: attempt
                .payment_params
                .get("jsapiPayload")
                .or_else(|| attempt.payment_params.get("appPayload"))
                .or_else(|| attempt.payment_params.get("orderStr"))
                .cloned(),
            next_action: attempt.payment_params.get("nextAction").cloned(),
            settle_supported: test_payment_settle_supported(&attempt.payment_method),
            provider_param_keys: attempt.payment_params.keys().cloned().collect(),
            required_payer_fields: sdkwork_payment_providers::product_payer_requirements(
                &attempt.payment_method,
            )
            .iter()
            .map(|requirement| TestPaymentPayerField {
                method_key: requirement.method_key.to_owned(),
                field: requirement.metadata_key.to_owned(),
                label: requirement.label.to_owned(),
            })
            .collect(),
            expires_at: attempt.payment_params.get("expiresAt").cloned(),
            created_at: now,
        },
    )
}

/// How the cashier launches a test payment, projected from the channel's
/// normalized `nextAction` plus the provider's own semantics.
///
/// The projection exists so the dialog never has to guess: a provider that
/// returns only a client payload (WeChat App PayReq, Alipay `orderStr`) is not
/// a "web cashier page", and the local sandbox has no surface at all. Only the
/// launch surface the renderer can actually drive is reported.
fn test_payment_launch_kind(
    provider_code: &str,
    payment_params: &std::collections::BTreeMap<String, String>,
) -> TestPaymentLaunchKind {
    let has = |key: &str| {
        payment_params
            .get(key)
            .is_some_and(|value| !value.trim().is_empty())
    };
    match provider_code {
        "sandbox" => return TestPaymentLaunchKind::LocalSandbox,
        "stripe" if has("clientSecret") => return TestPaymentLaunchKind::Card,
        _ => {}
    }
    match payment_params.get("nextAction").map(String::as_str) {
        Some("jsapi") if has("jsapiPayload") => TestPaymentLaunchKind::Jsapi,
        Some("app") if has("appPayload") || has("orderStr") => TestPaymentLaunchKind::App,
        Some("redirect") if has("payUrl") => {
            if has("payForm") {
                TestPaymentLaunchKind::FormRedirect
            } else {
                TestPaymentLaunchKind::Redirect
            }
        }
        Some("qr_code") if has("qrCodeUrl") => TestPaymentLaunchKind::QrCode,
        Some("stripe_confirm") if has("clientSecret") => TestPaymentLaunchKind::Card,
        // A resolved checkout always carries a launch surface; a channel whose
        // product returned none is a configuration/renderer mismatch, and the
        // dialog reports it instead of showing an empty QR frame.
        _ if has("payForm") => TestPaymentLaunchKind::FormRedirect,
        _ => TestPaymentLaunchKind::Unsupported,
    }
}

/// Stable wire discriminator for the test-payment launch surface.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum TestPaymentLaunchKind {
    QrCode,
    Redirect,
    FormRedirect,
    Card,
    Jsapi,
    App,
    LocalSandbox,
    Unsupported,
}

impl TestPaymentLaunchKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::QrCode => "qr_code",
            Self::Redirect => "redirect",
            Self::FormRedirect => "form_redirect",
            Self::Card => "card",
            Self::Jsapi => "jsapi",
            Self::App => "app",
            Self::LocalSandbox => "local_sandbox",
            Self::Unsupported => "unsupported",
        }
    }
}

/// Persists the payer metadata of a one-cent test payment on its attempt.
///
/// The attempt row records payer input inside `callback_payload.paymentMetadata`
/// (the same place `POST /orders/{orderId}/payment` writes it), and the provider
/// enrichment reader in `sdkwork_payment_repository_sqlx` falls back to the
/// object itself when that key is absent. The write therefore merges
/// `{ paymentMetadata: <payer identity> }` into the existing snapshot instead of
/// replacing it, so the provider account / transaction id already recorded for
/// the attempt survive.
async fn persist_test_attempt_payment_metadata_postgres(
    pool: &PgPool,
    tenant_id: &str,
    attempt_id: &str,
    payer_identity: &Value,
) -> Result<(), sdkwork_contract_service::CommerceServiceError> {
    let patch = json!({ "paymentMetadata": payer_identity }).to_string();
    let update = sqlx::query(
        r#"
        UPDATE commerce_payment_attempt
        SET callback_payload = COALESCE(callback_payload, '{}'::jsonb) || $1::jsonb,
            updated_at = CAST($2 AS timestamptz)
        WHERE id = CAST($3 AS TEXT)
          AND tenant_id = CAST($4 AS TEXT)
          AND deleted_at IS NULL
        "#,
    )
    .bind(&patch)
    .bind(now_string())
    .bind(attempt_id)
    .bind(tenant_id)
    .execute(pool)
    .await
    .map_err(storage)?;
    if update.rows_affected() == 1 {
        Ok(())
    } else {
        Err(sdkwork_contract_service::CommerceServiceError::conflict(
            "the test payment attempt disappeared before its payer identity was recorded",
        ))
    }
}

async fn create_test_payment_checkout(
    pool: &IntegrationPool,
    subject: &AppRuntimeSubject,
    order_id: &str,
    method_key: &str,
    payer_identity: &Value,
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
            // The order checkout path persists payer metadata (openid,
            // client_ip, buyer_id, return_url, …) on the attempt so the
            // provider enrichment can read it. The admin test payment has no
            // app checkout behind it, so the same snapshot is written here
            // and merged with the provider enrichment already recorded.
            persist_test_attempt_payment_metadata_postgres(
                pool,
                &subject.tenant_id,
                &attempt.attempt_id,
                payer_identity,
            )
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
    // Command headers are enforced even though this endpoint derives no
    // operation id from them.
    let _write = match validate_command(ctx, &headers, "check-attempt-status", &body) {
        Ok(write) => write,
        Err(response) => return response,
    };
    let payment_intent_id = match required_text(&body.payment_intent_id, "paymentIntentId", ctx) {
        Ok(value) => value,
        Err(response) => return response,
    };

    // IntegrationPool carries exactly the authoritative Postgres pool; the
    // irrefutable bind documents that there is no second engine.
    let IntegrationPool::Postgres(pool) = &state.pool;
    let attempt = match load_test_attempt_for_check(pool, &subject, &payment_intent_id).await {
        Ok(Some(attempt)) => attempt,
        Ok(None) => {
            return not_found(ctx, "payment attempt was not found for this payment intent");
        }
        Err(error) => return map_service_error(ctx, error),
    };

    if attempt.is_terminal() {
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

    // The settle override is a separate, explicit capability: it is re-checked
    // here (deployment gate + product class) so the flag cannot borrow the
    // status query to settle money-moving products.
    if body.settle_if_unpaid {
        if !state.sandbox_trigger_enabled || !test_payment_settle_supported(&attempt.payment_method) {
            return conflict(
                ctx,
                format!(
                    "attempt {} of product {} settles real money at the provider and cannot be settled from the status check; pay it through the provider instead",
                    attempt.attempt_id, attempt.payment_method
                ),
            );
        }
    }

    // Resolve the provider account bound to the attempt's channel so the PSP
    // query uses the same credentials as the original checkout.
    let account = match load_provider_account_for_attempt(pool, &attempt).await {
        Ok(account) => account,
        Err(error) => return map_service_error(ctx, error),
    };
    let registry = provider_registry_for_account(
        &EnvPaymentCredentialResolver::load(),
        account
            .as_ref()
            .map(|account| account.binding(account.environment.clone())),
    );

    // A sandbox trade has no PSP to ask: the only authority is local state, so
    // the check reports the current attempt status and never calls out.
    if attempt.provider_code == "sandbox" {
        if body.settle_if_unpaid {
            if let Some(result) =
                apply_test_payment_event(pool, &attempt, "settle-test-payment", "succeeded", "sdkwork.test.settled")
                    .await
            {
                return success_item(
                    ctx,
                    CheckAttemptStatusResult {
                        payment_intent_id: result.payment_intent_id,
                        attempt_id: result.attempt_id,
                        provider_status: Some("succeeded".to_owned()),
                        local_status: result.local_status.clone(),
                        paid: result.local_status == "succeeded",
                    },
                );
            }
            return map_service_error(
                ctx,
                storage("the simulated settlement could not be applied"),
            );
        }
        return success_item(
            ctx,
            CheckAttemptStatusResult {
                payment_intent_id: payment_intent_id.clone(),
                attempt_id: attempt.attempt_id.clone(),
                provider_status: None,
                local_status: attempt.status.clone(),
                paid: false,
            },
        );
    }

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
            let message = sdkwork_contract_service::CommerceServiceError::from(error)
                .message()
                .to_string();
            return validation(
                ctx,
                format!(
                    "payment provider query failed: {message}; check the provider credentials and merchant configuration"
                ),
            );
        }
    };
    let mut raw_status = match query_outcome
        .raw_status
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
    {
        Some(raw_status) => raw_status,
        None => {
            return success_item(
                ctx,
                CheckAttemptStatusResult {
                    payment_intent_id,
                    attempt_id: attempt.attempt_id.clone(),
                    provider_status: None,
                    local_status: attempt.status.clone(),
                    paid: false,
                },
            );
        }
    };
    // PayPal Orders v2 stops at APPROVED: money has not moved, and no success
    // callback arrives until the merchant captures. The admin cannot complete
    // that step from the dialog, so the status check — the operator's "confirm
    // this payment" action — performs the capture with a deterministic
    // idempotency key instead of reporting "not paid" forever. Every other
    // channel captures as part of its own checkout.
    if provider_payment_awaits_capture(&attempt.provider_code, Some(&raw_status)) {
        let capture_key = format!("capture:{}:{}", attempt.provider_code, attempt.out_trade_no);
        match capture_provider_payment(
            &registry,
            &attempt.provider_code,
            &attempt.out_trade_no,
            Some(&capture_key),
        )
        .await
        {
            Ok(Some(captured)) => raw_status = captured,
            // The capture reported a state the mapper may not consume; the
            // status check then reports what the provider returned.
            Ok(None) => {}
            Err(error) => {
                return validation(
                    ctx,
                    format!(
                        "the provider order was approved but could not be captured: {}; retry the status check",
                        error.message()
                    ),
                );
            }
        }
    }
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
        // The PSP query response carries the charge amount too; the same
        // settlement integrity check applies to query-driven confirmation.
        let notified_amount = sdkwork_payment_repository_sqlx::extract_notified_payment_amount(
            &attempt.provider_code,
            &query_outcome.payload,
        );
        let applied = match apply_webhook_payment_status_postgres(
            &mut tx,
            &identity,
            Some(&raw_status),
            &now_string(),
            notified_amount.as_ref(),
        )
        .await
        {
            Ok(applied) => applied,
            Err(error) => {
                // A terminal local state (e.g. already closed) is a normal
                // outcome, not a failure: report the current state.
                tracing::debug!(
                    error = %error.message(),
                    "attempt status check settle skipped"
                );
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
        // A settlement refused for an integrity reason (amount/currency
        // mismatch) leaves the attempt untouched: report the current local
        // state, not the provider status that was rejected.
        if applied.rejected_reason.is_none() {
            if let Some(applied) = applied.status {
                local_status = applied;
            }
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
    /// Product key of the intent (`wechat_jsapi`, `paypal`, ...). The attempt
    /// row itself only stores the provider code and a legacy method column, so
    /// the settle gate reads the authoritative intent row.
    payment_method: String,
    provider_code: String,
    out_trade_no: String,
    provider_transaction_id: Option<String>,
    status: String,
    channel_id: Option<String>,
    tenant_id: String,
    organization_id: Option<String>,
    owner_user_id: String,
    order_id: String,
    /// Captured amount in major units (`0.01`), used as the refund cap.
    amount: Option<String>,
    /// Attempt currency, passed to the refund command.
    currency_code: Option<String>,
}

impl TestAttemptForCheck {
    /// Terminal states are never re-applied: a settled or closed attempt keeps
    /// its result even when a later provider/query event disagrees.
    fn is_terminal(&self) -> bool {
        matches!(
            self.status.as_str(),
            "succeeded" | "failed" | "closed" | "canceled" | "cancelled"
        )
    }
}

impl Clone for TestAttemptForCheck {
    fn clone(&self) -> Self {
        Self {
            attempt_id: self.attempt_id.clone(),
            payment_intent_id: self.payment_intent_id.clone(),
            payment_method: self.payment_method.clone(),
            provider_code: self.provider_code.clone(),
            out_trade_no: self.out_trade_no.clone(),
            provider_transaction_id: self.provider_transaction_id.clone(),
            status: self.status.clone(),
            channel_id: self.channel_id.clone(),
            tenant_id: self.tenant_id.clone(),
            organization_id: self.organization_id.clone(),
            owner_user_id: self.owner_user_id.clone(),
            order_id: self.order_id.clone(),
            amount: self.amount.clone(),
            currency_code: self.currency_code.clone(),
        }
    }
}

/// Cancels a one-cent test payment's provider trade and closes the local
/// attempt.
///
/// Every product needs a close path in the test surface: a QR code that is
/// abandoned, a PayPal order the tester never approved, or a wallet trade whose
/// launcher cannot complete would otherwise sit pending until it expires, and
/// the operator could not tell a close regression from a slow PSP. The
/// provider close runs first (so a provider refusal never leaves a locally
/// closed trade open at the PSP), then the same status machine the webhook path
/// uses is driven with a `closed` event, which records a webhook event exactly
/// like a real PSP close notification.
async fn cancel_test_payment(
    State(state): State<IntegrationState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    headers: HeaderMap,
    Json(body): Json<TestPaymentActionBody>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match require_subject(runtime_context, ctx) {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    if let Err(response) = validate_command(ctx, &headers, "cancel-test-payment", &body) {
        return response;
    }
    let payment_intent_id = match required_text(&body.payment_intent_id, "paymentIntentId", ctx) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let IntegrationPool::Postgres(pool) = &state.pool;
    let attempt = match load_test_attempt_for_check(pool, &subject, &payment_intent_id).await {
        Ok(Some(attempt)) => attempt,
        Ok(None) => return not_found(ctx, "payment attempt was not found for this payment intent"),
        Err(error) => return map_service_error(ctx, error),
    };
    if attempt.is_terminal() {
        // Closing a closed trade is a successful no-op (idempotent command).
        return success_item(
            ctx,
            test_payment_settle_payload(&attempt, attempt.status.clone(), None, None, None),
        );
    }
    let account = match load_provider_account_for_attempt(pool, &attempt).await {
        Ok(account) => account,
        Err(error) => return map_service_error(ctx, error),
    };
    let registry = provider_registry_for_account(
        &EnvPaymentCredentialResolver::load(),
        account
            .as_ref()
            .map(|account| account.binding(account.environment.clone())),
    );
    if let Err(error) = cancel_provider_payment(
        &registry,
        &attempt.provider_code,
        &attempt.out_trade_no,
        attempt.provider_transaction_id.as_deref(),
    )
    .await
    {
        return validation(
            ctx,
            format!(
                "the payment provider did not close the trade: {}; nothing was changed locally, retry after checking the provider state",
                error.message()
            ),
        );
    }
    match apply_test_payment_event(pool, &attempt, "cancel-test-payment", "closed", "closed").await {
        Some(result) => success_item(ctx, result),
        None => map_service_error(
            ctx,
            storage("the local payment attempt state could not be closed"),
        ),
    }
}

// ===========================================================================
// One-cent test coverage matrix
// ===========================================================================

/// One row of the service-provider × product coverage matrix.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TestPaymentProductView {
    provider_code: String,
    provider_label: String,
    method_key: String,
    /// `active` | `inactive` | `missing` — the tenant's method-catalog row.
    method_status: String,
    /// How the cashier launches the product, decided from the declaration.
    launch_kind: String,
    /// True when the operator can settle it from the console (sandbox and
    /// client-payload products) instead of paying for real.
    settle_supported: bool,
    /// True when a refund can be driven from the console (the product and its
    /// provider declare a refund operation).
    refund_supported: bool,
    /// True when the product is part of the admin test surface.
    test_enabled: bool,
    /// Every payer input the product's checkout requires (empty for products
    /// that address "whoever scans this").
    required_payer_fields: Vec<TestPaymentPayerField>,
    /// Active, non-deleted provider accounts serving this provider code.
    active_provider_accounts: i64,
    /// Active channels whose method resolves to this product.
    active_channel_count: i64,
    /// Ordered, human-readable findings: what still has to be configured, or
    /// `ready` when the product can be exercised now.
    readiness: Vec<String>,
    ready: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TestPaymentProductsResult {
    products: Vec<TestPaymentProductView>,
    /// Provider codes that declared no product at all (diagnosis only).
    unconfigured_providers: Vec<String>,
    ready_count: i64,
    total_count: i64,
    checked_at: String,
}

/// Product surfaces each provider code serves when the catalog row is missing.
///
/// The admin matrix must still describe a product a method-catalog row has not
/// been created for yet, otherwise "configure the provider" cannot be verified
/// before the catalog is seeded. The keys come from the provider declarations.
fn provider_product_label(provider_code: &str) -> &'static str {
    match provider_code {
        "stripe" => "Stripe",
        "alipay" => "Alipay",
        "wechat_pay" => "WeChat Pay",
        "paypal" => "PayPal",
        "sandbox" => "Sandbox",
        _ => "Unknown provider",
    }
}

/// The launch surface a product produces once the checkout resolves.
///
/// The declaration decides this, not a per-provider match: every product is
/// classified by the wire payload it is known to return, so the console can
/// label each row before any money moves. This mirrors
/// [`test_payment_launch_kind`]'s payload projection.
fn declared_launch_kind(provider_code: &str, method_key: &str) -> &'static str {
    let method_key = method_key.trim().to_ascii_lowercase();
    match (provider_code, method_key.as_str()) {
        ("sandbox", _) => "local_sandbox",
        ("stripe", _) => "card",
        ("paypal", _) => "redirect",
        (_, "wechat_native") | (_, "wechat_pay") | (_, "alipay_qr") => "qr_code",
        (_, "wechat_h5") | (_, "alipay_wap") | (_, "alipay_pc") => "redirect",
        (_, "wechat_jsapi") | (_, "alipay_jsapi") => "jsapi",
        (_, "wechat_app") | (_, "alipay_app") => "app",
        _ => "unsupported",
    }
}

/// True when the product hands the payer a client invocation payload, which an
/// admin browser cannot present (see [`TEST_PAYMENT_CLIENT_ONLY_METHOD_KEYS`]).
fn is_client_only_product(method_key: &str) -> bool {
    let normalized = method_key.trim().to_ascii_lowercase();
    TEST_PAYMENT_CLIENT_ONLY_METHOD_KEYS.contains(&normalized.as_str())
        || normalized.starts_with("stripe_")
}

/// The refund capability of a product, read from its provider declaration.
fn product_refund_supported(provider_code: &str, method_key: &str) -> bool {
    let Some(capabilities) =
        sdkwork_payment_providers::declared_provider_capabilities(provider_code)
    else {
        return false;
    };
    let declares_refund = capabilities.supports_operation(
        sdkwork_payment_providers::PaymentAdapterOperation::CreateRefund,
    );
    // The local sandbox declares no PSP refund, but a sandbox payment is
    // refunded locally, so the console can still prove the refund state
    // machine through it.
    (declares_refund && capabilities.supports_product(method_key))
        || provider_code == "sandbox"
}

/// Lists every provider product with its one-cent test readiness.
///
/// "Configure the provider account and the whole payment function works" is
/// only verifiable from the outside if the console can enumerate the whole
/// matrix. This endpoint joins the provider declarations (which products exist
/// and how they launch), the tenant's method catalog (which products are
/// active for this tenant) and the tenant's channels/accounts (which products
/// have a live route), so each row states exactly what is missing.
async fn list_test_payment_products(
    State(state): State<IntegrationState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match require_subject(runtime_context, ctx) {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    let IntegrationPool::Postgres(pool) = &state.pool;

    let method_rows = match load_testable_method_catalog(pool, &subject).await {
        Ok(rows) => rows,
        Err(error) => return map_service_error(ctx, error),
    };
    let channel_rows = match load_testable_channel_catalog(pool, &subject).await {
        Ok(rows) => rows,
        Err(error) => return map_service_error(ctx, error),
    };

    let mut products = Vec::new();
    let mut unconfigured_providers = Vec::new();
    for provider_code in sdkwork_payment_providers::declared_provider_codes() {
        let Some(capabilities) =
            sdkwork_payment_providers::declared_provider_capabilities(provider_code)
        else {
            continue;
        };
        if capabilities.products.is_empty() {
            unconfigured_providers.push(provider_code.to_owned());
            continue;
        }
        for method_key in capabilities.products {
            let test_enabled = TEST_PAYMENT_METHOD_KEYS.contains(method_key);
            let method_status = method_rows
                .iter()
                .find(|row| row.method_key.eq_ignore_ascii_case(method_key))
                .map(|row| row.status.clone())
                .unwrap_or_else(|| "missing".to_owned());
            let active_channels = channel_rows
                .iter()
                .filter(|row| row.channel_method_key.eq_ignore_ascii_case(method_key))
                .count() as i64;
            let active_accounts = channel_rows
                .iter()
                .filter(|row| {
                    row.channel_method_key.eq_ignore_ascii_case(method_key)
                        && row.account_status == "active"
                })
                .count() as i64;
            let settle_supported = test_payment_settle_supported(method_key);
            let refund_supported = product_refund_supported(provider_code, method_key);
            let required_payer_fields: Vec<TestPaymentPayerField> =
                sdkwork_payment_providers::product_payer_requirements(method_key)
                    .iter()
                    .map(|requirement| TestPaymentPayerField {
                        method_key: requirement.method_key.to_owned(),
                        field: requirement.metadata_key.to_owned(),
                        label: requirement.label.to_owned(),
                    })
                    .collect();

            let mut readiness = Vec::new();
            if method_status == "missing" {
                readiness.push(format!(
                    "the method {} is not in this tenant's payment-method catalog yet; create it in the payment-methods workspace",
                    method_key
                ));
            } else if method_status != "active" {
                readiness.push(format!(
                    "the method {} is {method_status}; enable it in the payment-methods workspace",
                    method_key
                ));
            }
            if active_channels == 0 {
                readiness.push(format!(
                    "no active payment channel routes {} to a provider account; create a channel in the channel workspace",
                    method_key
                ));
            } else if active_accounts == 0 {
                readiness.push(format!(
                    "the channel routing {} points at a provider account that is not active; activate the account or repoint the channel",
                    method_key
                ));
            }
            if provider_code != "sandbox"
                && method_status == "active"
                && active_channels > 0
                && !has_deployable_account_for_provider(&channel_rows, provider_code)
            {
                readiness.push(format!(
                    "no active provider account for {} exists in this tenant; create one in Provider Accounts and test its credentials",
                    provider_code
                ));
            }
            if !test_enabled {
                readiness.push(format!(
                    "{} is declared by the adapter but is not part of the admin test surface yet",
                    method_key
                ));
            }
            let provider_declared = capabilities.supports_product(method_key);
            if !provider_declared {
                readiness.push(format!(
                    "{} is not declared by the {} adapter in this build; the catalog row cannot be exercised",
                    method_key, provider_code
                ));
            }
            let ready = readiness.is_empty();
            if ready {
                readiness.push("ready".to_owned());
            }
            products.push(TestPaymentProductView {
                provider_code: provider_code.to_owned(),
                provider_label: provider_product_label(provider_code).to_owned(),
                method_key: (*method_key).to_owned(),
                method_status,
                launch_kind: declared_launch_kind(provider_code, method_key).to_owned(),
                settle_supported,
                refund_supported,
                test_enabled,
                required_payer_fields,
                active_provider_accounts: active_accounts,
                active_channel_count: active_channels,
                readiness,
                ready,
            });
        }
    }
    // Deterministic order for the console: providers in declaration order, then
    // the products as declared.
    products.sort_by_key(|product| {
        (
            sdkwork_payment_providers::declared_provider_codes()
                .iter()
                .position(|code| *code == product.provider_code)
                .unwrap_or(usize::MAX),
            product.method_key.clone(),
        )
    });
    let ready_count = products.iter().filter(|product| product.ready).count() as i64;
    let total_count = products.len() as i64;
    success_item(
        ctx,
        TestPaymentProductsResult {
            products,
            unconfigured_providers,
            ready_count,
            total_count,
            checked_at: now_string(),
        },
    )
}

#[derive(Debug, Clone)]
struct TestableMethodRow {
    method_key: String,
    status: String,
}

#[derive(Debug, Clone)]
struct TestableChannelRow {
    channel_method_key: String,
    account_status: String,
}

fn has_deployable_account_for_provider(rows: &[TestableChannelRow], provider_code: &str) -> bool {
    // The channel catalog carries the provider account status per method; an
    // account is deployable when at least one channel of a product served by
    // that provider resolves to an active account.
    rows.iter().any(|row| {
        row.account_status == "active"
            && sdkwork_payment_providers::declared_provider_capabilities(provider_code)
                .is_some_and(|capabilities| capabilities.supports_product(&row.channel_method_key))
    })
}

async fn load_testable_method_catalog(
    pool: &PgPool,
    subject: &AppRuntimeSubject,
) -> Result<Vec<TestableMethodRow>, sdkwork_contract_service::CommerceServiceError> {
    let organization_id = subject
        .organization_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("0");
    let rows = sqlx::query(
        "SELECT method_key, status FROM commerce_payment_method \
         WHERE tenant_id = CAST($1 AS TEXT) \
           AND (organization_id = CAST($2 AS TEXT) OR organization_id = '0') \
           AND deleted_at IS NULL",
    )
    .bind(&subject.tenant_id)
    .bind(organization_id)
    .fetch_all(pool)
    .await
    .map_err(storage)?;
    Ok(rows
        .into_iter()
        .map(|row| TestableMethodRow {
            method_key: row.try_get("method_key").unwrap_or_default(),
            status: row.try_get("status").unwrap_or_default(),
        })
        .collect())
}

async fn load_testable_channel_catalog(
    pool: &PgPool,
    subject: &AppRuntimeSubject,
) -> Result<Vec<TestableChannelRow>, sdkwork_contract_service::CommerceServiceError> {
    let organization_id = subject
        .organization_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("0");
    let rows = sqlx::query(
        "SELECT COALESCE(m.method_key, c.metadata->>'methodKey') AS channel_method_key, \
                COALESCE(a.status, '') AS account_status \
         FROM commerce_payment_channel c \
         LEFT JOIN commerce_payment_method m ON m.id = c.method_id AND m.tenant_id = c.tenant_id AND m.deleted_at IS NULL \
         LEFT JOIN commerce_payment_provider_account a ON a.id = c.provider_account_id AND a.deleted_at IS NULL \
         WHERE c.tenant_id = CAST($1 AS TEXT) \
           AND (c.organization_id = CAST($2 AS TEXT) OR c.organization_id = '0') \
           AND c.deleted_at IS NULL \
           AND c.status = 'active'",
    )
    .bind(&subject.tenant_id)
    .bind(organization_id)
    .fetch_all(pool)
    .await
    .map_err(storage)?;
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let method_key: Option<String> = row.try_get("channel_method_key").ok();
            let method_key = method_key.filter(|value| !value.trim().is_empty())?;
            Some(TestableChannelRow {
                channel_method_key: method_key,
                account_status: row.try_get("account_status").unwrap_or_default(),
            })
        })
        .collect())
}

/// Refund command for a one-cent test payment.
///
/// A one-cent test must prove the WHOLE chain, and refund is the last link: a
/// settled test payment that cannot be refunded leaves the admin unable to tell
/// a refund regression from an environment difference. The refund goes through
/// the same `CreateOwnerRefundCommand` the app surface uses (amount cap,
/// eligibility, provider submission, status machine), so the console exercises
/// production behaviour rather than a test-only shortcut.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct TestPaymentRefundBody {
    payment_intent_id: String,
    /// Refund amount; defaults to the full test amount (one cent).
    amount: Option<String>,
    reason_code: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TestPaymentRefundResult {
    payment_intent_id: String,
    attempt_id: String,
    refund_id: String,
    refund_no: String,
    amount: String,
    currency_code: String,
    /// Refund status after the provider submission attempt was applied
    /// (`succeeded`, `processing`, `failed`).
    status: String,
    provider_code: String,
    /// True when the provider submission produced a terminal success.
    refunded: bool,
    /// Next step for the operator, derived from the resulting status.
    next_step: String,
}

/// Refunds a settled one-cent test payment through the production refund path.
async fn refund_test_payment(
    State(state): State<IntegrationState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    headers: HeaderMap,
    Json(body): Json<TestPaymentRefundBody>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match require_subject(runtime_context, ctx) {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    let write = match validate_command(ctx, &headers, "refund-test-payment", &body) {
        Ok(write) => write,
        Err(response) => return response,
    };
    let payment_intent_id = match required_text(&body.payment_intent_id, "paymentIntentId", ctx) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let refund_amount = match normalized(body.amount.clone()) {
        Some(amount) => {
            if !is_money_amount(&amount) {
                return validation(ctx, "amount must match ^[0-9]+(\\.[0-9]{1,2})?$");
            }
            Some(amount)
        }
        None => None,
    };
    let IntegrationPool::Postgres(pool) = &state.pool;
    let attempt = match load_test_attempt_for_check(pool, &subject, &payment_intent_id).await {
        Ok(Some(attempt)) => attempt,
        Ok(None) => return not_found(ctx, "payment attempt was not found for this payment intent"),
        Err(error) => return map_service_error(ctx, error),
    };
    if attempt.status != "succeeded" {
        return conflict(
            ctx,
            format!(
                "attempt {} is {} and cannot be refunded; settle the test payment first (or pay it through the provider), then refund it",
                attempt.attempt_id, attempt.status
            ),
        );
    }
    let total_amount = match decimal_amount_minor_units(
        attempt
            .amount
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("0.01"),
    ) {
        Some(minor) => minor,
        None => {
            return validation(
                ctx,
                "the attempt amount could not be read; refund through the refunds workspace instead",
            )
        }
    };
    // The refund may not exceed the captured total: rebinding the same cap the
    // domain applies keeps the test from creating a provider request the PSP
    // would reject and that the status machine would then leave in limbo.
    if let Some(amount) = refund_amount.as_deref() {
        match decimal_amount_minor_units(amount) {
            Some(minor) if minor > 0 && minor <= total_amount => {}
            Some(_) => {
                return validation(
                    ctx,
                    format!(
                        "refund amount {} must be between 0.01 and the captured total ({})",
                        amount,
                        minor_units_to_decimal_string(total_amount)
                    ),
                )
            }
            None => return validation(ctx, "amount overflows the supported range"),
        }
    }
    let store = sdkwork_payment_repository_sqlx::PostgresCommerceRefundStore::new(pool.clone());
    let command = match sdkwork_payment_service::CreateOwnerRefundCommand::new_with_currency(
        &subject.tenant_id,
        subject.organization_id.as_deref(),
        &subject.user_id,
        &attempt.order_id,
        Some(&attempt.attempt_id),
        refund_amount.as_deref(),
        attempt
            .currency_code
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty()),
        body.reason_code
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty()),
        &write.request_no,
        &format!("refund-test-payment-{}", write.idempotency_key),
    ) {
        Ok(command) => match command.requested_by_operator(&subject.user_id) {
            Ok(command) => command,
            Err(error) => return map_service_error(ctx, error),
        },
        Err(error) => return map_service_error(ctx, error),
    };
    let mut refund = match store.create_owner_refund(command).await {
        Ok(refund) => refund,
        Err(error) => return map_service_error(ctx, error),
    };
    // A refund created in `submitted` has NOT reached the PSP yet: the
    // submission (claim → provider call → terminal status) is what the admin
    // refund path runs explicitly, so the test console must run the same
    // sequence instead of reporting a `submitted` refund as if it moved money.
    let mut submission_error = None;
    if refund_requires_provider_submission(&refund.status) {
        if refund_requires_processing_claim(&refund.status) {
            refund = match store
                .mark_owner_refund_provider_submission_processing(
                    &subject.tenant_id,
                    subject.organization_id.as_deref(),
                    &refund.refund_id,
                    "operator",
                    Some(&subject.user_id),
                    &write.request_no,
                    &format!("refund-test-payment-{}", write.idempotency_key),
                )
                .await
            {
                Ok(refund) => refund,
                Err(error) => return map_service_error(ctx, error),
            };
        }
        let submission_key = format!("refund-test-payment-{}", write.idempotency_key);
        match submit_test_payment_refund_with_retry(
            pool,
            &subject,
            &refund,
            body.reason_code
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned),
        )
        .await
        {
            Ok(sdkwork_payment_providers::ProviderRefundSubmissionState::Processing) => {}
            Ok(sdkwork_payment_providers::ProviderRefundSubmissionState::Succeeded) => {
                refund = match store
                    .mark_owner_refund_provider_submission_succeeded(
                        &subject.tenant_id,
                        subject.organization_id.as_deref(),
                        &refund.refund_id,
                        "operator",
                        Some(&subject.user_id),
                        &write.request_no,
                        &submission_key,
                    )
                    .await
                {
                    Ok(refund) => refund,
                    Err(error) => return map_service_error(ctx, error),
                };
            }
            Ok(sdkwork_payment_providers::ProviderRefundSubmissionState::Failed) => {
                refund = match store
                    .mark_owner_refund_provider_submission_failed(
                        &subject.tenant_id,
                        subject.organization_id.as_deref(),
                        &refund.refund_id,
                        "operator",
                        Some(&subject.user_id),
                        &write.request_no,
                        &submission_key,
                    )
                    .await
                {
                    Ok(refund) => refund,
                    Err(error) => return map_service_error(ctx, error),
                };
            }
            Err(error) => {
                if !refund_submission_retryable(&error) {
                    let _ = store
                        .mark_owner_refund_provider_submission_failed(
                            &subject.tenant_id,
                            subject.organization_id.as_deref(),
                            &refund.refund_id,
                            "operator",
                            Some(&subject.user_id),
                            &write.request_no,
                            &submission_key,
                        )
                        .await;
                }
                submission_error = Some(error.message().to_owned());
            }
        }
    }
    let refunded = refund.status == "succeeded";
    let next_step = match (refund.status.as_str(), submission_error.as_deref()) {
        (_, Some(_)) => "the refund was recorded but the provider submission failed; open the refunds workspace, inspect the provider response and retry",
        ("succeeded", _) => "the refund reached a terminal success; verify it in the refund records and the payment's refund summary",
        ("processing", _) => "the provider accepted the refund asynchronously; poll it with the refund retry action or wait for the refund callback",
        _ => "the refund did not succeed; open the refunds workspace to inspect the provider response and retry",
    };
    success_item(
        ctx,
        TestPaymentRefundResult {
            payment_intent_id,
            attempt_id: attempt.attempt_id,
            refund_id: refund.refund_id,
            refund_no: refund.refund_no,
            amount: refund.amount.as_str().to_owned(),
            currency_code: refund.currency_code,
            status: refund.status,
            provider_code: attempt.provider_code,
            refunded,
            next_step: next_step.to_owned(),
        },
    )
}

/// Provider submission for a test refund, retried on transport-class failures
/// only — the same bounded backoff the admin refund router applies.
async fn submit_test_payment_refund_with_retry(
    pool: &PgPool,
    subject: &AppRuntimeSubject,
    refund: &sdkwork_payment_service::RefundView,
    reason_code: Option<String>,
) -> Result<
    sdkwork_payment_providers::ProviderRefundSubmissionState,
    sdkwork_contract_service::CommerceServiceError,
> {
    const ATTEMPTS: u32 = 3;
    let mut last_error = None;
    for attempt in 0..ATTEMPTS {
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(150 * (1 << (attempt - 1))))
                .await;
        }
        match submit_test_payment_refund(pool, subject, refund, reason_code.clone()).await {
            Ok(state) => return Ok(state),
            Err(error) if refund_submission_retryable(&error) => last_error = Some(error),
            Err(error) => return Err(error),
        }
    }
    Err(last_error.expect("refund submission attempted at least once"))
}

/// Submits one refund to the provider through the shared operations layer so
/// the test console exercises the same credentials, trade reference and
/// idempotency key a production refund uses.
async fn submit_test_payment_refund(
    pool: &PgPool,
    subject: &AppRuntimeSubject,
    refund: &sdkwork_payment_service::RefundView,
    reason_code: Option<String>,
) -> Result<
    sdkwork_payment_providers::ProviderRefundSubmissionState,
    sdkwork_contract_service::CommerceServiceError,
> {
    let Some(context) =
        sdkwork_payment_repository_sqlx::load_payment_attempt_provider_context_by_id_postgres(
            pool,
            &refund.payment_attempt_id,
        )
        .await?
    else {
        return Err(sdkwork_contract_service::CommerceServiceError::not_found(
            "payment attempt provider context was not found for refund submission",
        ));
    };
    let account = match context.provider_account_id.as_deref() {
        Some(provider_account_id) => sdkwork_payment_repository_sqlx::
            load_provider_account_for_existing_payment_postgres(
                pool,
                &subject.tenant_id,
                subject.organization_id.as_deref(),
                provider_account_id,
            )
            .await?
            .map(Some)
            .ok_or_else(|| {
                sdkwork_contract_service::CommerceServiceError::conflict(
                    "the original payment provider account is unavailable for this refund",
                )
            })?,
        None => {
            sdkwork_payment_repository_sqlx::load_active_provider_account_postgres(
                pool,
                &subject.tenant_id,
                subject.organization_id.as_deref(),
                &context.provider_code,
            )
            .await?
        }
    };
    let credentials = EnvPaymentCredentialResolver::load();
    let registry = provider_registry_for_account(
        &credentials,
        account.map(|record| {
            sdkwork_payment_providers::ProviderAccountBinding {
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
        }),
    );
    let total_amount = sdkwork_contract_service::CommerceMoney::new(&context.amount)
        .map_err(|error| sdkwork_contract_service::CommerceServiceError::storage(error.to_string()))?;
    // A refund the console re-attempts while the provider is still working is
    // polled rather than resubmitted, so a duplicate PSP refund is impossible.
    if refund.status == "processing" {
        if let Some(state) = sdkwork_payment_providers::query_provider_refund(
            &registry,
            &context.provider_code,
            &context.out_trade_no,
            context.provider_transaction_id.as_deref(),
            &refund.refund_no,
        )
        .await?
        {
            return Ok(state);
        }
    }
    let refund_notify_url =
        sdkwork_payment_repository_sqlx::resolve_refund_notify_url_postgres(
            pool,
            &subject.tenant_id,
            subject.organization_id.as_deref(),
            &context.provider_code,
            credentials.webhook_base_url.as_deref(),
        )
        .await?;
    sdkwork_payment_providers::create_provider_refund(
        &registry,
        &context.provider_code,
        &context.out_trade_no,
        context.provider_transaction_id.as_deref(),
        &refund.refund_no,
        &refund.amount,
        &total_amount,
        reason_code,
        refund_notify_url.as_deref(),
    )
    .await
}

fn refund_submission_retryable(error: &sdkwork_contract_service::CommerceServiceError) -> bool {
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

/// A notification sample: the raw payload and headers a PSP would POST.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct TestPaymentNotificationBody {
    /// Provider account whose stored credentials sign/verify the notification.
    provider_account_id: String,
    /// Raw notification body. It is signed by the deployment (see
    /// `signer`) when it carries no signature header of its own.
    payload: String,
    /// Extra request headers to send verbatim (`stripe-signature`, …).
    #[serde(default)]
    headers: std::collections::BTreeMap<String, String>,
    /// `hmac_sha256` (Stripe) or `none` (unsigned, sandbox only). When
    /// omitted it is derived from the provider code.
    #[serde(default)]
    signer: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TestPaymentNotificationResult {
    provider_account_id: String,
    provider_code: String,
    /// `verified` | `rejected`.
    verification: String,
    /// True when the delivery passed verification and the event was ingested.
    accepted: bool,
    /// Native and document-signature verification results, side by side, so a
    /// header/config mismatch is visible instead of a single opaque verdict.
    native_signature: String,
    document_signature: String,
    signature_header: Option<String>,
    /// Provider event id resolved by the notification reader.
    provider_event_id: Option<String>,
    /// Merchant trade number the notification resolves to.
    out_trade_no: Option<String>,
    /// Webhook event row created by the delivery.
    webhook_event_id: Option<String>,
    applied_status: Option<String>,
    payment_attempt_id: Option<String>,
    diagnostic: Option<String>,
    delivered_at: String,
}

/// Builds the notification test entry: it acts as the PSP calling the deployment
/// webhook, so both notification verification and the ingest pipeline
/// (event de-duplication, trade resolution, status machine) are exercised in one
/// round trip — no PSP account, no public URL and no local tunnel needed.
async fn test_payment_notification(
    State(state): State<IntegrationState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    headers: HeaderMap,
    Json(body): Json<TestPaymentNotificationBody>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match require_subject(runtime_context, ctx) {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    if !state.sandbox_trigger_enabled {
        return forbidden(
            ctx,
            "simulated provider notifications are disabled on production-like deployments; the provider calls the webhook instead",
        );
    }
    if let Err(response) = validate_command(ctx, &headers, "test-payment-notification", &body) {
        return response;
    }
    let payload = match normalized(Some(body.payload.clone())) {
        Some(payload) => payload,
        None => return validation(ctx, "payload must be a non-empty notification body"),
    };
    let account =
        match load_provider_account(&state.pool, &subject, &body.provider_account_id).await {
            Ok(Some(account)) => account,
            Ok(None) => return not_found(ctx, "payment provider account was not found"),
            Err(error) => return map_service_error(ctx, error),
        };
    let provider_code =
        sdkwork_payment_providers::normalize_provider_code(&account.provider_code);
    let scheme = match body.signer.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => match sdkwork_payment_providers::NotificationSignScheme::parse(value) {
            Some(scheme) => scheme,
            None => {
                return validation(
                    ctx,
                    "signer must be one of hmac_sha256, wechat_pay_rsa_sha256, alipay_rsa_sha256, none",
                )
            }
        },
        None => match sdkwork_payment_providers::default_sign_scheme(&provider_code) {
            Some(scheme) => scheme,
            None => {
                return validation(
                    ctx,
                    format!(
                        "provider {provider_code} does not define a signed notification contract in this build; pass signer explicitly"
                    ),
                )
            }
        },
    };
    // The unsigned scheme carries unsigned bodies into the ingest pipeline; it
    // is only meaningful for the local sandbox adapter, never for a real PSP
    // whose verifier would reject them anyway.
    if scheme == sdkwork_payment_providers::NotificationSignScheme::Unsigned
        && provider_code != "sandbox"
    {
        return validation(
            ctx,
            format!(
                "provider {provider_code} requires a signed notification; the unsigned scheme is only available for the sandbox provider"
            ),
        );
    }
    let registry = provider_registry_for_account(
        &EnvPaymentCredentialResolver::load(),
        Some(account.binding(account.environment.clone())),
    );
    let Some(adapter) = registry.resolve(&provider_code) else {
        return validation(
            ctx,
            format!(
                "provider {provider_code} could not initialize its adapter; check the provider account credentials before testing notifications"
            ),
        );
    };
    let timestamp = sqlx::types::chrono::Utc::now().timestamp();
    let nonce = sdkwork_utils_rust::uuid().replace('-', "");
    let sign_inputs = sdkwork_payment_providers::ProviderNotificationSignInputs {
        webhook_secret: account.webhook_secret.clone(),
        private_key_pem: account
            .certificate
            .clone()
            .or_else(|| account.primary_secret.clone()),
        api_v3_key: account.webhook_secret.clone(),
        serial_no: metadata_text(&account.metadata, "verificationSerialNo")
            .or_else(|| metadata_text(&account.metadata, "verification_serial_no"))
            .or_else(|| metadata_text(&account.metadata, "publishKeyId")),
        merchant_id: account.merchant_id.clone(),
    };
    let signed = match sdkwork_payment_providers::sign_provider_notification(
        &provider_code,
        scheme,
        &payload,
        timestamp,
        &nonce,
        &sign_inputs,
    ) {
        Ok(signed) => signed,
        Err(error) => {
            let message = sdkwork_contract_service::CommerceServiceError::from(error)
                .message()
                .to_owned();
            return validation(
                ctx,
                format!("this delivery cannot be signed with the stored credential: {message}; rotate the provider account credential and retry"),
            );
        }
    };
    // Extra headers from the request are appended after the signed ones so an
    // operator can add a `Wechatpay-Serial` or a custom header without losing
    // the signature set.
    let mut provider_headers = signed.headers.clone();
    for (name, value) in &body.headers {
        if !provider_headers
            .iter()
            .any(|(existing, _)| existing.eq_ignore_ascii_case(name))
        {
            provider_headers.push((name.clone(), value.clone()));
        }
    }
    let verify_request = PaymentVerifyWebhookRequest {
        headers: provider_headers.clone(),
        body: signed.body.clone(),
        metadata: json!({ "notificationTest": true }),
    };
    let (native_signature, document_signature) = match adapter.verify_webhook(verify_request).await
    {
        Ok(outcome) if outcome.verified => ("verified", "verified"),
        Ok(_) => ("rejected", "rejected"),
        Err(error) => {
            let message = sdkwork_contract_service::CommerceServiceError::from(error)
                .message()
                .to_owned();
            return success_item(
                ctx,
                TestPaymentNotificationResult {
                    provider_account_id: account.id.clone(),
                    provider_code: provider_code.clone(),
                    verification: "rejected".to_owned(),
                    accepted: false,
                    native_signature: "error".to_owned(),
                    document_signature: "error".to_owned(),
                    signature_header: signed
                        .headers
                        .first()
                        .map(|(name, _)| name.clone()),
                    provider_event_id: None,
                    out_trade_no: None,
                    webhook_event_id: None,
                    applied_status: None,
                    payment_attempt_id: None,
                    diagnostic: Some(format!(
                        "the notification could not be verified with the stored credential: {message}"
                    )),
                    delivered_at: now_string(),
                },
            );
        }
    };
    if native_signature != "verified" {
        return success_item(
            ctx,
            TestPaymentNotificationResult {
                provider_account_id: account.id.clone(),
                provider_code: provider_code.clone(),
                verification: "rejected".to_owned(),
                accepted: false,
                native_signature: native_signature.to_owned(),
                document_signature: document_signature.to_owned(),
                signature_header: signed.headers.first().map(|(name, _)| name.clone()),
                provider_event_id: None,
                out_trade_no: None,
                webhook_event_id: None,
                applied_status: None,
                payment_attempt_id: None,
                diagnostic: Some(
                    "the deployment accepted the body but its own verifier rejected the signature; check that the stored webhook secret / merchant key / API v3 key matches the one this account was signed with"
                        .to_owned(),
                ),
                delivered_at: now_string(),
            },
        );
    }
    let normalize_request = sdkwork_payment_providers::PaymentNormalizeWebhookRequest {
        headers: provider_headers,
        body: signed.body.clone(),
        metadata: json!({ "notificationTest": true }),
    };
    let event = match adapter.normalize_webhook(normalize_request).await {
        Ok(event) => event,
        Err(error) => {
            let message = sdkwork_contract_service::CommerceServiceError::from(error)
                .message()
                .to_owned();
            return success_item(
                ctx,
                TestPaymentNotificationResult {
                    provider_account_id: account.id.clone(),
                    provider_code: provider_code.clone(),
                    verification: "verified".to_owned(),
                    accepted: false,
                    native_signature: native_signature.to_owned(),
                    document_signature: document_signature.to_owned(),
                    signature_header: signed.headers.first().map(|(name, _)| name.clone()),
                    provider_event_id: None,
                    out_trade_no: None,
                    webhook_event_id: None,
                    applied_status: None,
                    payment_attempt_id: None,
                    diagnostic: Some(format!(
                        "the signature verified but the body could not be normalized: {message}"
                    )),
                    delivered_at: now_string(),
                },
            );
        }
    };
    // Delivery runs the ingest pipeline with the tenant scope the admin session
    // proved, so an unmatched trade number is still recorded against this tenant
    // instead of being dropped.
    let IntegrationPool::Postgres(pool) = &state.pool;
    let command = IngestProviderWebhookCommand {
        provider_code: event.provider_code.clone(),
        provider_event_id: event.provider_event_id.clone().unwrap_or_default(),
        event_type: event.event_type.clone(),
        out_trade_no: event.out_trade_no.clone(),
        payment_status: event.payment_status.clone(),
        payload: event.payload.clone(),
        tenant_id: Some(subject.tenant_id.clone()),
        organization_id: subject.organization_id.clone(),
    };
    match ingest_provider_webhook_postgres(pool, command).await {
        Ok(outcome) => success_item(
            ctx,
            TestPaymentNotificationResult {
                provider_account_id: account.id.clone(),
                provider_code: provider_code.clone(),
                verification: "verified".to_owned(),
                accepted: true,
                native_signature: native_signature.to_owned(),
                document_signature: document_signature.to_owned(),
                signature_header: signed.headers.first().map(|(name, _)| name.clone()),
                provider_event_id: event.provider_event_id.clone(),
                out_trade_no: event.out_trade_no.clone(),
                webhook_event_id: Some(outcome.webhook_event_id),
                applied_status: outcome.applied_status,
                payment_attempt_id: outcome.payment_attempt_id,
                diagnostic: Some(if outcome.applied {
                    "delivered: the event was verified, recorded and applied"
                        .to_owned()
                } else {
                    "delivered: the event was verified and recorded, but the status machine did not transition (a terminal attempt or a stale status)"
                        .to_owned()
                }),
                delivered_at: now_string(),
            },
        ),
        Err(error) => map_service_error(ctx, error),
    }
}
///
/// The event is ingested through the PSP webhook pipeline (event de-duplication,
/// amount/currency integrity check, status machine), so everything downstream —
/// webhook event record, refund eligibility, order-side settlement — sees the
/// same state a real callback would produce. The capability is fail-closed on
/// production-like deployments, matching the sandbox trigger it shares its
/// permission with.
async fn settle_test_payment(
    State(state): State<IntegrationState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    headers: HeaderMap,
    Json(body): Json<TestPaymentSettleBody>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match require_subject(runtime_context, ctx) {
        Ok(subject) => subject,
        Err(response) => return response,
    };
    if !state.sandbox_trigger_enabled {
        return forbidden(
            ctx,
            "simulated settlement is disabled on production-like deployments; pay through the provider instead",
        );
    }
    if let Err(response) = validate_command(ctx, &headers, "settle-test-payment", &body) {
        return response;
    }
    let payment_intent_id = match required_text(&body.payment_intent_id, "paymentIntentId", ctx) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let outcome = body
        .outcome
        .as_deref()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "success".to_owned());
    let (event_status, event_type) = match outcome.as_str() {
        "success" | "succeeded" => ("succeeded", "sdkwork.test.settled"),
        "failed" | "failure" => ("failed", "sdkwork.test.failed"),
        other => {
            return validation(
                ctx,
                format!("outcome must be success or failed, got {other}"),
            );
        }
    };
    let IntegrationPool::Postgres(pool) = &state.pool;
    let attempt = match load_test_attempt_for_check(pool, &subject, &payment_intent_id).await {
        Ok(Some(attempt)) => attempt,
        Ok(None) => return not_found(ctx, "payment attempt was not found for this payment intent"),
        Err(error) => return map_service_error(ctx, error),
    };
    if !test_payment_settle_supported(&attempt.payment_method) {
        return conflict(
            ctx,
            format!(
                "payment method {} of attempt {} settles real money at the provider: pay it through the provider, or close it, instead of simulating settlement",
                attempt.payment_method, attempt.attempt_id
            ),
        );
    }
    if attempt.is_terminal() {
        return success_item(
            ctx,
            test_payment_settle_payload(&attempt, attempt.status.clone(), None, None, None),
        );
    }
    match apply_test_payment_event(pool, &attempt, "settle-test-payment", event_status, event_type).await {        Some(result) => success_item(ctx, result),
        None => map_service_error(
            ctx,
            storage("the simulated settlement could not be applied"),
        ),
    }
}

fn test_payment_settle_payload(
    attempt: &TestAttemptForCheck,
    local_status: String,
    applied_status: Option<String>,
    webhook_event_id: Option<String>,
    event_id: Option<String>,
) -> TestPaymentSettleResult {
    TestPaymentSettleResult {
        payment_intent_id: attempt.payment_intent_id.clone(),
        attempt_id: attempt.attempt_id.clone(),
        out_trade_no: attempt.out_trade_no.clone(),
        previous_status: attempt.status.clone(),
        local_status,
        provider_code: attempt.provider_code.clone(),
        applied_status,
        webhook_event_id: webhook_event_id.unwrap_or_default(),
        event_id: event_id.unwrap_or_default(),
    }
}

/// Applies one simulated provider event to a pending test-payment attempt
/// through the shared ingest pipeline and reports the resulting state.
async fn apply_test_payment_event(
    pool: &PgPool,
    attempt: &TestAttemptForCheck,
    scope: &str,
    event_status: &str,
    event_type: &str,
) -> Option<TestPaymentSettleResult> {
    let event_id = stable_id(
        format!("test-payment-{scope}-event").as_str(),
        &format!("{}:{}", attempt.attempt_id, event_status),
    );
    let payload = json!({
        "id": event_id,
        "type": event_type,
        "providerCode": attempt.provider_code.clone(),
        "outTradeNo": attempt.out_trade_no.clone(),
        "paymentStatus": event_status,
        "settledBy": scope,
    });
    let command = IngestProviderWebhookCommand {
        provider_code: attempt.provider_code.clone(),
        provider_event_id: event_id.clone(),
        event_type: Some(event_type.to_owned()),
        out_trade_no: Some(attempt.out_trade_no.clone()),
        payment_status: Some(event_status.to_owned()),
        payload,
        tenant_id: Some(attempt.tenant_id.clone()),
        organization_id: attempt.organization_id.clone(),
    };
    if let Err(error) = ingest_provider_webhook_postgres(pool, command).await {
        // A state-machine refusal (the attempt was settled concurrently by a
        // real callback) is a race, not a failure: the reload below reports the
        // authoritative state. Any other storage failure is a real error.
        let reloaded = load_test_attempt_by_id(pool, attempt).await.ok().flatten();
        match reloaded {
            Some(current) if current.status != attempt.status => {
                return Some(test_payment_settle_payload(
                    &current,
                    current.status.clone(),
                    None,
                    None,
                    Some(event_id),
                ));
            }
            _ => {
                tracing::warn!(
                    error = %error.message(),
                    attempt_id = %attempt.attempt_id,
                    "test payment simulated event could not be applied"
                );
                return None;
            }
        }
    }
    let current = load_test_attempt_by_id(pool, attempt)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| attempt.clone());
    Some(test_payment_settle_payload(
        &current,
        current.status.clone(),
        None,
        None,
        Some(event_id),
    ))
}

async fn load_test_attempt_by_id(
    pool: &PgPool,
    attempt: &TestAttemptForCheck,
) -> Result<Option<TestAttemptForCheck>, sdkwork_contract_service::CommerceServiceError> {
    let row = sqlx::query(
        "SELECT a.id, a.payment_intent_id, i.payment_method, a.provider_code, a.out_trade_no, \
                COALESCE(a.callback_payload->>'providerTransactionId', a.callback_payload->>'provider_transaction_id') AS provider_transaction_id, \
                a.status, a.channel_id, a.tenant_id, a.organization_id, a.owner_user_id, a.order_id, \
                CAST(a.amount AS BIGINT)::TEXT AS amount_minor, a.currency_code \
         FROM commerce_payment_attempt a \
         LEFT JOIN commerce_payment_intent i ON i.id = a.payment_intent_id AND i.tenant_id = a.tenant_id \
         WHERE a.tenant_id = CAST($1 AS TEXT) AND a.id = CAST($2 AS TEXT) AND a.deleted_at IS NULL \
         LIMIT 1",
    )
    .bind(&attempt.tenant_id)
    .bind(&attempt.attempt_id)
    .fetch_optional(pool)
    .await
    .map_err(storage)?;
    Ok(row.map(|row| TestAttemptForCheck {
        attempt_id: row.try_get("id").unwrap_or_default(),
        payment_intent_id: row.try_get("payment_intent_id").unwrap_or_default(),
        payment_method: row.try_get("payment_method").ok().flatten().unwrap_or_default(),
        provider_code: row.try_get("provider_code").unwrap_or_default(),
        out_trade_no: row.try_get("out_trade_no").unwrap_or_default(),
        provider_transaction_id: row.try_get("provider_transaction_id").ok().flatten(),
        status: row.try_get("status").unwrap_or_default(),
        channel_id: row.try_get("channel_id").ok().flatten(),
        tenant_id: row.try_get("tenant_id").unwrap_or_default(),
        organization_id: row.try_get("organization_id").ok().flatten(),
        owner_user_id: row.try_get("owner_user_id").unwrap_or_default(),
        order_id: row.try_get("order_id").unwrap_or_default(),
        amount: row
            .try_get::<Option<String>, _>("amount_minor")
            .ok()
            .flatten()
            .and_then(|minor| minor.trim().parse::<i64>().ok())
            .map(minor_units_to_decimal_string),
        currency_code: row
            .try_get::<Option<String>, _>("currency_code")
            .ok()
            .flatten()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty()),
    }))
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
        "SELECT a.id, a.payment_intent_id, i.payment_method, a.provider_code, a.out_trade_no, \
                COALESCE(a.callback_payload->>'providerTransactionId', a.callback_payload->>'provider_transaction_id') AS provider_transaction_id, \
                a.status, a.channel_id, a.tenant_id, a.organization_id, a.owner_user_id, a.order_id, \
                CAST(a.amount AS BIGINT)::TEXT AS amount_minor, a.currency_code \
         FROM commerce_payment_attempt a \
         LEFT JOIN commerce_payment_intent i ON i.id = a.payment_intent_id AND i.tenant_id = a.tenant_id \
         WHERE a.tenant_id = CAST($1 AS TEXT) \
           AND ((a.organization_id = CAST($2 AS TEXT)) OR (a.organization_id = '0' AND $2::text IS NULL)) \
           AND a.owner_user_id = CAST($3 AS TEXT) \
           AND a.payment_intent_id = CAST($4 AS TEXT) AND a.deleted_at IS NULL \
         ORDER BY a.created_at DESC, a.id DESC LIMIT 1",
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
        payment_method: row.try_get("payment_method").ok().flatten().unwrap_or_default(),
        provider_code: row.try_get("provider_code").unwrap_or_default(),
        out_trade_no: row.try_get("out_trade_no").unwrap_or_default(),
        provider_transaction_id: row.try_get("provider_transaction_id").ok().flatten(),
        status: row.try_get("status").unwrap_or_default(),
        channel_id: row.try_get("channel_id").ok().flatten(),
        tenant_id: row.try_get("tenant_id").unwrap_or_default(),
        organization_id: row.try_get("organization_id").ok().flatten(),
        owner_user_id: row.try_get("owner_user_id").unwrap_or_default(),
        order_id: row.try_get("order_id").unwrap_or_default(),
        amount: row
            .try_get::<Option<String>, _>("amount_minor")
            .ok()
            .flatten()
            .and_then(|minor| minor.trim().parse::<i64>().ok())
            .map(minor_units_to_decimal_string),
        currency_code: row
            .try_get::<Option<String>, _>("currency_code")
            .ok()
            .flatten()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty()),
    }))
}

async fn load_provider_account_for_attempt(
    pool: &PgPool,
    attempt: &TestAttemptForCheck,
) -> Result<Option<ProviderAccountRecord>, sdkwork_contract_service::CommerceServiceError> {
    let Some(channel_id) = attempt.channel_id.as_deref() else {
        return Ok(None);
    };
    // The channel→account join resolves inside the attempt's tenant; the
    // subject organization is not part of this lookup's identity.
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
        metadata: row
            .try_get("metadata")
            .ok()
            .flatten()
            .unwrap_or_else(|| json!({})),
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
    // The breakdown amount must be persisted as the canonical minor-unit
    // integer (e.g. "1" for ¥0.01) — the same representation real orders
    // store and every downstream reader expects (`CommerceMoney::new`,
    // `CAST(... AS BIGINT)`). Writing the major-unit string ("0.01") makes the
    // intent/attempt amount decode fail with "money amount must be a
    // non-negative integer smallest-unit amount".
    let amount_minor = decimal_amount_minor_units(amount).ok_or_else(|| {
        sdkwork_contract_service::CommerceServiceError::validation(
            "amount overflows the supported range",
        )
    })?;
    // Bind the canonical minor-unit integer (not a text value): the order
    // module migrated its money columns to native NUMERIC
    // (`0016_money_columns_native_numeric`), and a text parameter cannot be
    // assigned to NUMERIC. An integer parameter is accepted by both the
    // NUMERIC columns and the older text columns (assignment cast).
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
            .bind(amount_minor)
            .bind(amount_minor)
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
    // IntegrationPool carries exactly the authoritative Postgres pool.
    let IntegrationPool::Postgres(pool) = pool;
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
    // IntegrationPool carries exactly the authoritative Postgres pool.
    let IntegrationPool::Postgres(pool) = pool;
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

/// Formats integer smallest units (1) back to the decimal display string
/// ("0.01") for the admin response.
fn minor_units_to_decimal_string(minor_units: i64) -> String {
    format!("{}.{:02}", minor_units / 100, minor_units % 100)
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
        WriteCommandHeaderError::MissingHeader(name) => {
            validation(ctx, format!("{name} header is required"))
        }
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
                "SELECT id, provider_code, merchant_id, environment, secret_ref, webhook_secret_ref, certificate_ref, CAST(metadata AS TEXT) AS metadata FROM commerce_payment_provider_account WHERE id = CAST($1 AS TEXT) AND tenant_id = CAST($2 AS TEXT) AND ((organization_id = CAST($3 AS TEXT)) OR (organization_id = '0' AND $3 IS NULL)) AND deleted_at IS NULL LIMIT 1",
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
            sqlx::query("UPDATE commerce_payment_provider_account SET last_tested_at = CAST($1 AS TIMESTAMPTZ), last_test_status = $2, updated_at = CAST($1 AS TIMESTAMPTZ) WHERE id = CAST($3 AS TEXT) AND tenant_id = CAST($4 AS TEXT) AND ((organization_id = CAST($5 AS TEXT)) OR (organization_id = '0' AND $5 IS NULL)) AND deleted_at IS NULL")
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
            sqlx::query("UPDATE commerce_payment_provider_account SET metadata = COALESCE(metadata, '{}'::jsonb) || CAST($1 AS JSONB), last_tested_at = NULL, last_test_status = NULL, updated_at = CAST($2 AS TIMESTAMPTZ) WHERE id = CAST($3 AS TEXT) AND tenant_id = CAST($4 AS TEXT) AND ((organization_id = CAST($5 AS TEXT)) OR (organization_id = '0' AND $5 IS NULL)) AND deleted_at IS NULL")
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
            let rows = sqlx::query("SELECT sm.id, sm.provider_account_id, sm.external_sub_merchant_id, sm.sub_appid, sm.sub_mch_id, sm.display_name, sm.status, CAST(sm.metadata AS TEXT) AS metadata, CAST(sm.created_at AS TEXT) AS created_at, CAST(sm.updated_at AS TEXT) AS updated_at, pa.provider_code, COUNT(*) OVER() AS total_count FROM commerce_payment_sub_merchant sm JOIN commerce_payment_provider_account pa ON pa.id = sm.provider_account_id AND pa.tenant_id = sm.tenant_id WHERE sm.tenant_id = CAST($1 AS TEXT) AND sm.organization_id = CAST($2 AS TEXT) AND sm.deleted_at IS NULL AND ($3 IS NULL OR sm.provider_account_id = CAST($3 AS TEXT)) AND ($4 IS NULL OR sm.status = CAST($4 AS TEXT)) AND ($5 IS NULL OR sm.external_sub_merchant_id ILIKE '%' || CAST($5 AS TEXT) || '%' OR COALESCE(sm.display_name, '') ILIKE '%' || CAST($5 AS TEXT) || '%') AND ($6 IS NULL OR pa.provider_code = CAST($6 AS TEXT)) ORDER BY sm.updated_at DESC, sm.id DESC LIMIT $7 OFFSET $8")
                .bind(&subject.tenant_id).bind(&subject.organization_id).bind(&query.provider_account_id).bind(&query.status).bind(&query.q).bind(&query.provider_code).bind(page.page_size).bind(page.offset)
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
    // `commerce_payment_sub_merchant.status` accepts exactly these values; a typo
    // must not travel into the INSERT and surface as a 500 from the CHECK
    // constraint.
    let status = body.status.as_deref().map(str::trim).unwrap_or("active");
    const SUB_MERCHANT_STATUSES: &[&str] =
        &["active", "inactive", "suspended", "deprecated", "pending_review"];
    if !SUB_MERCHANT_STATUSES.contains(&status) {
        return Err(
            sdkwork_contract_service::CommerceServiceError::validation(format!(
                "status must be one of {}",
                SUB_MERCHANT_STATUSES.join(", ")
            )),
        );
    }
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
    let (table, _columns) = match kind {
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
        PayerIdentityBody, TEST_PAYMENT_ORDER_TTL_MINUTES,
    };
    use crate::command_headers::AppWriteCommandHeaders;
    use serde_json::json;

    #[test]
    fn test_payment_launch_kind_reports_only_drivable_surfaces() {
        use super::{test_payment_launch_kind, TestPaymentLaunchKind};
        use std::collections::BTreeMap;

        let params = |pairs: &[(&str, &str)]| {
            pairs
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect::<BTreeMap<String, String>>()
        };

        // The sandbox has no PSP surface at all, whatever the channel recorded.
        assert_eq!(
            test_payment_launch_kind("sandbox", &params(&[("nextAction", "cashier")])),
            TestPaymentLaunchKind::LocalSandbox
        );
        // Stripe is a Stripe.js card element, not a QR or a redirect.
        assert_eq!(
            test_payment_launch_kind("stripe", &params(&[("clientSecret", "pi_1_secret")])),
            TestPaymentLaunchKind::Card
        );
        // A client payload is its own launch kind: rendering it as a web
        // cashier page would hand the operator an unusable link.
        assert_eq!(
            test_payment_launch_kind(
                "wechat_pay",
                &params(&[("nextAction", "jsapi"), ("jsapiPayload", "{}")])
            ),
            TestPaymentLaunchKind::Jsapi
        );
        assert_eq!(
            test_payment_launch_kind(
                "alipay",
                &params(&[("nextAction", "app"), ("orderStr", "app_id=1")])
            ),
            TestPaymentLaunchKind::App
        );
        // A hosted approve link (PayPal) and the Alipay H5 cashier redirect.
        assert_eq!(
            test_payment_launch_kind(
                "paypal",
                &params(&[("nextAction", "redirect"), ("payUrl", "https://paypal.test/approve")])
            ),
            TestPaymentLaunchKind::Redirect
        );
        // Alipay PC returns both a URL and an auto-submit form.
        assert_eq!(
            test_payment_launch_kind(
                "alipay",
                &params(&[
                    ("nextAction", "redirect"),
                    ("payUrl", "https://openapi.alipay.test"),
                    ("payForm", "<form></form>"),
                ])
            ),
            TestPaymentLaunchKind::FormRedirect
        );
        assert_eq!(
            test_payment_launch_kind(
                "wechat_pay",
                &params(&[("nextAction", "qr_code"), ("qrCodeUrl", "weixin://wxpay/1")])
            ),
            TestPaymentLaunchKind::QrCode
        );
        // A channel that returned nothing launchable is reported, not masked.
        assert_eq!(
            test_payment_launch_kind("alipay", &params(&[("nextAction", "cashier")])),
            TestPaymentLaunchKind::Unsupported
        );
    }

    #[test]
    fn every_configured_product_class_reaches_the_test_surface() {
        use super::{
            test_payment_settle_supported, TEST_PAYMENT_CLIENT_ONLY_METHOD_KEYS,
            TEST_PAYMENT_METHOD_KEYS,
        };

        // Service-provider × product coverage: every product each adapter
        // declares must be testable, otherwise "configure the provider and the
        // whole payment function works" cannot be verified from the admin.
        for method_key in [
            "stripe_card",
            "stripe_apple_pay",
            "stripe_google_pay",
            "stripe_alipay",
            "stripe_wechat_pay",
            "alipay_qr",
            "alipay_pc",
            "alipay_wap",
            "alipay_app",
            "alipay_jsapi",
            "wechat_native",
            "wechat_jsapi",
            "wechat_h5",
            "wechat_app",
            "paypal",
            "sandbox_test",
        ] {
            assert!(
                TEST_PAYMENT_METHOD_KEYS.contains(&method_key),
                "{method_key} must be testable"
            );
        }
        // Settlement simulation is limited to products that cannot settle a
        // real payment from the dialog.
        for method_key in TEST_PAYMENT_CLIENT_ONLY_METHOD_KEYS {
            assert!(test_payment_settle_supported(method_key));
        }
        assert!(test_payment_settle_supported("sandbox_test"));
        for method_key in [
            "stripe_card",
            "stripe_apple_pay",
            "alipay_qr",
            "alipay_pc",
            "alipay_wap",
            "wechat_native",
            "wechat_h5",
            "paypal",
        ] {
            assert!(
                !test_payment_settle_supported(method_key),
                "{method_key} moves real money and must not be settled by simulation"
            );
        }
        // Case/whitespace tolerated, unknown keys fail closed.
        assert!(test_payment_settle_supported(" WeChat_JSAPI "));
        assert!(!test_payment_settle_supported("wechat_unknown"));
    }

    #[test]
    fn payer_identity_round_trips_into_provider_metadata() {
        // The console collects what the product addresses; the value handed to
        // the attempt snapshot must use the metadata keys the adapters read.
        let identity = PayerIdentityBody {
            openid: Some("  o-payer-1 ".to_owned()),
            client_ip: Some("203.0.113.7".to_owned()),
            buyer_id: None,
            buyer_open_id: None,
        };
        assert!(identity.has("openid"));
        assert!(identity.has("client_ip"));
        assert!(!identity.has("buyer_id"));
        let value = identity.into_value();
        assert_eq!(value["openid"], json!("o-payer-1"));
        assert_eq!(value["client_ip"], json!("203.0.113.7"));
        assert!(value.get("buyer_id").is_none());
        assert!(value.get("buyer_open_id").is_none());

        // `buyer_open_id` is the alternate Alipay key the adapter accepts, so
        // filling only it still satisfies the `buyer_id` requirement.
        let alipay = PayerIdentityBody {
            buyer_open_id: Some("alipay-open-1".to_owned()),
            ..PayerIdentityBody::default()
        };
        assert!(alipay.has("buyer_id"));
        assert_eq!(alipay.into_value()["buyer_open_id"], json!("alipay-open-1"));

        // Blank input never counts as satisfied.
        let blank = PayerIdentityBody {
            openid: Some("   ".to_owned()),
            ..PayerIdentityBody::default()
        };
        assert!(!blank.has("openid"));
        assert_eq!(blank.into_value(), json!({}));
    }

    #[test]
    fn test_payment_methods_match_the_provider_adapter_declarations() {
        use super::TEST_PAYMENT_METHOD_KEYS;

        // Every product an adapter declares must be reachable from the admin
        // test surface: "configure the provider, then exercise the whole
        // payment function" is a coverage guarantee, not a curated list. The
        // declaration is read statically so the check also runs on deployments
        // that have no PSP credentials configured yet.
        let mut declared = std::collections::BTreeSet::new();
        for provider_code in sdkwork_payment_providers::declared_provider_codes() {
            let capabilities = sdkwork_payment_providers::declared_provider_capabilities(provider_code)
                .expect("declared provider must resolve its capabilities");
            for product in capabilities.products {
                declared.insert((*product).to_owned());
            }
        }
        for product in &declared {
            // `wechat_pay` is the legacy alias of `wechat_native`; it stays
            // testable for pre-existing method rows.
            assert!(
                TEST_PAYMENT_METHOD_KEYS.contains(&product.as_str()),
                "{product} is declared by an adapter but the admin test surface cannot reach it"
            );
        }
        // And the reverse: a key the console offers must belong to a real
        // declaration, so a removed product cannot linger as a dead button.
        for product in TEST_PAYMENT_METHOD_KEYS {
            assert!(
                declared.contains(product),
                "{product} is offered by the test surface but no adapter declares it"
            );
        }
    }

    #[test]
    fn registered_adapters_share_the_static_declarations() {
        // A registered adapter and the static table must agree, otherwise the
        // console would report capabilities the checkout does not honour.
        let credentials = sdkwork_payment_providers::ProviderCredentialBundle::from_env();
        let registry =
            sdkwork_payment_providers::PaymentProviderRegistry::from_credentials(credentials);
        for provider_code in sdkwork_payment_providers::declared_provider_codes() {
            let declared =
                sdkwork_payment_providers::declared_provider_capabilities(provider_code)
                    .expect("declared capabilities");
            assert_eq!(
                registry.capabilities(provider_code).map(|item| item.provider_code),
                Some(declared.provider_code)
            );
            if let Some(adapter) = registry.resolve(provider_code) {
                assert_eq!(adapter.capabilities().products, declared.products);
                assert_eq!(adapter.capabilities().operations, declared.operations);
            }
        }
        assert!(registry.capabilities("unknown_provider").is_none());
    }

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
        // Isolate the fixture in its own schema and apply the production
        // payment baseline plus the order-owned tables this checkout path
        // reads. These unit tests share one test database; a shared `public`
        // schema makes the shape of `commerce_payment_intent` (and friends)
        // depend on whichever test ran first, which is exactly how this test
        // used to fail with a missing `payment_method` column.
        let schema = format!("payment_dev_checkout_{}", std::process::id());
        let admin = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .expect("postgres admin pool");
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP SCHEMA IF EXISTS {schema} CASCADE"
        )))
        .execute(&admin)
        .await
        .expect("drop stale dev-checkout schema");
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
            .execute(&admin)
            .await
            .expect("create dev-checkout schema");
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .after_connect({
                let schema = schema.clone();
                move |connection, _| {
                    let schema = schema.clone();
                    Box::pin(async move {
                        // Every pooled connection resolves the isolated schema
                        // first, then public (extensions/uuid helpers).
                        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
                            "SET search_path TO {schema}, public"
                        )))
                        .execute(connection)
                        .await?;
                        Ok(())
                    })
                }
            })
            .connect(&url)
            .await
            .expect("postgres pool");
        sqlx::raw_sql(sqlx::AssertSqlSafe(PAYMENT_BASELINE_SCHEMA))
            .execute(&pool)
            .await
            .expect("payment baseline schema must apply");
        sqlx::raw_sql(sqlx::AssertSqlSafe(ORDER_FIXTURE_SCHEMA))
            .execute(&pool)
            .await
            .expect("order fixture schema must apply");
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
        match create_test_payment_checkout(
            &pool,
            &subject,
            &order_id,
            "wechat_native",
            &json!({}),
            &write,
        )
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
        drop(cleanup_pool);
        let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP SCHEMA IF EXISTS {schema} CASCADE"
        )))
        .execute(&admin)
        .await;
    }

    /// Consolidated payment DDL, applied into the isolated schema so the
    /// fixture always matches production column shapes instead of a hand-kept
    /// subset.
    const PAYMENT_BASELINE_SCHEMA: &str =
        include_str!("../../../database/ddl/baseline/postgres/0001_payment_baseline.sql");

    /// Minimal order-owned tables this checkout path reads. Payment must not
    /// depend on the order crate, so the shape is mirrored here.
    const ORDER_FIXTURE_SCHEMA: &str = r#"
CREATE TABLE commerce_order (
    id TEXT NOT NULL PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    organization_id TEXT NOT NULL DEFAULT '0',
    owner_user_id TEXT NOT NULL,
    order_no TEXT NOT NULL,
    subject TEXT,
    status TEXT NOT NULL,
    currency_code TEXT,
    paid_at TIMESTAMPTZ,
    expired_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE commerce_order_amount_breakdown (
    id TEXT NOT NULL PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    organization_id TEXT NOT NULL DEFAULT '0',
    order_id TEXT NOT NULL,
    allocation_type TEXT NOT NULL,
    original_amount NUMERIC(20,0) NOT NULL DEFAULT 0,
    discount_amount NUMERIC(20,0) NOT NULL DEFAULT 0,
    payable_amount NUMERIC(20,0) NOT NULL,
    currency_code TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
"#;

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
