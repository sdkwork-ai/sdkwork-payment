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
    /// Completes a two-phase provider payment whose buyer flow ends in an
    /// approval that still needs a merchant capture (PayPal Orders v2).
    CapturePaymentIntent,
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

impl PaymentProviderCapabilities {
    /// True when the adapter declares support for `operation`.
    #[must_use]
    pub fn supports_operation(&self, operation: PaymentAdapterOperation) -> bool {
        self.operations.contains(&operation)
    }
}

/// One payer-supplied input a product's checkout cannot be created without.
///
/// Several products address a specific payer instead of "whoever scans this":
/// WeChat JSAPI needs the payer's `openid`, WeChat H5 needs the browser IP the
/// H5 trade is scoped to, Alipay JSAPI needs the buyer's `user_id`. Reading it
/// off the adapter contract (instead of an admin-side table) keeps the operator
/// form and the provider request from drifting apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaymentProductRequirement {
    /// Wire method key the requirement belongs to (`wechat_jsapi`, …).
    pub method_key: &'static str,
    /// Metadata key the adapter reads (`openid`, `client_ip`, `buyer_id`).
    pub metadata_key: &'static str,
    /// Human description used by operator-facing diagnostics.
    pub label: &'static str,
}

const PAYER_REQUIREMENTS: &[PaymentProductRequirement] = &[
    PaymentProductRequirement {
        method_key: "wechat_jsapi",
        metadata_key: "openid",
        label: "payer openid (the WeChat user paying)",
    },
    PaymentProductRequirement {
        method_key: "wechat_h5",
        metadata_key: "client_ip",
        label: "payer client IP (the browser paying)",
    },
    PaymentProductRequirement {
        method_key: "alipay_jsapi",
        metadata_key: "buyer_id",
        label: "buyer user id (the Alipay user paying)",
    },
];

/// Every payer input `method_key` requires.
///
/// The table groups one product's inputs contiguously, so the run is returned as
/// a borrowed slice of the static table instead of an allocation.
#[must_use]
pub fn product_payer_requirements(method_key: &str) -> &'static [PaymentProductRequirement] {
    const EMPTY: &[PaymentProductRequirement] = &[];
    let normalized = method_key.trim();
    if normalized.is_empty() {
        return EMPTY;
    }
    let belongs = |requirement: &PaymentProductRequirement| {
        requirement.method_key.eq_ignore_ascii_case(normalized)
    };
    let Some(start) = PAYER_REQUIREMENTS.iter().position(|item| belongs(item)) else {
        return EMPTY;
    };
    let run = PAYER_REQUIREMENTS[start..]
        .iter()
        .take_while(|item| belongs(item))
        .count();
    &PAYER_REQUIREMENTS[start..start + run]
}

/// Every provider adapter this build ships, whether or not the deployment has
/// the credentials to construct it.
///
/// A registry only registers an adapter when its credentials exist, which is
/// correct for money movement but hides the declaration from diagnostics. The
/// admin one-cent test console needs to say "WeChat Pay has no active account"
/// instead of "unknown provider", so the static declarations stay readable.
pub fn declared_provider_capabilities(
    provider_code: &str,
) -> Option<&'static PaymentProviderCapabilities> {
    let normalized = normalize_provider_code(provider_code);
    let capabilities: [&'static PaymentProviderCapabilities; 5] = [
        crate::stripe::stripe_capabilities(),
        crate::alipay::alipay_capabilities(),
        crate::wechat_pay::wechat_pay_capabilities(),
        crate::paypal::paypal_capabilities(),
        crate::sandbox_webhook::sandbox_capabilities(),
    ];
    capabilities
        .iter()
        .find(|candidate| candidate.provider_code == normalized)
        .copied()
}

/// Provider codes with a static adapter declaration, sorted.
pub fn declared_provider_codes() -> Vec<&'static str> {
    let mut codes = vec!["alipay", "paypal", "sandbox", "stripe", "wechat_pay"];
    codes.sort_unstable();
    codes
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

    /// Captures a payment whose buyer approval is already recorded.
    ///
    /// Providers that take the money on create inherit this default and report
    /// the operation as unsupported; only a channel whose buyer flow stops at
    /// approval (PayPal Orders v2) overrides it.
    fn capture_payment_intent<'a>(
        &'a self,
        request: PaymentCapturePaymentIntentRequest,
    ) -> PaymentAdapterFuture<'a, PaymentProviderOperationOutcome> {
        let _ = request;
        Box::pin(async move {
            Err(crate::error::ProviderError::invalid_request(
                PaymentAdapterOperation::CapturePaymentIntent,
                "this provider does not require a separate capture",
            ))
        })
    }

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

/// Requests the capture of a provider order that stopped at buyer approval.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PaymentCapturePaymentIntentRequest {
    pub payment_intent_id: Option<String>,
    /// Idempotency key for the capture call (`PayPal-Request-Id`).
    pub idempotency_key: Option<String>,
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

    #[test]
    fn operation_declaration_is_readable_by_data() {
        assert!(CNY_ONLY.supports_operation(PaymentAdapterOperation::CreatePaymentIntent));
        assert!(!CNY_ONLY.supports_operation(PaymentAdapterOperation::CreateRefund));
    }

    #[test]
    fn payer_requirements_are_declared_per_product_and_fail_closed() {
        let openid = product_payer_requirements("wechat_jsapi");
        assert_eq!(openid.len(), 1);
        assert_eq!(openid[0].metadata_key, "openid");
        // Case and surrounding space are tolerated exactly like the adapters'.
        assert_eq!(product_payer_requirements(" WeChat_JSAPI "), openid);

        let client_ip = product_payer_requirements("wechat_h5");
        assert_eq!(client_ip.len(), 1);
        assert_eq!(client_ip[0].metadata_key, "client_ip");

        let buyer_id = product_payer_requirements("alipay_jsapi");
        assert_eq!(buyer_id.len(), 1);
        assert_eq!(buyer_id[0].metadata_key, "buyer_id");

        // Products that address "whoever pays" declare no payer input.
        for method_key in [
            "wechat_native",
            "wechat_app",
            "alipay_qr",
            "alipay_pc",
            "alipay_wap",
            "alipay_app",
            "stripe_card",
            "paypal",
            "sandbox_test",
            "",
            "wechat_unknown",
        ] {
            assert!(
                product_payer_requirements(method_key).is_empty(),
                "{method_key} must not demand a payer input"
            );
        }
    }
}