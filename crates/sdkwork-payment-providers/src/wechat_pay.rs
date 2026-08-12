use std::fmt;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce as AesNonce};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use rsa::pkcs1v15::{Signature, SigningKey, VerifyingKey};
use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey};
use rsa::signature::{SignatureEncoding, Signer, Verifier};
use rsa::{RsaPrivateKey, RsaPublicKey};
use serde_json::{json, Value};
use sha2::Sha256;
use x509_parser::parse_x509_certificate;

use crate::adapter::{
    metadata_string, normalized_optional, require_non_empty, require_positive_amount,
    PaymentAdapterFuture, PaymentAdapterOperation, PaymentCancelPaymentIntentRequest,
    PaymentCreateIntentRequest, PaymentCreateRefundRequest, PaymentNormalizeWebhookRequest,
    PaymentNormalizedWebhookEvent, PaymentProviderAdapter, PaymentProviderCapabilities,
    PaymentProviderOperationOutcome, PaymentQueryPaymentIntentRequest, PaymentQueryRefundRequest,
    PaymentVerifyWebhookRequest, PaymentWebhookVerificationOutcome,
};
use crate::error::{ProviderError, ProviderResult};
use crate::http::ReqwestHttpClient;

const WECHAT_PAY_PROVIDER_CODE: &str = "wechat_pay";
const WECHAT_PAY_API_BASE_URL: &str = "https://api.mch.weixin.qq.com";
const WECHAT_PAY_WEBHOOK_TIMESTAMP_TOLERANCE_SECONDS: u64 = 300;

static WECHAT_PAY_CAPABILITIES: PaymentProviderCapabilities = PaymentProviderCapabilities {
    provider_code: WECHAT_PAY_PROVIDER_CODE,
    operations: &[
        PaymentAdapterOperation::CreatePaymentIntent,
        PaymentAdapterOperation::QueryPaymentIntent,
        PaymentAdapterOperation::CancelPaymentIntent,
        PaymentAdapterOperation::CreateRefund,
        PaymentAdapterOperation::QueryRefund,
        PaymentAdapterOperation::VerifyWebhook,
        PaymentAdapterOperation::NormalizeWebhook,
    ],
};

/// 微信支付 API v3 验签凭据模式（官方"证书密钥概览"二选一）：
/// - `WeChatPayPublicKey`：微信支付公钥（`pub_key.pem`，SPKI 公钥，无过期时间，
///   官方推荐且新商户默认；`Wechatpay-Serial` 头携带 `PUB_KEY_ID_` 前缀的公钥 ID）。
/// - `PlatformCertificate`：平台证书（`wechatpay_cert.pem`，X.509，5 年有效期需轮换；
///   `Wechatpay-Serial` 头携带平台证书序列号）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WeChatPaySignVerifyMode {
    PlatformCertificate,
    WeChatPayPublicKey,
}

impl WeChatPaySignVerifyMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PlatformCertificate => "platform_certificate",
            Self::WeChatPayPublicKey => "wechatpay_public_key",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "platform_certificate" => Some(Self::PlatformCertificate),
            "wechatpay_public_key" => Some(Self::WeChatPayPublicKey),
            _ => None,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct WeChatPayProviderConfig {
    pub app_id: String,
    pub mch_id: String,
    pub merchant_serial_no: String,
    pub merchant_private_key_pem: String,
    pub api_v3_key: String,
    pub notify_url: Option<String>,
    /// 验签凭据模式：微信支付公钥（默认、官方推荐）或平台证书。
    pub sign_verify_mode: WeChatPaySignVerifyMode,
    /// 验签密钥 PEM：公钥模式为 `pub_key.pem`（SPKI 公钥），证书模式为
    /// `wechatpay_cert.pem`（X.509 平台证书）。来自账户 certificate 槽。
    pub verification_key_pem: Option<String>,
    /// 微信支付公钥 ID（`PUB_KEY_ID_` 前缀）或平台证书序列号，用于与
    /// `Wechatpay-Serial` 头匹配。
    pub verification_serial_no: Option<String>,
}

impl fmt::Debug for WeChatPayProviderConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WeChatPayProviderConfig")
            .field("app_id", &self.app_id)
            .field("mch_id", &self.mch_id)
            .field("merchant_serial_no", &self.merchant_serial_no)
            .field("merchant_private_key_pem", &"<redacted>")
            .field("api_v3_key", &"<redacted>")
            .field("sign_verify_mode", &self.sign_verify_mode)
            .field(
                "verification_key_pem",
                &self.verification_key_pem.as_deref().map(|_| "<redacted>"),
            )
            .field("verification_serial_no", &"<redacted>")
            .field("notify_url", &self.notify_url)
            .finish()
    }
}

impl Default for WeChatPaySignVerifyMode {
    fn default() -> Self {
        Self::WeChatPayPublicKey
    }
}

pub struct WeChatPayRsaCrypto {
    signing_key: SigningKey<Sha256>,
    api_v3_key: Vec<u8>,
}

impl WeChatPayRsaCrypto {
    pub fn new(merchant_private_key_pem: &str, api_v3_key: &str) -> ProviderResult<Self> {
        let private_key =
            RsaPrivateKey::from_pkcs8_pem(merchant_private_key_pem).map_err(|error| {
                ProviderError::invalid_request(
                    PaymentAdapterOperation::CreatePaymentIntent,
                    format!("invalid WeChat Pay merchant private key: {error}"),
                )
            })?;
        if api_v3_key.len() != 32 {
            return Err(ProviderError::invalid_request(
                PaymentAdapterOperation::VerifyWebhook,
                "WeChat Pay api_v3_key must be 32 bytes",
            ));
        }
        Ok(Self {
            signing_key: SigningKey::<Sha256>::new(private_key),
            api_v3_key: api_v3_key.as_bytes().to_vec(),
        })
    }

    pub fn sign(&self, payload: &str) -> ProviderResult<String> {
        let signature: Signature = self.signing_key.sign(payload.as_bytes());
        Ok(BASE64.encode(signature.to_bytes()))
    }

    /// 按验签模式解析验签密钥（SPKI 公钥 PEM 或 X.509 平台证书 PEM）并做
    /// SHA256withRSA 验签。与官方"证书密钥概览"对齐：微信支付公钥模式直接用
    /// 公钥，平台证书模式从证书提取 RSA 公钥。
    pub fn verify_with_verification_key(
        mode: WeChatPaySignVerifyMode,
        verification_key_pem: &str,
        payload: &str,
        signature: &str,
    ) -> ProviderResult<bool> {
        let public_key = parse_verification_key(mode, verification_key_pem)?;
        let decoded = BASE64.decode(signature).map_err(|error| {
            ProviderError::invalid_request(
                PaymentAdapterOperation::VerifyWebhook,
                format!("invalid WeChat Pay signature encoding: {error}"),
            )
        })?;
        let signature = Signature::try_from(decoded.as_slice()).map_err(|error| {
            ProviderError::invalid_request(
                PaymentAdapterOperation::VerifyWebhook,
                format!("invalid WeChat Pay signature: {error}"),
            )
        })?;
        let verifying_key = VerifyingKey::<Sha256>::new(public_key);
        Ok(verifying_key.verify(payload.as_bytes(), &signature).is_ok())
    }

    pub fn decrypt_resource(
        &self,
        associated_data: &str,
        nonce: &str,
        ciphertext: &str,
    ) -> ProviderResult<Vec<u8>> {
        let cipher = Aes256Gcm::new_from_slice(&self.api_v3_key).map_err(|error| {
            ProviderError::invalid_request(
                PaymentAdapterOperation::NormalizeWebhook,
                format!("invalid WeChat Pay api_v3_key: {error}"),
            )
        })?;
        let nonce_bytes = nonce.as_bytes();
        if nonce_bytes.len() != 12 {
            return Err(ProviderError::invalid_response(
                PaymentAdapterOperation::NormalizeWebhook,
                "WeChat Pay resource nonce must be 12 bytes",
            ));
        }
        let ciphertext = BASE64.decode(ciphertext).map_err(|error| {
            ProviderError::invalid_response(
                PaymentAdapterOperation::NormalizeWebhook,
                format!("invalid WeChat Pay ciphertext: {error}"),
            )
        })?;
        let plaintext = cipher
            .decrypt(
                AesNonce::from_slice(nonce_bytes),
                aes_gcm::aead::Payload {
                    msg: &ciphertext,
                    aad: associated_data.as_bytes(),
                },
            )
            .map_err(|error| {
                ProviderError::invalid_response(
                    PaymentAdapterOperation::NormalizeWebhook,
                    format!("WeChat Pay resource decrypt failed: {error}"),
                )
            })?;
        Ok(plaintext)
    }
}

pub struct WeChatPayApiClient {
    config: WeChatPayProviderConfig,
    crypto: Arc<WeChatPayRsaCrypto>,
    http: ReqwestHttpClient,
}

impl WeChatPayApiClient {
    pub fn new(
        config: WeChatPayProviderConfig,
        crypto: Arc<WeChatPayRsaCrypto>,
    ) -> ProviderResult<Self> {
        Ok(Self {
            config,
            crypto,
            http: ReqwestHttpClient::new(WECHAT_PAY_API_BASE_URL)?,
        })
    }

    async fn send(
        &self,
        method: &str,
        path: &str,
        payload: Option<Value>,
    ) -> ProviderResult<Value> {
        let body = match payload {
            Some(payload) => serde_json::to_vec(&payload).map_err(|error| {
                ProviderError::invalid_request(
                    PaymentAdapterOperation::CreatePaymentIntent,
                    format!("WeChat Pay request payload could not be serialized: {error}"),
                )
            })?,
            None => Vec::new(),
        };
        let timestamp = unix_timestamp().to_string();
        let nonce = format!("sdkwork-{timestamp}");
        let body_text = String::from_utf8_lossy(&body);
        let sign_payload = format!("{method}\n{path}\n{timestamp}\n{nonce}\n{body_text}\n");
        let signature = self.crypto.sign(&sign_payload)?;
        let authorization = format!(
            "WECHATPAY2-SHA256-RSA2048 mchid=\"{}\",nonce_str=\"{}\",signature=\"{}\",timestamp=\"{}\",serial_no=\"{}\"",
            self.config.mch_id, nonce, signature, timestamp, self.config.merchant_serial_no
        );
        let mut headers = vec![
            ("Authorization".to_owned(), authorization),
            ("Accept".to_owned(), "application/json".to_owned()),
        ];
        if !body.is_empty() {
            headers.push(("Content-Type".to_owned(), "application/json".to_owned()));
        }
        let url = format!("{WECHAT_PAY_API_BASE_URL}{path}");
        let response = self
            .http
            .request_with_headers_detailed(WECHAT_PAY_PROVIDER_CODE, method, &url, body, headers)
            .await?;
        // 官方要求验证应答签名（Wechatpay-Timestamp/Nonce/Signature/Serial + 原始 body）。
        // 空响应体与账单文件下载接口跳过验签。
        if response.status < 300
            && !response.body_text.is_empty()
            && !path.starts_with("/v3/billdownload")
        {
            verify_wechat_pay_response_signature(
                &self.config,
                &response.headers,
                response.body_text.as_bytes(),
            )?;
        }
        Ok(response.body)
    }

    async fn post_json(&self, path: &str, payload: Value) -> ProviderResult<Value> {
        self.send("POST", path, Some(payload)).await
    }

    async fn get(&self, path: &str) -> ProviderResult<Value> {
        self.send("GET", path, None).await
    }
}

pub struct WeChatPayProviderAdapter {
    config: WeChatPayProviderConfig,
    client: WeChatPayApiClient,
    crypto: Arc<WeChatPayRsaCrypto>,
}

impl WeChatPayProviderAdapter {
    pub fn new(config: WeChatPayProviderConfig) -> ProviderResult<Self> {
        validate_config_secret("app_id", &config.app_id)?;
        validate_config_secret("mch_id", &config.mch_id)?;
        validate_config_secret("merchant_serial_no", &config.merchant_serial_no)?;
        validate_config_secret("merchant_private_key_pem", &config.merchant_private_key_pem)?;
        validate_config_secret("api_v3_key", &config.api_v3_key)?;
        let crypto = Arc::new(WeChatPayRsaCrypto::new(
            &config.merchant_private_key_pem,
            &config.api_v3_key,
        )?);
        let client = WeChatPayApiClient::new(config.clone(), crypto.clone())?;
        Ok(Self {
            config,
            client,
            crypto,
        })
    }

    /// Builds the JSAPI SDK invocation parameters with an RSA-SHA256
    /// signature so the cashier can hand them directly to `wx.requestPayment`
    /// (JSAPI / Mini Program).
    ///
    /// V3 签名串格式（per WeChat Pay V3 文档）:
    /// ```text
    /// {appId}\n{timeStamp}\n{nonceStr}\n{package}\n
    /// ```
    fn build_wechat_jsapi_invoke_params(&self, prepay_id: &str) -> ProviderResult<Value> {
        let timestamp = unix_timestamp().to_string();
        let nonce = format!("sdkwork-pay-{timestamp}");
        let package = format!("prepay_id={prepay_id}");
        let pay_sign = self.crypto.sign(&wechat_jsapi_sign_payload(
            &self.config.app_id,
            &timestamp,
            &nonce,
            &package,
        ))?;
        Ok(build_wechat_jsapi_invoke_params(
            &self.config.app_id,
            prepay_id,
            &timestamp,
            &nonce,
            &pay_sign,
        ))
    }

    /// Builds the native App SDK (PayReq) invocation parameters. The App
    /// 调起参数键集和签名串与 JSAPI 不同：`package` 固定为 `Sign=WXPay`，
    /// 签名串第 4 行为裸 `prepayId`，且需要 `partnerid`（商户号）。
    fn build_wechat_app_invoke_params(&self, prepay_id: &str) -> ProviderResult<Value> {
        let timestamp = unix_timestamp().to_string();
        let nonce = format!("sdkwork-pay-{timestamp}");
        let sign = self.crypto.sign(&wechat_app_sign_payload(
            &self.config.app_id,
            &timestamp,
            &nonce,
            prepay_id,
        ))?;
        Ok(build_wechat_app_invoke_params(
            &self.config.app_id,
            &self.config.mch_id,
            prepay_id,
            &timestamp,
            &nonce,
            &sign,
        ))
    }
}

/// V3 JSAPI 调起签名串：`{appId}\n{timeStamp}\n{nonceStr}\n{package}\n`。
fn wechat_jsapi_sign_payload(app_id: &str, timestamp: &str, nonce: &str, package: &str) -> String {
    format!("{app_id}\n{timestamp}\n{nonce}\n{package}\n")
}

/// V3 App 调起签名串：`{appId}\n{timeStamp}\n{nonceStr}\n{prepayId}\n`
/// （第 4 行为裸 prepayId，不带 `prepay_id=` 前缀）。
fn wechat_app_sign_payload(app_id: &str, timestamp: &str, nonce: &str, prepay_id: &str) -> String {
    format!("{app_id}\n{timestamp}\n{nonce}\n{prepay_id}\n")
}

/// JSAPI（`wx.requestPayment`）调起参数键集。
fn build_wechat_jsapi_invoke_params(
    app_id: &str,
    prepay_id: &str,
    timestamp: &str,
    nonce: &str,
    pay_sign: &str,
) -> Value {
    json!({
        "appId": app_id,
        "timeStamp": timestamp,
        "nonceStr": nonce,
        "package": format!("prepay_id={prepay_id}"),
        "signType": "RSA",
        "paySign": pay_sign,
    })
}

/// 原生 App SDK（PayReq）调起参数键集：partnerid/prepayid/package=Sign=WXPay。
fn build_wechat_app_invoke_params(
    app_id: &str,
    mch_id: &str,
    prepay_id: &str,
    timestamp: &str,
    nonce: &str,
    sign: &str,
) -> Value {
    json!({
        "appid": app_id,
        "partnerid": mch_id,
        "prepayid": prepay_id,
        "package": "Sign=WXPay",
        "noncestr": nonce,
        "timestamp": timestamp,
        "sign": sign,
    })
}

impl PaymentProviderAdapter for WeChatPayProviderAdapter {
    fn capabilities(&self) -> &'static PaymentProviderCapabilities {
        &WECHAT_PAY_CAPABILITIES
    }

    fn create_payment_intent<'a>(
        &'a self,
        request: PaymentCreateIntentRequest,
    ) -> PaymentAdapterFuture<'a, PaymentProviderOperationOutcome> {
        Box::pin(async move {
            let out_trade_no = require_non_empty(
                request.merchant_order_no.as_deref(),
                PaymentAdapterOperation::CreatePaymentIntent,
                "merchant_order_no",
            )?;
            let amount_minor = require_positive_amount(
                request.amount_minor,
                PaymentAdapterOperation::CreatePaymentIntent,
                "amount_minor",
            )?;
            require_cny(
                request.currency.as_deref(),
                PaymentAdapterOperation::CreatePaymentIntent,
            )?;
            let description = metadata_string(&request.metadata, "description")
                .map(str::to_owned)
                .unwrap_or_else(|| out_trade_no.clone());
            let notify_url = require_non_empty(
                resolved_notify_url(&request, &self.config),
                PaymentAdapterOperation::CreatePaymentIntent,
                "notify_url",
            )?;
            let method_key = request
                .payment_scene
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("wechat_native");
            let path = wechat_pay_path_for_key(method_key);
            let mut payload = json!({
                "appid": self.config.app_id,
                "mchid": self.config.mch_id,
                "description": description,
                "out_trade_no": out_trade_no,
                "notify_url": notify_url,
                "amount": {
                    "total": amount_minor,
                    "currency": "CNY",
                },
            });
            if let Some(expires_at) = request
                .expires_at
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                payload["time_expire"] = json!(expires_at);
            }
            // Method-specific request extensions
            match method_key {
                "wechat_jsapi" => {
                    let openid = metadata_string(&request.metadata, "openid")
                        .or_else(|| metadata_string(&request.metadata, "buyer_id"))
                        .ok_or_else(|| {
                            ProviderError::invalid_request(
                                PaymentAdapterOperation::CreatePaymentIntent,
                                "wechat_jsapi requires metadata.openid (payer's openid)",
                            )
                        })?;
                    payload["payer"] = json!({ "openid": openid });
                }
                "wechat_h5" => {
                    let client_ip = metadata_string(&request.metadata, "client_ip")
                        .or_else(|| metadata_string(&request.metadata, "payer_client_ip"))
                        .ok_or_else(|| {
                            ProviderError::invalid_request(
                                PaymentAdapterOperation::CreatePaymentIntent,
                                "wechat_h5 requires metadata.client_ip",
                            )
                        })?;
                    let scene_type =
                        metadata_string(&request.metadata, "scene_type").unwrap_or("Wap");
                    payload["scene_info"] = json!({
                        "payer": { "client_ip": client_ip },
                        "h5_info": { "type": scene_type },
                    });
                }
                "wechat_app" => {
                    // App 支付不需要 payer/scene_info; prepay_id returned for SDK signing
                }
                _ => {}
            }
            let response = self.client.post_json(path, payload).await?;
            // For JSAPI/App, generate the SDK invocation signature so the
            // cashier can hand it directly to the WeChat JS SDK / App SDK.
            // JSAPI uses the wx.requestPayment parameter set; the native App
            // uses the PayReq set (partnerid/prepayid/package=Sign=WXPay).
            let mut response = response;
            if matches!(method_key, "wechat_jsapi" | "wechat_app") {
                if let Some(prepay_id) = response
                    .get("prepay_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                {
                    if method_key == "wechat_app" {
                        response["app_invoke_params"] =
                            self.build_wechat_app_invoke_params(&prepay_id)?;
                    } else {
                        response["sdk_invoke_params"] =
                            self.build_wechat_jsapi_invoke_params(&prepay_id)?;
                    }
                }
            }
            wechat_pay_operation_outcome(PaymentAdapterOperation::CreatePaymentIntent, response)
        })
    }

    fn query_payment_intent<'a>(
        &'a self,
        request: PaymentQueryPaymentIntentRequest,
    ) -> PaymentAdapterFuture<'a, PaymentProviderOperationOutcome> {
        Box::pin(async move {
            let out_trade_no = require_non_empty(
                request.payment_intent_id.as_deref(),
                PaymentAdapterOperation::QueryPaymentIntent,
                "payment_intent_id",
            )?;
            let response = self
                .client
                .get(&format!(
                    "/v3/pay/transactions/out-trade-no/{out_trade_no}?mchid={}",
                    self.config.mch_id
                ))
                .await?;
            wechat_pay_operation_outcome(PaymentAdapterOperation::QueryPaymentIntent, response)
        })
    }

    fn cancel_payment_intent<'a>(
        &'a self,
        request: PaymentCancelPaymentIntentRequest,
    ) -> PaymentAdapterFuture<'a, PaymentProviderOperationOutcome> {
        Box::pin(async move {
            let out_trade_no = require_non_empty(
                request.payment_intent_id.as_deref(),
                PaymentAdapterOperation::CancelPaymentIntent,
                "payment_intent_id",
            )?;
            self.client
                .post_json(
                    &format!("/v3/pay/transactions/out-trade-no/{out_trade_no}/close"),
                    json!({ "mchid": self.config.mch_id }),
                )
                .await?;
            Ok(PaymentProviderOperationOutcome {
                provider_code: WECHAT_PAY_PROVIDER_CODE.to_owned(),
                native_id: Some(out_trade_no.clone()),
                raw_status: Some("CLOSED".to_owned()),
                payload: json!({ "out_trade_no": out_trade_no, "status": "CLOSED" }),
            })
        })
    }

    fn create_refund<'a>(
        &'a self,
        request: PaymentCreateRefundRequest,
    ) -> PaymentAdapterFuture<'a, PaymentProviderOperationOutcome> {
        Box::pin(async move {
            let out_trade_no = require_non_empty(
                request.payment_intent_id.as_deref(),
                PaymentAdapterOperation::CreateRefund,
                "payment_intent_id",
            )?;
            let out_refund_no = require_non_empty(
                request.refund_no.as_deref(),
                PaymentAdapterOperation::CreateRefund,
                "refund_no",
            )?;
            let refund_amount = require_positive_amount(
                request.amount_minor,
                PaymentAdapterOperation::CreateRefund,
                "amount_minor",
            )?;
            let total_amount = request
                .metadata
                .get("total_amount_minor")
                .and_then(Value::as_i64)
                .filter(|amount| *amount > 0)
                .ok_or_else(|| {
                    ProviderError::invalid_request(
                        PaymentAdapterOperation::CreateRefund,
                        "WeChat Pay metadata.total_amount_minor is required",
                    )
                })?;
            let mut payload = json!({
                "out_trade_no": out_trade_no,
                "out_refund_no": out_refund_no,
                "amount": {
                    "refund": refund_amount,
                    "total": total_amount,
                    "currency": "CNY",
                },
            });
            if let Some(reason) = normalized_optional(request.reason) {
                payload["reason"] = json!(reason);
            }
            let response = self
                .client
                .post_json("/v3/refund/domestic/refunds", payload)
                .await?;
            wechat_pay_operation_outcome(PaymentAdapterOperation::CreateRefund, response)
        })
    }

    fn query_refund<'a>(
        &'a self,
        request: PaymentQueryRefundRequest,
    ) -> PaymentAdapterFuture<'a, PaymentProviderOperationOutcome> {
        Box::pin(async move {
            let out_refund_no = require_non_empty(
                request.refund_no.as_deref(),
                PaymentAdapterOperation::QueryRefund,
                "refund_no",
            )?;
            let response = self
                .client
                .get(&format!("/v3/refund/domestic/refunds/{out_refund_no}"))
                .await?;
            wechat_pay_operation_outcome(PaymentAdapterOperation::QueryRefund, response)
        })
    }

    fn verify_webhook<'a>(
        &'a self,
        request: PaymentVerifyWebhookRequest,
    ) -> PaymentAdapterFuture<'a, PaymentWebhookVerificationOutcome> {
        let config = self.config.clone();
        Box::pin(async move {
            let timestamp = require_header(&request.headers, "wechatpay-timestamp")?;
            let nonce = require_header(&request.headers, "wechatpay-nonce")?;
            let signature = require_header(&request.headers, "wechatpay-signature")?;
            if !wechat_webhook_timestamp_is_fresh(
                &timestamp,
                unix_timestamp(),
                WECHAT_PAY_WEBHOOK_TIMESTAMP_TOLERANCE_SECONDS,
            ) {
                return Ok(PaymentWebhookVerificationOutcome {
                    verified: false,
                    provider_event_id: None,
                });
            }
            // 官方验签流程要求按 `Wechatpay-Serial`（公钥 ID 或平台证书序列号）
            // 识别签名密钥；配置了序列号时强制匹配。
            let serial = optional_header(&request.headers, "wechatpay-serial");
            if let Some(configured) = config.verification_serial_no.as_deref() {
                match serial.as_deref() {
                    Some(header_serial) if header_serial == configured => {}
                    Some(_) => {
                        return Ok(PaymentWebhookVerificationOutcome {
                            verified: false,
                            provider_event_id: None,
                        })
                    }
                    None => {
                        return Err(ProviderError::invalid_request(
                            PaymentAdapterOperation::VerifyWebhook,
                            "WeChat Pay webhook header wechatpay-serial is required when a verification serial is configured",
                        ))
                    }
                }
            }
            let body = std::str::from_utf8(&request.body).map_err(|error| {
                ProviderError::invalid_response(
                    PaymentAdapterOperation::VerifyWebhook,
                    format!("WeChat Pay webhook body must be UTF-8: {error}"),
                )
            })?;
            let payload = format!("{timestamp}\n{nonce}\n{body}\n");
            let verified = match config.verification_key_pem.as_deref() {
                Some(verification_key_pem) => WeChatPayRsaCrypto::verify_with_verification_key(
                    config.sign_verify_mode,
                    verification_key_pem,
                    &payload,
                    &signature,
                )?,
                None => {
                    return Err(ProviderError::invalid_request(
                        PaymentAdapterOperation::VerifyWebhook,
                        "WeChat Pay verification key (platform certificate or WeChat Pay public key) is required for webhook verification",
                    ))
                }
            };
            Ok(PaymentWebhookVerificationOutcome {
                verified,
                provider_event_id: if verified {
                    parse_webhook_event_id(&request.body)
                } else {
                    None
                },
            })
        })
    }

    fn normalize_webhook<'a>(
        &'a self,
        request: PaymentNormalizeWebhookRequest,
    ) -> PaymentAdapterFuture<'a, PaymentNormalizedWebhookEvent> {
        let crypto = self.crypto.clone();
        Box::pin(async move {
            let mut payload = parse_body_json(&request.body)?;
            let mut out_trade_no = None;
            let mut payment_status = None;
            if let Some(resource) = payload.get("resource") {
                if let (Some(associated_data), Some(nonce), Some(ciphertext)) = (
                    resource.get("associated_data").and_then(Value::as_str),
                    resource.get("nonce").and_then(Value::as_str),
                    resource.get("ciphertext").and_then(Value::as_str),
                ) {
                    let plaintext = crypto.decrypt_resource(associated_data, nonce, ciphertext)?;
                    let plaintext =
                        serde_json::from_slice::<Value>(&plaintext).map_err(|error| {
                            ProviderError::invalid_response(
                                PaymentAdapterOperation::NormalizeWebhook,
                                format!("WeChat Pay decrypted resource is invalid JSON: {error}"),
                            )
                        })?;
                    out_trade_no = plaintext
                        .get("out_trade_no")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    payment_status = plaintext
                        .get("trade_state")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    payload["resource_plaintext"] = plaintext;
                }
            }
            Ok(PaymentNormalizedWebhookEvent {
                provider_code: WECHAT_PAY_PROVIDER_CODE.to_owned(),
                event_type: payload
                    .get("event_type")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                provider_event_id: payload.get("id").and_then(Value::as_str).map(str::to_owned),
                out_trade_no,
                payment_status,
                payload,
            })
        })
    }
}

/// Resolves the effective notify URL for a WeChat Pay down-order: the
/// order-gateway-resolved request value wins, then the per-account/env config.
fn resolved_notify_url<'a>(
    request: &'a PaymentCreateIntentRequest,
    config: &'a WeChatPayProviderConfig,
) -> Option<&'a str> {
    request
        .notify_url
        .as_deref()
        .or_else(|| config.notify_url.as_deref())
}

fn wechat_pay_operation_outcome(
    operation: PaymentAdapterOperation,
    response: Value,
) -> ProviderResult<PaymentProviderOperationOutcome> {
    // The V3 down-order responses carry a single scenario-specific field:
    // native → `code_url`, jsapi/app → `prepay_id`, h5 → `h5_url`; query and
    // refund responses carry `id`/`out_trade_no`/`refund_id`. The chain covers
    // every response shape so no down-order can fail on the id extraction.
    let native_id = response
        .get("id")
        .and_then(Value::as_str)
        .or_else(|| response.get("refund_id").and_then(Value::as_str))
        .or_else(|| response.get("out_trade_no").and_then(Value::as_str))
        .or_else(|| response.get("out_refund_no").and_then(Value::as_str))
        .or_else(|| response.get("prepay_id").and_then(Value::as_str))
        .or_else(|| response.get("code_url").and_then(Value::as_str))
        .or_else(|| response.get("h5_url").and_then(Value::as_str))
        .map(str::to_owned)
        .ok_or_else(|| {
            ProviderError::invalid_response(operation, "WeChat Pay response is missing id")
        })?;
    Ok(PaymentProviderOperationOutcome {
        provider_code: WECHAT_PAY_PROVIDER_CODE.to_owned(),
        native_id: Some(native_id),
        raw_status: response
            .get("trade_state")
            .or_else(|| response.get("status"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        payload: response,
    })
}

fn require_cny(currency: Option<&str>, operation: PaymentAdapterOperation) -> ProviderResult<()> {
    let currency = require_non_empty(currency, operation, "currency")?;
    if !currency.eq_ignore_ascii_case("CNY") {
        return Err(ProviderError::invalid_request(
            operation,
            "WeChat Pay domestic baseline currently supports CNY only",
        ));
    }
    Ok(())
}

fn validate_config_secret(field: &str, value: &str) -> ProviderResult<()> {
    if value.trim().is_empty() {
        return Err(ProviderError::invalid_request(
            PaymentAdapterOperation::CreatePaymentIntent,
            format!("WeChat Pay {field} is required"),
        ));
    }
    Ok(())
}

fn require_header(headers: &[(String, String)], name: &str) -> ProviderResult<String> {
    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.clone())
        .ok_or_else(|| {
            ProviderError::invalid_request(
                PaymentAdapterOperation::VerifyWebhook,
                format!("WeChat Pay header {name} is required"),
            )
        })
}

fn optional_header(headers: &[(String, String)], name: &str) -> Option<String> {
    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

/// 按验签模式解析验签密钥：微信支付公钥模式解析 SPKI 公钥 PEM；
/// 平台证书模式解析 X.509 证书 PEM 并提取 RSA 公钥。
fn parse_verification_key(
    mode: WeChatPaySignVerifyMode,
    verification_key_pem: &str,
) -> ProviderResult<RsaPublicKey> {
    match mode {
        WeChatPaySignVerifyMode::WeChatPayPublicKey => {
            RsaPublicKey::from_public_key_pem(verification_key_pem).map_err(|error| {
                ProviderError::invalid_request(
                    PaymentAdapterOperation::VerifyWebhook,
                    format!("invalid WeChat Pay public key: {error}"),
                )
            })
        }
        WeChatPaySignVerifyMode::PlatformCertificate => {
            let (_, pem) = x509_parser::pem::parse_x509_pem(verification_key_pem.as_bytes())
                .map_err(|error| {
                    ProviderError::invalid_request(
                        PaymentAdapterOperation::VerifyWebhook,
                        format!("invalid WeChat Pay platform certificate PEM: {error}"),
                    )
                })?;
            let (_, certificate) = parse_x509_certificate(&pem.contents).map_err(|error| {
                ProviderError::invalid_request(
                    PaymentAdapterOperation::VerifyWebhook,
                    format!("invalid WeChat Pay platform certificate: {error}"),
                )
            })?;
            RsaPublicKey::from_public_key_der(certificate.tbs_certificate.subject_pki.raw).map_err(
                |error| {
                    ProviderError::invalid_request(
                        PaymentAdapterOperation::VerifyWebhook,
                        format!("invalid WeChat Pay platform certificate public key: {error}"),
                    )
                },
            )
        }
    }
}

/// 官方应答签名验证：验签串 `{Wechatpay-Timestamp}\n{Wechatpay-Nonce}\n{body}\n`，
/// 用 `Wechatpay-Serial`（公钥 ID 或平台证书序列号）标识的验签密钥 SHA256withRSA
/// 验证 `Wechatpay-Signature`。缺少任一验签头时跳过（与官方"下载接口跳过验签"
/// 语义一致）；已配置 `verification_serial_no` 时强制匹配，避免误用旧密钥。
fn verify_wechat_pay_response_signature(
    config: &WeChatPayProviderConfig,
    headers: &[(String, String)],
    body: &[u8],
) -> ProviderResult<()> {
    let Some(timestamp) = optional_header(headers, "wechatpay-timestamp") else {
        return Ok(());
    };
    let Some(nonce) = optional_header(headers, "wechatpay-nonce") else {
        return Ok(());
    };
    let Some(signature) = optional_header(headers, "wechatpay-signature") else {
        return Ok(());
    };
    let serial = optional_header(headers, "wechatpay-serial");
    if let Some(configured) = config.verification_serial_no.as_deref() {
        match serial.as_deref() {
            Some(header_serial) if header_serial == configured => {}
            Some(_) => {
                return Err(ProviderError::invalid_response(
                    PaymentAdapterOperation::QueryPaymentIntent,
                    "WeChat Pay response wechatpay-serial does not match the configured verification serial",
                ))
            }
            None => {
                return Err(ProviderError::invalid_response(
                    PaymentAdapterOperation::QueryPaymentIntent,
                    "WeChat Pay response header wechatpay-serial is required when a verification serial is configured",
                ))
            }
        }
    }
    let body = std::str::from_utf8(body).map_err(|error| {
        ProviderError::invalid_response(
            PaymentAdapterOperation::QueryPaymentIntent,
            format!("WeChat Pay response body must be UTF-8: {error}"),
        )
    })?;
    let payload = format!("{timestamp}\n{nonce}\n{body}\n");
    let verified = match config.verification_key_pem.as_deref() {
        Some(verification_key_pem) => WeChatPayRsaCrypto::verify_with_verification_key(
            config.sign_verify_mode,
            verification_key_pem,
            &payload,
            &signature,
        )?,
        None => {
            return Err(ProviderError::invalid_request(
                PaymentAdapterOperation::QueryPaymentIntent,
                "WeChat Pay verification key (platform certificate or WeChat Pay public key) is required for response verification",
            ))
        }
    };
    if !verified {
        return Err(ProviderError::invalid_response(
            PaymentAdapterOperation::QueryPaymentIntent,
            "WeChat Pay response signature verification failed",
        ));
    }
    Ok(())
}

fn parse_body_json(body: &[u8]) -> ProviderResult<Value> {
    serde_json::from_slice(body).map_err(|error| {
        ProviderError::invalid_response(
            PaymentAdapterOperation::NormalizeWebhook,
            format!("invalid WeChat Pay webhook JSON: {error}"),
        )
    })
}

fn parse_webhook_event_id(body: &[u8]) -> Option<String> {
    serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|payload| payload.get("id").and_then(Value::as_str).map(str::to_owned))
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn wechat_webhook_timestamp_is_fresh(timestamp: &str, now: u64, tolerance_seconds: u64) -> bool {
    timestamp
        .trim()
        .parse::<u64>()
        .map(|timestamp| now.abs_diff(timestamp) <= tolerance_seconds)
        .unwrap_or(false)
}

/// Maps a method_key to the WeChat Pay V3 API path.
///
/// Supported method_keys (mirrors `commerce_payment_method.method_key` DB rows):
/// - `wechat_native` → `/v3/pay/transactions/native`  (扫码支付, returns `code_url`)
/// - `wechat_jsapi`  → `/v3/pay/transactions/jsapi`  (JSAPI/小程序, returns `prepay_id`)
/// - `wechat_h5`     → `/v3/pay/transactions/h5`     (H5, returns `h5_url`)
/// - `wechat_app`    → `/v3/pay/transactions/app`    (App, returns `prepay_id`)
fn wechat_pay_path_for_key(method_key: &str) -> &'static str {
    match method_key {
        "wechat_native" => "/v3/pay/transactions/native",
        "wechat_jsapi" => "/v3/pay/transactions/jsapi",
        "wechat_h5" => "/v3/pay/transactions/h5",
        "wechat_app" => "/v3/pay/transactions/app",
        _ => "/v3/pay/transactions/native",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_wechat_app_invoke_params, build_wechat_jsapi_invoke_params, resolved_notify_url,
        unix_timestamp, wechat_app_sign_payload, wechat_jsapi_sign_payload,
        wechat_pay_operation_outcome, wechat_webhook_timestamp_is_fresh, WeChatPayProviderAdapter,
        WeChatPayProviderConfig, WeChatPayRsaCrypto, WeChatPaySignVerifyMode,
    };
    use crate::adapter::{
        PaymentAdapterOperation, PaymentCreateIntentRequest, PaymentProviderAdapter,
        PaymentVerifyWebhookRequest,
    };
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
    use rsa::pkcs1v15::{Signature, SigningKey};
    use rsa::pkcs8::{DecodePrivateKey, EncodePrivateKey, EncodePublicKey, LineEnding};
    use rsa::signature::{SignatureEncoding, Signer};
    use rsa::{RsaPrivateKey, RsaPublicKey};
    use sha2::Sha256;

    fn rsa_keypair_pem() -> (String, String) {
        let mut rng = rand::thread_rng();
        let private_key = RsaPrivateKey::new(&mut rng, 2048).expect("rsa key generation");
        let private_pem = private_key
            .to_pkcs8_pem(LineEnding::LF)
            .expect("private pem")
            .to_string();
        let public_pem = RsaPublicKey::from(&private_key)
            .to_public_key_pem(LineEnding::LF)
            .expect("public pem");
        (private_pem, public_pem)
    }

    fn sign_rsa_sha256(private_key: &RsaPrivateKey, payload: &str) -> String {
        let signature: Signature =
            SigningKey::<Sha256>::new(private_key.clone()).sign(payload.as_bytes());
        BASE64.encode(signature.to_bytes())
    }

    /// Generates a self-signed RSA-2048 X.509 certificate (wechatpay_cert.pem
    /// shape) plus its PKCS#8 private key PEM. The key is generated with the
    /// `rsa` crate (rcgen's ring backend cannot generate RSA) and loaded into
    /// rcgen for signing.
    fn rsa_certificate_and_private_key_pem() -> (String, String) {
        let (private_pem, _) = rsa_keypair_pem();
        let key_pair = rcgen::KeyPair::from_pem(&private_pem).expect("rcgen rsa key pair");
        let params =
            rcgen::CertificateParams::new(vec!["wechat.example".to_owned()]).expect("params");
        let certificate = params.self_signed(&key_pair).expect("self signed cert");
        (certificate.pem(), private_pem)
    }

    fn wechat_api_v3_key() -> String {
        "0123456789abcdef0123456789abcdef".to_owned()
    }

    #[test]
    fn verify_with_wechat_pay_public_key_mode_accepts_matching_signature() {
        let payload = "1717171717\nnonce-1\n{\"id\":\"event-1\"}\n";
        let (private_pem, public_pem) = rsa_keypair_pem();
        let private_key = RsaPrivateKey::from_pkcs8_pem(&private_pem).expect("pkcs8");
        let signature = sign_rsa_sha256(&private_key, payload);

        let verified = WeChatPayRsaCrypto::verify_with_verification_key(
            WeChatPaySignVerifyMode::WeChatPayPublicKey,
            &public_pem,
            payload,
            &signature,
        )
        .expect("public key mode must parse");
        assert!(verified, "matching signature must verify");

        let tampered = WeChatPayRsaCrypto::verify_with_verification_key(
            WeChatPaySignVerifyMode::WeChatPayPublicKey,
            &public_pem,
            "1717171717\nnonce-1\n{\"id\":\"event-2\"}\n",
            &signature,
        )
        .expect("verification must not error");
        assert!(!tampered, "tampered payload must not verify");
    }

    #[test]
    fn verify_with_platform_certificate_mode_extracts_rsa_public_key() {
        let payload = "1717171717\nnonce-1\n{\"id\":\"event-1\"}\n";
        let (certificate_pem, private_key_pem) = rsa_certificate_and_private_key_pem();
        assert!(certificate_pem.starts_with("-----BEGIN CERTIFICATE-----"));
        let private_key = RsaPrivateKey::from_pkcs8_pem(&private_key_pem).expect("pkcs8");
        let signature = sign_rsa_sha256(&private_key, payload);

        let verified = WeChatPayRsaCrypto::verify_with_verification_key(
            WeChatPaySignVerifyMode::PlatformCertificate,
            &certificate_pem,
            payload,
            &signature,
        )
        .expect("platform certificate mode must parse");
        assert!(verified, "certificate public key must verify the signature");
    }

    #[tokio::test]
    async fn verify_webhook_matches_configured_public_key_id() {
        let (private_pem, public_pem) = rsa_keypair_pem();
        let private_key = RsaPrivateKey::from_pkcs8_pem(&private_pem).expect("pkcs8");
        let body = "{\"id\":\"event-1\",\"event_type\":\"TRANSACTION.SUCCESS\"}";
        let timestamp = unix_timestamp().to_string();
        let payload = format!("{timestamp}\nnonce-1\n{body}\n");
        let signature = sign_rsa_sha256(&private_key, &payload);

        let adapter = WeChatPayProviderAdapter::new(WeChatPayProviderConfig {
            app_id: "wx-app".to_owned(),
            mch_id: "1900000109".to_owned(),
            merchant_serial_no: "serial-no".to_owned(),
            merchant_private_key_pem: private_pem,
            api_v3_key: wechat_api_v3_key(),
            notify_url: None,
            sign_verify_mode: WeChatPaySignVerifyMode::WeChatPayPublicKey,
            verification_key_pem: Some(public_pem),
            verification_serial_no: Some("PUB_KEY_ID_00000000000000000000000000000001".to_owned()),
        })
        .expect("adapter");

        let outcome = adapter
            .verify_webhook(PaymentVerifyWebhookRequest {
                headers: vec![
                    ("Wechatpay-Timestamp".to_owned(), timestamp.clone()),
                    ("Wechatpay-Nonce".to_owned(), "nonce-1".to_owned()),
                    ("Wechatpay-Signature".to_owned(), signature.clone()),
                    (
                        "Wechatpay-Serial".to_owned(),
                        "PUB_KEY_ID_00000000000000000000000000000001".to_owned(),
                    ),
                ],
                body: body.as_bytes().to_vec(),
                metadata: serde_json::json!({}),
            })
            .await
            .expect("verify_webhook must not error");

        assert!(outcome.verified);
        assert_eq!(Some("event-1".to_owned()), outcome.provider_event_id);

        let mismatched = adapter
            .verify_webhook(PaymentVerifyWebhookRequest {
                headers: vec![
                    ("Wechatpay-Timestamp".to_owned(), timestamp),
                    ("Wechatpay-Nonce".to_owned(), "nonce-1".to_owned()),
                    ("Wechatpay-Signature".to_owned(), signature),
                    (
                        "Wechatpay-Serial".to_owned(),
                        "6EB892196BEAA85D5E59B06F077C8A2903683649".to_owned(),
                    ),
                ],
                body: body.as_bytes().to_vec(),
                metadata: serde_json::json!({}),
            })
            .await
            .expect("mismatched serial must not error");

        assert!(!mismatched.verified);
    }

    #[test]
    fn resolved_notify_url_prefers_request_value_over_config() {
        let config = WeChatPayProviderConfig {
            app_id: "app".to_owned(),
            mch_id: "mch".to_owned(),
            merchant_serial_no: "serial".to_owned(),
            merchant_private_key_pem: "key".to_owned(),
            api_v3_key: "v3key".to_owned(),
            notify_url: Some("https://config.example.com/webhooks/wechat_pay".to_owned()),
            sign_verify_mode: WeChatPaySignVerifyMode::WeChatPayPublicKey,
            verification_key_pem: None,
            verification_serial_no: None,
        };
        let request = PaymentCreateIntentRequest {
            notify_url: Some(
                "https://order.example.com/app/v3/api/orders/payments/webhooks/wechat_pay"
                    .to_owned(),
            ),
            ..Default::default()
        };
        assert_eq!(
            resolved_notify_url(&request, &config),
            Some("https://order.example.com/app/v3/api/orders/payments/webhooks/wechat_pay")
        );

        let request_without = PaymentCreateIntentRequest::default();
        assert_eq!(
            resolved_notify_url(&request_without, &config),
            Some("https://config.example.com/webhooks/wechat_pay")
        );

        let config_without = WeChatPayProviderConfig {
            notify_url: None,
            ..config
        };
        assert_eq!(resolved_notify_url(&request_without, &config_without), None);
    }

    #[test]
    fn down_order_responses_never_fail_id_extraction() {
        // Official V3 down-order responses carry only the scenario field:
        // native → code_url, jsapi/app → prepay_id, h5 → h5_url.
        let native = wechat_pay_operation_outcome(
            PaymentAdapterOperation::CreatePaymentIntent,
            serde_json::json!({ "code_url": "weixin://wxpay/bizpayurl?pr=abc" }),
        )
        .expect("native down-order must not fail");
        assert_eq!(
            native.native_id.as_deref(),
            Some("weixin://wxpay/bizpayurl?pr=abc")
        );

        let jsapi = wechat_pay_operation_outcome(
            PaymentAdapterOperation::CreatePaymentIntent,
            serde_json::json!({ "prepay_id": "wx-prepay-1" }),
        )
        .expect("jsapi down-order must not fail");
        assert_eq!(jsapi.native_id.as_deref(), Some("wx-prepay-1"));

        let app = wechat_pay_operation_outcome(
            PaymentAdapterOperation::CreatePaymentIntent,
            serde_json::json!({ "prepay_id": "wx-prepay-2" }),
        )
        .expect("app down-order must not fail");
        assert_eq!(app.native_id.as_deref(), Some("wx-prepay-2"));

        let h5 = wechat_pay_operation_outcome(
            PaymentAdapterOperation::CreatePaymentIntent,
            serde_json::json!({ "h5_url": "https://wx.tenpay.com/cgi-bin/mmpayweb-bin/checkmweb?prepay_id=abc" }),
        )
        .expect("h5 down-order must not fail");
        assert_eq!(
            h5.native_id.as_deref(),
            Some("https://wx.tenpay.com/cgi-bin/mmpayweb-bin/checkmweb?prepay_id=abc")
        );

        // Query/refund responses keep preferring their own identifiers.
        let query = wechat_pay_operation_outcome(
            PaymentAdapterOperation::QueryPaymentIntent,
            serde_json::json!({ "id": "4200001", "out_trade_no": "trade-1" }),
        )
        .expect("query response must keep working");
        assert_eq!(query.native_id.as_deref(), Some("4200001"));
    }

    #[test]
    fn app_invoke_params_use_the_payreq_key_set() {
        let params = build_wechat_app_invoke_params(
            "wx-appid",
            "1900977762",
            "wx-prepay-1",
            "1720000000",
            "nonce-1",
            "sig",
        );
        assert_eq!(params["appid"], "wx-appid");
        assert_eq!(params["partnerid"], "1900977762");
        assert_eq!(params["prepayid"], "wx-prepay-1");
        assert_eq!(params["package"], "Sign=WXPay");
        assert_eq!(params["noncestr"], "nonce-1");
        assert_eq!(params["timestamp"], "1720000000");
        assert_eq!(params["sign"], "sig");
        assert!(params.get("paySign").is_none());

        let jsapi = build_wechat_jsapi_invoke_params(
            "wx-appid",
            "wx-prepay-1",
            "1720000000",
            "nonce-1",
            "sig",
        );
        assert_eq!(jsapi["package"], "prepay_id=wx-prepay-1");
        assert_eq!(jsapi["signType"], "RSA");
        assert_eq!(jsapi["paySign"], "sig");
    }

    #[test]
    fn app_sign_payload_uses_the_bare_prepay_id() {
        assert_eq!(
            wechat_app_sign_payload("wx-appid", "1720000000", "nonce-1", "wx-prepay-1"),
            "wx-appid\n1720000000\nnonce-1\nwx-prepay-1\n"
        );
        assert_eq!(
            wechat_jsapi_sign_payload("wx-appid", "1720000000", "nonce-1", "prepay_id=wx-prepay-1"),
            "wx-appid\n1720000000\nnonce-1\nprepay_id=wx-prepay-1\n"
        );
    }

    #[test]
    fn refund_outcome_accepts_wechat_refund_identifiers() {
        let outcome = wechat_pay_operation_outcome(
            PaymentAdapterOperation::CreateRefund,
            serde_json::json!({
                "refund_id": "5030000701202601010000000001",
                "out_refund_no": "refund-1",
                "status": "PROCESSING"
            }),
        )
        .expect("valid WeChat refund response");
        assert_eq!(
            outcome.native_id.as_deref(),
            Some("5030000701202601010000000001")
        );
        assert_eq!(outcome.raw_status.as_deref(), Some("PROCESSING"));
    }

    #[test]
    fn webhook_timestamp_requires_five_minute_freshness() {
        assert!(wechat_webhook_timestamp_is_fresh(
            "1700000000",
            1_700_000_000,
            300
        ));
        assert!(wechat_webhook_timestamp_is_fresh(
            "1700000300",
            1_700_000_000,
            300
        ));
        assert!(!wechat_webhook_timestamp_is_fresh(
            "1700000301",
            1_700_000_000,
            300
        ));
        assert!(!wechat_webhook_timestamp_is_fresh(
            "not-a-timestamp",
            1_700_000_000,
            300
        ));
    }
}
