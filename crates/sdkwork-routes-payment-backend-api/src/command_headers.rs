use axum::http::HeaderMap;
#[cfg(test)]
use sdkwork_utils_rust::command_headers::sdkwork_stable_command_request_hash;
use sdkwork_utils_rust::command_headers::{
    SDKWORK_IDEMPOTENCY_KEY_HEADER, SDKWORK_REQUEST_HASH_HEADER, SDKWORK_REQUEST_NO_HEADER,
    SdkWorkWriteCommandHeaderError, SdkWorkWriteCommandHeaders,
    parse_sdkwork_write_command_headers_with_derived_hash, sdkwork_stable_json_request_hash,
};
use serde::Serialize;

pub(crate) const IDEMPOTENCY_KEY_HEADER: &str = SDKWORK_IDEMPOTENCY_KEY_HEADER;
pub(crate) const REQUEST_HASH_HEADER: &str = SDKWORK_REQUEST_HASH_HEADER;
pub(crate) const REQUEST_NO_HEADER: &str = SDKWORK_REQUEST_NO_HEADER;

pub(crate) type AppWriteCommandHeaders = SdkWorkWriteCommandHeaders;
pub(crate) type WriteCommandHeaderError = SdkWorkWriteCommandHeaderError;

/// Validates the idempotent-command headers for a backend write route.
///
/// `Idempotency-Key` is always required (`API_SPEC.md` §14/§15: the OpenAPI
/// `IdempotencyKey` parameter is `required: true`). The scope-canonical
/// `Sdkwork-Request-Hash` is optional on the wire: when the client sends it the
/// value must match the parsed payload, and when it omits it (the authority
/// OpenAPI declares only `Idempotency-Key`) the server derives the same hash
/// from the request body. Either way the persisted idempotency fingerprint is
/// the canonical body hash, so replaying a key with a different payload still
/// conflicts instead of executing side effects again.
#[allow(clippy::result_large_err)]
pub(crate) fn validate_write_payload(
    headers: &HeaderMap,
    scope: &str,
    body: &impl Serialize,
    fallback_request_no: impl FnOnce(&str) -> String,
) -> Result<AppWriteCommandHeaders, WriteCommandHeaderError> {
    let expected_hash = sdkwork_stable_json_request_hash(scope, body)?;
    let write_headers =
        parse_write_command_headers_with_hash(headers, expected_hash.clone(), fallback_request_no)?;
    if expected_hash.trim() != write_headers.request_hash.trim() {
        return Err(WriteCommandHeaderError::InvalidHeader(
            "Sdkwork-Request-Hash does not match the command payload",
        ));
    }
    Ok(write_headers)
}

/// Parses write-command headers, deriving the request hash from the
/// server-computed expected hash when the client omitted the header.
#[allow(clippy::result_large_err)]
fn parse_write_command_headers_with_hash(
    headers: &HeaderMap,
    expected_hash: String,
    fallback_request_no: impl FnOnce(&str) -> String,
) -> Result<AppWriteCommandHeaders, WriteCommandHeaderError> {
    parse_sdkwork_write_command_headers_with_derived_hash(
        header_text(headers, IDEMPOTENCY_KEY_HEADER).as_deref(),
        header_text(headers, REQUEST_HASH_HEADER).as_deref(),
        header_text(headers, REQUEST_NO_HEADER).as_deref(),
        || Ok(expected_hash),
        fallback_request_no,
    )
}

fn header_text(headers: &HeaderMap, name: &'static str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
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
        let request_hash = sdkwork_stable_json_request_hash(scope, &body).expect("hash");
        let mut headers = HeaderMap::new();
        headers.insert(
            IDEMPOTENCY_KEY_HEADER,
            HeaderValue::from_static("idem-key-1"),
        );
        headers.insert(
            REQUEST_HASH_HEADER,
            HeaderValue::from_str(&request_hash).expect("header value"),
        );
        let parsed = validate_write_payload(&headers, scope, &body, |key| format!("request-{key}"))
            .expect("headers");
        assert_eq!(parsed.idempotency_key, "idem-key-1");
        assert_eq!(parsed.request_no, "request-idem-key-1");
        assert_eq!(parsed.request_hash, request_hash);
    }

    #[test]
    fn stable_command_request_hash_is_deterministic() {
        let first = sdkwork_stable_command_request_hash("scope", &["100001", "request-1"]);
        let second = sdkwork_stable_command_request_hash("scope", &["100001", "request-1"]);
        assert_eq!(first, second);
        assert!(!first.is_empty());
    }

    #[test]
    fn missing_request_hash_is_derived_from_the_payload() {
        let scope = "payment-domain-setting-upsert";
        let body = serde_json::json!({
            "h5Protocol": "https",
            "h5Hostname": "h5.example.com",
            "h5Port": serde_json::Value::Null,
            "h5Status": "active",
        });
        let mut headers = HeaderMap::new();
        headers.insert(IDEMPOTENCY_KEY_HEADER, HeaderValue::from_static("idem-1"));
        let parsed = validate_write_payload(&headers, scope, &body, |key| format!("request-{key}"))
            .expect("a contract-only client omitting Sdkwork-Request-Hash must still be accepted");
        assert_eq!(
            parsed.request_hash,
            sdkwork_stable_json_request_hash(scope, &body).expect("hash")
        );
        assert_eq!(parsed.request_no, "request-idem-1");
    }

    #[test]
    fn validate_write_payload_rejects_request_hash_mismatch() {
        let mut headers = HeaderMap::new();
        headers.insert(
            IDEMPOTENCY_KEY_HEADER,
            HeaderValue::from_static("idem-key-1"),
        );
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
