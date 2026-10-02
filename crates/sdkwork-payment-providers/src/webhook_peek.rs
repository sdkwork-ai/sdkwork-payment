//! Best-effort extraction of routing fields from unsigned webhook bodies.
//!
//! Used only to resolve tenant-scoped PSP credentials before signature verification.

use serde_json::Value;

pub struct WebhookPeekOutcome {
    pub out_trade_no: Option<String>,
    pub merchant_id: Option<String>,
}

pub fn peek_webhook_routing_fields(provider_code: &str, body: &[u8]) -> WebhookPeekOutcome {
    match provider_code.to_ascii_lowercase().as_str() {
        "stripe" => peek_stripe(body),
        "alipay" => peek_alipay(body),
        "paypal" => peek_paypal(body),
        "wechat_pay" => WebhookPeekOutcome {
            out_trade_no: None,
            merchant_id: None,
        },
        _ => WebhookPeekOutcome {
            out_trade_no: None,
            merchant_id: None,
        },
    }
}

/// PayPal events carry the merchant trade number in the `custom_id` the
/// order was created with — duplicated at the top of capture resources and
/// nested under purchase_units on order resources.
fn peek_paypal(body: &[u8]) -> WebhookPeekOutcome {
    let Ok(payload) = serde_json::from_slice::<Value>(body) else {
        return WebhookPeekOutcome {
            out_trade_no: None,
            merchant_id: None,
        };
    };
    let custom_id = payload
        .pointer("/resource/custom_id")
        .or_else(|| payload.pointer("/resource/purchase_units/0/custom_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    WebhookPeekOutcome {
        out_trade_no: custom_id,
        merchant_id: None,
    }
}

fn peek_stripe(body: &[u8]) -> WebhookPeekOutcome {
    let Ok(payload) = serde_json::from_slice::<Value>(body) else {
        return WebhookPeekOutcome {
            out_trade_no: None,
            merchant_id: None,
        };
    };
    let object = payload.get("data").and_then(|data| data.get("object"));
    let out_trade_no = object
        .and_then(|value| {
            value
                .get("metadata")
                .and_then(|metadata| metadata.get("merchant_order_no"))
                .and_then(Value::as_str)
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    WebhookPeekOutcome {
        out_trade_no,
        merchant_id: None,
    }
}

fn peek_alipay(body: &[u8]) -> WebhookPeekOutcome {
    let Ok(text) = std::str::from_utf8(body) else {
        return WebhookPeekOutcome {
            out_trade_no: None,
            merchant_id: None,
        };
    };
    let mut out_trade_no = None;
    let mut merchant_id = None;
    for pair in text.split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        let key = urlencoding::decode(key)
            .map(|value| value.into_owned())
            .unwrap_or_else(|_| key.to_owned());
        let value = urlencoding::decode(value)
            .map(|value| value.into_owned())
            .unwrap_or_else(|_| value.to_owned());
        match key.as_str() {
            "out_trade_no" if !value.trim().is_empty() => out_trade_no = Some(value),
            "app_id" if !value.trim().is_empty() => merchant_id = Some(value),
            _ => {}
        }
    }
    WebhookPeekOutcome {
        out_trade_no,
        merchant_id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peek_alipay_form_extracts_routing_fields() {
        let body = b"out_trade_no=OT-123&app_id=20210001&trade_status=TRADE_SUCCESS";
        let peek = peek_webhook_routing_fields("alipay", body);
        assert_eq!(peek.out_trade_no.as_deref(), Some("OT-123"));
        assert_eq!(peek.merchant_id.as_deref(), Some("20210001"));
    }

    #[test]
    fn peek_stripe_json_extracts_merchant_order_no() {
        let body = br#"{"data":{"object":{"metadata":{"merchant_order_no":"OT-456"},"status":"succeeded"}}}"#;
        let peek = peek_webhook_routing_fields("stripe", body);
        assert_eq!(peek.out_trade_no.as_deref(), Some("OT-456"));
    }
}
