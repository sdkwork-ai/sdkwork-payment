//! Canonical provider notify-URL contract（支付渠道回调通知 URL 标准）.
//!
//! Every notify/webhook URL handed to a payment channel is built here and
//! nowhere else. The HTTP side of the contract is owned by `sdkwork-order`
//! (see `specs/PAYMENT_CALLBACK_NOTIFY_SPEC.md` there); this module owns only
//! the *URL shape* shared by checkout, refund submission, admin surfaces, and
//! the notify-domain configuration:
//!
//! ```text
//! {base}/app/v3/api/orders/payments/webhooks/{providerCode}   payment family
//! {base}/app/v3/api/orders/refunds/webhooks/{providerCode}    refund family
//! ```
//!
//! Rules encoded here (and regression-tested):
//!
//! - ONE path template per event family. A channel never receives a bespoke
//!   URL, an environment-suffixed URL, or a query string — PSPs compare the
//!   configured notify URL literally and some reject query strings outright.
//! - The base is normalized once (`trim_end_matches('/')`) so a trailing slash
//!   in configuration can never produce `//app/...`.
//! - The `{providerCode}` placeholder is substituted with the *normalized*
//!   provider code, so `WeChat-Pay` and `wechat_pay` cannot produce two
//!   different delivery paths for the same channel.
//! - A provider that does not deliver a refund callback (Alipay queries
//!   refunds instead of notifying) still gets a well-formed refund URL; the
//!   channel capability matrix — not this builder — decides whether it is
//!   submitted.

use crate::adapter::normalize_provider_code;

/// Canonical order payment (success/failure) notify path. HTTP owned by
/// `sdkwork-order` app api.
pub const ORDER_PAYMENT_WEBHOOK_PATH: &str = "/app/v3/api/orders/payments/webhooks/{providerCode}";

/// Canonical order refund notify path. HTTP owned by `sdkwork-order` app api.
pub const ORDER_REFUND_WEBHOOK_PATH: &str = "/app/v3/api/orders/refunds/webhooks/{providerCode}";

/// Event family a notify URL is built for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotifyFamily {
    /// Payment success / failure / closure notifications.
    Payment,
    /// Refund result notifications.
    Refund,
}

impl NotifyFamily {
    /// Canonical path template for this family.
    pub fn path_template(self) -> &'static str {
        match self {
            Self::Payment => ORDER_PAYMENT_WEBHOOK_PATH,
            Self::Refund => ORDER_REFUND_WEBHOOK_PATH,
        }
    }

    /// Stable wire identifier used in configuration and audit records.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Payment => "payment",
            Self::Refund => "refund",
        }
    }
}

/// The complete notify-URL set for one channel account: both families always
/// resolve together so a deployment can never configure one family and forget
/// the other.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ProviderNotifyUrls {
    /// Payment-family callback URL.
    pub payment: String,
    /// Refund-family callback URL.
    pub refund: String,
}

impl ProviderNotifyUrls {
    /// URL for one family.
    pub fn for_family(&self, family: NotifyFamily) -> &str {
        match family {
            NotifyFamily::Payment => &self.payment,
            NotifyFamily::Refund => &self.refund,
        }
    }

    /// The pair as `(payment, refund)`.
    pub fn into_pair(self) -> (String, String) {
        (self.payment, self.refund)
    }
}

/// Normalizes a configured notify base (`{scheme}://{host}[:{port}]`).
///
/// Only surrounding whitespace and trailing separators are removed; the value
/// is never re-parsed or rewritten, so an operator sees exactly the origin
/// they configured in every derived URL.
pub fn normalize_notify_base(base: &str) -> &str {
    base.trim().trim_end_matches('/')
}

/// Builds the notify URL of `family` for `provider_code` on `base`.
pub fn build_provider_notify_url(
    base: &str,
    provider_code: &str,
    family: NotifyFamily,
) -> String {
    let base = normalize_notify_base(base);
    let provider_code = normalize_provider_code(provider_code);
    format!(
        "{}{}",
        base,
        family.path_template().replace("{providerCode}", &provider_code)
    )
}

/// Builds the complete notify-URL pair for `provider_code` on `base`.
pub fn build_provider_notify_urls(base: &str, provider_code: &str) -> ProviderNotifyUrls {
    ProviderNotifyUrls {
        payment: build_provider_notify_url(base, provider_code, NotifyFamily::Payment),
        refund: build_provider_notify_url(base, provider_code, NotifyFamily::Refund),
    }
}

/// Builds the order-gateway payment notify URL for `provider_code`.
///
/// Compatibility spelling kept for existing call sites; identical to
/// [`build_provider_notify_url`] with [`NotifyFamily::Payment`].
pub fn build_order_payment_webhook_url(base: &str, provider_code: &str) -> String {
    build_provider_notify_url(base, provider_code, NotifyFamily::Payment)
}

/// Builds the order-gateway refund notify URL for `provider_code`.
pub fn build_order_refund_webhook_url(base: &str, provider_code: &str) -> String {
    build_provider_notify_url(base, provider_code, NotifyFamily::Refund)
}

/// How a channel delivers refund results.
///
/// This is capability data, not configuration: it decides whether a refund
/// submission may hand the channel a dedicated refund URL, and it is the
/// single place the "退款通知是否可用" question is answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefundNotifyCapability {
    /// The refund request carries its own `notify_url` (WeChat Pay v3
    /// `notify_url`, sandbox mirror).
    DedicatedUrl,
    /// Refund events arrive on the payment notify URL as a distinct event
    /// family (`charge.refunded` / `PAYMENT.CAPTURE.REFUNDED`); there is no
    /// separate URL to configure.
    PaymentUrlFamily,
    /// No asynchronous refund notification exists; refund state is resolved by
    /// the refund query/compensation path only (Alipay).
    Unsupported,
}

impl RefundNotifyCapability {
    /// Stable wire identifier for configuration and audit records.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DedicatedUrl => "dedicated_url",
            Self::PaymentUrlFamily => "payment_url_family",
            Self::Unsupported => "unsupported",
        }
    }
}

/// Refund-notify capability of `provider_code`.
///
/// Unknown channels degrade to [`RefundNotifyCapability::Unsupported`]: never
/// send a callback URL to a channel whose refund contract is not verified.
pub fn refund_notify_capability(provider_code: &str) -> RefundNotifyCapability {
    match normalize_provider_code(provider_code).as_str() {
        "wechat_pay" | "sandbox" => RefundNotifyCapability::DedicatedUrl,
        "stripe" | "paypal" => RefundNotifyCapability::PaymentUrlFamily,
        _ => RefundNotifyCapability::Unsupported,
    }
}

/// Whether the channel accepts a dedicated refund notify URL on refund
/// submission.
pub fn provider_supports_refund_notify(provider_code: &str) -> bool {
    refund_notify_capability(provider_code) == RefundNotifyCapability::DedicatedUrl
}

/// The notify URL a refund submission may actually carry.
///
/// THE rule every refund submission path applies (order refund executor,
/// payment app/backend refund routers, payment compensation re-submission):
/// forward the caller's URL only for channels whose refund contract defines
/// one, and never emit a blank value.
pub fn refund_notify_url_for_channel(
    provider_code: &str,
    notify_url: Option<&str>,
) -> Option<String> {
    if !provider_supports_refund_notify(provider_code) {
        return None;
    }
    notify_url
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payment_and_refund_urls_use_the_canonical_order_paths() {
        let urls = build_provider_notify_urls("https://pay.example.com", "wechat_pay");
        assert_eq!(
            "https://pay.example.com/app/v3/api/orders/payments/webhooks/wechat_pay",
            urls.payment
        );
        assert_eq!(
            "https://pay.example.com/app/v3/api/orders/refunds/webhooks/wechat_pay",
            urls.refund
        );
        assert_eq!(
            "https://pay.example.com/app/v3/api/orders/payments/webhooks/wechat_pay",
            urls.for_family(NotifyFamily::Payment)
        );
    }

    #[test]
    fn trailing_slashes_and_whitespace_never_double_up() {
        for base in [
            "https://pay.example.com",
            "https://pay.example.com/",
            "https://pay.example.com///",
            "  https://pay.example.com/  ",
        ] {
            assert_eq!(
                "https://pay.example.com/app/v3/api/orders/refunds/webhooks/alipay",
                build_order_refund_webhook_url(base, "alipay")
            );
        }
    }

    #[test]
    fn provider_code_is_normalized_before_substitution() {
        assert_eq!(
            "https://pay.example.com/app/v3/api/orders/payments/webhooks/wechat_pay",
            build_order_payment_webhook_url("https://pay.example.com", "WeChat-Pay")
        );
    }

    #[test]
    fn dev_loopback_port_is_preserved() {
        assert_eq!(
            "http://127.0.0.1:3905/app/v3/api/orders/payments/webhooks/sandbox",
            build_order_payment_webhook_url("http://127.0.0.1:3905/", "sandbox")
        );
    }

    #[test]
    fn notify_urls_never_carry_a_query_string() {
        let urls = build_provider_notify_urls("https://pay.example.com", "stripe");
        for url in [urls.payment, urls.refund] {
            assert!(!url.contains('?'), "notify URL must not carry a query: {url}");
            assert!(!url.contains('#'), "notify URL must not carry a fragment: {url}");
        }
    }

    #[test]
    fn refund_capability_matrix_matches_the_channel_contracts() {
        assert_eq!(
            RefundNotifyCapability::DedicatedUrl,
            refund_notify_capability("wechat_pay")
        );
        assert_eq!(
            RefundNotifyCapability::DedicatedUrl,
            refund_notify_capability("sandbox")
        );
        assert_eq!(
            RefundNotifyCapability::PaymentUrlFamily,
            refund_notify_capability("stripe")
        );
        assert_eq!(
            RefundNotifyCapability::PaymentUrlFamily,
            refund_notify_capability("paypal")
        );
        // Alipay resolves refunds by query; it has no async refund callback.
        assert_eq!(
            RefundNotifyCapability::Unsupported,
            refund_notify_capability("alipay")
        );
        // Unknown channels must never be handed a callback URL.
        assert_eq!(
            RefundNotifyCapability::Unsupported,
            refund_notify_capability("mystery_pay")
        );
        assert!(provider_supports_refund_notify("WeChat-Pay"));
        assert!(!provider_supports_refund_notify("stripe"));
        assert!(!provider_supports_refund_notify("alipay"));
    }

    #[test]
    fn only_dedicated_url_channels_receive_a_refund_notify_url() {
        let url = "https://pay.example.com/app/v3/api/orders/refunds/webhooks/wechat_pay";
        assert_eq!(
            Some(url.to_owned()),
            refund_notify_url_for_channel("wechat_pay", Some(url))
        );
        assert_eq!(
            Some(url.to_owned()),
            refund_notify_url_for_channel("sandbox", Some(url))
        );
        // Channels that do not define a refund callback parameter must never be
        // handed one: emitting it would be a payload field their contract does
        // not describe.
        assert_eq!(None, refund_notify_url_for_channel("stripe", Some(url)));
        assert_eq!(None, refund_notify_url_for_channel("paypal", Some(url)));
        assert_eq!(None, refund_notify_url_for_channel("alipay", Some(url)));
        assert_eq!(None, refund_notify_url_for_channel("mystery_pay", Some(url)));
    }

    #[test]
    fn blank_refund_notify_url_never_reaches_a_channel() {
        for blank in [None, Some(""), Some("   ")] {
            assert_eq!(
                None,
                refund_notify_url_for_channel("wechat_pay", blank),
                "a blank notify URL must never be forwarded"
            );
        }
    }
}
