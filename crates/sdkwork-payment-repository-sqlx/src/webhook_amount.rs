//! Notified amount extraction for provider webhook settlement（通知金额校验）.
//!
//! Payment platform integration checklists (Alipay, WeChat Pay, Stripe alike)
//! require the settlement path to verify that the amount a notification
//! carries equals the locally stored attempt amount before confirming a
//! payment. Signature verification proves the notification came from the
//! provider; the amount check proves it belongs to THIS charge — closing the
//! cross-merchant replay / mis-routed-notification window where a genuine,
//! correctly-signed notification describes someone else's money.
//!
//! Extraction is tolerant: a payload shape the module does not recognize
//! yields `None` and settlement proceeds exactly as before (compensation
//! synthetic events and sandbox injections carry no provider amounts). A
//! recognized amount that MISMATCHES the stored attempt amount is a hard
//! settlement rejection, surfaced as a failed webhook event.

use serde_json::Value;

/// Amount facts extracted from a provider notification payload.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NotifiedWebhookAmount {
    /// Notification amount in smallest currency units (cents).
    pub minor: i64,
    /// Notification currency when the provider states one (Stripe). Payment
    /// adapters for Alipay/WeChat enforce CNY at intent creation, so their
    /// notifications carry no independent currency fact.
    pub currency: Option<String>,
}

/// Extracts the charge amount a provider payment notification carries, or
/// `None` when the payload shape carries no recognizable amount.
///
/// Both the async-notification shape (WeChat decrypted under
/// `resource_plaintext`, Stripe under `data.object`, PayPal under
/// `resource`) and the direct query-response shape (top-level fields, used
/// by PSP status queries) are accepted so the same check covers settlement
/// from either source.
pub fn extract_notified_payment_amount(
    provider_code: &str,
    payload: &Value,
) -> Option<NotifiedWebhookAmount> {
    match provider_code {
        "alipay" => json_string(payload, &["total_amount"])
            .and_then(|value| major_decimal_to_minor(&value))
            .map(|minor| NotifiedWebhookAmount {
                minor,
                currency: None,
            }),
        "wechat_pay" => {
            let plaintext = payload.get("resource_plaintext");
            let total = plaintext
                .and_then(|candidate| candidate.pointer("/amount/total"))
                .or_else(|| payload.pointer("/amount/total"))
                .and_then(Value::as_i64)?;
            Some(NotifiedWebhookAmount {
                minor: total,
                currency: None,
            })
        }
        "stripe" => {
            let object = payload
                .pointer("/data/object")
                .unwrap_or(payload);
            let amount = object.get("amount").and_then(Value::as_i64)?;
            let currency = object
                .get("currency")
                .and_then(Value::as_str)
                .map(|value| value.trim().to_ascii_uppercase())
                .filter(|value| !value.is_empty());
            Some(NotifiedWebhookAmount {
                minor: amount,
                currency,
            })
        }
        "paypal" => {
            // Capture/refund events embed the amount on the resource; order
            // query responses nest it under the first purchase unit (with or
            // without the event envelope's `resource` wrapper); capture GET
            // responses carry it at the top level. PayPal amounts are
            // major-unit decimal strings.
            let resource = payload.get("resource");
            let amount = resource
                .and_then(|candidate| candidate.get("amount"))
                .or_else(|| payload.pointer("/resource/purchase_units/0/amount"))
                .or_else(|| payload.pointer("/purchase_units/0/amount"))
                .or_else(|| payload.get("amount"))?;
            let value = amount.get("value").and_then(Value::as_str)?;
            let minor = major_decimal_to_minor(value)?;
            let currency = amount
                .get("currency_code")
                .and_then(Value::as_str)
                .map(|value| value.trim().to_ascii_uppercase())
                .filter(|value| !value.is_empty());
            Some(NotifiedWebhookAmount { minor, currency })
        }
        _ => None,
    }
}

/// Parses a refund notification amount into smallest units. WeChat
/// (`refund_fee`/`refund_amount`) and Stripe (`amount`) travel as minor-unit
/// integers; Alipay's async `refund_fee` travels as a major-unit decimal
/// string, mirroring its payment `total_amount` field.
pub(crate) fn parse_notified_refund_minor(value: &str) -> Option<i64> {
    let value = value.trim();
    if value.contains('.') {
        major_decimal_to_minor(value)
    } else if !value.is_empty() && value.chars().all(|c| c.is_ascii_digit()) {
        value.parse().ok()
    } else {
        None
    }
}

/// Converts a major-unit decimal string (`"88.88"`) into minor units.
/// Accepts at most two fraction digits; anything else (signs, exponent form,
/// three-digit fractions) is unrecognized and yields `None` instead of a
/// silently wrong amount.
fn major_decimal_to_minor(value: &str) -> Option<i64> {
    let value = value.trim();
    let (whole, fraction) = match value.split_once('.') {
        Some((whole, fraction)) => (whole, fraction),
        None => (value, ""),
    };
    let well_formed = !whole.is_empty()
        && whole.chars().all(|c| c.is_ascii_digit())
        && fraction.chars().all(|c| c.is_ascii_digit())
        && fraction.len() <= 2;
    if !well_formed {
        return None;
    }
    let whole: i64 = whole.parse().ok()?;
    let fraction_minor: i64 = if fraction.is_empty() {
        0
    } else {
        let mut padded = fraction.to_string();
        while padded.len() < 2 {
            padded.push('0');
        }
        padded.parse().ok()?
    };
    whole.checked_mul(100)?.checked_add(fraction_minor)
}

fn json_string(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| match value.get(*key) {
        Some(Value::String(value)) => Some(value.clone()),
        Some(Value::Number(value)) => Some(value.to_string()),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn alipay_total_amount_parses_as_major_decimal() {
        let amount = extract_notified_payment_amount(
            "alipay",
            &json!({ "total_amount": "88.88", "trade_status": "TRADE_SUCCESS" }),
        )
        .expect("alipay notify carries total_amount");
        assert_eq!(8888, amount.minor);
        assert_eq!(None, amount.currency);
    }

    #[test]
    fn alipay_whole_yuan_amount_has_zero_fraction() {
        let amount = extract_notified_payment_amount(
            "alipay",
            &json!({ "total_amount": "100" }),
        )
        .expect("integer yuan string parses");
        assert_eq!(10000, amount.minor);
    }

    #[test]
    fn wechat_amount_comes_from_decrypted_resource() {
        let amount = extract_notified_payment_amount(
            "wechat_pay",
            &json!({
                "event_type": "TRANSACTION.SUCCESS",
                "resource_plaintext": { "amount": { "total": 8888, "payer_total": 8888 } },
            }),
        )
        .expect("wechat notify carries resource amount");
        assert_eq!(8888, amount.minor);
    }

    #[test]
    fn wechat_query_response_amount_is_top_level() {
        let amount = extract_notified_payment_amount(
            "wechat_pay",
            &json!({ "out_trade_no": "t-1", "amount": { "total": 500 } }),
        )
        .expect("query response carries amount");
        assert_eq!(500, amount.minor);
    }

    #[test]
    fn stripe_amount_and_currency_come_from_event_object() {
        let amount = extract_notified_payment_amount(
            "stripe",
            &json!({
                "type": "payment_intent.succeeded",
                "data": { "object": { "amount": 950, "currency": "usd" } },
            }),
        )
        .expect("stripe event carries object amount");
        assert_eq!(950, amount.minor);
        assert_eq!(Some("USD".to_owned()), amount.currency);
    }

    #[test]
    fn stripe_query_object_is_accepted_top_level() {
        let amount = extract_notified_payment_amount(
            "stripe",
            &json!({ "id": "pi_1", "amount": 1200, "currency": "EUR" }),
        )
        .expect("query object carries amount");
        assert_eq!(1200, amount.minor);
        assert_eq!(Some("EUR".to_owned()), amount.currency);
    }

    #[test]
    fn paypal_amount_and_currency_come_from_the_capture_resource() {
        let amount = extract_notified_payment_amount(
            "paypal",
            &json!({
                "id": "WH-1",
                "event_type": "PAYMENT.CAPTURE.COMPLETED",
                "resource": {
                    "custom_id": "trade-1",
                    "amount": { "value": "10.50", "currency_code": "USD" },
                },
            }),
        )
        .expect("paypal capture event carries resource amount");
        assert_eq!(1050, amount.minor);
        assert_eq!(Some("USD".to_owned()), amount.currency);

        // Order query responses nest the amount under the purchase unit.
        let order_amount = extract_notified_payment_amount(
            "paypal",
            &json!({
                "id": "5O190127TN364715T",
                "status": "COMPLETED",
                "purchase_units": [{
                    "amount": { "value": "88.88", "currency_code": "USD" },
                }],
            }),
        )
        .expect("paypal order response carries purchase-unit amount");
        assert_eq!(8888, order_amount.minor);
    }

    #[test]
    fn unknown_provider_and_shapeless_payloads_yield_none() {
        assert_eq!(
            None,
            extract_notified_payment_amount("sandbox", &json!({ "amount": 100 }))
        );
        assert_eq!(
            None,
            extract_notified_payment_amount("stripe", &json!({ "type": "payout.paid" }))
        );
        assert_eq!(
            None,
            extract_notified_payment_amount("alipay", &json!({ "trade_status": "TRADE_SUCCESS" }))
        );
    }

    #[test]
    fn malformed_decimal_amounts_are_rejected_not_misparsed() {
        assert_eq!(None, major_decimal_to_minor("88.888"));
        assert_eq!(None, major_decimal_to_minor("-1.00"));
        assert_eq!(None, major_decimal_to_minor(""));
        assert_eq!(None, major_decimal_to_minor("abc"));
        assert_eq!(Some(0), major_decimal_to_minor("0.00"));
    }

    #[test]
    fn refund_amount_parses_minor_integers_and_alipay_decimals() {
        assert_eq!(Some(880), parse_notified_refund_minor("880"));
        assert_eq!(Some(8888), parse_notified_refund_minor("88.88"));
        assert_eq!(None, parse_notified_refund_minor("88.888"));
        assert_eq!(None, parse_notified_refund_minor("x"));
        assert_eq!(None, parse_notified_refund_minor(""));
    }
}
