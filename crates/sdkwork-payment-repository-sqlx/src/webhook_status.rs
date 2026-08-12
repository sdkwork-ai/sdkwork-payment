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

/// Maps provider webhook refund statuses to commerce persistence wire
/// values. Refund notifications are a distinct event family from payment
/// notifications: WeChat refund resources carry `refund_status`, Stripe
/// `charge.refunded` carries the refund object status, and Alipay refund
/// async notifications carry their own status field.
pub fn map_provider_refund_status(provider_code: &str, raw_status: &str) -> Option<&'static str> {
    let status = raw_status.trim().to_ascii_lowercase();
    match provider_code.trim().to_ascii_lowercase().as_str() {
        "wechat_pay" | "wechat-pay" => match status.as_str() {
            "success" | "succeeded" => Some("succeeded"),
            "processing" | "process" => Some("processing"),
            "abnormal" | "closed" | "failed" | "fail" => Some("failed"),
            _ => None,
        },
        "stripe" => match status.as_str() {
            "succeeded" => Some("succeeded"),
            "pending" | "processing" => Some("processing"),
            "failed" | "canceled" | "cancelled" => Some("failed"),
            _ => None,
        },
        "alipay" => match status.as_str() {
            "refund_success" | "refund_succeeded" | "succeeded" | "success" => Some("succeeded"),
            "refund_failed" | "failed" | "fail" => Some("failed"),
            "processing" | "pending" => Some("processing"),
            _ => None,
        },
        // The sandbox provider mirrors WeChat-style refund statuses so local
        // development can simulate a PSP refund-success webhook end to end.
        "sandbox" => match status.as_str() {
            "succeeded" | "success" | "refunded" => Some("succeeded"),
            "processing" | "pending" | "created" => Some("processing"),
            "failed" | "cancelled" | "canceled" | "closed" | "abnormal" => Some("failed"),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_wechat_refund_success() {
        assert_eq!(
            map_provider_refund_status("wechat_pay", "SUCCESS"),
            Some("succeeded")
        );
        assert_eq!(
            map_provider_refund_status("wechat_pay", "succeeded"),
            Some("succeeded")
        );
    }

    #[test]
    fn maps_wechat_refund_closed_and_abnormal_to_failed() {
        assert_eq!(
            map_provider_refund_status("wechat_pay", "CLOSED"),
            Some("failed")
        );
        assert_eq!(
            map_provider_refund_status("wechat_pay", "ABNORMAL"),
            Some("failed")
        );
    }

    #[test]
    fn maps_stripe_refund_statuses() {
        assert_eq!(
            map_provider_refund_status("stripe", "succeeded"),
            Some("succeeded")
        );
        assert_eq!(
            map_provider_refund_status("stripe", "pending"),
            Some("processing")
        );
        assert_eq!(
            map_provider_refund_status("stripe", "failed"),
            Some("failed")
        );
    }

    #[test]
    fn maps_alipay_refund_statuses() {
        assert_eq!(
            map_provider_refund_status("alipay", "refund_success"),
            Some("succeeded")
        );
        assert_eq!(
            map_provider_refund_status("alipay", "refund_failed"),
            Some("failed")
        );
    }

    #[test]
    fn maps_sandbox_refund_statuses() {
        assert_eq!(
            map_provider_refund_status("sandbox", "SUCCESS"),
            Some("succeeded")
        );
        assert_eq!(
            map_provider_refund_status("sandbox", "CLOSED"),
            Some("failed")
        );
    }

    #[test]
    fn maps_unknown_refund_statuses_to_none() {
        assert_eq!(map_provider_refund_status("wechat_pay", "UNKNOWN"), None);
        assert_eq!(map_provider_refund_status("stripe", "whatever"), None);
        assert_eq!(
            map_provider_refund_status("unknown_provider", "succeeded"),
            None
        );
    }

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
