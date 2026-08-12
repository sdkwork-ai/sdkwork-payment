//! SDKWork HTTP API 响应信封构建器（`API_SPEC.md` §4.5/§14/§15/§16）。
//!
//! 所有 success 响应 `MUST` 使用 `SdkWorkApiResponse<T>` 输出 `{code:0, data, traceId}`。
//! 所有 error 响应 `MUST` 使用 `application/problem+json`（`SdkWorkProblemDetail`）。
//!
//! - 单资源：`data: { item: T }`（`success_item`）
//! - 列表：`data: { items: [T], pageInfo: PageInfo }`（`success_list`）
//! - 命令：`data: { accepted: true, resourceId?, status? }`（`success_command`）
//!
//! 禁止在 handler 中手工构造 `{code, msg, data}` 旧业务信封。

use axum::http::{header, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use sdkwork_contract_service::CommerceServiceError;
use sdkwork_utils_rust::{
    offset_list_page_data, OffsetListPageParams, SdkWorkApiResponse, SdkWorkCommandData,
    SdkWorkPageData, SdkWorkProblemDetail, SdkWorkResourceData, SdkWorkResultCode,
};
use sdkwork_web_core::WebRequestContext;
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SdkWorkAsyncCommandData {
    pub accepted: bool,
    pub operation_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll_url: Option<String>,
}

/// 解析 trace_id：优先从请求上下文提取，否则生成新 uuid。
pub fn resolve_trace_id(context: Option<&WebRequestContext>) -> String {
    context
        .and_then(|ctx| ctx.trace_id.clone())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(sdkwork_utils_rust::uuid)
}

/// 构建 HTTP 200 单资源成功响应：`{code:0, data:{item}, traceId}`。
pub fn success_item<T: Serialize>(context: Option<&WebRequestContext>, item: T) -> Response {
    let trace_id = resolve_trace_id(context);
    let envelope = SdkWorkApiResponse::success(SdkWorkResourceData { item }, trace_id.clone());
    attach_trace_header((StatusCode::OK, Json(envelope)).into_response(), &trace_id)
}

/// Build an HTTP 201 create response with the created resource under `data.item`.
pub fn success_created_item<T: Serialize>(
    context: Option<&WebRequestContext>,
    item: T,
) -> Response {
    let trace_id = resolve_trace_id(context);
    let envelope = SdkWorkApiResponse::success(SdkWorkResourceData { item }, trace_id.clone());
    attach_trace_header(
        (StatusCode::CREATED, Json(envelope)).into_response(),
        &trace_id,
    )
}

/// Build an HTTP 204 delete response. Trace correlation remains available in the header.
pub fn success_no_content(context: Option<&WebRequestContext>) -> Response {
    let trace_id = resolve_trace_id(context);
    attach_trace_header(StatusCode::NO_CONTENT.into_response(), &trace_id)
}

/// 构建 HTTP 200 列表成功响应：`{code:0, data:{items, pageInfo}, traceId}`。
///
/// `total_items` 为满足查询条件的总记录数（来自 `COUNT(*) OVER()` 或独立 count 查询），
/// `params` 为已解析的分页参数。`PageInfo.mode` 固定为 `offset`。
pub fn success_list<T: Serialize>(
    context: Option<&WebRequestContext>,
    items: Vec<T>,
    total_items: i64,
    params: OffsetListPageParams,
) -> Response {
    let trace_id = resolve_trace_id(context);
    let page_data: SdkWorkPageData<T> = offset_list_page_data(items, total_items, params);
    let envelope = SdkWorkApiResponse::success(page_data, trace_id.clone());
    attach_trace_header((StatusCode::OK, Json(envelope)).into_response(), &trace_id)
}

/// 构建 HTTP 200 命令成功响应：`{code:0, data:{accepted:true, resourceId?, status?}, traceId}`。
pub fn success_command(
    context: Option<&WebRequestContext>,
    command: SdkWorkCommandData,
) -> Response {
    let trace_id = resolve_trace_id(context);
    let envelope = SdkWorkApiResponse::success(command, trace_id.clone());
    attach_trace_header((StatusCode::OK, Json(envelope)).into_response(), &trace_id)
}

/// 构建 HTTP 200 命令成功响应（便捷版）：`accepted:true` + 可选 `resourceId`。
pub fn success_command_accepted(
    context: Option<&WebRequestContext>,
    resource_id: Option<String>,
) -> Response {
    let command = SdkWorkCommandData {
        accepted: true,
        resource_id,
        status: None,
    };
    success_command(context, command)
}

/// 构建 HTTP 202 异步命令接受响应：`{code:0, data:{accepted:true, operationId, status, pollUrl?}, traceId}`。
///
/// 注：`SdkWorkCommandData` 当前只暴露 `accepted/resource_id/status`，对于完整异步语义
/// （`operationId/status/pollUrl`）需要扩展类型；此处沿用同步命令结构，HTTP 状态码仍为 202。
pub fn accepted_command(
    context: Option<&WebRequestContext>,
    resource_id: Option<String>,
    status: Option<String>,
) -> Response {
    let trace_id = resolve_trace_id(context);
    let command = SdkWorkCommandData {
        accepted: true,
        resource_id,
        status,
    };
    let envelope = SdkWorkApiResponse::success(command, trace_id.clone());
    attach_trace_header(
        (StatusCode::ACCEPTED, Json(envelope)).into_response(),
        &trace_id,
    )
}

/// Build the canonical HTTP 202 response for an accepted asynchronous operation.
pub fn accepted_async_command(
    context: Option<&WebRequestContext>,
    operation_id: String,
    status: impl Into<String>,
    poll_url: Option<String>,
) -> Response {
    let trace_id = resolve_trace_id(context);
    let data = SdkWorkAsyncCommandData {
        accepted: true,
        operation_id,
        status: status.into(),
        poll_url,
    };
    let envelope = SdkWorkApiResponse::success(data, trace_id.clone());
    attach_trace_header(
        (StatusCode::ACCEPTED, Json(envelope)).into_response(),
        &trace_id,
    )
}

/// 将 `CommerceServiceError` 映射为 `application/problem+json` 响应。
///
/// 映射遵循 `API_SPEC.md` §15.3 的平台错误码：
/// - `validation` → 400 `ValidationError` (40001)
/// - `not-found` → 404 `NotFound` (40401)
/// - `conflict` → 409 `Conflict` (40901)
/// - `unauthorized`/`unauthenticated` → 401 `AuthenticationRequired` (40101)
/// - 其它 → 500 `InternalError` (50001)
pub fn map_service_error(
    context: Option<&WebRequestContext>,
    error: CommerceServiceError,
) -> Response {
    let trace_id = resolve_trace_id(context);
    let (status, result_code, detail) = match error.code() {
        "validation" => (
            StatusCode::BAD_REQUEST,
            SdkWorkResultCode::ValidationError,
            error.message().to_string(),
        ),
        "not-found" => (
            StatusCode::NOT_FOUND,
            SdkWorkResultCode::NotFound,
            error.message().to_string(),
        ),
        "conflict" | "unsupported-capability" => (
            StatusCode::CONFLICT,
            SdkWorkResultCode::Conflict,
            error.message().to_string(),
        ),
        "invalid-state" => (
            StatusCode::UNPROCESSABLE_ENTITY,
            SdkWorkResultCode::UnprocessableEntity,
            error.message().to_string(),
        ),
        "unauthorized" | "unauthenticated" => (
            StatusCode::UNAUTHORIZED,
            SdkWorkResultCode::AuthenticationRequired,
            error.message().to_string(),
        ),
        "provider-unavailable" => (
            StatusCode::SERVICE_UNAVAILABLE,
            SdkWorkResultCode::ServiceUnavailable,
            sanitize_provider_error_message(error.message()),
        ),
        "transport" => (
            StatusCode::BAD_GATEWAY,
            SdkWorkResultCode::BadGateway,
            sanitize_provider_error_message(error.message()),
        ),
        "storage" => (
            StatusCode::INTERNAL_SERVER_ERROR,
            SdkWorkResultCode::InternalError,
            error.message().to_string(),
        ),
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            SdkWorkResultCode::InternalError,
            error.message().to_string(),
        ),
    };
    let problem = SdkWorkProblemDetail::platform(result_code, detail, trace_id.clone());
    problem_json_response(status, problem, trace_id)
}

/// 构建 502 Bad Gateway Problem+json 响应（上游依赖不可用）。
pub fn bad_gateway(context: Option<&WebRequestContext>, detail: impl Into<String>) -> Response {
    let trace_id = resolve_trace_id(context);
    let problem =
        SdkWorkProblemDetail::platform(SdkWorkResultCode::BadGateway, detail, trace_id.clone());
    problem_json_response(StatusCode::BAD_GATEWAY, problem, trace_id)
}

/// 构建 401 Unauthorized Problem+json 响应。
pub fn unauthorized(context: Option<&WebRequestContext>, detail: impl Into<String>) -> Response {
    let trace_id = resolve_trace_id(context);
    let problem = SdkWorkProblemDetail::platform(
        SdkWorkResultCode::AuthenticationRequired,
        detail,
        trace_id.clone(),
    );
    problem_json_response(StatusCode::UNAUTHORIZED, problem, trace_id)
}

/// 构建 403 Forbidden Problem+json 响应。
pub fn forbidden(context: Option<&WebRequestContext>, detail: impl Into<String>) -> Response {
    let trace_id = resolve_trace_id(context);
    let problem = SdkWorkProblemDetail::platform(
        SdkWorkResultCode::PermissionRequired,
        detail,
        trace_id.clone(),
    );
    problem_json_response(StatusCode::FORBIDDEN, problem, trace_id)
}

/// Strips the PSP's own HTTP status prefix (`"HTTP 401 Unauthorized: {...}"`)
/// from a provider transport message so business errors never carry
/// `"401"`/`"Unauthorized"` text — admin session boundaries match such
/// patterns and would mistake a payment-gateway rejection for a login problem.
pub fn sanitize_provider_error_message(message: &str) -> String {
    let mut sanitized = message.trim().to_owned();
    for prefix in [
        "HTTP 401 Unauthorized: ",
        "HTTP 401: ",
        "HTTP 400: ",
        "HTTP 403: ",
        "HTTP 404: ",
        "HTTP 409: ",
        "HTTP 429: ",
        "HTTP 500: ",
        "HTTP 502: ",
        "HTTP 503: ",
    ] {
        if sanitized.starts_with(prefix) {
            sanitized = format!("payment gateway rejected: {}", &sanitized[prefix.len()..]);
            break;
        }
    }
    sanitized
}

/// 构建 400 Bad Request Problem+json 响应（校验错误）。
pub fn validation(context: Option<&WebRequestContext>, detail: impl Into<String>) -> Response {
    let trace_id = resolve_trace_id(context);
    let problem = SdkWorkProblemDetail::platform(
        SdkWorkResultCode::ValidationError,
        detail,
        trace_id.clone(),
    );
    problem_json_response(StatusCode::BAD_REQUEST, problem, trace_id)
}

/// 构建 400 Problem+json 响应：支付网关拒绝了请求（如微信 `SIGN_ERROR`、
/// Stripe `Invalid API Key`）。
///
/// 使用独立的 `41101` 错误码（41 开头系列，远离 401xx 认证码），HTTP 状态为
/// 400 而非 401——前端会话边界不会把支付网关的凭据拒绝误判为登录失效而跳转
/// 登录页。
pub fn provider_rejected(
    context: Option<&WebRequestContext>,
    detail: impl Into<String>,
) -> Response {
    let trace_id = resolve_trace_id(context);
    let problem = SdkWorkProblemDetail::platform(
        SdkWorkResultCode::PaymentGatewayRejected,
        detail,
        trace_id.clone(),
    );
    problem_json_response(StatusCode::BAD_REQUEST, problem, trace_id)
}

/// 构建 404 Not Found Problem+json 响应。
pub fn not_found(context: Option<&WebRequestContext>, detail: impl Into<String>) -> Response {
    let trace_id = resolve_trace_id(context);
    let problem =
        SdkWorkProblemDetail::platform(SdkWorkResultCode::NotFound, detail, trace_id.clone());
    problem_json_response(StatusCode::NOT_FOUND, problem, trace_id)
}

/// 构建 409 Conflict Problem+json 响应。
pub fn conflict(context: Option<&WebRequestContext>, detail: impl Into<String>) -> Response {
    let trace_id = resolve_trace_id(context);
    let problem =
        SdkWorkProblemDetail::platform(SdkWorkResultCode::Conflict, detail, trace_id.clone());
    problem_json_response(StatusCode::CONFLICT, problem, trace_id)
}

/// 仅为 offset_list_page_info 的再导出，便于 handler 直接构造 `PageInfo`。
pub use sdkwork_utils_rust::offset_list_page_info as build_offset_page_info;

fn problem_json_response(
    status: StatusCode,
    problem: SdkWorkProblemDetail,
    trace_id: String,
) -> Response {
    let mut response = (status, Json(problem)).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/problem+json"),
    );
    attach_trace_header(response, &trace_id)
}

fn attach_trace_header(response: Response, trace_id: &str) -> Response {
    let mut response = response;
    if let Ok(value) = HeaderValue::from_str(trace_id) {
        response
            .headers_mut()
            .insert(HeaderName::from_static("x-sdkwork-trace-id"), value);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use axum::http::header;

    #[test]
    fn not_found_response_uses_problem_json_content_type() {
        let response = not_found(None, "payment method was not found");
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        assert_eq!("application/problem+json", content_type);
    }

    #[tokio::test]
    async fn not_found_response_uses_numeric_platform_code() {
        let response = not_found(None, "payment method was not found");
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert!(payload["code"].is_number());
        assert!(payload["traceId"].is_string());
    }

    #[test]
    fn create_and_delete_helpers_use_standard_http_statuses() {
        assert_eq!(
            success_created_item(None, serde_json::json!({"id": "method-1"})).status(),
            StatusCode::CREATED,
        );
        assert_eq!(success_no_content(None).status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn provider_rejected_uses_41101_and_http_400_not_401() {
        // Regression: a PSP rejection (WeChat SIGN_ERROR, Stripe Invalid API
        // Key) must NOT surface as 40001/401xx or HTTP 401 — admin session
        // boundaries would mistake it for a login problem and redirect to the
        // login page.
        let response = provider_rejected(None, "payment provider checkout failed");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(41101, payload["code"].as_i64().unwrap());
        assert_eq!(400, payload["status"].as_u64().unwrap());
    }

    #[test]
    fn sanitizes_psp_http_status_prefix_from_transport_messages() {
        assert_eq!(
            sanitize_provider_error_message(
                r#"HTTP 401 Unauthorized: {"code":"SIGN_ERROR","message":"签名错误"}"#,
            ),
            r#"payment gateway rejected: {"code":"SIGN_ERROR","message":"签名错误"}"#,
        );
        assert_eq!(
            sanitize_provider_error_message("HTTP 404: ORDER_NOT_EXIST"),
            "payment gateway rejected: ORDER_NOT_EXIST",
        );
        // Messages without a PSP HTTP status prefix pass through unchanged.
        assert_eq!(
            sanitize_provider_error_message("payment provider wechat_pay is not configured"),
            "payment provider wechat_pay is not configured",
        );
        // The sanitized message must never carry 401/Unauthorized patterns
        // that admin session boundaries match.
        let sanitized = sanitize_provider_error_message(
            r#"HTTP 401 Unauthorized: {"code":"SIGN_ERROR","message":"签名错误"}"#,
        );
        assert!(!sanitized.contains("401"));
        assert!(!sanitized.contains("Unauthorized"));
    }
}
