//! Maps provider webhook payment statuses to commerce persistence wire values.

pub fn map_provider_payment_status(provider_code: &str, raw_status: &str) -> Option<&'static str> {
    let status = raw_status.trim().to_ascii_lowercase();
    match provider_code.trim().to_ascii_lowercase().as_str() {
        "stripe" => match status.as_str() {
            "succeeded" => Some("succeeded"),
            "canceled" | "cancelled" => Some("canceled"),
            "requires_payment_method"
            | "requires_confirmation"
            | "requires_action"
            | "processing"
            | "requires_capture" => Some("pending"),
            "payment_failed" => Some("failed"),
            _ => None,
        },
        "alipay" => match status.as_str() {
            "trade_success" | "trade_finished" => Some("succeeded"),
            "trade_closed" => Some("canceled"),
            "wait_buyer_pay" => Some("pending"),
            _ => None,
        },
        "wechat_pay" | "wechat-pay" => match status.as_str() {
            "success" => Some("succeeded"),
            "refund" => Some("refunding"),
            "revoked" => Some("canceled"),
            "closed" | "payerror" => Some("canceled"),
            "notpay" | "userpaying" => Some("pending"),
            _ => None,
        },
        // The sandbox provider mirrors WeChat-style statuses so local
        // development can simulate a PSP payment-success webhook end to end.
        "sandbox" => match status.as_str() {
            "succeeded" | "success" | "paid" => Some("succeeded"),
            "pending" | "created" | "processing" => Some("pending"),
            "failed" | "cancelled" | "canceled" => Some("canceled"),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_stripe_succeeded() {
        assert_eq!(
            map_provider_payment_status("stripe", "succeeded"),
            Some("succeeded")
        );
    }

    #[test]
    fn maps_alipay_trade_success() {
        assert_eq!(
            map_provider_payment_status("alipay", "TRADE_SUCCESS"),
            Some("succeeded")
        );
    }

    #[test]
    fn maps_wechat_success() {
        assert_eq!(
            map_provider_payment_status("wechat_pay", "SUCCESS"),
            Some("succeeded")
        );
    }

    #[test]
    fn maps_wechat_refund_to_refunding() {
        assert_eq!(
            map_provider_payment_status("wechat_pay", "REFUND"),
            Some("refunding")
        );
    }

    #[test]
    fn maps_wechat_revoked_to_canceled() {
        assert_eq!(
            map_provider_payment_status("wechat_pay", "REVOKED"),
            Some("canceled")
        );
    }
}
