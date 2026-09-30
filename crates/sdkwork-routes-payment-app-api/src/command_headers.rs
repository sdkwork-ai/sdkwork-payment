use axum::http::HeaderMap;
use axum::response::Response;

use crate::api_response::validation;
use sdkwork_utils_rust::command_headers::{
    parse_sdkwork_write_command_headers, sdkwork_stable_json_request_hash,
    sdkwork_write_payload_with_route_param, SdkWorkWriteCommandHeaderError,
    SdkWorkWriteCommandHeaders, SDKWORK_IDEMPOTENCY_KEY_HEADER, SDKWORK_REQUEST_HASH_HEADER,
    SDKWORK_REQUEST_NO_HEADER,
};

pub(crate) const IDEMPOTENCY_KEY_HEADER: &str = SDKWORK_IDEMPOTENCY_KEY_HEADER;
pub(crate) const REQUEST_HASH_HEADER: &str = SDKWORK_REQUEST_HASH_HEADER;
pub(crate) const REQUEST_NO_HEADER: &str = SDKWORK_REQUEST_NO_HEADER;

pub(crate) type AppWriteCommandHeaders = SdkWorkWriteCommandHeaders;
pub(crate) type WriteCommandHeaderError = SdkWorkWriteCommandHeaderError;

#[allow(clippy::result_large_err)]
pub(crate) fn validate_write_payload(
    headers: &HeaderMap,
    scope: &str,
    body: &impl serde::Serialize,
    fallback_request_no: impl FnOnce(&str) -> String,
) -> Result<AppWriteCommandHeaders, WriteCommandHeaderError> {
    let write_headers = parse_required_write_command_headers(headers, fallback_request_no)?;
    let expected_hash = sdkwork_stable_json_request_hash(scope, body)?;
    if expected_hash.trim() != write_headers.request_hash.trim() {
        return Err(WriteCommandHeaderError::InvalidHeader(
            "Sdkwork-Request-Hash does not match the command payload",
        ));
    }
    Ok(write_headers)
}

fn write_command_header_error_to_app_response(error: WriteCommandHeaderError) -> Response {
    match error {
        WriteCommandHeaderError::MissingHeader(name) => {
            command_header_error_response(format!("{name} header is required"))
        }
        WriteCommandHeaderError::InvalidHeader(message) => validation_response(message),
    }
}

#[allow(clippy::result_large_err)]
pub(crate) fn validate_app_write_payload(
    headers: &HeaderMap,
    scope: &str,
    body: &impl serde::Serialize,
    fallback_request_no: impl FnOnce(&str) -> String,
) -> Result<AppWriteCommandHeaders, Response> {
    validate_write_payload(headers, scope, body, fallback_request_no)
        .map_err(write_command_header_error_to_app_response)
}

pub(crate) fn write_payload_with_route_param(
    route_param_key: &str,
    route_param_value: &str,
    body: &impl serde::Serialize,
) -> serde_json::Value {
    sdkwork_write_payload_with_route_param(route_param_key, route_param_value, body)
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

fn command_header_error_response(message: impl Into<String>) -> Response {
    validation(None, message)
}

fn validation_response(message: impl Into<String>) -> Response {
    validation(None, message)
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    #[test]
    fn required_app_write_command_headers_requires_idempotency_and_request_hash() {
        let mut headers = HeaderMap::new();
        headers.insert(IDEMPOTENCY_KEY_HEADER, HeaderValue::from_static("idem-1"));
        headers.insert(REQUEST_HASH_HEADER, HeaderValue::from_static("hash-1"));

        let parsed = parse_required_write_command_headers(&headers, |_| "request-1".to_owned())
            .expect("headers");
        assert_eq!(parsed.idempotency_key, "idem-1");
        assert_eq!(parsed.request_hash, "hash-1");
        assert_eq!(parsed.request_no, "request-1");
    }

    #[test]
    fn missing_required_headers_fail_closed() {
        let headers = HeaderMap::new();
        let error = parse_required_write_command_headers(&headers, |_| "request-1".to_owned())
            .expect_err("missing idempotency key");
        assert!(matches!(error, WriteCommandHeaderError::MissingHeader(_)));
    }

    #[test]
    fn idempotency_key_contract_is_enforced() {
        let mut headers = HeaderMap::new();
        headers.insert(IDEMPOTENCY_KEY_HEADER, HeaderValue::from_static("bad key!"));
        headers.insert(REQUEST_HASH_HEADER, HeaderValue::from_static("hash-1"));
        let error = parse_required_write_command_headers(&headers, |_| "request-1".to_owned())
            .expect_err("invalid key charset");
        assert!(matches!(error, WriteCommandHeaderError::InvalidHeader(_)));
    }

    #[test]
    fn stable_command_request_hash_is_deterministic() {
        use sdkwork_utils_rust::command_headers::sdkwork_stable_command_request_hash;
        let first = sdkwork_stable_command_request_hash("scope", &["100001", "request-1"]);
        let second = sdkwork_stable_command_request_hash("scope", &["100001", "request-1"]);
        assert_eq!(first, second);
        assert!(!first.is_empty());
    }

    #[test]
    fn stable_json_request_hash_matches_struct_and_value_payloads() {
        use sdkwork_utils_rust::command_headers::{
            sdkwork_stable_canonical_json_request_hash, sdkwork_stable_json_request_hash,
        };
        use serde::{Deserialize, Serialize};

        let body_json = r#"{"methodKey":"wechat_pay","displayName":"WeChat Pay","providerCode":"wechat_pay","status":"active"}"#;
        let value: serde_json::Value = serde_json::from_str(body_json).expect("json");
        let from_value = sdkwork_stable_canonical_json_request_hash("payment-method-upsert", &value);

        #[derive(Serialize, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct UpsertPaymentMethodBody {
            method_key: Option<String>,
            display_name: Option<String>,
            provider_code: Option<String>,
            status: Option<String>,
            sort_order: Option<i64>,
        }

        let body: UpsertPaymentMethodBody = serde_json::from_str(body_json).expect("body");
        let from_struct =
            sdkwork_stable_json_request_hash("payment-method-upsert", &body).expect("hash");

        assert_eq!(from_value, from_struct);
    }

    #[test]
    fn validate_write_payload_rejects_request_hash_mismatch() {
        let mut headers = HeaderMap::new();
        headers.insert(IDEMPOTENCY_KEY_HEADER, HeaderValue::from_static("idem-1"));
        headers.insert(REQUEST_HASH_HEADER, HeaderValue::from_static("wrong"));
        let error = validate_write_payload(
            &headers,
            "scope",
            &serde_json::json!({"orderId":"o-1"}),
            |_| "request-1".to_owned(),
        )
        .expect_err("mismatch");
        assert!(matches!(error, WriteCommandHeaderError::InvalidHeader(_)));
    }
}
