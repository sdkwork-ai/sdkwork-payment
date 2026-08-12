use std::collections::BTreeMap;

use sdkwork_contract_service::CommerceServiceError;
use sdkwork_payment_service::PayOwnerOrderOutcome;
use serde_json::{json, Value};

use crate::adapter::{
    normalize_provider_code, PaymentCreateIntentRequest, PaymentProviderOperationOutcome,
};
use crate::money::money_to_minor;
use crate::registry::PaymentProviderRegistry;

pub struct CheckoutContext {
    pub provider_code: String,
    pub currency_code: String,
    pub tenant_id: String,
    pub order_id: String,
    pub idempotency_key: String,
    pub expires_at: Option<String>,
    pub notify_url: Option<String>,
    pub payment_scene: Option<String>,
    pub payment_metadata: Option<Value>,
}

pub async fn enrich_pay_owner_order_outcome(
    registry: &PaymentProviderRegistry,
    context: &CheckoutContext,
    mut outcome: PayOwnerOrderOutcome,
) -> Result<PayOwnerOrderOutcome, CommerceServiceError> {
    let provider_code = normalize_provider_code(&context.provider_code);
    if provider_code == "sandbox" || provider_code.is_empty() {
        return Ok(outcome);
    }
    let adapter = registry.resolve(&provider_code).ok_or_else(|| {
        CommerceServiceError::provider_unavailable(format!(
            "payment provider {provider_code} is not configured"
        ))
    })?;

    let amount_minor = money_to_minor(&outcome.amount)?;
    // The notify URL the PSP will call back on is resolved at checkout:
    // explicit order context wins, then the deployment standard order
    // webhook URL (ORDER_PAYMENT_WEBHOOK_BASE_URL + provider path), then the
    // per-provider env/account config inside the adapter. Passing it here
    // guarantees the PSP registers exactly the URL the order gateway serves.
    let notify_url = context
        .notify_url
        .clone()
        .or_else(|| registry.default_notify_url(&provider_code));
    let (provider_method_key, metadata) = provider_request_context(context, &outcome);

    let request = PaymentCreateIntentRequest {
        tenant_id: Some(context.tenant_id.clone()),
        merchant_order_no: Some(outcome.out_trade_no.clone()),
        amount_minor: Some(amount_minor),
        currency: Some(context.currency_code.clone()),
        notify_url,
        expires_at: context.expires_at.clone(),
        payment_scene: Some(provider_method_key),
        metadata,
    };

    let provider_outcome = adapter.create_payment_intent(request).await?;
    let payment_params = payment_params_from_provider(&provider_code, &provider_outcome);
    outcome.payment_params.extend(payment_params);
    if let Some(expires_at) = context.expires_at.as_ref() {
        outcome
            .payment_params
            .insert("expiresAt".to_owned(), expires_at.clone());
    }
    if let Some(url) = cashier_url_from_provider(&provider_code, &provider_outcome) {
        outcome.payment_params.insert("cashierUrl".to_owned(), url);
    }
    Ok(outcome)
}

fn provider_request_context(
    context: &CheckoutContext,
    outcome: &PayOwnerOrderOutcome,
) -> (String, Value) {
    let mut metadata = context
        .payment_metadata
        .clone()
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}));
    metadata["order_id"] = json!(context.order_id);
    metadata["idempotency_key"] = json!(provider_checkout_idempotency_key(context, outcome));
    metadata["subject"] = json!(format!(
        "Order {}",
        outcome
            .payment_params
            .get("orderSn")
            .cloned()
            .unwrap_or_default()
    ));
    metadata["payment_scene"] = json!(context
        .payment_scene
        .clone()
        .unwrap_or_else(|| outcome.payment_method.clone()));
    (outcome.payment_method.clone(), metadata)
}

fn provider_checkout_idempotency_key(
    context: &CheckoutContext,
    outcome: &PayOwnerOrderOutcome,
) -> String {
    let fingerprint = format!(
        "payment-provider-checkout:v1|{}:{}|{}:{}|{}:{}|{}:{}",
        context.tenant_id.len(),
        context.tenant_id,
        context.order_id.len(),
        context.order_id,
        outcome.out_trade_no.len(),
        outcome.out_trade_no,
        context.idempotency_key.len(),
        context.idempotency_key,
    );
    format!(
        "sdkwork-create-{}",
        sdkwork_utils_rust::crypto::sha256_hash(fingerprint.as_bytes())
    )
}

fn payment_params_from_provider(
    provider_code: &str,
    outcome: &PaymentProviderOperationOutcome,
) -> BTreeMap<String, String> {
    let mut params = BTreeMap::new();
    params.insert("providerCode".to_owned(), provider_code.to_owned());
    if let Some(native_id) = &outcome.native_id {
        params.insert("providerTransactionId".to_owned(), native_id.clone());
    }
    if let Some(status) = &outcome.raw_status {
        params.insert("providerStatus".to_owned(), status.clone());
    }
    match provider_code {
        "stripe" => {
            if let Some(secret) = outcome.payload.get("client_secret").and_then(Value::as_str) {
                params.insert("clientSecret".to_owned(), secret.to_owned());
            }
            params.insert("nextAction".to_owned(), "stripe_confirm".to_owned());
        }
        "alipay" => {
            // WAP redirect is preferred in-app/browser: the cashier jumps to
            // the Alipay H5 cashier page instead of showing a scan QR code.
            // PC website pays surface the full cashier form for the browser
            // to render and auto-submit (`payForm`).
            if let Some(redirect) = outcome.payload.get("redirect_url").and_then(Value::as_str) {
                params.insert("payUrl".to_owned(), redirect.to_owned());
                params.insert("nextAction".to_owned(), "redirect".to_owned());
            } else if let Some(qr) = outcome.payload.get("qr_code").and_then(Value::as_str) {
                params.insert("qrCodeUrl".to_owned(), qr.to_owned());
                params.insert("nextAction".to_owned(), "qr_code".to_owned());
            }
            if let Some(form) = outcome.payload.get("payForm").and_then(Value::as_str) {
                params.insert("payForm".to_owned(), form.to_owned());
            }
            // App pay returns the signed `orderStr` for the Alipay App SDK.
            if let Some(order_str) = outcome.payload.get("orderStr").and_then(Value::as_str) {
                params.insert("orderStr".to_owned(), order_str.to_owned());
            }
        }
        "wechat_pay" => {
            // JSAPI invocation params take priority inside the WeChat app;
            // the native code_url remains as the scan/press-and-hold
            // fallback, the H5 h5_url as the mobile-browser cashier link,
            // and the App PayReq params for the native App SDK.
            if let Some(sdk_params) = outcome.payload.get("sdk_invoke_params") {
                if let Ok(serialized) = serde_json::to_string(sdk_params) {
                    params.insert("jsapiPayload".to_owned(), serialized);
                    params.insert("nextAction".to_owned(), "jsapi".to_owned());
                }
            } else if let Some(app_params) = outcome.payload.get("app_invoke_params") {
                if let Ok(serialized) = serde_json::to_string(app_params) {
                    params.insert("appPayload".to_owned(), serialized);
                    params.insert("nextAction".to_owned(), "app".to_owned());
                }
            } else if let Some(h5) = outcome.payload.get("h5_url").and_then(Value::as_str) {
                params.insert("payUrl".to_owned(), h5.to_owned());
                params.insert("nextAction".to_owned(), "redirect".to_owned());
            } else if let Some(qr) = outcome.payload.get("code_url").and_then(Value::as_str) {
                params.insert("qrCodeUrl".to_owned(), qr.to_owned());
                params.insert("nextAction".to_owned(), "qr_code".to_owned());
            }
        }
        _ => {
            params.insert("nextAction".to_owned(), "cashier".to_owned());
        }
    }
    params
}

fn cashier_url_from_provider(
    provider_code: &str,
    outcome: &PaymentProviderOperationOutcome,
) -> Option<String> {
    match provider_code {
        "alipay" => outcome
            .payload
            .get("qr_code")
            .and_then(Value::as_str)
            .map(str::to_owned),
        "wechat_pay" => outcome
            .payload
            .get("code_url")
            .and_then(Value::as_str)
            .map(str::to_owned),
        "stripe" => outcome
            .native_id
            .as_ref()
            .map(|id| format!("stripe://payment_intent/{id}")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        payment_params_from_provider, provider_checkout_idempotency_key, provider_request_context,
        CheckoutContext,
    };
    use crate::adapter::PaymentProviderOperationOutcome;
    use sdkwork_contract_service::CommerceMoney;
    use sdkwork_payment_service::PayOwnerOrderOutcome;
    use serde_json::json;

    #[test]
    fn provider_request_uses_method_key_and_preserves_payer_metadata() {
        let context = CheckoutContext {
            provider_code: "wechat_pay".to_owned(),
            currency_code: "CNY".to_owned(),
            tenant_id: "tenant-1".to_owned(),
            order_id: "order-1".to_owned(),
            idempotency_key: "idem-1".to_owned(),
            expires_at: Some("2026-07-26T15:00:00Z".to_owned()),
            notify_url: Some("https://pay.example.test/webhook".to_owned()),
            payment_scene: Some("mini_program".to_owned()),
            payment_metadata: Some(serde_json::json!({"openid":"payer-openid"})),
        };
        let outcome = PayOwnerOrderOutcome {
            amount: CommerceMoney::new("100").expect("amount"),
            order_id: "order-1".to_owned(),
            out_trade_no: "trade-1".to_owned(),
            payment_id: "payment-1".to_owned(),
            payment_method: "wechat_jsapi".to_owned(),
            status: "pending".to_owned(),
            payment_params: Default::default(),
        };

        let (method_key, metadata) = provider_request_context(&context, &outcome);
        assert_eq!(method_key, "wechat_jsapi");
        assert_eq!(metadata["openid"], "payer-openid");
        assert_eq!(metadata["payment_scene"], "mini_program");
        assert_eq!(
            metadata["idempotency_key"],
            provider_checkout_idempotency_key(&context, &outcome)
        );
        assert!(metadata["idempotency_key"]
            .as_str()
            .expect("provider idempotency key")
            .is_ascii());
    }

    #[test]
    fn alipay_pc_cashier_form_is_surfaced_as_pay_form() {
        let outcome = PaymentProviderOperationOutcome {
            provider_code: "alipay".to_owned(),
            native_id: Some("trade-1".to_owned()),
            raw_status: None,
            payload: json!({
                "out_trade_no": "trade-1",
                "redirect_url": "https://cashier.alipay.com/gateway.do?biz=1",
                "payForm": r#"<form action="https://cashier.alipay.com/gateway.do"><input type="hidden" name="biz_content" value="{}"/></form>"#,
            }),
        };
        let params = payment_params_from_provider("alipay", &outcome);
        assert_eq!(
            params.get("payUrl").map(String::as_str),
            Some("https://cashier.alipay.com/gateway.do?biz=1")
        );
        assert_eq!(
            params.get("nextAction").map(String::as_str),
            Some("redirect")
        );
        assert!(params
            .get("payForm")
            .map(String::as_str)
            .is_some_and(|form| form.starts_with("<form")));
    }

    #[test]
    fn wechat_h5_url_is_surfaced_as_pay_url() {
        let outcome = PaymentProviderOperationOutcome {
            provider_code: "wechat_pay".to_owned(),
            native_id: Some("h5-1".to_owned()),
            raw_status: None,
            payload: json!({
                "h5_url": "https://wx.tenpay.com/cgi-bin/mmpayweb-bin/checkmweb?prepay_id=abc",
            }),
        };
        let params = payment_params_from_provider("wechat_pay", &outcome);
        assert_eq!(
            params.get("payUrl").map(String::as_str),
            Some("https://wx.tenpay.com/cgi-bin/mmpayweb-bin/checkmweb?prepay_id=abc")
        );
        assert_eq!(
            params.get("nextAction").map(String::as_str),
            Some("redirect")
        );
        assert!(!params.contains_key("qrCodeUrl"));
    }

    #[test]
    fn wechat_app_payreq_params_are_surfaced_as_app_payload() {
        let outcome = PaymentProviderOperationOutcome {
            provider_code: "wechat_pay".to_owned(),
            native_id: Some("prepay-1".to_owned()),
            raw_status: None,
            payload: json!({
                "prepay_id": "prepay-1",
                "app_invoke_params": {
                    "appid": "wxappid",
                    "partnerid": "1900977762",
                    "prepayid": "prepay-1",
                    "package": "Sign=WXPay",
                    "noncestr": "n",
                    "timestamp": "1720000000",
                    "sign": "sig",
                },
            }),
        };
        let params = payment_params_from_provider("wechat_pay", &outcome);
        assert_eq!(params.get("nextAction").map(String::as_str), Some("app"));
        let parsed = params
            .get("appPayload")
            .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
            .expect("app payload must be serialized json");
        assert_eq!(parsed["partnerid"], "1900977762");
        assert_eq!(parsed["package"], "Sign=WXPay");
        assert!(!params.contains_key("jsapiPayload"));
        assert!(!params.contains_key("qrCodeUrl"));
    }

    #[test]
    fn alipay_app_order_str_is_surfaced_for_the_app_sdk() {
        let outcome = PaymentProviderOperationOutcome {
            provider_code: "alipay".to_owned(),
            native_id: Some("trade-1".to_owned()),
            raw_status: None,
            payload: json!({
                "out_trade_no": "trade-1",
                "orderStr": "alipay_sdk=alipay-sdk-java-4.38.10.ALL&app_id=2021&biz_content=...",
            }),
        };
        let params = payment_params_from_provider("alipay", &outcome);
        assert_eq!(
            params.get("orderStr").map(String::as_str),
            Some("alipay_sdk=alipay-sdk-java-4.38.10.ALL&app_id=2021&biz_content=...")
        );
    }

    #[test]
    fn alipay_wap_redirect_is_surfaced_as_pay_url() {
        let outcome = PaymentProviderOperationOutcome {
            provider_code: "alipay".to_owned(),
            native_id: Some("trade-1".to_owned()),
            raw_status: Some("pending".to_owned()),
            payload: json!({
                "qr_code": "https://qr.alipay.com/example",
                "redirect_url": "https://cashier.alipay.com/example?biz=1",
            }),
        };
        let params = payment_params_from_provider("alipay", &outcome);
        assert_eq!(
            params.get("payUrl").map(String::as_str),
            Some("https://cashier.alipay.com/example?biz=1")
        );
        assert_eq!(
            params.get("nextAction").map(String::as_str),
            Some("redirect")
        );
        assert!(!params.contains_key("qrCodeUrl"));
    }

    #[test]
    fn alipay_precreate_falls_back_to_qr_code() {
        let outcome = PaymentProviderOperationOutcome {
            provider_code: "alipay".to_owned(),
            native_id: Some("trade-1".to_owned()),
            raw_status: None,
            payload: json!({ "qr_code": "https://qr.alipay.com/example" }),
        };
        let params = payment_params_from_provider("alipay", &outcome);
        assert_eq!(
            params.get("qrCodeUrl").map(String::as_str),
            Some("https://qr.alipay.com/example")
        );
        assert_eq!(
            params.get("nextAction").map(String::as_str),
            Some("qr_code")
        );
        assert!(!params.contains_key("payUrl"));
    }

    #[test]
    fn wechat_jsapi_sdk_params_are_surfaced_as_jsapi_payload() {
        let outcome = PaymentProviderOperationOutcome {
            provider_code: "wechat_pay".to_owned(),
            native_id: Some("prepay-1".to_owned()),
            raw_status: None,
            payload: json!({
                "code_url": "weixin://wxpay/bizpayurl?pr=abc",
                "sdk_invoke_params": {
                    "appId": "wxappid",
                    "timeStamp": "1720000000",
                    "nonceStr": "n",
                    "package": "prepay_id=prepay-1",
                    "signType": "RSA",
                    "paySign": "sig",
                },
            }),
        };
        let params = payment_params_from_provider("wechat_pay", &outcome);
        assert_eq!(params.get("nextAction").map(String::as_str), Some("jsapi"));
        let parsed = params
            .get("jsapiPayload")
            .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
            .expect("jsapi payload must be serialized json");
        assert_eq!(parsed["appId"], "wxappid");
        assert_eq!(parsed["package"], "prepay_id=prepay-1");
        assert!(!params.contains_key("qrCodeUrl"));
    }

    #[test]
    fn wechat_native_falls_back_to_qr_code() {
        let outcome = PaymentProviderOperationOutcome {
            provider_code: "wechat_pay".to_owned(),
            native_id: Some("out-trade-1".to_owned()),
            raw_status: None,
            payload: json!({ "code_url": "weixin://wxpay/bizpayurl?pr=abc" }),
        };
        let params = payment_params_from_provider("wechat_pay", &outcome);
        assert_eq!(
            params.get("qrCodeUrl").map(String::as_str),
            Some("weixin://wxpay/bizpayurl?pr=abc")
        );
        assert_eq!(
            params.get("nextAction").map(String::as_str),
            Some("qr_code")
        );
        assert!(!params.contains_key("jsapiPayload"));
    }
}
