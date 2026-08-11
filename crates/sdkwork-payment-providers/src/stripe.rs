use std::fmt;

use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::adapter::{
    metadata_string, normalized_optional, require_non_empty, require_positive_amount,
    PaymentAdapterFuture, PaymentAdapterOperation, PaymentCancelPaymentIntentRequest,
    PaymentCreateIntentRequest, PaymentCreateRefundRequest, PaymentNormalizeWebhookRequest,
    PaymentNormalizedWebhookEvent, PaymentProviderAdapter, PaymentProviderCapabilities,
    PaymentProviderOperationOutcome, PaymentQueryPaymentIntentRequest, PaymentQueryRefundRequest,
    PaymentVerifyWebhookRequest, PaymentWebhookVerificationOutcome,
};
use crate::error::{ProviderError, ProviderResult};
use crate::http::ReqwestHttpClient;

type HmacSha256 = Hmac<Sha256>;

pub const STRIPE_PROVIDER_CODE: &str = "stripe";
const STRIPE_API_BASE_URL: &str = "https://api.stripe.com";

static STRIPE_CAPABILITIES: PaymentProviderCapabilities = PaymentProviderCapabilities {
    provider_code: STRIPE_PROVIDER_CODE,
    operations: &[
        PaymentAdapterOperation::CreatePaymentIntent,
        PaymentAdapterOperation::QueryPaymentIntent,
        PaymentAdapterOperation::CancelPaymentIntent,
        PaymentAdapterOperation::CreateRefund,
        PaymentAdapterOperation::QueryRefund,
        PaymentAdapterOperation::VerifyWebhook,
        PaymentAdapterOperation::NormalizeWebhook,
    ],
};

#[derive(Clone, PartialEq, Eq)]
pub struct StripePaymentProviderConfig {
    pub secret_key: String,
    pub webhook_secret: Option<String>,
    /// Publishable key for client-side Stripe.js card collection.
    pub publishable_key: Option<String>,
}

impl fmt::Debug for StripePaymentProviderConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StripePaymentProviderConfig")
            .field("publishable_key", &"[REDACTED]")
            .field("secret_key", &"<redacted>")
            .field(
                "webhook_secret",
                &self.webhook_secret.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

pub struct StripePaymentProviderAdapter {
    config: StripePaymentProviderConfig,
    http: ReqwestHttpClient,
}

const STRIPE_WEBHOOK_TIMESTAMP_TOLERANCE_SECONDS: i64 = 300;

impl StripePaymentProviderAdapter {
    pub fn with_default_http_client(config: StripePaymentProviderConfig) -> ProviderResult<Self> {
        validate_secret_key(&config.secret_key)?;
        if let Some(webhook_secret) = &config.webhook_secret {
            if webhook_secret.trim().is_empty() {
                return Err(ProviderError::invalid_request(
                    PaymentAdapterOperation::VerifyWebhook,
                    "Stripe webhook secret must not be empty when configured",
                ));
            }
        }
        let http = ReqwestHttpClient::new(STRIPE_API_BASE_URL)?
            .with_bearer_auth(config.secret_key.clone());
        Ok(Self { config, http })
    }
}

impl PaymentProviderAdapter for StripePaymentProviderAdapter {
    fn capabilities(&self) -> &'static PaymentProviderCapabilities {
        &STRIPE_CAPABILITIES
    }

    fn create_payment_intent<'a>(
        &'a self,
        request: PaymentCreateIntentRequest,
    ) -> PaymentAdapterFuture<'a, PaymentProviderOperationOutcome> {
        Box::pin(async move {
            let amount_minor = require_positive_amount(
                request.amount_minor,
                PaymentAdapterOperation::CreatePaymentIntent,
                "amount_minor",
            )?;
            let currency = require_currency(
                request.currency.as_deref(),
                PaymentAdapterOperation::CreatePaymentIntent,
            )?;
            let idempotency_key =
                metadata_string(&request.metadata, "idempotency_key").map(str::to_owned);
            let mut form = vec![
                ("amount".to_owned(), amount_minor.to_string()),
                ("currency".to_owned(), currency.to_ascii_lowercase()),
                (
                    "automatic_payment_methods[enabled]".to_owned(),
                    "true".to_owned(),
                ),
            ];
            // When a method_key is supplied (e.g., `stripe_card`,
            // `stripe_apple_pay`, `stripe_google_pay`), pass the corresponding
            // `payment_method_types` so Stripe restricts the PaymentIntent to
            // the requested instrument. Apple Pay / Google Pay are supported via
            // Stripe's `card` payment method type with wallet support enabled
            // in the Stripe Dashboard.
            if let Some(method_key) = request
                .payment_scene
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                if let Some(types) = stripe_payment_method_types_for_key(method_key) {
                    for pm_type in types {
                        form.push(("payment_method_types[]".to_owned(), (*pm_type).to_owned()));
                    }
                }
            }
            if let Some(tenant_id) = request.tenant_id {
                form.push(("metadata[tenant_id]".to_owned(), tenant_id));
            }
            if let Some(merchant_order_no) = normalized_optional(request.merchant_order_no) {
                form.push(("metadata[merchant_order_no]".to_owned(), merchant_order_no));
            }
            append_flat_metadata(&mut form, &request.metadata);

            let response = self
                .http
                .post_form(
                    STRIPE_PROVIDER_CODE,
                    "/v1/payment_intents",
                    form,
                    idempotency_key.as_deref(),
                )
                .await?;
            stripe_operation_outcome(PaymentAdapterOperation::CreatePaymentIntent, response)
        })
    }

    fn query_payment_intent<'a>(
        &'a self,
        request: PaymentQueryPaymentIntentRequest,
    ) -> PaymentAdapterFuture<'a, PaymentProviderOperationOutcome> {
        Box::pin(async move {
            let payment_intent_id = require_stripe_resource_id(
                request.payment_intent_id.as_deref(),
                PaymentAdapterOperation::QueryPaymentIntent,
                "payment_intent_id",
            )?;
            let response = self
                .http
                .get(
                    STRIPE_PROVIDER_CODE,
                    &format!("/v1/payment_intents/{payment_intent_id}"),
                )
                .await?;
            stripe_operation_outcome(PaymentAdapterOperation::QueryPaymentIntent, response)
        })
    }

    fn cancel_payment_intent<'a>(
        &'a self,
        request: PaymentCancelPaymentIntentRequest,
    ) -> PaymentAdapterFuture<'a, PaymentProviderOperationOutcome> {
        Box::pin(async move {
            let payment_intent_id = require_stripe_resource_id(
                request.payment_intent_id.as_deref(),
                PaymentAdapterOperation::CancelPaymentIntent,
                "payment_intent_id",
            )?;
            let idempotency_key =
                metadata_string(&request.metadata, "idempotency_key").map(str::to_owned);
            let mut form = Vec::new();
            if let Some(reason) = stripe_cancellation_reason(request.reason.as_deref()) {
                form.push(("cancellation_reason".to_owned(), reason.to_owned()));
            }
            let response = self
                .http
                .post_form(
                    STRIPE_PROVIDER_CODE,
                    &format!("/v1/payment_intents/{payment_intent_id}/cancel"),
                    form,
                    idempotency_key.as_deref(),
                )
                .await?;
            stripe_operation_outcome(PaymentAdapterOperation::CancelPaymentIntent, response)
        })
    }

    fn create_refund<'a>(
        &'a self,
        request: PaymentCreateRefundRequest,
    ) -> PaymentAdapterFuture<'a, PaymentProviderOperationOutcome> {
        Box::pin(async move {
            let payment_intent_id = require_non_empty(
                request.payment_intent_id.as_deref(),
                PaymentAdapterOperation::CreateRefund,
                "payment_intent_id",
            )?;
            let amount_minor = require_positive_amount(
                request.amount_minor,
                PaymentAdapterOperation::CreateRefund,
                "amount_minor",
            )?;
            let idempotency_key =
                metadata_string(&request.metadata, "idempotency_key").map(str::to_owned);
            let mut form = vec![
                ("payment_intent".to_owned(), payment_intent_id),
                ("amount".to_owned(), amount_minor.to_string()),
            ];
            if let Some(reason) = stripe_refund_reason(request.reason.as_deref()) {
                form.push(("reason".to_owned(), reason.to_owned()));
            }
            if let Some(refund_no) = normalized_optional(request.refund_no) {
                form.push(("metadata[refund_no]".to_owned(), refund_no));
            }
            append_flat_metadata(&mut form, &request.metadata);

            let response = self
                .http
                .post_form(
                    STRIPE_PROVIDER_CODE,
                    "/v1/refunds",
                    form,
                    idempotency_key.as_deref(),
                )
                .await?;
            stripe_operation_outcome(PaymentAdapterOperation::CreateRefund, response)
        })
    }

    fn query_refund<'a>(
        &'a self,
        request: PaymentQueryRefundRequest,
    ) -> PaymentAdapterFuture<'a, PaymentProviderOperationOutcome> {
        Box::pin(async move {
            let response = if let Some(refund_id) = request.refund_id.as_deref() {
                let refund_id = require_stripe_resource_id(
                    Some(refund_id),
                    PaymentAdapterOperation::QueryRefund,
                    "refund_id",
                )?;
                self.http
                    .get(STRIPE_PROVIDER_CODE, &format!("/v1/refunds/{refund_id}"))
                    .await?
            } else {
                let payment_intent_id = require_stripe_resource_id(
                    metadata_string(&request.metadata, "payment_intent_id"),
                    PaymentAdapterOperation::QueryRefund,
                    "metadata.payment_intent_id",
                )?;
                let refund_no = require_non_empty(
                    request.refund_no.as_deref(),
                    PaymentAdapterOperation::QueryRefund,
                    "refund_no",
                )?;
                let response = self
                    .http
                    .get(
                        STRIPE_PROVIDER_CODE,
                        &format!(
                            "/v1/refunds?payment_intent={}&limit=100",
                            urlencoding::encode(&payment_intent_id)
                        ),
                    )
                    .await?;
                stripe_refund_by_merchant_no(response, &refund_no)?
            };
            stripe_operation_outcome(PaymentAdapterOperation::QueryRefund, response)
        })
    }

    fn verify_webhook<'a>(
        &'a self,
        request: PaymentVerifyWebhookRequest,
    ) -> PaymentAdapterFuture<'a, PaymentWebhookVerificationOutcome> {
        Box::pin(async move {
            let Some(webhook_secret) = self.config.webhook_secret.as_deref() else {
                return Err(ProviderError::invalid_request(
                    PaymentAdapterOperation::VerifyWebhook,
                    "Stripe webhook secret is required to verify webhook deliveries",
                ));
            };
            let Some(signature_header) = find_header(&request.headers, "stripe-signature") else {
                return Ok(PaymentWebhookVerificationOutcome {
                    verified: false,
                    provider_event_id: None,
                });
            };
            let verified = verify_stripe_signature(webhook_secret, signature_header, &request.body);
            let provider_event_id = if verified {
                parse_webhook_event_id(&request.body)?
            } else {
                None
            };
            Ok(PaymentWebhookVerificationOutcome {
                verified,
                provider_event_id,
            })
        })
    }

    fn normalize_webhook<'a>(
        &'a self,
        request: PaymentNormalizeWebhookRequest,
    ) -> PaymentAdapterFuture<'a, PaymentNormalizedWebhookEvent> {
        Box::pin(async move {
            let payload = serde_json::from_slice::<Value>(&request.body).map_err(|error| {
                ProviderError::invalid_response(
                    PaymentAdapterOperation::NormalizeWebhook,
                    format!("Stripe webhook JSON is invalid: {error}"),
                )
            })?;
            let object = payload.get("data").and_then(|data| data.get("object"));
            let out_trade_no = object
                .and_then(|value| {
                    value
                        .get("metadata")
                        .and_then(|metadata| metadata.get("merchant_order_no"))
                        .and_then(Value::as_str)
                })
                .map(str::to_owned);
            let payment_status = object
                .and_then(|value| value.get("status").and_then(Value::as_str))
                .map(str::to_owned);
            Ok(PaymentNormalizedWebhookEvent {
                provider_code: STRIPE_PROVIDER_CODE.to_owned(),
                event_type: payload
                    .get("type")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                provider_event_id: payload.get("id").and_then(Value::as_str).map(str::to_owned),
                out_trade_no,
                payment_status,
                payload,
            })
        })
    }
}

fn stripe_operation_outcome(
    operation: PaymentAdapterOperation,
    response: Value,
) -> ProviderResult<PaymentProviderOperationOutcome> {
    let native_id = response
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| {
            ProviderError::invalid_response(operation, "Stripe response is missing id")
        })?;
    Ok(PaymentProviderOperationOutcome {
        provider_code: STRIPE_PROVIDER_CODE.to_owned(),
        native_id: Some(native_id),
        raw_status: response
            .get("status")
            .and_then(Value::as_str)
            .map(str::to_owned),
        payload: response,
    })
}

fn stripe_refund_by_merchant_no(response: Value, refund_no: &str) -> ProviderResult<Value> {
    if let Some(refund) = response
        .get("data")
        .and_then(Value::as_array)
        .and_then(|items| {
            items.iter().find(|item| {
                item.get("metadata")
                    .and_then(|metadata| metadata.get("refund_no"))
                    .and_then(Value::as_str)
                    == Some(refund_no)
            })
        })
        .cloned()
    {
        return Ok(refund);
    }
    if response.get("has_more").and_then(Value::as_bool) == Some(true) {
        return Err(ProviderError::invalid_response(
            PaymentAdapterOperation::QueryRefund,
            "Stripe refund query requires another page before absence can be established",
        ));
    }
    Err(ProviderError::invalid_response(
        PaymentAdapterOperation::QueryRefund,
        "Stripe refund was not found for the supplied refund_no",
    ))
}

fn require_currency(
    currency: Option<&str>,
    operation: PaymentAdapterOperation,
) -> ProviderResult<String> {
    let currency = require_non_empty(currency, operation, "currency")?;
    if currency.len() != 3
        || !currency
            .chars()
            .all(|character| character.is_ascii_alphabetic())
    {
        return Err(ProviderError::invalid_request(
            operation,
            "Stripe currency must be an ISO 4217 three-letter code",
        ));
    }
    Ok(currency)
}

fn require_stripe_resource_id(
    value: Option<&str>,
    operation: PaymentAdapterOperation,
    field: &str,
) -> ProviderResult<String> {
    let value = require_non_empty(value, operation, field)?;
    if value.contains('/') || value.contains('?') || value.contains('#') {
        return Err(ProviderError::invalid_request(
            operation,
            format!("Stripe {field} must be a resource id, not a path or URL"),
        ));
    }
    Ok(value)
}

fn append_flat_metadata(form: &mut Vec<(String, String)>, metadata: &Value) {
    let Some(object) = metadata.as_object() else {
        return;
    };
    for (key, value) in object {
        if key == "idempotency_key" || key.starts_with("stripe_") {
            continue;
        }
        if let Some(value) = metadata_value_as_string(value) {
            form.push((format!("metadata[{key}]"), value));
        }
    }
}

fn metadata_value_as_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => normalized_optional(Some(value.clone())),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn stripe_refund_reason(reason: Option<&str>) -> Option<&'static str> {
    match reason.map(str::trim).filter(|value| !value.is_empty()) {
        Some("duplicate") => Some("duplicate"),
        Some("fraudulent") => Some("fraudulent"),
        Some("requested_by_customer" | "customer_requested" | "user_requested") => {
            Some("requested_by_customer")
        }
        Some(_) => Some("requested_by_customer"),
        None => None,
    }
}

fn stripe_cancellation_reason(reason: Option<&str>) -> Option<&'static str> {
    match reason.map(str::trim).filter(|value| !value.is_empty()) {
        Some("duplicate") => Some("duplicate"),
        Some("fraudulent") => Some("fraudulent"),
        Some("requested_by_customer" | "customer_requested" | "user_requested") => {
            Some("requested_by_customer")
        }
        Some("abandoned") => Some("abandoned"),
        Some(_) => Some("requested_by_customer"),
        None => None,
    }
}

fn find_header<'a>(headers: &'a [(String, String)], header_name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(header_name))
        .map(|(_, value)| value.as_str())
}

pub(crate) fn verify_stripe_signature(
    webhook_secret: &str,
    signature_header: &str,
    body: &[u8],
) -> bool {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_secs()).ok());
    let Some(now) = now else {
        return false;
    };
    verify_stripe_signature_at(
        webhook_secret,
        signature_header,
        body,
        now,
        STRIPE_WEBHOOK_TIMESTAMP_TOLERANCE_SECONDS,
    )
}

fn verify_stripe_signature_at(
    webhook_secret: &str,
    signature_header: &str,
    body: &[u8],
    now: i64,
    tolerance_seconds: i64,
) -> bool {
    let Some(timestamp) = stripe_signature_value(signature_header, "t") else {
        return false;
    };
    let Ok(timestamp) = timestamp.parse::<i64>() else {
        return false;
    };
    if tolerance_seconds < 0 || now.abs_diff(timestamp) > tolerance_seconds as u64 {
        return false;
    }
    let signatures = stripe_signature_values(signature_header, "v1");
    if signatures.is_empty() {
        return false;
    }
    let signed_payload = format!("{timestamp}.");
    let Ok(mut mac) = HmacSha256::new_from_slice(webhook_secret.as_bytes()) else {
        return false;
    };
    mac.update(signed_payload.as_bytes());
    mac.update(body);
    let expected = hex_encode(mac.finalize().into_bytes());
    signatures
        .iter()
        .any(|signature| constant_time_eq(expected.as_bytes(), signature.as_bytes()))
}

fn stripe_signature_value<'a>(signature_header: &'a str, key: &str) -> Option<&'a str> {
    stripe_signature_values(signature_header, key)
        .into_iter()
        .next()
}

fn stripe_signature_values<'a>(signature_header: &'a str, key: &str) -> Vec<&'a str> {
    signature_header
        .split(',')
        .filter_map(|part| {
            let (name, value) = part.trim().split_once('=')?;
            if name == key && !value.is_empty() {
                Some(value)
            } else {
                None
            }
        })
        .collect()
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right.iter())
        .fold(0_u8, |acc, (left, right)| acc | (left ^ right))
        == 0
}

fn parse_webhook_event_id(body: &[u8]) -> ProviderResult<Option<String>> {
    let payload = serde_json::from_slice::<Value>(body).map_err(|error| {
        ProviderError::invalid_response(
            PaymentAdapterOperation::VerifyWebhook,
            format!("Stripe webhook JSON is invalid: {error}"),
        )
    })?;
    Ok(payload.get("id").and_then(Value::as_str).map(str::to_owned))
}

fn validate_secret_key(secret_key: &str) -> ProviderResult<()> {
    if secret_key.trim().is_empty() {
        return Err(ProviderError::invalid_request(
            PaymentAdapterOperation::CreatePaymentIntent,
            "Stripe secret key is required",
        ));
    }
    Ok(())
}

/// Maps a method_key to Stripe `payment_method_types[]` values.
///
/// Supported method_keys (mirrors `commerce_payment_method.method_key` DB rows):
/// - `stripe_card`       → `["card"]` (credit/debit cards)
/// - `stripe_apple_pay`  → `["card"]`  (Apple Pay is a card wallet in Stripe;
///   requires Apple Pay enabled in Stripe Dashboard + domain verification)
/// - `stripe_google_pay` → `["card"]`  (Google Pay is a card wallet in Stripe;
///   requires Google Pay enabled in Stripe Dashboard)
/// - `stripe_alipay`     → `["alipay"]`
/// - `stripe_wechat_pay` → `["wechat_pay"]`
///
/// When `None` is returned (unknown method_key), Stripe falls back to
/// `automatic_payment_methods` which surfaces all Dashboard-enabled instruments.
///
/// Apple Pay / Google Pay are not separate `payment_method_types` in Stripe;
/// they are card-based wallets. The Stripe.js frontend automatically renders
/// the Apple Pay / Google Pay button when the PaymentIntent allows `card` and
/// the merchant has the wallet enabled in Dashboard.
fn stripe_payment_method_types_for_key(method_key: &str) -> Option<&'static [&'static str]> {
    match method_key {
        "stripe_card" | "stripe_apple_pay" | "stripe_google_pay" => Some(&["card"]),
        "stripe_alipay" => Some(&["alipay"]),
        "stripe_wechat_pay" => Some(&["wechat_pay"]),
        _ => None,
    }
}

fn hex_encode(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stripe_refund_query_matches_the_merchant_refund_number() {
        let response = serde_json::json!({
            "data": [
                {"id": "re_other", "metadata": {"refund_no": "refund-other"}},
                {"id": "re_expected", "status": "succeeded", "metadata": {"refund_no": "refund-1"}}
            ]
        });
        let matched =
            stripe_refund_by_merchant_no(response, "refund-1").expect("matching Stripe refund");
        assert_eq!(matched["id"], "re_expected");
        assert!(stripe_refund_by_merchant_no(serde_json::json!({"data": []}), "refund-1").is_err());
        let truncated = stripe_refund_by_merchant_no(
            serde_json::json!({"data": [], "has_more": true}),
            "refund-1",
        )
        .expect_err("a truncated list cannot prove refund absence");
        assert!(matches!(
            truncated,
            ProviderError::InvalidResponse { message, .. } if message.contains("another page")
        ));
    }

    #[test]
    fn verify_stripe_webhook_signature_accepts_valid_hmac() {
        let secret = "whsec_test_secret";
        let body = br#"{"id":"evt_123","type":"payment_intent.succeeded"}"#;
        let timestamp = 1_700_000_000_i64;
        let signature = stripe_test_signature(secret, timestamp, body);

        assert!(verify_stripe_signature_at(
            secret, &signature, body, timestamp, 300
        ));
        assert!(!verify_stripe_signature_at(
            secret,
            &signature,
            body,
            timestamp + 301,
            300
        ));
        assert!(!verify_stripe_signature_at(
            secret,
            &signature,
            body,
            timestamp - 301,
            300
        ));
        assert!(!verify_stripe_signature(secret, "t=1,v1=deadbeef", body));
    }

    fn stripe_test_signature(secret: &str, timestamp: i64, body: &[u8]) -> String {
        let signed_payload = format!("{timestamp}.");
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(signed_payload.as_bytes());
        mac.update(body);
        format!(
            "t={timestamp},v1={}",
            hex_encode(mac.finalize().into_bytes())
        )
    }
}
