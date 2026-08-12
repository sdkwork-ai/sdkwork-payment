use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use reqwest::Client;
use serde_json::Value;

use crate::adapter::PaymentAdapterFuture;
use crate::error::{ProviderError, ProviderResult};

/// 带响应头的完整 HTTP 响应，供需要验证响应签名/序列号的 PSP
/// （如微信支付 API v3 应答验签 `Wechatpay-Timestamp/Nonce/Signature/Serial`）使用。
#[derive(Debug, Clone)]
pub struct DetailedHttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Value,
    /// 原始响应体文本；签名验证必须使用原始字节，不能用反序列化后的 JSON。
    pub body_text: String,
}

#[derive(Clone)]
pub struct ReqwestHttpClient {
    client: Client,
    base_url: String,
    default_headers: Vec<(String, String)>,
}

impl ReqwestHttpClient {
    pub fn new(base_url: impl Into<String>) -> ProviderResult<Self> {
        let client = Client::builder()
            .build()
            .map_err(|error| ProviderError::transport("http", error.to_string()))?;
        Ok(Self {
            client,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            default_headers: Vec::new(),
        })
    }

    pub fn with_bearer_auth(mut self, token: impl Into<String>) -> Self {
        self.default_headers.push((
            AUTHORIZATION.to_string(),
            format!("Bearer {}", token.into()),
        ));
        self
    }

    pub fn post_form<'a>(
        &'a self,
        provider_code: &'a str,
        path: &'a str,
        form: Vec<(String, String)>,
        idempotency_key: Option<&'a str>,
    ) -> PaymentAdapterFuture<'a, Value> {
        let url = format!("{}{}", self.base_url, path);
        let client = self.clone();
        let provider_code = provider_code.to_owned();
        let idempotency_key = idempotency_key.map(str::to_owned);
        Box::pin(async move {
            let mut request = client.client.post(&url);
            for (name, value) in &client.default_headers {
                request = request.header(name.as_str(), value.as_str());
            }
            if let Some(key) = idempotency_key.as_deref() {
                request = request.header("Idempotency-Key", key);
            }
            request = request.header(CONTENT_TYPE, "application/x-www-form-urlencoded");
            let response = request
                .form(&form)
                .send()
                .await
                .map_err(|error| ProviderError::transport(&provider_code, error.to_string()))?;
            client.parse_json_response(&provider_code, response).await
        })
    }

    pub fn get<'a>(
        &'a self,
        provider_code: &'a str,
        path: &'a str,
    ) -> PaymentAdapterFuture<'a, Value> {
        let url = format!("{}{}", self.base_url, path);
        let client = self.clone();
        let provider_code = provider_code.to_owned();
        Box::pin(async move {
            let mut request = client.client.get(&url);
            for (name, value) in &client.default_headers {
                request = request.header(name.as_str(), value.as_str());
            }
            let response = request
                .send()
                .await
                .map_err(|error| ProviderError::transport(&provider_code, error.to_string()))?;
            client.parse_json_response(&provider_code, response).await
        })
    }

    /// 与 `request_with_headers` 相同，但返回带响应头的完整响应，供
    /// 微信支付应答签名验证等场景使用。
    pub fn request_with_headers_detailed<'a>(
        &'a self,
        provider_code: &'a str,
        method: &'a str,
        url: &'a str,
        body: Vec<u8>,
        extra_headers: Vec<(String, String)>,
    ) -> PaymentAdapterFuture<'a, DetailedHttpResponse> {
        let client = self.clone();
        let provider_code = provider_code.to_owned();
        let url = url.to_owned();
        let method = method.to_owned();
        Box::pin(async move {
            client
                .send_with_headers(&provider_code, &method, &url, body, extra_headers)
                .await
        })
    }

    async fn send_with_headers(
        &self,
        provider_code: &str,
        method: &str,
        url: &str,
        body: Vec<u8>,
        extra_headers: Vec<(String, String)>,
    ) -> ProviderResult<DetailedHttpResponse> {
        let mut request = match method {
            "GET" => self.client.get(url),
            "POST" => self.client.post(url),
            _ => {
                return Err(ProviderError::transport(
                    provider_code,
                    format!("unsupported HTTP method {method}"),
                ))
            }
        };
        for (name, value) in &self.default_headers {
            request = request.header(name.as_str(), value.as_str());
        }
        for (name, value) in &extra_headers {
            request = request.header(name.as_str(), value.as_str());
        }
        if !body.is_empty() {
            request = request.body(body);
        }
        let response = request
            .send()
            .await
            .map_err(|error| ProviderError::transport(provider_code, error.to_string()))?;
        let status = response.status();
        let headers = response
            .headers()
            .iter()
            .map(|(name, value)| {
                (
                    name.as_str().to_owned(),
                    value.to_str().unwrap_or_default().to_owned(),
                )
            })
            .collect::<Vec<_>>();
        let text = response
            .text()
            .await
            .map_err(|error| ProviderError::transport(provider_code, error.to_string()))?;
        if text.trim().is_empty() {
            if status.is_success() {
                return Ok(DetailedHttpResponse {
                    status: status.as_u16(),
                    headers,
                    body: Value::Null,
                    body_text: String::new(),
                });
            }
            return Err(ProviderError::transport(
                provider_code,
                format!("HTTP {status} with empty body"),
            ));
        }
        let value: Value =
            serde_json::from_str(&text).unwrap_or_else(|_| Value::String(text.clone()));
        if status.is_success() {
            Ok(DetailedHttpResponse {
                status: status.as_u16(),
                headers,
                body: value,
                body_text: text,
            })
        } else {
            Err(ProviderError::transport(
                provider_code,
                format!("HTTP {status}: {text}"),
            ))
        }
    }

    async fn parse_json_response(
        &self,
        provider_code: &str,
        response: reqwest::Response,
    ) -> ProviderResult<Value> {
        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|error| ProviderError::transport(provider_code, error.to_string()))?;
        if text.trim().is_empty() {
            if status.is_success() {
                return Ok(Value::Null);
            }
            return Err(ProviderError::transport(
                provider_code,
                format!("HTTP {status} with empty body"),
            ));
        }
        let value: Value =
            serde_json::from_str(&text).unwrap_or_else(|_| Value::String(text.clone()));
        if status.is_success() {
            Ok(value)
        } else {
            Err(ProviderError::transport(
                provider_code,
                format!("HTTP {status}: {text}"),
            ))
        }
    }
}
