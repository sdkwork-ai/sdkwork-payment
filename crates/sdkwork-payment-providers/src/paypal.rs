use std::fmt;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde_json::{json, Value};

use crate::adapter::{
    metadata_string, require_non_empty, require_positive_amount, PaymentAdapterFuture,
    PaymentAdapterOperation, PaymentCancelPaymentIntentRequest, PaymentCreateIntentRequest,
    PaymentCreateRefundRequest, PaymentNormalizeWebhookRequest, PaymentNormalizedWebhookEvent,
    PaymentProviderAdapter, PaymentProviderCapabilities, PaymentProviderOperationOutcome,
    PaymentQueryPaymentIntentRequest, PaymentQueryRefundRequest, PaymentVerifyWebhookRequest,
    PaymentWebhookVerificationOutcome,
};
use crate::error::{ProviderError, ProviderResult};
use crate::http::ReqwestHttpClient;
use crate::money::minor_to_decimal_string;

pub const PAYPAL_PROVIDER_CODE: &str = "paypal";
const PAYPAL_LIVE_API_BASE_URL: &str = "https://api-m.paypal.com";
const PAYPAL_SANDBOX_API_BASE_URL: &str = "https://api-m.sandbox.paypal.com";
const PAYPAL_TOKEN_PATH: &str = "/v1/oauth2/token";
/// OAuth tokens are cached until shortly before their real expiry so a call
/// landing exactly at the boundary never sends a just-expired bearer token.
const TOKEN_REFRESH_SKEW_SECONDS: u64 = 60;

static PAYPAL_CAPABILITIES: PaymentProviderCapabilities = PaymentProviderCapabilities {
    provider_code: PAYPAL_PROVIDER_CODE,
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
pub struct PayPalPaymentProviderConfig {
    pub client_id: String,
    pub client_secret: String,
    /// Webhook ID from the PayPal developer dashboard; the remote
    /// `verify-webhook-signature` call fails closed without it.
    pub webhook_id: Option<String>,
    pub sandbox: bool,
}

impl fmt::Debug for PayPalPaymentProviderConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PayPalPaymentProviderConfig")
            .field("client_id", &"[REDACTED]")
            .field("client_secret", &"<redacted>")
            .field(
                "webhook_id",
                &self.webhook_id.as_ref().map(|_| "<redacted>"),
            )
            .field("sandbox", &self.sandbox)
            .finish()
    }
}

#[derive(Default)]
struct TokenCache {
    inner: Mutex<Option<CachedToken>>,
}

struct CachedToken {
    access_token: String,
    expires_at: Instant,
}

pub struct PayPalPaymentProviderAdapter {
    config: PayPalPaymentProviderConfig,
    api: ReqwestHttpClient,
    token_base: ReqwestHttpClient,
    tokens: TokenCache,
}

impl PayPalPaymentProviderAdapter {
    pub fn with_default_http_client(config: PayPalPaymentProviderConfig) -> ProviderResult<Self> {
        if config.client_id.trim().is_empty() {
            return Err(ProviderError::invalid_request(
                PaymentAdapterOperation::CreatePaymentIntent,
                "PayPal client_id is required",
            ));
        }
        if config.client_secret.trim().is_empty() {
            return Err(ProviderError::invalid_request(
                PaymentAdapterOperation::CreatePaymentIntent,
                "PayPal client_secret is required",
            ));
        }
        if let Some(webhook_id) = &config.webhook_id {
            if webhook_id.trim().is_empty() {
                return Err(ProviderError::invalid_request(
                    PaymentAdapterOperation::VerifyWebhook,
                    "PayPal webhook_id must not be empty when configured",
                ));
            }
        }
        let base = if config.sandbox {
            PAYPAL_SANDBOX_API_BASE_URL
        } else {
            PAYPAL_LIVE_API_BASE_URL
        };
        Ok(Self {
            api: ReqwestHttpClient::new(base)?,
            token_base: ReqwestHttpClient::new(base)?,
            config,
            tokens: TokenCache::default(),
        })
    }

    /// Client-credentials access token with in-process caching. The cache
    /// lock is never held across the token fetch: a stale/expired token
    /// drops the guard first and refreshes outside it.
    async fn access_token(&self) -> ProviderResult<String> {
        if let Some(token) = self.cached_token() {
            return Ok(token);
        }
        let basic = format!("{}:{}", self.config.client_id, self.config.client_secret);
        let headers = vec![
            (
                "Authorization".to_owned(),
                format!("Basic {}", BASE64.encode(basic.as_bytes())),
            ),
            ("Content-Type".to_owned(), "application/x-www-form-urlencoded".to_owned()),
        ];
        let url = PAYPAL_TOKEN_PATH.to_owned();
        let response = self
            .token_base
            .request_with_headers_detailed(
                PAYPAL_PROVIDER_CODE,
                "POST",
                &url,
                b"grant_type=client_credentials".to_vec(),
                headers,
            )
            .await?;
        let access_token = response
            .body
            .get("access_token")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| {
                ProviderError::invalid_response(
                    PaymentAdapterOperation::CreatePaymentIntent,
                    "PayPal OAuth token response is missing access_token",
                )
            })?;
        let expires_in = response
            .body
            .get("expires_in")
            .and_then(Value::as_i64)
            .unwrap_or((TOKEN_REFRESH_SKEW_SECONDS * 2) as i64);
        let ttl = Duration::from_secs(
            expires_in
                .clamp((TOKEN_REFRESH_SKEW_SECONDS * 2) as i64, 3600)
                as u64,
        )
        .saturating_sub(Duration::from_secs(TOKEN_REFRESH_SKEW_SECONDS));
        self.store_token(CachedToken {
            access_token: access_token.clone(),
            expires_at: Instant::now() + ttl,
        });
        Ok(access_token)
    }

    fn cached_token(&self) -> Option<String> {
        let guard = self.tokens.inner.lock().ok()?;
        let cached = guard.as_ref()?;
        if cached.expires_at > Instant::now() {
            Some(cached.access_token.clone())
        } else {
            None
        }
    }

    fn store_token(&self, token: CachedToken) {
        if let Ok(mut guard) = self.tokens.inner.lock() {
            *guard = Some(token);
        }
    }

    async fn send_json(
        &self,
        method: &str,
        path: &str,
        payload: Option<Value>,
        request_id: Option<&str>,
    ) -> ProviderResult<Value> {
        let token = self.access_token().await?;
        let body = match payload {
            Some(payload) => serde_json::to_vec(&payload).map_err(|error| {
                ProviderError::invalid_request(
                    PaymentAdapterOperation::CreatePaymentIntent,
                    format!("PayPal request payload could not be serialized: {error}"),
                )
            })?,
            None => Vec::new(),
        };
        let mut headers = vec![
            ("Authorization".to_owned(), format!("Bearer {token}")),
            ("Accept".to_owned(), "application/json".to_owned()),
        ];
        if !body.is_empty() {
            headers.push(("Content-Type".to_owned(), "application/json".to_owned()));
        }
        // PayPal-Request-Id is PayPal's idempotency key: retries with the
        // same value return the original result instead of creating a
        // duplicate order/refund.
        if let Some(request_id) = request_id {
            headers.push(("PayPal-Request-Id".to_owned(), request_id.to_owned()));
        }
        let url = format!("{}{path}", self.base_url());
        let response = self
            .api
            .request_with_headers_detailed(PAYPAL_PROVIDER_CODE, method, &url, body, headers)
            .await?;
        Ok(response.body)
    }

    fn base_url(&self) -> &'static str {
        if self.config.sandbox {
            PAYPAL_SANDBOX_API_BASE_URL
        } else {
            PAYPAL_LIVE_API_BASE_URL
        }
    }
}

impl PaymentProviderAdapter for PayPalPaymentProviderAdapter {
    fn capabilities(&self) -> &'static PaymentProviderCapabilities {
        &PAYPAL_CAPABILITIES
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
            let out_trade_no = require_non_empty(
                request.merchant_order_no.as_deref(),
                PaymentAdapterOperation::CreatePaymentIntent,
                "merchant_order_no",
            )?;
            require_url_safe_trade_no(
                &out_trade_no,
                PaymentAdapterOperation::CreatePaymentIntent,
                "merchant_order_no",
            )?;
            let request_id = metadata_string(&request.metadata, "idempotency_key");
            // `custom_id` rides the order and every capture/rename webhook,
            // which is how settlement resolves the local attempt;
            // `invoice_id` additionally gives PayPal-side duplicate-payment
            // protection for the same merchant trade number.
            let body = json!({
                "intent": "CAPTURE",
                "purchase_units": [{
                    "custom_id": out_trade_no,
                    "invoice_id": out_trade_no,
                    "amount": {
                        "currency_code": currency,
                        "value": minor_to_decimal_string(amount_minor),
                    },
                }],
            });
            let response = self
                .send_json(
                    "POST",
                    "/v2/checkout/orders",
                    Some(body),
                    request_id,
                )
                .await?;
            paypal_order_outcome(PaymentAdapterOperation::CreatePaymentIntent, response)
        })
    }

    fn query_payment_intent<'a>(
        &'a self,
        request: PaymentQueryPaymentIntentRequest,
    ) -> PaymentAdapterFuture<'a, PaymentProviderOperationOutcome> {
        Box::pin(async move {
            let order_id = require_paypal_resource_id(
                request.payment_intent_id.as_deref(),
                PaymentAdapterOperation::QueryPaymentIntent,
                "payment_intent_id",
            )?;
            let response = self
                .send_json("GET", &format!("/v2/checkout/orders/{order_id}"), None, None)
                .await?;
            paypal_order_outcome(PaymentAdapterOperation::QueryPaymentIntent, response)
        })
    }

    /// PayPal Orders v2 has no server-side cancel for an unapproved order:
    /// an order the payer never approves simply expires. Cancellation here
    /// re-reads the current state so the caller sees COMPLETED (money moved)
    /// as a conflict, and any other state as "nothing to cancel".
    fn cancel_payment_intent<'a>(
        &'a self,
        request: PaymentCancelPaymentIntentRequest,
    ) -> PaymentAdapterFuture<'a, PaymentProviderOperationOutcome> {
        Box::pin(async move {
            let order_id = require_paypal_resource_id(
                request.payment_intent_id.as_deref(),
                PaymentAdapterOperation::CancelPaymentIntent,
                "payment_intent_id",
            )?;
            let response = self
                .send_json("GET", &format!("/v2/checkout/orders/{order_id}"), None, None)
                .await?;
            paypal_order_outcome(PaymentAdapterOperation::CancelPaymentIntent, response)
        })
    }

    /// Refunds a capture resolved from the stored order id. PayPal refunds
    /// address a `capture_id`, not the order; the order is fetched first and
    /// its (single) capture is refunded with the exact requested amount in
    /// the capture's own currency.
    fn create_refund<'a>(
        &'a self,
        request: PaymentCreateRefundRequest,
    ) -> PaymentAdapterFuture<'a, PaymentProviderOperationOutcome> {
        Box::pin(async move {
            let order_reference = require_non_empty(
                request.payment_intent_id.as_deref(),
                PaymentAdapterOperation::CreateRefund,
                "payment_intent_id",
            )?;
            let order_id = require_paypal_resource_id(
                Some(&order_reference),
                PaymentAdapterOperation::CreateRefund,
                "payment_intent_id",
            )?;
            let amount_minor = require_positive_amount(
                request.amount_minor,
                PaymentAdapterOperation::CreateRefund,
                "amount_minor",
            )?;
            let request_id = metadata_string(&request.metadata, "idempotency_key");
            let order = self
                .send_json("GET", &format!("/v2/checkout/orders/{order_id}"), None, None)
                .await?;
            let capture = paypal_capture_resource(order, PaymentAdapterOperation::CreateRefund)?;
            let capture_id = capture
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| {
                    ProviderError::invalid_response(
                        PaymentAdapterOperation::CreateRefund,
                        "PayPal capture response is missing id",
                    )
                })?;
            let currency = require_currency(
                capture
                    .pointer("/amount/currency_code")
                    .and_then(Value::as_str),
                PaymentAdapterOperation::CreateRefund,
            )?;
            let body = json!({
                "amount": {
                    "currency_code": currency,
                    "value": minor_to_decimal_string(amount_minor),
                },
            });
            let response = self
                .send_json(
                    "POST",
                    &format!("/v2/payments/captures/{capture_id}/refund"),
                    Some(body),
                    request_id,
                )
                .await?;
            paypal_refund_outcome(PaymentAdapterOperation::CreateRefund, response)
        })
    }

    /// Refund state query. A stored PayPal refund id is fetched directly;
    /// otherwise the capture state is reported conservatively — refund
    /// completeness is only claimed from an explicit refund object, never
    /// inferred from a partially-refunded capture.
    fn query_refund<'a>(
        &'a self,
        request: PaymentQueryRefundRequest,
    ) -> PaymentAdapterFuture<'a, PaymentProviderOperationOutcome> {
        Box::pin(async move {
            if let Some(refund_id) = request
                .refund_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                let refund_id = require_paypal_resource_id(
                    Some(refund_id),
                    PaymentAdapterOperation::QueryRefund,
                    "refund_id",
                )?;
                let response = self
                    .send_json("GET", &format!("/v2/payments/refunds/{refund_id}"), None, None)
                    .await?;
                return paypal_refund_outcome(PaymentAdapterOperation::QueryRefund, response);
            }
            let order_reference = require_paypal_resource_id(
                metadata_string(&request.metadata, "payment_intent_id"),
                PaymentAdapterOperation::QueryRefund,
                "metadata.payment_intent_id",
            )?;
            let order = self
                .send_json("GET", &format!("/v2/checkout/orders/{order_reference}"), None, None)
                .await?;
            let capture = paypal_capture_resource(
                order,
                PaymentAdapterOperation::QueryRefund,
            )?;
            let status = capture
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or_default();
            // COMPLETED means the refund has not landed yet (the capture is
            // intact) — reported as still processing so the compensation
            // loop keeps the row claimable instead of inventing a terminal
            // state PayPal has not confirmed.
            let raw_status = match status.to_ascii_uppercase().as_str() {
                "REFUNDED" => "REFUNDED".to_owned(),
                "PARTIALLY_REFUNDED" | "COMPLETED" => "PROCESSING".to_owned(),
                other => other.to_owned(),
            };
            Ok(PaymentProviderOperationOutcome {
                provider_code: PAYPAL_PROVIDER_CODE.to_owned(),
                native_id: capture
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                raw_status: Some(raw_status),
                payload: capture,
            })
        })
    }

    /// PayPal webhooks are verified remotely: the transmission headers and
    /// the raw event body are posted to
    /// `/v1/notifications/verify-webhook-signature` together with the
    /// account's `webhook_id`, and only `verification_status: SUCCESS`
    /// counts as verified. Without a configured webhook_id the check fails
    /// closed.
    fn verify_webhook<'a>(
        &'a self,
        request: PaymentVerifyWebhookRequest,
    ) -> PaymentAdapterFuture<'a, PaymentWebhookVerificationOutcome> {
        Box::pin(async move {
            let Some(webhook_id) = self.config.webhook_id.as_deref() else {
                return Err(ProviderError::invalid_request(
                    PaymentAdapterOperation::VerifyWebhook,
                    "PayPal webhook_id is required to verify webhook deliveries",
                ));
            };
            let event = serde_json::from_slice::<Value>(&request.body).map_err(|error| {
                ProviderError::invalid_response(
                    PaymentAdapterOperation::VerifyWebhook,
                    format!("PayPal webhook JSON is invalid: {error}"),
                )
            })?;
            let transmission = |name: &str| find_header(&request.headers, name);
            let Some(auth_algo) = transmission("paypal-auth-algo") else {
                return Ok(unverified());
            };
            let Some(cert_url) = transmission("paypal-cert-url") else {
                return Ok(unverified());
            };
            let Some(transmission_id) = transmission("paypal-transmission-id") else {
                return Ok(unverified());
            };
            let Some(transmission_sig) = transmission("paypal-transmission-sig") else {
                return Ok(unverified());
            };
            let Some(transmission_time) = transmission("paypal-transmission-time") else {
                return Ok(unverified());
            };
            let body = json!({
                "auth_algo": auth_algo,
                "cert_url": cert_url,
                "transmission_id": transmission_id,
                "transmission_sig": transmission_sig,
                "transmission_time": transmission_time,
                "webhook_id": webhook_id,
                "webhook_event": event,
            });
            let response = self
                .send_json("POST", "/v1/notifications/verify-webhook-signature", Some(body), None)
                .await?;
            let verified = response.get("verification_status").and_then(Value::as_str)
                == Some("SUCCESS");
            Ok(PaymentWebhookVerificationOutcome {
                verified,
                provider_event_id: if verified {
                    event.get("id").and_then(Value::as_str).map(str::to_owned)
                } else {
                    None
                },
            })
        })
    }

    /// Normalizes a PayPal webhook event. `out_trade_no` comes from the
    /// `custom_id` the order carried (captures duplicate it at the top of
    /// the resource; orders nest it under purchase_units). `payment_status`
    /// is the PayPal order/capture status vocabulary, mapped per event type;
    /// unknown events keep the payment untouched (status None) while the
    /// event itself is still recorded.
    fn normalize_webhook<'a>(
        &'a self,
        request: PaymentNormalizeWebhookRequest,
    ) -> PaymentAdapterFuture<'a, PaymentNormalizedWebhookEvent> {
        Box::pin(async move {
            let payload = serde_json::from_slice::<Value>(&request.body).map_err(|error| {
                ProviderError::invalid_response(
                    PaymentAdapterOperation::NormalizeWebhook,
                    format!("PayPal webhook JSON is invalid: {error}"),
                )
            })?;
            let resource = payload.get("resource");
            let out_trade_no = resource
                .and_then(|value| value.get("custom_id"))
                .or_else(|| payload.pointer("/resource/purchase_units/0/custom_id"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned);
            let event_type = payload
                .get("event_type")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let payment_status = event_type
                .as_deref()
                .and_then(paypal_event_payment_status)
                .map(str::to_owned);
            Ok(PaymentNormalizedWebhookEvent {
                provider_code: PAYPAL_PROVIDER_CODE.to_owned(),
                event_type,
                provider_event_id: payload.get("id").and_then(Value::as_str).map(str::to_owned),
                out_trade_no,
                payment_status,
                payload,
            })
        })
    }
}

fn unverified() -> PaymentWebhookVerificationOutcome {
    PaymentWebhookVerificationOutcome {
        verified: false,
        provider_event_id: None,
    }
}

fn paypal_order_outcome(
    operation: PaymentAdapterOperation,
    response: Value,
) -> ProviderResult<PaymentProviderOperationOutcome> {
    let native_id = response
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| {
            ProviderError::invalid_response(operation, "PayPal order response is missing id")
        })?;
    let mut payload = response;
    let approve_url = payload
        .get("links")
        .and_then(Value::as_array)
        .and_then(|links| {
            links
                .iter()
                .find(|link| link.get("rel").and_then(Value::as_str) == Some("approve"))
                .and_then(|link| link.get("href"))
                .and_then(Value::as_str)
        })
        .map(str::to_owned);
    if let Some(approve_url) = approve_url {
        if let Some(object) = payload.as_object_mut() {
            object.insert("approve_url".to_owned(), Value::String(approve_url));
        }
    }
    Ok(PaymentProviderOperationOutcome {
        provider_code: PAYPAL_PROVIDER_CODE.to_owned(),
        native_id: Some(native_id),
        raw_status: payload
            .get("status")
            .and_then(Value::as_str)
            .map(str::to_owned),
        payload,
    })
}

fn paypal_refund_outcome(
    operation: PaymentAdapterOperation,
    response: Value,
) -> ProviderResult<PaymentProviderOperationOutcome> {
    let native_id = response
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| {
            ProviderError::invalid_response(operation, "PayPal refund response is missing id")
        })?;
    Ok(PaymentProviderOperationOutcome {
        provider_code: PAYPAL_PROVIDER_CODE.to_owned(),
        native_id: Some(native_id),
        raw_status: response
            .get("status")
            .and_then(Value::as_str)
            .map(str::to_owned),
        payload: response,
    })
}

fn paypal_capture_resource(
    order: Value,
    operation: PaymentAdapterOperation,
) -> ProviderResult<Value> {
    order
        .pointer("/purchase_units/0/payments/captures/0")
        .cloned()
        .filter(|capture| capture.get("id").and_then(Value::as_str).is_some())
        .ok_or_else(|| {
            ProviderError::invalid_response(
                operation,
                "PayPal order has no capture to refund or query yet",
            )
        })
}

/// Maps a PayPal webhook event type onto the PayPal order/capture status
/// vocabulary used by `map_provider_payment_status`. Refund events stay
/// `None` on the payment: refund completeness is tracked on
/// `commerce_refund` rows by the refund webhook family, never on the
/// attempt status (mirrors the WeChat `REFUND` handling).
fn paypal_event_payment_status(event_type: &str) -> Option<&'static str> {
    match event_type.trim().to_ascii_uppercase().as_str() {
        "PAYMENT.CAPTURE.COMPLETED" | "CHECKOUT.ORDER.COMPLETED" => Some("COMPLETED"),
        "CHECKOUT.ORDER.APPROVED" => Some("APPROVED"),
        "PAYMENT.CAPTURE.PENDING" => Some("PENDING"),
        "PAYMENT.CAPTURE.DENIED" | "PAYMENT.CAPTURE.DECLINED" => Some("DECLINED"),
        "PAYMENT.CAPTURE.REVERSED" | "CHECKOUT.ORDER.VOIDED" => Some("VOIDED"),
        // PAYMENT.CAPTURE.REFUNDED / REFUND.* settle refunds, not payments.
        _ => None,
    }
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
            "PayPal currency must be an ISO 4217 three-letter code",
        ));
    }
    Ok(currency.to_ascii_uppercase())
}

fn require_url_safe_trade_no(
    value: &str,
    operation: PaymentAdapterOperation,
    field: &str,
) -> ProviderResult<String> {
    let trimmed = value.trim();
    let valid = (1..=64).contains(&trimmed.len())
        && trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
    if valid {
        Ok(trimmed.to_owned())
    } else {
        Err(ProviderError::invalid_request(
            operation,
            format!("{field} must be 1 to 64 letters, digits, hyphens, or underscores"),
        ))
    }
}

fn require_paypal_resource_id(
    value: Option<&str>,
    operation: PaymentAdapterOperation,
    field: &str,
) -> ProviderResult<String> {
    let value = require_non_empty(value, operation, field)?;
    if value.contains('/') || value.contains('?') || value.contains('#') {
        return Err(ProviderError::invalid_request(
            operation,
            format!("PayPal {field} must be a resource id, not a path or URL"),
        ));
    }
    Ok(value)
}

fn find_header<'a>(headers: &'a [(String, String)], header_name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(header_name))
        .map(|(_, value)| value.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn approve_link_is_surfaced_from_order_links() {
        let outcome = paypal_order_outcome(
            PaymentAdapterOperation::CreatePaymentIntent,
            json!({
                "id": "5O190127TN364715T",
                "status": "CREATED",
                "links": [
                    {"rel": "self", "href": "https://api-m.paypal.com/v2/checkout/orders/5O190127TN364715T"},
                    {"rel": "approve", "href": "https://www.paypal.com/checkoutnow?token=5O190127TN364715T"},
                ],
            }),
        )
        .expect("order outcome");
        assert_eq!(outcome.native_id.as_deref(), Some("5O190127TN364715T"));
        assert_eq!(outcome.raw_status.as_deref(), Some("CREATED"));
        assert_eq!(
            outcome.payload["approve_url"],
            "https://www.paypal.com/checkoutnow?token=5O190127TN364715T"
        );
    }

    #[test]
    fn order_without_id_is_rejected() {
        let error = paypal_order_outcome(
            PaymentAdapterOperation::QueryPaymentIntent,
            json!({"status": "CREATED"}),
        )
        .expect_err("missing id must fail");
        assert!(matches!(
            error,
            ProviderError::InvalidResponse { message, .. } if message.contains("missing id")
        ));
    }

    #[test]
    fn capture_is_resolved_from_the_first_purchase_unit() {
        let capture = paypal_capture_resource(
            json!({
                "purchase_units": [{
                    "payments": {"captures": [{"id": "8XA8470355", "status": "COMPLETED"}]}
                }]
            }),
            PaymentAdapterOperation::CreateRefund,
        )
        .expect("capture");
        assert_eq!(capture["id"], "8XA8470355");
        assert!(paypal_capture_resource(json!({}), PaymentAdapterOperation::CreateRefund).is_err());
    }

    #[test]
    fn event_statuses_map_to_the_paypal_status_vocabulary() {
        assert_eq!(
            paypal_event_payment_status("PAYMENT.CAPTURE.COMPLETED"),
            Some("COMPLETED")
        );
        assert_eq!(
            paypal_event_payment_status("CHECKOUT.ORDER.APPROVED"),
            Some("APPROVED")
        );
        assert_eq!(
            paypal_event_payment_status("PAYMENT.CAPTURE.DENIED"),
            Some("DECLINED")
        );
        // Refund events never settle the payment attempt.
        assert_eq!(paypal_event_payment_status("PAYMENT.CAPTURE.REFUNDED"), None);
        assert_eq!(paypal_event_payment_status("CUSTOMER.DISPUTE.CREATED"), None);
    }

    #[test]
    fn resource_ids_reject_path_injection() {
        assert!(require_paypal_resource_id(
            Some("abc/../v1/oauth2/token"),
            PaymentAdapterOperation::QueryPaymentIntent,
            "id"
        )
        .is_err());
        assert!(require_paypal_resource_id(
            Some("5O190127TN364715T"),
            PaymentAdapterOperation::QueryPaymentIntent,
            "id"
        )
        .is_ok());
    }

    #[test]
    fn trade_numbers_must_stay_url_safe() {
        assert!(require_url_safe_trade_no("../token", PaymentAdapterOperation::CreatePaymentIntent, "no").is_err());
        assert!(require_url_safe_trade_no("trade-1", PaymentAdapterOperation::CreatePaymentIntent, "no").is_ok());
    }

    #[test]
    fn currency_is_normalized_to_uppercase_iso_code() {
        assert_eq!(
            require_currency(Some("usd"), PaymentAdapterOperation::CreatePaymentIntent)
                .expect("currency"),
            "USD"
        );
        assert!(require_currency(Some("US"), PaymentAdapterOperation::CreatePaymentIntent).is_err());
        assert!(require_currency(None, PaymentAdapterOperation::CreatePaymentIntent).is_err());
    }
}
