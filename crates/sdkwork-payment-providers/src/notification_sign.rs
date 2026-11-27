//! Signs a provider notification with the merchant credentials the deployment
//! holds for a provider account.
//!
//! The signing side is the merchant side: it exists so the payment centre can
//! ask "does my stored credential and configured API version actually verify a
//! correctly-signed notification for this account?" without a PSP, a public
//! callback URL or an external tunnel. It is deliberately the mirror of each
//! adapter's `verify_webhook`: whatever it emits, the real verifier must accept.
//!
//! Only signature-and-canonicalization lives here. Delivery, de-duplication and
//! status transitions stay in the webhook pipeline, so a self-signed
//! notification travels exactly the path a PSP callback travels.
use std::collections::BTreeMap;

use serde_json::Value;

use crate::error::{ProviderError, ProviderResult};

/// A notification this deployment signed, ready to hand to the webhook pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedProviderNotification {
    pub provider_code: String,
    /// `application/json` or `application/x-www-form-urlencoded`.
    pub content_type: &'static str,
    /// Exact bytes that were signed; they must be delivered unchanged.
    pub body: Vec<u8>,
    /// Signature headers to send with the body.
    pub headers: Vec<(String, String)>,
    /// Human description of the algorithm used (`hmac_sha256`, `rsa-sha256`).
    pub algorithm: &'static str,
}

/// Inputs a signature needs for one provider.
#[derive(Debug, Clone, Default)]
pub struct ProviderNotificationSignInputs {
    /// HMAC signing secret (Stripe `whsec_…`).
    pub webhook_secret: Option<String>,
    /// Merchant/PEM private key used for RSA signing.
    pub private_key_pem: Option<String>,
    /// WeChat Pay API v3 key; the notification `resource` is AES-GCM encrypted
    /// with it, so no notification can be normalized without it.
    pub api_v3_key: Option<String>,
    /// WeChat Pay merchant platform public key id (`PUB_KEY_ID_…`) or the
    /// legacy platform certificate serial number sent as `Wechatpay-Serial`.
    pub serial_no: Option<String>,
    /// Merchant app id used by channels whose notifications carry `app_id`
    /// (Alipay) and by every WeChat request payload.
    pub merchant_id: Option<String>,
}

/// Signing schemes a notification can be produced with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationSignScheme {
    /// Stripe: `t=<unix>,v1=<hex hmac_sha256 of "<t>.<body>">`.
    HmacSha256,
    /// WeChat Pay API v3: RSA-SHA256 over `<t>\n<nonce>\n<body>\n`.
    WechatPayRsa,
    /// Alipay async notify: RSA-SHA256 over the canonical signed form fields.
    AlipayRsa,
    /// The local sandbox accepts unsigned bodies.
    Unsigned,
}

impl NotificationSignScheme {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HmacSha256 => "hmac_sha256",
            Self::WechatPayRsa => "wechat_pay_rsa_sha256",
            Self::AlipayRsa => "alipay_rsa_sha256",
            Self::Unsigned => "none",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "hmac_sha256" | "hmac" => Some(Self::HmacSha256),
            "wechat_pay_rsa_sha256" | "wechat_pay" | "wechat" => Some(Self::WechatPayRsa),
            "alipay_rsa_sha256" | "alipay" => Some(Self::AlipayRsa),
            "none" | "unsigned" => Some(Self::Unsigned),
            _ => None,
        }
    }
}

/// The scheme a provider code defaults to.
#[must_use]
pub fn default_sign_scheme(provider_code: &str) -> Option<NotificationSignScheme> {
    match crate::adapter::normalize_provider_code(provider_code).as_str() {
        "stripe" => Some(NotificationSignScheme::HmacSha256),
        "wechat_pay" => Some(NotificationSignScheme::WechatPayRsa),
        "alipay" => Some(NotificationSignScheme::AlipayRsa),
        "sandbox" => Some(NotificationSignScheme::Unsigned),
        _ => None,
    }
}

/// Signs `body` for `provider_code`.
///
/// `timestamp` and `nonce` are parameters instead of being read from the clock
/// so a test can reproduce a delivery and so freshness windows are observable.
pub fn sign_provider_notification(
    provider_code: &str,
    scheme: NotificationSignScheme,
    body: &str,
    timestamp: i64,
    nonce: &str,
    inputs: &ProviderNotificationSignInputs,
) -> ProviderResult<SignedProviderNotification> {
    let provider_code = crate::adapter::normalize_provider_code(provider_code);
    match scheme {
        NotificationSignScheme::Unsigned => Ok(SignedProviderNotification {
            provider_code,
            content_type: "application/json",
            body: body.as_bytes().to_vec(),
            headers: Vec::new(),
            algorithm: NotificationSignScheme::Unsigned.as_str(),
        }),
        NotificationSignScheme::HmacSha256 => {
            let secret = required(inputs.webhook_secret.as_deref(), "webhook secret")?;
            let signature = crate::stripe::sign_stripe_webhook(secret, body, timestamp);
            Ok(SignedProviderNotification {
                provider_code,
                content_type: "application/json",
                body: body.as_bytes().to_vec(),
                headers: vec![("stripe-signature".to_owned(), signature)],
                algorithm: NotificationSignScheme::HmacSha256.as_str(),
            })
        }
        NotificationSignScheme::WechatPayRsa => {
            let private_key = required(inputs.private_key_pem.as_deref(), "merchant private key")?;
            let timestamp = timestamp.to_string();
            let message = format!("{timestamp}\n{nonce}\n{body}\n");
            let signature =
                crate::wechat_pay::sign_wechat_pay_message(private_key, message.as_bytes())?;
            let mut headers = vec![
                ("wechatpay-timestamp".to_owned(), timestamp),
                ("wechatpay-nonce".to_owned(), nonce.to_owned()),
                ("wechatpay-signature".to_owned(), signature),
            ];
            if let Some(serial) = inputs
                .serial_no
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                headers.push(("wechatpay-serial".to_owned(), serial.to_owned()));
            }
            Ok(SignedProviderNotification {
                provider_code,
                content_type: "application/json",
                body: body.as_bytes().to_vec(),
                headers,
                algorithm: NotificationSignScheme::WechatPayRsa.as_str(),
            })
        }
        NotificationSignScheme::AlipayRsa => {
            let private_key = required(inputs.private_key_pem.as_deref(), "merchant private key")?;
            let mut fields = alipay_notify_fields(body, inputs.merchant_id.as_deref(), timestamp)?;
            let canonical = crate::alipay::canonical_alipay_form(&fields)?;
            let signature = crate::alipay::sign_alipay_canonical(private_key, &canonical)?;
            let mut form = canonical;
            form.push_str("&sign=");
            form.push_str(&signature);
            // The signature header is unused by Alipay (the `sign` field is part
            // of the form), but the signed account is still reported so a test
            // can assert which credential signed.
            fields.clear();
            Ok(SignedProviderNotification {
                provider_code,
                content_type: "application/x-www-form-urlencoded",
                body: form.into_bytes(),
                headers: Vec::new(),
                algorithm: NotificationSignScheme::AlipayRsa.as_str(),
            })
        }
    }
}

/// Normalizes a notification payload into the Alipay signed form.
///
/// A JSON body is flattened into `key=value` pairs plus the mandatory
/// `app_id`/`sign_type`, because Alipay's notify is a form POST. A body that is
/// already a form is used as-is (minus any `sign`), so a copied PSP notify can
/// be re-signed verbatim.
fn alipay_notify_fields(
    body: &str,
    app_id: Option<&str>,
    timestamp: i64,
) -> ProviderResult<BTreeMap<String, String>> {
    let trimmed = body.trim();
    let mut fields = BTreeMap::new();
    if trimmed.starts_with('{') {
        let payload: Value = serde_json::from_str(trimmed).map_err(|error| {
            ProviderError::invalid_request(
                crate::adapter::PaymentAdapterOperation::VerifyWebhook,
                format!("notification payload is not valid JSON: {error}"),
            )
        })?;
        let Some(object) = payload.as_object() else {
            return Err(ProviderError::invalid_request(
                crate::adapter::PaymentAdapterOperation::VerifyWebhook,
                "notification payload must be a JSON object",
            ));
        };
        flatten_json_fields("", &Value::Object(object.clone()), &mut fields);
    } else {
        for pair in trimmed.split('&').filter(|pair| !pair.is_empty()) {
            let Some((key, value)) = pair.split_once('=') else {
                return Err(ProviderError::invalid_request(
                    crate::adapter::PaymentAdapterOperation::VerifyWebhook,
                    "notification form payload contains a malformed field",
                ));
            };
            if key == "sign" {
                continue;
            }
            fields.insert(key.to_owned(), value.to_owned());
        }
    }
    if !fields.contains_key("sign_type") {
        fields.insert("sign_type".to_owned(), "RSA2".to_owned());
    }
    if let Some(app_id) = app_id.map(str::trim).filter(|value| !value.is_empty()) {
        fields.entry("app_id".to_owned()).or_insert(app_id.to_owned());
    }
    // `notify_id` is required for a real Alipay async notify; a synthesized
    // value keeps the payload a valid notify while staying traceable.
    fields
        .entry("notify_id".to_owned())
        .or_insert_with(|| format!("sdkwork-notify-{timestamp}"));
    Ok(fields)
}

fn flatten_json_fields(prefix: &str, value: &Value, out: &mut BTreeMap<String, String>) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                let next = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                flatten_json_fields(&next, value, out);
            }
        }
        Value::Null => {}
        Value::String(text) => {
            out.insert(prefix.to_owned(), text.clone());
        }
        Value::Bool(boolean) => {
            out.insert(prefix.to_owned(), boolean.to_string());
        }
        Value::Number(number) => {
            out.insert(prefix.to_owned(), number.to_string());
        }
        Value::Array(_) => {}
    }
}

fn required<'a>(value: Option<&'a str>, field: &str) -> ProviderResult<&'a str> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ProviderError::invalid_request(
                crate::adapter::PaymentAdapterOperation::VerifyWebhook,
                format!("this notification cannot be signed: {field} is not configured for the provider account"),
            )
        })
}

/// Validates that a private-key PEM can sign, so the diagnostic reports a
/// credential problem before a delivery is attempted.
pub fn private_key_signs(pem: &str) -> ProviderResult<()> {
    crate::rsa_pem::parse_rsa_private_key_pem("notification signer", pem).map(|_| ())
}

/// The message a signature covers, for diagnostics that must explain what a
/// verifier will reconstruct.
#[must_use]
pub fn signed_message_for(scheme: NotificationSignScheme, body: &str, timestamp: i64, nonce: &str) -> String {
    match scheme {
        NotificationSignScheme::HmacSha256 => format!("{timestamp}.{body}"),
        NotificationSignScheme::WechatPayRsa => format!("{timestamp}\n{nonce}\n{body}\n"),
        NotificationSignScheme::AlipayRsa | NotificationSignScheme::Unsigned => body.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Error text without leaning on `Display`, which the provider error enum
    /// deliberately does not implement (it converts into the domain error).
    fn error_message(error: &ProviderError) -> String {
        let domain: sdkwork_contract_service::CommerceServiceError = error.clone().into();
        domain.message().to_owned()
    }

    #[test]
    fn scheme_parsing_round_trips_and_fails_closed() {
        for scheme in [
            NotificationSignScheme::HmacSha256,
            NotificationSignScheme::WechatPayRsa,
            NotificationSignScheme::AlipayRsa,
            NotificationSignScheme::Unsigned,
        ] {
            assert_eq!(NotificationSignScheme::parse(scheme.as_str()), Some(scheme));
        }
        assert_eq!(NotificationSignScheme::parse(" RANSOM "), None);
        assert_eq!(default_sign_scheme("stripe"), Some(NotificationSignScheme::HmacSha256));
        assert_eq!(
            default_sign_scheme("wechat_pay"),
            Some(NotificationSignScheme::WechatPayRsa)
        );
        assert_eq!(default_sign_scheme("alipay"), Some(NotificationSignScheme::AlipayRsa));
        assert_eq!(default_sign_scheme("paypal"), None);
    }

    #[test]
    fn a_missing_credential_fails_before_a_body_is_built() {
        let inputs = ProviderNotificationSignInputs::default();
        let error = sign_provider_notification(
            "stripe",
            NotificationSignScheme::HmacSha256,
            "{\"id\":\"evt_1\"}",
            1_700_000_000,
            "nonce",
            &inputs,
        )
        .expect_err("no webhook secret must fail");
        assert!(error_message(&error).contains("webhook secret"));

        let error = sign_provider_notification(
            "wechat_pay",
            NotificationSignScheme::WechatPayRsa,
            "{}",
            1_700_000_000,
            "nonce",
            &inputs,
        )
        .expect_err("no private key must fail");
        assert!(error_message(&error).contains("private key"));
    }

    #[test]
    fn sandbox_accepts_an_unsigned_body() {
        let signed = sign_provider_notification(
            "sandbox",
            NotificationSignScheme::Unsigned,
            "{\"paymentStatus\":\"succeeded\"}",
            0,
            "",
            &ProviderNotificationSignInputs::default(),
        )
        .expect("sandbox signing");
        assert!(signed.headers.is_empty());
        assert_eq!(signed.content_type, "application/json");
        assert_eq!(signed.algorithm, "none");
    }

    #[test]
    fn the_signed_message_matches_the_verifier_reconstruction() {
        assert_eq!(
            signed_message_for(NotificationSignScheme::HmacSha256, "{}", 12, "n"),
            "12.{}"
        );
        assert_eq!(
            signed_message_for(NotificationSignScheme::WechatPayRsa, "{}", 12, "n"),
            "12\nn\n{}\n"
        );
    }

    #[test]
    fn json_payloads_flatten_into_alipay_form_fields() {
        let fields = alipay_notify_fields(
            r#"{"out_trade_no":"T1","total_amount":"0.01","subject":"test"}"#,
            Some("2021000000000001"),
            42,
        )
        .expect("flatten");
        assert_eq!(fields.get("out_trade_no"), Some(&"T1".to_owned()));
        assert_eq!(fields.get("total_amount"), Some(&"0.01".to_owned()));
        assert_eq!(fields.get("app_id"), Some(&"2021000000000001".to_owned()));
        assert_eq!(fields.get("sign_type"), Some(&"RSA2".to_owned()));
        assert_eq!(fields.get("notify_id"), Some(&"sdkwork-notify-42".to_owned()));
    }

    #[test]
    fn an_existing_form_payload_keeps_its_fields_and_drops_the_old_signature() {
        let fields = alipay_notify_fields(
            "out_trade_no=T1&sign_type=RSA2&sign=stale&trade_status=TRADE_SUCCESS",
            None,
            7,
        )
        .expect("parse form");
        assert_eq!(fields.get("out_trade_no"), Some(&"T1".to_owned()));
        assert_eq!(fields.get("trade_status"), Some(&"TRADE_SUCCESS".to_owned()));
        assert_eq!(fields.get("sign"), None);
    }
}
