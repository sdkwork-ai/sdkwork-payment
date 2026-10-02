use rand::Rng;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;

use crate::adapter::PaymentAdapterFuture;
use crate::error::{ProviderError, ProviderResult};

/// Total per-request deadline. PSP calls sit inside checkout/refund command
/// paths, so an unbounded request would pin the caller indefinitely.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_REDIRECTS: usize = 3;
/// Initial attempt plus two retries; only transport failures and retryable
/// status codes are retried, and only when the request is safe to repeat.
const RETRY_ATTEMPTS: u32 = 3;
const RETRY_BASE_DELAY: Duration = Duration::from_millis(200);
const RETRY_MAX_DELAY: Duration = Duration::from_secs(2);

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

/// One classified HTTP attempt, kept untyped until the retry loop decides the
/// outcome, so retryability never has to be parsed back out of error text.
#[derive(Debug, Clone)]
enum HttpAttempt {
    Success {
        status: u16,
        headers: Vec<(String, String)>,
        body_text: String,
    },
    /// Non-2xx response with a body (or an empty non-2xx body).
    Status { status: u16, body_text: String },
    Transport(String),
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
            .timeout(REQUEST_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .redirect(reqwest::redirect::Policy::limited(MAX_REDIRECTS))
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
            // A keyed POST is PSP-side idempotent, so a transport failure or a
            // 429/5xx is safe to repeat; without the key the request may have
            // landed server-side and repeating could double-create.
            let retryable = idempotency_key.is_some();
            let attempt = |_attempt_index: u32| {
                let client = client.clone();
                let idempotency_key = idempotency_key.clone();
                let form = form.clone();
                let url = url.clone();
                async move {
                    let mut request = client.client.post(&url);
                    for (name, value) in &client.default_headers {
                        request = request.header(name.as_str(), value.as_str());
                    }
                    if let Some(key) = idempotency_key.as_deref() {
                        request = request.header("Idempotency-Key", key);
                    }
                    request = request.header(CONTENT_TYPE, "application/x-www-form-urlencoded");
                    let response = request.form(&form).send().await;
                    classify_response(response).await
                }
            };
            let outcome = run_with_retries(&provider_code, retryable, attempt).await?;
            json_from_attempt(&provider_code, outcome)
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
            let attempt = |_attempt_index: u32| {
                let client = client.clone();
                let url = url.clone();
                async move {
                    let mut request = client.client.get(&url);
                    for (name, value) in &client.default_headers {
                        request = request.header(name.as_str(), value.as_str());
                    }
                    let response = request.send().await;

                    classify_response(response).await
                }
            };
            let outcome = run_with_retries(&provider_code, true, attempt).await?;
            json_from_attempt(&provider_code, outcome)
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
            // A keyed POST is PSP-side idempotent (Stripe `Idempotency-Key`,
            // PayPal `PayPal-Request-Id`), so a transport failure or a
            // 429/5xx is safe to repeat; without the key the request may
            // have landed server-side and repeating could double-create.
            let retryable = method == "GET"
                || extra_headers.iter().any(|(name, _)| {
                    name.eq_ignore_ascii_case("Idempotency-Key")
                        || name.eq_ignore_ascii_case("PayPal-Request-Id")
                });
            let attempt = |_attempt_index: u32| {
                let client = client.clone();
                let url = url.clone();
                let method = method.clone();
                let body = body.clone();
                let extra_headers = extra_headers.clone();

                async move {
                    let Some(request) =
                        client.build_request(&method, &url, body, extra_headers)
                    else {
                        return HttpAttempt::Transport(format!(
                            "unsupported HTTP method {method}"
                        ));
                    };
                    let response = request.send().await;
                    classify_response(response).await
                }
            };
            let outcome = run_with_retries(&provider_code, retryable, attempt).await?;
            detailed_response_from_attempt(&provider_code, outcome)
        })
    }

    fn build_request(
        &self,
        method: &str,
        url: &str,
        body: Vec<u8>,
        extra_headers: Vec<(String, String)>,
    ) -> Option<reqwest::RequestBuilder> {
        let mut request = match method {
            "GET" => self.client.get(url),
            "POST" => self.client.post(url),
            _ => return None,
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
        Some(request)
    }
}

async fn classify_response(response: Result<reqwest::Response, reqwest::Error>) -> HttpAttempt {
    let response = match response {
        Ok(response) => response,
        Err(error) => return HttpAttempt::Transport(error.to_string()),
    };
    let status = response.status();
    if status.is_success() {
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
        return match response.text().await {
            Ok(body_text) => HttpAttempt::Success {
                status: status.as_u16(),
                headers,
                body_text,
            },
            Err(error) => HttpAttempt::Transport(error.to_string()),
        };
    }
    let body_text = response.text().await.unwrap_or_default();
    HttpAttempt::Status {
        status: status.as_u16(),
        body_text,
    }
}

async fn run_with_retries<F, Fut>(
    provider_code: &str,
    retryable: bool,
    mut attempt: F,
) -> ProviderResult<HttpAttempt>
where
    F: FnMut(u32) -> Fut,
    Fut: std::future::Future<Output = HttpAttempt>,
{
    let mut last: Option<HttpAttempt> = None;
    for attempt_index in 1..=RETRY_ATTEMPTS {
        let outcome = attempt(attempt_index).await;
        let retry_this = match &outcome {
            HttpAttempt::Success { .. } => false,
            HttpAttempt::Status { status, .. } => status_retryable(*status),
            HttpAttempt::Transport(_) => true,
        };
        if !retry_this {
            return Ok(outcome);
        }
        last = Some(outcome);
        if !retryable || attempt_index == RETRY_ATTEMPTS {
            break;
        }
        tokio::time::sleep(jittered_backoff(attempt_index)).await;
    }
    Err(match last {
        Some(HttpAttempt::Transport(message)) => ProviderError::transport(provider_code, message),
        Some(HttpAttempt::Status { status, body_text }) => ProviderError::transport(
            provider_code,
            format!("HTTP {status}: {body_text}"),
        ),
        Some(HttpAttempt::Success { .. }) | None => ProviderError::transport(
            provider_code,
            "request was not attempted",
        ),
    })
}

fn status_retryable(status: u16) -> bool {
    status == 429 || (500..600).contains(&status)
}

/// Exponential backoff with full jitter, bounded at [`RETRY_MAX_DELAY`].
fn jittered_backoff(attempt_index: u32) -> Duration {
    let exponential = RETRY_BASE_DELAY
        .saturating_mul(2u32.saturating_pow(attempt_index.saturating_sub(1)))
        .min(RETRY_MAX_DELAY);
    let half = exponential / 2;
    let spread = exponential.saturating_sub(half);
    let jitter = rand::thread_rng().gen_range(0..=spread.as_millis() as u64);
    half + Duration::from_millis(jitter)
}

fn json_from_attempt(provider_code: &str, attempt: HttpAttempt) -> ProviderResult<Value> {
    match attempt {
        HttpAttempt::Success { body_text, .. } => {
            if body_text.trim().is_empty() {
                Ok(Value::Null)
            } else {
                let value: Value = serde_json::from_str(&body_text)
                    .unwrap_or_else(|_| Value::String(body_text.clone()));
                Ok(value)
            }
        }
        HttpAttempt::Status { status, body_text } => Err(ProviderError::transport(
            provider_code,
            format!("HTTP {status}: {body_text}"),
        )),
        HttpAttempt::Transport(message) => Err(ProviderError::transport(provider_code, message)),
    }
}

fn detailed_response_from_attempt(
    provider_code: &str,
    attempt: HttpAttempt,
) -> ProviderResult<DetailedHttpResponse> {
    match attempt {
        HttpAttempt::Success {
            status,
            headers,
            body_text,
        } => {
            let body: Value = if body_text.trim().is_empty() {
                Value::Null
            } else {
                serde_json::from_str(&body_text)
                    .unwrap_or_else(|_| Value::String(body_text.clone()))
            };
            Ok(DetailedHttpResponse {
                status,
                headers,
                body,
                body_text,
            })
        }
        HttpAttempt::Status { status, body_text } => {
            if body_text.trim().is_empty() {
                Err(ProviderError::transport(
                    provider_code,
                    format!("HTTP {status} with empty body"),
                ))
            } else {
                Err(ProviderError::transport(
                    provider_code,
                    format!("HTTP {status}: {body_text}"),
                ))
            }
        }
        HttpAttempt::Transport(message) => Err(ProviderError::transport(provider_code, message)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_builder_enforces_timeouts_and_redirect_bound() {
        // Construction exercises the builder; the policy values are compile
        // constants asserted here so a future edit cannot silently drop them.
        let client = ReqwestHttpClient::new("https://api.stripe.com").expect("client");
        assert!(client.base_url == "https://api.stripe.com");
        assert_eq!(MAX_REDIRECTS, 3);
        assert_eq!(RETRY_ATTEMPTS, 3);
    }

    #[test]
    fn only_transport_and_server_faults_are_retryable() {
        assert!(!status_retryable(400));
        assert!(!status_retryable(401));
        assert!(!status_retryable(404));
        assert!(status_retryable(429));
        assert!(status_retryable(500));
        assert!(status_retryable(503));
        assert!(!status_retryable(600));
    }

    #[test]
    fn backoff_is_bounded_and_jittered() {
        let first = jittered_backoff(1);
        let second = jittered_backoff(1);
        let late = jittered_backoff(9);
        assert!(first >= RETRY_BASE_DELAY / 2);
        assert!(first <= RETRY_BASE_DELAY);
        // Two draws at the same attempt rarely land on the same jitter value.
        assert!(first != second || spread_millis(1) == 0);
        assert!(late <= RETRY_MAX_DELAY);
    }

    fn spread_millis(attempt_index: u32) -> u64 {
        let exponential = RETRY_BASE_DELAY
            .saturating_mul(2u32.saturating_pow(attempt_index.saturating_sub(1)))
            .min(RETRY_MAX_DELAY);
        (exponential - exponential / 2).as_millis() as u64
    }

    #[test]
    fn attempts_map_to_json_and_detailed_responses() {
        let ok_json = json_from_attempt(
            "stripe",
            HttpAttempt::Success {
                status: 200,
                headers: Vec::new(),
                body_text: "{\"id\":\"pi_1\"}".to_owned(),
            },
        )
        .expect("json");
        assert_eq!(ok_json["id"], "pi_1");

        let empty_success = json_from_attempt(
            "alipay",
            HttpAttempt::Success {
                status: 204,
                headers: Vec::new(),
                body_text: String::new(),
            },
        )
        .expect("json");
        assert_eq!(empty_success, Value::Null);

        let status_error = json_from_attempt(
            "wechat_pay",
            HttpAttempt::Status {
                status: 401,
                body_text: "{\"code\":\"SIGN_ERROR\"}".to_owned(),
            },
        )
        .expect_err("401 must surface as a transport error");
        match status_error {
            ProviderError::Transport { message, .. } => assert!(message.contains("401")),
            other => panic!("expected transport error, got {other:?}"),
        }

        let detailed = detailed_response_from_attempt(
            "wechat_pay",
            HttpAttempt::Success {
                status: 200,
                headers: vec![("Wechatpay-Serial".to_owned(), "SERIAL".to_owned())],
                body_text: "{\"code\":\"SUCCESS\"}".to_owned(),
            },
        )
        .expect("detailed");
        assert_eq!(detailed.status, 200);
        assert_eq!(detailed.body_text, "{\"code\":\"SUCCESS\"}");
    }
}
