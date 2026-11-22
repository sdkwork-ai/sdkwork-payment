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

/// Currencies an adapter can actually settle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurrencyPolicy {
    /// Every ISO currency whose minor unit is 1/100. Non-two-decimal
    /// currencies stay rejected until the storage scale is migrated.
    AnyTwoDecimal,
    /// Exactly these ISO-4217 codes (e.g. WeChat Pay and Alipay settle in CNY).
    Only(&'static [&'static str]),
}

/// Which refunds an adapter can express.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefundMode {
    /// No refund API at all.
    Unsupported,
    /// Refund API that accepts any amount up to the captured total.
    FullOrPartial,
}

/// What an adapter declares it can do, as data rather than as code paths.
///
/// The cashier, the admin surface and the checkout guards read this instead of
/// switching on `provider_code`, which is what keeps a new channel additive.
/// `products` are the wire method keys the adapter serves; each adapter asserts
/// in its own tests that every declared key reaches a real upstream product, so a
/// declaration cannot drift away from the implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaymentProviderCapabilities {
    pub provider_code: &'static str,
    pub operations: &'static [PaymentAdapterOperation],
    /// Wire method keys this adapter can serve (`wechat_jsapi`, `alipay_wap`, …).
    pub products: &'static [&'static str],
    pub currencies: CurrencyPolicy,
    pub refund_mode: RefundMode,
}

impl PaymentProviderCapabilities {
    /// True when the adapter declares support for `method_key`.
    #[must_use]
    pub fn supports_product(&self, method_key: &str) -> bool {
        self.products
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(method_key.trim()))
    }

    /// True when the adapter can settle `currency_code`.
    #[must_use]
    pub fn supports_currency(&self, currency_code: &str) -> bool {
        let code = currency_code.trim().to_ascii_uppercase();
        match self.currencies {
            CurrencyPolicy::AnyTwoDecimal => {
                crate::money::ensure_currency_supported(&code).is_ok()
            }
            CurrencyPolicy::Only(codes) => codes.iter().any(|candidate| *candidate == code),
        }
    }
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
    /// Refund-result callback URL for channels whose refund contract carries
    /// one (`RefundNotifyCapability::DedicatedUrl`). Channels that deliver
    /// refund events on the payment URL must receive `None` so no channel is
    /// ever asked to call a URL its contract does not define.
    pub notify_url: Option<String>,
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

/// Validates a merchant trade/reference number that will be interpolated
/// into a WeChat Pay V3 URL path (`out-trade-no/{...}` / `refunds/{...}`).
/// WeChat allows 1-64 `[A-Za-z0-9-_]` characters; rejecting everything else
/// keeps a crafted number from steering a signed request at a different
/// endpoint (mirrors Stripe's resource-id guard).
pub(crate) fn require_url_safe_trade_no(
    value: &str,
    operation: PaymentAdapterOperation,
    field: &str,
) -> ProviderResult<String> {
    let trimmed = value.trim();
    let valid_length = (1..=64).contains(&trimmed.len());
    let valid_characters = trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
    if valid_length && valid_characters {
        Ok(trimmed.to_owned())
    } else {
        Err(crate::error::ProviderError::invalid_request(
            operation,
            format!(
                "{field} must be 1 to 64 letters, digits, hyphens, or underscores"
            ),
        ))
    }
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

#[cfg(test)]
mod capability_tests {
    use super::*;

    const CNY_ONLY: PaymentProviderCapabilities = PaymentProviderCapabilities {
        provider_code: "test",
        operations: &[PaymentAdapterOperation::CreatePaymentIntent],
        products: &["wechat_jsapi", "wechat_h5"],
        currencies: CurrencyPolicy::Only(&["CNY"]),
        refund_mode: RefundMode::FullOrPartial,
    };

    const ANY_TWO_DECIMAL: PaymentProviderCapabilities = PaymentProviderCapabilities {
        provider_code: "test",
        operations: &[PaymentAdapterOperation::CreatePaymentIntent],
        products: &["stripe_card"],
        currencies: CurrencyPolicy::AnyTwoDecimal,
        refund_mode: RefundMode::FullOrPartial,
    };

    #[test]
    fn product_lookup_ignores_case_and_surrounding_space() {
        assert!(CNY_ONLY.supports_product("wechat_jsapi"));
        assert!(CNY_ONLY.supports_product(" WeChat_JSAPI "));
        assert!(!CNY_ONLY.supports_product("wechat_native"));
        assert!(!CNY_ONLY.supports_product(""));
    }

    #[test]
    fn a_cny_only_channel_never_advertises_another_currency() {
        assert!(CNY_ONLY.supports_currency("CNY"));
        assert!(CNY_ONLY.supports_currency(" cny "));
        assert!(!CNY_ONLY.supports_currency("USD"));
        assert!(!CNY_ONLY.supports_currency("JPY"));
    }

    #[test]
    fn any_two_decimal_still_rejects_currencies_the_storage_cannot_hold() {
        assert!(ANY_TWO_DECIMAL.supports_currency("USD"));
        assert!(ANY_TWO_DECIMAL.supports_currency("eur"));
        // The ISO exponent table decides, not a separate list.
        assert!(!ANY_TWO_DECIMAL.supports_currency("JPY"));
        assert!(!ANY_TWO_DECIMAL.supports_currency("KWD"));
        assert!(!ANY_TWO_DECIMAL.supports_currency("EURO"));
    }
}