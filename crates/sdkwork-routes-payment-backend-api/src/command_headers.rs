use axum::http::HeaderMap;
use sdkwork_utils_rust::command_headers::{
    parse_sdkwork_write_command_headers, sdkwork_stable_canonical_json_request_hash,
    sdkwork_stable_command_request_hash, sdkwork_stable_json_request_hash,
    SdkWorkWriteCommandHeaderError, SdkWorkWriteCommandHeaders, SDKWORK_IDEMPOTENCY_KEY_HEADER,
    SDKWORK_REQUEST_HASH_HEADER, SDKWORK_REQUEST_NO_HEADER,
};
use serde::Serialize;

pub(crate) const IDEMPOTENCY_KEY_HEADER: &str = SDKWORK_IDEMPOTENCY_KEY_HEADER;
pub(crate) const REQUEST_HASH_HEADER: &str = SDKWORK_REQUEST_HASH_HEADER;
pub(crate) const REQUEST_NO_HEADER: &str = SDKWORK_REQUEST_NO_HEADER;

pub(crate) type AppWriteCommandHeaders = SdkWorkWriteCommandHeaders;
pub(crate) type WriteCommandHeaderError = SdkWorkWriteCommandHeaderError;

pub(crate) fn stable_command_request_hash(scope: &str, parts: &[&str]) -> String {
    sdkwork_stable_command_request_hash(scope, parts)
}

pub(crate) fn stable_json_request_hash(
    scope: &str,
    value: &impl Serialize,
) -> Result<String, WriteCommandHeaderError> {
    sdkwork_stable_json_request_hash(scope, value)
}

pub(crate) fn stable_canonical_json_request_hash(scope: &str, value: &serde_json::Value) -> String {
    sdkwork_stable_canonical_json_request_hash(scope, value)
}

/// Validates the idempotent-command headers for a backend write route.
///
/// Both `Idempotency-Key` and `Sdkwork-Request-Hash` are required by the
/// OpenAPI contract (`IdempotencyKey` parameter, `required: true`) and by
/// `API_SPEC.md` idempotent command rules: a missing key must be rejected so
/// client retries cannot silently bypass replay protection.
#[allow(clippy::result_large_err)]
pub(crate) fn validate_write_payload(
    headers: &HeaderMap,
    scope: &str,
    body: &impl Serialize,
    fallback_request_no: impl FnOnce(&str) -> String,
) -> Result<AppWriteCommandHeaders, WriteCommandHeaderError> {
    let write_headers = parse_required_write_command_headers(headers, fallback_request_no)?;
    let expected_hash = stable_json_request_hash(scope, body)?;
    if expected_hash.trim() != write_headers.request_hash.trim() {
        return Err(WriteCommandHeaderError::InvalidHeader(
            "Sdkwork-Request-Hash does not match the command payload",
        ));
    }
    Ok(write_headers)
}

fn header_text(headers: &HeaderMap, name: &'static str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

#[allow(clippy::result_large_err)]
pub(crate) fn parse_required_write_command_headers(
    headers: &HeaderMap,
    fallback_request_no: impl FnOnce(&str) -> String,
) -> Result<AppWriteCommandHeaders, WriteCommandHeaderError> {
    parse_sdkwork_write_command_headers(
        header_text(headers, IDEMPOTENCY_KEY_HEADER).as_deref(),
        header_text(headers, REQUEST_HASH_HEADER).as_deref(),
        header_text(headers, REQUEST_NO_HEADER).as_deref(),
        fallback_request_no,
    )
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    #[test]
    fn write_headers_require_client_identity() {
        let error = validate_write_payload(
            &HeaderMap::new(),
            "scope",
            &serde_json::json!({"orderId":"o-1"}),
            |key| format!("request-{key}"),
        )
        .expect_err("missing headers must be rejected");
        assert!(matches!(error, WriteCommandHeaderError::MissingHeader(_)));
    }

    #[test]
    fn write_headers_accept_contract_compliant_identity() {
        let scope = "scope";
        let body = serde_json::json!({"orderId":"o-1"});
        let request_hash = stable_json_request_hash(scope, &body).expect("hash");
        let mut headers = HeaderMap::new();
        headers.insert(IDEMPOTENCY_KEY_HEADER, HeaderValue::from_static("idem-key-1"));
        headers.insert(REQUEST_HASH_HEADER, HeaderValue::from_str(&request_hash).expect("header value"));
        let parsed = validate_write_payload(&headers, scope, &body, |key| {
            format!("request-{key}")
        })
        .expect("headers");
        assert_eq!(parsed.idempotency_key, "idem-key-1");
        assert_eq!(parsed.request_no, "request-idem-key-1");
        assert_eq!(parsed.request_hash, request_hash);
    }

    #[test]
    fn stable_command_request_hash_is_deterministic() {
        let first = stable_command_request_hash("scope", &["100001", "request-1"]);
        let second = stable_command_request_hash("scope", &["100001", "request-1"]);
        assert_eq!(first, second);
        assert!(!first.is_empty());
    }

    #[test]
    fn validate_write_payload_rejects_request_hash_mismatch() {
        let mut headers = HeaderMap::new();
        headers.insert(IDEMPOTENCY_KEY_HEADER, HeaderValue::from_static("idem-key-1"));
        headers.insert(REQUEST_HASH_HEADER, HeaderValue::from_static("wrong"));
        let error = validate_write_payload(
            &headers,
            "scope",
            &serde_json::json!({"orderId":"o-1"}),
            |key| format!("request-{key}"),
        )
        .expect_err("mismatch");
        assert!(matches!(error, WriteCommandHeaderError::InvalidHeader(_)));
    }
}
