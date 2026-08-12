use std::future::Future;
use std::pin::Pin;

use serde_json::Value;

use crate::error::ProviderResult;

pub type PaymentAdapterFuture<'a, T> = Pin<Box<dyn Future<Output = ProviderResult<T>> + Send + 'a>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaymentAdapterOperation {
    CreatePaymentIntent,
    QueryPaymentIntent,
    CancelPaymentIntent,
    CreateRefund,
    QueryRefund,
    VerifyWebhook,
    NormalizeWebhook,
}

impl std::fmt::Display for PaymentAdapterOperation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentProviderCapabilities {
    pub provider_code: &'static str,
    pub operations: &'static [PaymentAdapterOperation],
}

pub trait PaymentProviderAdapter: Send + Sync {
    fn capabilities(&self) -> &'static PaymentProviderCapabilities;

    fn create_payment_intent<'a>(
        &'a self,
        request: PaymentCreateIntentRequest,
    ) -> PaymentAdapterFuture<'a, PaymentProviderOperationOutcome>;

    fn query_payment_intent<'a>(
        &'a self,
        request: PaymentQueryPaymentIntentRequest,
    ) -> PaymentAdapterFuture<'a, PaymentProviderOperationOutcome>;

    fn cancel_payment_intent<'a>(
        &'a self,
        request: PaymentCancelPaymentIntentRequest,
    ) -> PaymentAdapterFuture<'a, PaymentProviderOperationOutcome>;

    fn create_refund<'a>(
        &'a self,
        request: PaymentCreateRefundRequest,
    ) -> PaymentAdapterFuture<'a, PaymentProviderOperationOutcome>;

    fn query_refund<'a>(
        &'a self,
        request: PaymentQueryRefundRequest,
    ) -> PaymentAdapterFuture<'a, PaymentProviderOperationOutcome>;

    fn verify_webhook<'a>(
        &'a self,
        request: PaymentVerifyWebhookRequest,
    ) -> PaymentAdapterFuture<'a, PaymentWebhookVerificationOutcome>;

    fn normalize_webhook<'a>(
        &'a self,
        request: PaymentNormalizeWebhookRequest,
    ) -> PaymentAdapterFuture<'a, PaymentNormalizedWebhookEvent>;
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PaymentCreateIntentRequest {
    pub tenant_id: Option<String>,
    pub merchant_order_no: Option<String>,
    pub amount_minor: Option<i64>,
    pub currency: Option<String>,
    /// Effective notify URL resolved by the order gateway checkout
    /// (explicit override or the deployment standard order webhook URL).
    /// Providers call this URL on asynchronous payment results; adapters
    /// prefer it over their per-account/env config value.
    pub notify_url: Option<String>,
    pub expires_at: Option<String>,
    pub payment_scene: Option<String>,
    pub metadata: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PaymentQueryPaymentIntentRequest {
    pub payment_intent_id: Option<String>,
    pub metadata: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PaymentCancelPaymentIntentRequest {
    pub payment_intent_id: Option<String>,
    pub reason: Option<String>,
    pub metadata: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PaymentCreateRefundRequest {
    pub payment_intent_id: Option<String>,
    pub refund_no: Option<String>,
    pub amount_minor: Option<i64>,
    pub reason: Option<String>,
    pub metadata: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PaymentQueryRefundRequest {
    pub refund_id: Option<String>,
    pub refund_no: Option<String>,
    pub metadata: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PaymentVerifyWebhookRequest {
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub metadata: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PaymentNormalizeWebhookRequest {
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub metadata: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PaymentProviderOperationOutcome {
    pub provider_code: String,
    pub native_id: Option<String>,
    pub raw_status: Option<String>,
    pub payload: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PaymentWebhookVerificationOutcome {
    pub verified: bool,
    pub provider_event_id: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PaymentNormalizedWebhookEvent {
    pub provider_code: String,
    pub event_type: Option<String>,
    pub provider_event_id: Option<String>,
    pub out_trade_no: Option<String>,
    pub payment_status: Option<String>,
    pub payload: Value,
}

pub(crate) fn require_positive_amount(
    amount: Option<i64>,
    operation: PaymentAdapterOperation,
    field: &str,
) -> ProviderResult<i64> {
    match amount {
        Some(amount) if amount > 0 => Ok(amount),
        _ => Err(crate::error::ProviderError::invalid_request(
            operation,
            format!("{field} must be a positive minor-unit amount"),
        )),
    }
}

pub(crate) fn require_non_empty(
    value: Option<&str>,
    operation: PaymentAdapterOperation,
    field: &str,
) -> ProviderResult<String> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Err(crate::error::ProviderError::invalid_request(
            operation,
            format!("{field} is required"),
        ));
    };
    Ok(value.to_owned())
}

pub(crate) fn normalized_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub(crate) fn metadata_string<'a>(metadata: &'a Value, key: &str) -> Option<&'a str> {
    metadata
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub fn normalize_provider_code(provider_code: &str) -> String {
    provider_code.trim().to_ascii_lowercase().replace('-', "_")
}
