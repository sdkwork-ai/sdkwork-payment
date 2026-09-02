use crate::api_response::{map_service_error, not_found, success_item, success_list, unauthorized};
use crate::subject::backend_runtime_subject_from_extension;
use axum::extract::{Extension, Path, Query, State};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use sdkwork_contract_service::CommerceServiceError;
use sdkwork_iam_context_service::IamAppContext;
use sdkwork_utils_rust::OffsetListPageParams;
use sdkwork_web_core::WebRequestContext;
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgRow, PgPool, Row};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
pub type CommerceBackendPaymentIntentFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, CommerceServiceError>> + Send + 'a>>;
pub trait CommerceBackendPaymentIntentStore: Send + Sync {
    fn list_payment_intents<'a>(
        &'a self,
        query: BackendPaymentIntentListQuery,
    ) -> CommerceBackendPaymentIntentFuture<'a, BackendPaymentIntentListPage>;
    fn retrieve_payment_intent<'a>(
        &'a self,
        payment_intent_id: String,
        tenant_scope: TenantScope,
    ) -> CommerceBackendPaymentIntentFuture<'a, Option<BackendPaymentIntentView>>;
}
/// Phase 1.3：标准分页结果，store 一次性返回当前页 items + 满足条件的总记录数。
///
/// `total_items` 来自 SQL `COUNT(*) OVER()` 窗口函数（单次往返），
/// handler 据此填充 `data.pageInfo`，禁止在进程内对全量数据做 skip/take。
#[derive(Debug, Clone)]
pub struct BackendPaymentIntentListPage {
    pub items: Vec<BackendPaymentIntentView>,
    pub total_items: i64,
}
/// C12 修复：Backend 查询必须携带租户范围，避免跨租户数据泄露。
#[derive(Debug, Clone)]
pub struct TenantScope {
    pub tenant_id: String,
    pub organization_id: Option<String>,
}
#[derive(Clone)]
struct BackendPaymentIntentState {
    store: Arc<dyn CommerceBackendPaymentIntentStore>,
}
/// C12 修复：Backend 内部查询结构，tenant_scope 由 IAM context 注入，
/// 不允许从 URL 反序列化，杜绝跨租户越权读取。
///
/// Phase 1.3 合规：分页参数由 handler 通过 `OffsetListPageParams::parse`
/// 解析为 `offset`/`limit` 后传入 store，store 直接下推到 SQL `OFFSET`/`LIMIT`，
/// 不允许 fetch_all + 进程内 skip/take（PAGINATION_SPEC §2）。
#[derive(Debug, Clone)]
pub struct BackendPaymentIntentListQuery {
    pub tenant_scope: TenantScope,
    pub status: Option<String>,
    pub offset: i64,
    pub limit: i64,
}
/// C12 修复：URL 查询参数仅暴露过滤/分页字段，tenant_scope 强制从 IAM context 解析。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackendPaymentIntentListQueryParams {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    page: Option<i64>,
    #[serde(default, rename = "page_size")]
    page_size: Option<i64>,
}
#[derive(Clone, Debug)]
pub struct BackendPaymentIntentView {
    pub payment_intent_id: String,
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub owner_user_id: String,
    pub order_id: String,
    pub payment_intent_no: String,
    pub payment_method: String,
    pub provider_code: String,
    pub amount: String,
    pub currency_code: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BackendPaymentIntentResponse {
    id: String,
    tenant_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    organization_id: Option<String>,
    owner_user_id: String,
    order_id: String,
    payment_intent_no: String,
    payment_method: String,
    provider_code: String,
    amount: String,
    currency_code: String,
    status: String,
    created_at: String,
    updated_at: String,
}
#[derive(Clone)]
struct PostgresBackendPaymentIntentStore {
    pool: PgPool,
}
impl PostgresBackendPaymentIntentStore {
    fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
impl CommerceBackendPaymentIntentStore for PostgresBackendPaymentIntentStore {
    fn list_payment_intents<'a>(
        &'a self,
        query: BackendPaymentIntentListQuery,
    ) -> CommerceBackendPaymentIntentFuture<'a, BackendPaymentIntentListPage> {
        Box::pin(async move {
            // C12 修复：强制 tenant_id 谓词。
            // Phase 1.3：LIMIT/OFFSET 下推到 SQL，COUNT(*) OVER() 一次往返给出总记录数。
            let rows = sqlx::query(
                r#"
                SELECT id, tenant_id, organization_id, owner_user_id, order_id, payment_intent_no,
                       payment_method, provider_code, CAST(amount AS BIGINT)::TEXT AS amount,
                       currency_code, status, created_at, updated_at,
                       COUNT(*) OVER() AS total_count
                FROM commerce_payment_intent
                WHERE tenant_id = CAST($1 AS TEXT)
                  AND ((organization_id = CAST($2 AS TEXT)) OR (organization_id IS NULL AND $3 IS NULL) OR (organization_id = '0' AND $3 IS NULL))
                  AND ($4::text IS NULL OR LOWER(COALESCE(status, '')) = LOWER($4::text))
                ORDER BY created_at DESC, id DESC
                LIMIT $5 OFFSET $6
                "#,
            )
            .bind(&query.tenant_scope.tenant_id)
            .bind(query.tenant_scope.organization_id.as_deref())
            .bind(query.tenant_scope.organization_id.as_deref())
            .bind(query.status.as_deref())
            .bind(query.limit)
            .bind(query.offset)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| {
                CommerceServiceError::storage(format!("failed to list payment intents: {error}"))
            })?;
            let total_items = rows
                .first()
                .and_then(|row| row.try_get::<i64, _>("total_count").ok())
                .unwrap_or(0);
            let items = rows.iter().map(map_payment_intent_row_pg).collect();
            Ok(BackendPaymentIntentListPage { items, total_items })
        })
    }
    fn retrieve_payment_intent<'a>(
        &'a self,
        payment_intent_id: String,
        tenant_scope: TenantScope,
    ) -> CommerceBackendPaymentIntentFuture<'a, Option<BackendPaymentIntentView>> {
        Box::pin(async move {
            let row = sqlx::query(
                r#"
                SELECT id, tenant_id, organization_id, owner_user_id, order_id, payment_intent_no,
                       payment_method, provider_code, CAST(amount AS BIGINT)::TEXT AS amount,
                       currency_code, status, created_at, updated_at
                FROM commerce_payment_intent
                WHERE id = CAST($1 AS TEXT)
                  AND tenant_id = CAST($2 AS TEXT)
                  AND ((organization_id = CAST($3 AS TEXT)) OR (organization_id IS NULL AND $4 IS NULL) OR (organization_id = '0' AND $4 IS NULL))
                LIMIT 1
                "#,
            )
            .bind(&payment_intent_id)
            .bind(&tenant_scope.tenant_id)
            .bind(tenant_scope.organization_id.as_deref())
            .bind(tenant_scope.organization_id.as_deref())
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| {
                CommerceServiceError::storage(format!("failed to retrieve payment intent: {error}"))
            })?;
            Ok(row.as_ref().map(map_payment_intent_row_pg))
        })
    }
}
pub fn backend_payment_intent_router_with_postgres_pool(pool: PgPool) -> Router {
    build_backend_payment_intent_router(Arc::new(PostgresBackendPaymentIntentStore::new(pool)))
}
pub fn build_backend_payment_intent_router(
    store: Arc<dyn CommerceBackendPaymentIntentStore>,
) -> Router {
    Router::new()
        .route(
            "/backend/v3/api/payments/intents",
            get(list_payment_intents),
        )
        .route(
            "/backend/v3/api/payments/intents/{paymentIntentId}",
            get(retrieve_payment_intent),
        )
        .with_state(BackendPaymentIntentState { store })
}
async fn list_payment_intents(
    State(state): State<BackendPaymentIntentState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    Query(params): Query<BackendPaymentIntentListQueryParams>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    // C12 修复：tenant_scope 必须从 IAM context 解析，绝不接受 URL 传入。
    let subject = match backend_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized(ctx, message),
    };
    // Phase 1.3：在 handler 解析标准分页参数（page/page_size），下推为 offset/limit 到 SQL。
    // 默认 page=1, page_size=20，page_size 上限 200（PAGINATION_SPEC §2）。
    let page_params = OffsetListPageParams::parse(params.page, params.page_size);
    let query = BackendPaymentIntentListQuery {
        tenant_scope: TenantScope {
            tenant_id: subject.tenant_id,
            organization_id: subject.organization_id,
        },
        status: params.status,
        offset: page_params.offset,
        limit: page_params.page_size,
    };
    match state.store.list_payment_intents(query).await {
        Ok(page) => {
            // Phase 1.3：store 已在 SQL 层完成 LIMIT/OFFSET 并返回真实 total_items，
            // handler 不再做进程内 skip/take（PAGINATION_SPEC §2 合规）。
            let items: Vec<_> = page.items.into_iter().map(map_payment_intent).collect();
            success_list(ctx, items, page.total_items, page_params)
        }
        Err(error) => backend_payment_intent_error_response(
            ctx,
            "payment intent management list is unavailable",
            error,
        ),
    }
}
async fn retrieve_payment_intent(
    State(state): State<BackendPaymentIntentState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    Path(payment_intent_id): Path<String>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    // C12 修复：retrieve 必须携带 tenant_scope，防止跨租户读取任意 payment intent。
    let subject = match backend_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized(ctx, message),
    };
    let tenant_scope = TenantScope {
        tenant_id: subject.tenant_id,
        organization_id: subject.organization_id,
    };
    match state
        .store
        .retrieve_payment_intent(payment_intent_id, tenant_scope)
        .await
    {
        Ok(Some(intent)) => success_item(ctx, map_payment_intent(intent)),
        Ok(None) => not_found(ctx, "payment intent was not found"),
        Err(error) => backend_payment_intent_error_response(
            ctx,
            "payment intent management read model is unavailable",
            error,
        ),
    }
}
fn backend_payment_intent_error_response(
    ctx: Option<&WebRequestContext>,
    _context: &str,
    error: CommerceServiceError,
) -> Response {
    map_service_error(ctx, error)
}
fn map_payment_intent_row_pg(row: &PgRow) -> BackendPaymentIntentView {
    BackendPaymentIntentView {
        payment_intent_id: pg_string(row, "id"),
        tenant_id: pg_string(row, "tenant_id"),
        organization_id: pg_optional_string(row, "organization_id"),
        owner_user_id: pg_string(row, "owner_user_id"),
        order_id: pg_string(row, "order_id"),
        payment_intent_no: pg_string(row, "payment_intent_no"),
        payment_method: pg_string(row, "payment_method"),
        provider_code: pg_string(row, "provider_code"),
        amount: pg_string(row, "amount"),
        currency_code: pg_string(row, "currency_code"),
        status: pg_string(row, "status"),
        created_at: pg_string(row, "created_at"),
        updated_at: pg_string(row, "updated_at"),
    }
}
fn map_payment_intent(value: BackendPaymentIntentView) -> BackendPaymentIntentResponse {
    BackendPaymentIntentResponse {
        id: value.payment_intent_id,
        tenant_id: value.tenant_id,
        organization_id: value.organization_id,
        owner_user_id: value.owner_user_id,
        order_id: value.order_id,
        payment_intent_no: value.payment_intent_no,
        payment_method: value.payment_method,
        provider_code: value.provider_code,
        amount: value.amount,
        currency_code: value.currency_code,
        status: value.status,
        created_at: value.created_at,
        updated_at: value.updated_at,
    }
}
#[cfg(test)]
mod tests {
    use super::{map_payment_intent, BackendPaymentIntentView};
    #[test]
    fn payment_intent_response_uses_the_openapi_id_field() {
        let response = map_payment_intent(BackendPaymentIntentView {
            payment_intent_id: "intent-1".to_owned(),
            tenant_id: "100001".to_owned(),
            organization_id: Some("100002".to_owned()),
            owner_user_id: "1".to_owned(),
            order_id: "order-1".to_owned(),
            payment_intent_no: "PI-1".to_owned(),
            payment_method: "sandbox_test".to_owned(),
            provider_code: "sandbox".to_owned(),
            amount: "9.99".to_owned(),
            currency_code: "CNY".to_owned(),
            status: "succeeded".to_owned(),
            created_at: "2026-07-20T00:00:00Z".to_owned(),
            updated_at: "2026-07-20T00:00:01Z".to_owned(),
        });
        let value = serde_json::to_value(response).expect("serialize payment intent response");
        assert_eq!(value["id"], "intent-1");
        assert!(value.get("paymentIntentId").is_none());
    }
}
fn pg_optional_string(row: &PgRow, column: &str) -> Option<String> {
    row.try_get::<Option<String>, _>(column).ok().flatten()
}
fn pg_string(row: &PgRow, column: &str) -> String {
    pg_optional_string(row, column).unwrap_or_default()
}
