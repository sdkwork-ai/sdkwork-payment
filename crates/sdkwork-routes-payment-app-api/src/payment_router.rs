use crate::api_response::{
    map_service_error, not_found, success_command_accepted, success_created_item, success_item,
    success_list, unauthorized, validation,
};
use crate::command_headers::{validate_app_write_payload, write_payload_with_route_param};
use crate::subject::app_runtime_subject_from_extension;
use axum::extract::{Extension, Path, Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use sdkwork_contract_service::CommerceServiceError;
use sdkwork_iam_context_service::IamAppContext;
use sdkwork_payment_providers::{
    cancel_provider_payment, provider_registry_for_account, PaymentProviderRegistry,
    ProviderCredentialBundle,
};
use sdkwork_payment_repository_sqlx::{
    enrich_owner_order_payment_postgres, enrich_payment_record_checkout_postgres,
    load_active_provider_account_postgres, load_payment_attempt_provider_context_postgres,
    load_provider_account_for_existing_payment_postgres, provider_account_binding,
    OwnerOrderPaymentEnrichmentContext, PostgresCommerceOwnerOrderPaymentStore,
    PostgresCommercePaymentMethodStore, PostgresCommercePaymentRecordStore,
};
use sdkwork_payment_service::{
    scene_code_filter_from_client_type, ClosePaymentRecordCommand, PayOwnerOrderCommand,
    PayOwnerOrderCommandInput, PayOwnerOrderOutcome, PaymentMethodItem, PaymentMethodListPage,
    PaymentMethodListQuery, PaymentRecordDetailQuery, PaymentRecordItem, PaymentRecordListPage,
    PaymentRecordListQuery, PaymentRecordOrderListPage, PaymentRecordOrderListQuery,
    PaymentRecordOutTradeNoQuery, PaymentRecordStatistics, PaymentRecordStatisticsQuery,
};
use sdkwork_utils_rust::OffsetListPageParams;
use sdkwork_web_core::WebRequestContext;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
pub type CommercePaymentFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, CommerceServiceError>> + Send + 'a>>;
pub trait CommercePaymentStore: Send + Sync {
    fn list_payment_methods<'a>(
        &'a self,
        query: PaymentMethodListQuery,
    ) -> CommercePaymentFuture<'a, PaymentMethodListPage>;
    fn list_payment_records<'a>(
        &'a self,
        query: PaymentRecordListQuery,
    ) -> CommercePaymentFuture<'a, PaymentRecordListPage>;
    fn retrieve_payment_record<'a>(
        &'a self,
        query: PaymentRecordDetailQuery,
    ) -> CommercePaymentFuture<'a, PaymentRecordItem>;
    fn list_payment_records_by_order<'a>(
        &'a self,
        query: PaymentRecordOrderListQuery,
    ) -> CommercePaymentFuture<'a, PaymentRecordOrderListPage>;
    fn retrieve_payment_record_by_out_trade_no<'a>(
        &'a self,
        query: PaymentRecordOutTradeNoQuery,
    ) -> CommercePaymentFuture<'a, PaymentRecordItem>;
    fn fetch_payment_statistics<'a>(
        &'a self,
        query: PaymentRecordStatisticsQuery,
    ) -> CommercePaymentFuture<'a, PaymentRecordStatistics>;
    fn close_payment_record<'a>(
        &'a self,
        command: ClosePaymentRecordCommand,
    ) -> CommercePaymentFuture<'a, ()>;
    fn pay_owner_order<'a>(
        &'a self,
        command: PayOwnerOrderCommand,
    ) -> CommercePaymentFuture<'a, PayOwnerOrderOutcome>;
}
#[derive(Clone)]
pub(crate) enum PaymentCheckoutDeps {
    Postgres {
        pool: PgPool,
        registry: Arc<PaymentProviderRegistry>,
        credentials: ProviderCredentialBundle,
    },
}
#[derive(Clone)]
pub(crate) struct AppPaymentState {
    store: Arc<dyn CommercePaymentStore>,
    checkout: Option<PaymentCheckoutDeps>,
}
#[derive(Debug, Deserialize)]
struct PaymentRecordsQueryParams {
    page: Option<i64>,
    page_size: Option<i64>,
    #[serde(alias = "orderId")]
    order_id: Option<String>,
}
#[derive(Debug, Deserialize)]
struct PaymentMethodsQueryParams {
    page: Option<i64>,
    page_size: Option<i64>,
    #[serde(rename = "clientType", alias = "client_type")]
    client_type: Option<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PaymentRecordResponse {
    payment_id: String,
    order_id: String,
    out_trade_no: String,
    payment_method: String,
    amount: String,
    created_at: String,
    status: String,
    status_name: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PaymentMethodResponse {
    method_id: String,
    code: String,
    method_name: String,
    available: bool,
    sort: i64,
    product_types: Vec<PaymentMethodProductTypeResponse>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PaymentMethodProductTypeResponse {
    code: String,
    name: String,
    available: bool,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppCommercePaymentAttemptRecordResponse {
    id: String,
    order_no: String,
    method: String,
    amount: String,
    date: String,
    status: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PaymentStatisticsResponse {
    total_payments: i64,
    pending_payments: i64,
    success_payments: i64,
    failed_payments: i64,
    timeout_payments: i64,
    closed_payments: i64,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReconcilePaymentRequest {
    order_id: Option<String>,
    #[serde(rename = "outTradeNo", alias = "out_trade_no")]
    out_trade_no: Option<String>,
    #[serde(rename = "reconcileType", alias = "reconcile_type")]
    reconcile_type: Option<String>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreatePaymentRequest {
    order_id: String,
    #[serde(rename = "paymentMethod", alias = "payment_method")]
    payment_method: Option<String>,
    amount: Option<String>,
    #[serde(rename = "businessOrderId", alias = "business_order_id")]
    business_order_id: Option<String>,
    #[serde(rename = "businessType", alias = "business_type")]
    business_type: Option<String>,
    #[serde(rename = "clientIp", alias = "client_ip")]
    client_ip: Option<String>,
    #[serde(rename = "payerOpenId", alias = "payer_open_id", alias = "openid")]
    payer_open_id: Option<String>,
    #[serde(rename = "paymentProvider", alias = "payment_provider")]
    payment_provider: Option<String>,
    #[serde(rename = "paymentScene", alias = "payment_scene")]
    payment_scene: Option<String>,
    #[serde(rename = "productType", alias = "product_type")]
    product_type: Option<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreatePaymentResponse {
    payment_id: String,
    order_id: String,
    out_trade_no: String,
    amount: String,
    payment_method: String,
    status: String,
    status_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    payment_params: Option<std::collections::BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    payment_url: Option<String>,
}
pub fn app_payment_router_with_postgres_pool(
    pool: PgPool,
    registry: Arc<PaymentProviderRegistry>,
    credentials: ProviderCredentialBundle,
) -> Router {
    let method_store = Arc::new(PostgresCommercePaymentMethodStore::new(pool.clone()));
    let store = Arc::new(CompositeCommercePaymentStore {
        methods: method_store,
        payments: Arc::new(ProviderEnrichedPostgresPayments {
            inner: Arc::new(PostgresCommerceOwnerOrderPaymentStore::new(pool.clone())),
            pool: pool.clone(),
            registry: registry.clone(),
            credentials: credentials.clone(),
        }),
        records: Arc::new(ProviderEnrichedPostgresPaymentRecords {
            inner: Arc::new(PostgresCommercePaymentRecordStore::new(pool.clone())),
            pool: pool.clone(),
            credentials: credentials.clone(),
        }),
    });
    build_app_payment_router_with_state(AppPaymentState {
        store,
        checkout: Some(PaymentCheckoutDeps::Postgres {
            pool,
            registry,
            credentials,
        }),
    })
}
/// Store-only router without PSP checkout enrichment. Prefer [`app_payment_router_with_postgres_pool`]
/// or [`app_payment_router_with_postgres_pool`] in production gateways.
pub fn build_app_payment_router(store: Arc<dyn CommercePaymentStore>) -> Router {
    build_app_payment_router_with_state(AppPaymentState {
        store,
        checkout: None,
    })
}
fn build_app_payment_router_with_state(state: AppPaymentState) -> Router {
    Router::new()
        .route("/app/v3/api/payments/methods", get(list_payment_methods))
        .route("/app/v3/api/payments/records", get(list_payment_records))
        .route(
            "/app/v3/api/payments/records/{paymentId}",
            get(retrieve_payment_record),
        )
        .route(
            "/app/v3/api/payments/attempts/{paymentAttemptId}",
            get(retrieve_payment_attempt),
        )
        .route(
            "/app/v3/api/payments/statistics/summary",
            get(fetch_payment_statistics),
        )
        .route(
            "/app/v3/api/payments/checkout/{paymentId}",
            get(retrieve_payment_checkout),
        )
        .route(
            "/app/v3/api/payments/status/{paymentId}",
            get(retrieve_payment_status),
        )
        .route(
            "/app/v3/api/payments/status/out_trade_no/{outTradeNo}",
            get(retrieve_payment_status_by_out_trade_no),
        )
        .route("/app/v3/api/payments", post(create_payment))
        .route("/app/v3/api/payments/reconcile", post(reconcile_payment))
        .route(
            "/app/v3/api/payments/{paymentId}/close",
            post(close_payment_record),
        )
        .with_state(state)
}
struct CompositeCommercePaymentStore {
    methods: Arc<dyn PaymentMethodSource>,
    payments: Arc<dyn OwnerOrderPaymentSource>,
    records: Arc<dyn PaymentRecordSource>,
}
struct ProviderEnrichedPostgresPayments {
    inner: Arc<PostgresCommerceOwnerOrderPaymentStore>,
    pool: PgPool,
    registry: Arc<PaymentProviderRegistry>,
    credentials: ProviderCredentialBundle,
}
struct ProviderEnrichedPostgresPaymentRecords {
    inner: Arc<PostgresCommercePaymentRecordStore>,
    pool: PgPool,
    credentials: ProviderCredentialBundle,
}
impl OwnerOrderPaymentSource for ProviderEnrichedPostgresPayments {
    fn pay_owner_order<'a>(
        &'a self,
        command: PayOwnerOrderCommand,
    ) -> CommercePaymentFuture<'a, PayOwnerOrderOutcome> {
        let registry = self.registry.clone();
        let credentials = self.credentials.clone();
        let pool = self.pool.clone();
        Box::pin(async move {
            let tenant_id = command.tenant_id.clone();
            let organization_id = command.organization_id.clone();
            let order_id = command.order_id.clone();
            let payment_scene = command.payment_scene.clone();
            let outcome = self.inner.pay_owner_order(command).await?;
            enrich_owner_order_payment_postgres(
                &pool,
                OwnerOrderPaymentEnrichmentContext {
                    deployment_registry: &registry,
                    credentials: &credentials,
                    tenant_id: &tenant_id,
                    organization_id: organization_id.as_deref(),
                    order_id: &order_id,
                    payment_scene: payment_scene.as_deref(),
                },
                outcome,
            )
            .await
        })
    }
}
trait PaymentMethodSource: Send + Sync {
    fn list_payment_methods<'a>(
        &'a self,
        query: PaymentMethodListQuery,
    ) -> CommercePaymentFuture<'a, PaymentMethodListPage>;
}
trait OwnerOrderPaymentSource: Send + Sync {
    fn pay_owner_order<'a>(
        &'a self,
        command: PayOwnerOrderCommand,
    ) -> CommercePaymentFuture<'a, PayOwnerOrderOutcome>;
}
trait PaymentRecordSource: Send + Sync {
    fn list_payment_records<'a>(
        &'a self,
        query: PaymentRecordListQuery,
    ) -> CommercePaymentFuture<'a, PaymentRecordListPage>;
    fn retrieve_payment_record<'a>(
        &'a self,
        query: PaymentRecordDetailQuery,
    ) -> CommercePaymentFuture<'a, PaymentRecordItem>;
    fn list_payment_records_by_order<'a>(
        &'a self,
        query: PaymentRecordOrderListQuery,
    ) -> CommercePaymentFuture<'a, PaymentRecordOrderListPage>;
    fn retrieve_payment_record_by_out_trade_no<'a>(
        &'a self,
        query: PaymentRecordOutTradeNoQuery,
    ) -> CommercePaymentFuture<'a, PaymentRecordItem>;
    fn fetch_payment_statistics<'a>(
        &'a self,
        query: PaymentRecordStatisticsQuery,
    ) -> CommercePaymentFuture<'a, PaymentRecordStatistics>;
    fn close_payment_record<'a>(
        &'a self,
        command: ClosePaymentRecordCommand,
    ) -> CommercePaymentFuture<'a, ()>;
}
impl PaymentMethodSource for PostgresCommercePaymentMethodStore {
    fn list_payment_methods<'a>(
        &'a self,
        query: PaymentMethodListQuery,
    ) -> CommercePaymentFuture<'a, PaymentMethodListPage> {
        Box::pin(async move { self.list_payment_methods(query).await })
    }
}
impl OwnerOrderPaymentSource for PostgresCommerceOwnerOrderPaymentStore {
    fn pay_owner_order<'a>(
        &'a self,
        command: PayOwnerOrderCommand,
    ) -> CommercePaymentFuture<'a, PayOwnerOrderOutcome> {
        Box::pin(async move { self.pay_owner_order(command).await })
    }
}
impl PaymentRecordSource for PostgresCommercePaymentRecordStore {
    fn list_payment_records<'a>(
        &'a self,
        query: PaymentRecordListQuery,
    ) -> CommercePaymentFuture<'a, PaymentRecordListPage> {
        Box::pin(async move { self.list_payment_records(query).await })
    }
    fn retrieve_payment_record<'a>(
        &'a self,
        query: PaymentRecordDetailQuery,
    ) -> CommercePaymentFuture<'a, PaymentRecordItem> {
        Box::pin(async move { self.retrieve_payment_record(query).await })
    }
    fn list_payment_records_by_order<'a>(
        &'a self,
        query: PaymentRecordOrderListQuery,
    ) -> CommercePaymentFuture<'a, PaymentRecordOrderListPage> {
        Box::pin(async move { self.list_payment_records_by_order(query).await })
    }
    fn retrieve_payment_record_by_out_trade_no<'a>(
        &'a self,
        query: PaymentRecordOutTradeNoQuery,
    ) -> CommercePaymentFuture<'a, PaymentRecordItem> {
        Box::pin(async move { self.retrieve_payment_record_by_out_trade_no(query).await })
    }
    fn fetch_payment_statistics<'a>(
        &'a self,
        query: PaymentRecordStatisticsQuery,
    ) -> CommercePaymentFuture<'a, PaymentRecordStatistics> {
        Box::pin(async move { self.fetch_payment_statistics(query).await })
    }
    fn close_payment_record<'a>(
        &'a self,
        command: ClosePaymentRecordCommand,
    ) -> CommercePaymentFuture<'a, ()> {
        Box::pin(async move { self.close_payment_record(command).await })
    }
}
impl PaymentRecordSource for ProviderEnrichedPostgresPaymentRecords {
    fn list_payment_records<'a>(
        &'a self,
        query: PaymentRecordListQuery,
    ) -> CommercePaymentFuture<'a, PaymentRecordListPage> {
        let inner = self.inner.clone();
        Box::pin(async move { inner.list_payment_records(query).await })
    }
    fn retrieve_payment_record<'a>(
        &'a self,
        query: PaymentRecordDetailQuery,
    ) -> CommercePaymentFuture<'a, PaymentRecordItem> {
        let inner = self.inner.clone();
        Box::pin(async move { inner.retrieve_payment_record(query).await })
    }
    fn list_payment_records_by_order<'a>(
        &'a self,
        query: PaymentRecordOrderListQuery,
    ) -> CommercePaymentFuture<'a, PaymentRecordOrderListPage> {
        let inner = self.inner.clone();
        Box::pin(async move { inner.list_payment_records_by_order(query).await })
    }
    fn retrieve_payment_record_by_out_trade_no<'a>(
        &'a self,
        query: PaymentRecordOutTradeNoQuery,
    ) -> CommercePaymentFuture<'a, PaymentRecordItem> {
        let inner = self.inner.clone();
        Box::pin(async move { inner.retrieve_payment_record_by_out_trade_no(query).await })
    }
    fn fetch_payment_statistics<'a>(
        &'a self,
        query: PaymentRecordStatisticsQuery,
    ) -> CommercePaymentFuture<'a, PaymentRecordStatistics> {
        let inner = self.inner.clone();
        Box::pin(async move { inner.fetch_payment_statistics(query).await })
    }
    fn close_payment_record<'a>(
        &'a self,
        command: ClosePaymentRecordCommand,
    ) -> CommercePaymentFuture<'a, ()> {
        let pool = self.pool.clone();
        let credentials = self.credentials.clone();
        let inner = self.inner.clone();
        Box::pin(async move {
            let tenant_id = command.tenant_id.clone();
            let organization_id = command.organization_id.clone();
            let provider_ctx = load_payment_attempt_provider_context_postgres(
                &pool,
                &tenant_id,
                &command.owner_user_id,
                &command.payment_id,
            )
            .await?;
            if let Some(ctx) = provider_ctx {
                let account = match ctx.provider_account_id.as_deref() {
                    Some(provider_account_id) => Some(
                        load_provider_account_for_existing_payment_postgres(
                            &pool,
                            &tenant_id,
                            organization_id.as_deref(),
                            provider_account_id,
                        )
                        .await?
                        .ok_or_else(|| {
                            CommerceServiceError::conflict(
                                "original payment provider account is unavailable for close",
                            )
                        })?,
                    ),
                    None if ctx.channel_id.is_some() => None,
                    None => {
                        load_active_provider_account_postgres(
                            &pool,
                            &tenant_id,
                            organization_id.as_deref(),
                            &ctx.provider_code,
                        )
                        .await?
                    }
                };
                let registry = provider_registry_for_account(
                    &credentials,
                    account.map(|record| provider_account_binding(&record)),
                );
                cancel_provider_payment(
                    &registry,
                    &ctx.provider_code,
                    &ctx.out_trade_no,
                    ctx.provider_transaction_id.as_deref(),
                )
                .await?;
            }
            inner.close_payment_record(command).await?;
            Ok(())
        })
    }
}
impl CommercePaymentStore for CompositeCommercePaymentStore {
    fn list_payment_methods<'a>(
        &'a self,
        query: PaymentMethodListQuery,
    ) -> CommercePaymentFuture<'a, PaymentMethodListPage> {
        self.methods.list_payment_methods(query)
    }
    fn list_payment_records<'a>(
        &'a self,
        query: PaymentRecordListQuery,
    ) -> CommercePaymentFuture<'a, PaymentRecordListPage> {
        self.records.list_payment_records(query)
    }
    fn retrieve_payment_record<'a>(
        &'a self,
        query: PaymentRecordDetailQuery,
    ) -> CommercePaymentFuture<'a, PaymentRecordItem> {
        self.records.retrieve_payment_record(query)
    }
    fn list_payment_records_by_order<'a>(
        &'a self,
        query: PaymentRecordOrderListQuery,
    ) -> CommercePaymentFuture<'a, PaymentRecordOrderListPage> {
        self.records.list_payment_records_by_order(query)
    }
    fn retrieve_payment_record_by_out_trade_no<'a>(
        &'a self,
        query: PaymentRecordOutTradeNoQuery,
    ) -> CommercePaymentFuture<'a, PaymentRecordItem> {
        self.records.retrieve_payment_record_by_out_trade_no(query)
    }
    fn fetch_payment_statistics<'a>(
        &'a self,
        query: PaymentRecordStatisticsQuery,
    ) -> CommercePaymentFuture<'a, PaymentRecordStatistics> {
        self.records.fetch_payment_statistics(query)
    }
    fn close_payment_record<'a>(
        &'a self,
        command: ClosePaymentRecordCommand,
    ) -> CommercePaymentFuture<'a, ()> {
        self.records.close_payment_record(command)
    }
    fn pay_owner_order<'a>(
        &'a self,
        command: PayOwnerOrderCommand,
    ) -> CommercePaymentFuture<'a, PayOwnerOrderOutcome> {
        self.payments.pay_owner_order(command)
    }
}
async fn list_payment_methods(
    State(state): State<AppPaymentState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    Query(params): Query<PaymentMethodsQueryParams>,
) -> Response {
    let ctx = request_ctx(&request_context);
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized(ctx, message),
    };
    let page_params = OffsetListPageParams::parse(params.page, params.page_size);
    let scene_filter = scene_code_filter_from_client_type(params.client_type.as_deref());
    let query =
        match PaymentMethodListQuery::new(&subject.tenant_id, subject.organization_id.as_deref()) {
            Ok(query) => query
                .with_paging(page_params.offset, page_params.page_size)
                .with_scene_code_filter(scene_filter),
            Err(error) => return validation(ctx, error.message()),
        };
    match state.store.list_payment_methods(query).await {
        Ok(page) => {
            let items = page
                .items
                .into_iter()
                .map(map_payment_method)
                .collect::<Vec<_>>();
            success_list(ctx, items, page.total_items, page_params)
        }
        Err(error) => {
            payment_system_response(ctx, "payment methods read model is unavailable", error)
        }
    }
}
async fn list_payment_records(
    State(state): State<AppPaymentState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    Query(params): Query<PaymentRecordsQueryParams>,
) -> Response {
    let ctx = request_ctx(&request_context);
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized(ctx, message),
    };
    // Phase 1.3：在 handler 解析标准分页参数（page/page_size），下推为 offset/limit 到 SQL。
    // 默认 page=1, page_size=20，page_size 上限 200（PAGINATION_SPEC §2）。
    let page_params = OffsetListPageParams::parse(params.page, params.page_size);
    let page = if let Some(order_id) = params.order_id.as_deref() {
        let query = match PaymentRecordOrderListQuery::new(
            &subject.tenant_id,
            subject.organization_id.as_deref(),
            &subject.user_id,
            order_id,
        ) {
            Ok(query) => query.with_paging(page_params.offset, page_params.page_size),
            Err(error) => return validation(ctx, error.message()),
        };
        state
            .store
            .list_payment_records_by_order(query)
            .await
            .map(|page| (page.items, page.total_items))
    } else {
        let query = match PaymentRecordListQuery::new(
            &subject.tenant_id,
            subject.organization_id.as_deref(),
            &subject.user_id,
        ) {
            Ok(query) => query.with_paging(page_params.offset, page_params.page_size),
            Err(error) => return validation(ctx, error.message()),
        };
        state
            .store
            .list_payment_records(query)
            .await
            .map(|page| (page.items, page.total_items))
    };
    match page {
        Ok((page_items, total_items)) => {
            // Phase 1.3：store 已在 SQL 层完成 LIMIT/OFFSET 并返回真实 total_items，
            // handler 不再做进程内 skip/take（PAGINATION_SPEC §2 合规）。
            let items: Vec<_> = page_items.into_iter().map(map_payment_record).collect();
            success_list(ctx, items, total_items, page_params)
        }
        Err(error) => {
            payment_system_response(ctx, "payment records read model is unavailable", error)
        }
    }
}
async fn retrieve_payment_record(
    State(state): State<AppPaymentState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    Path(payment_id): Path<String>,
) -> Response {
    let ctx = request_ctx(&request_context);
    match load_payment_record(state, runtime_context, ctx, payment_id).await {
        Ok(record) => success_item(ctx, map_payment_record(record)),
        Err(response) => response,
    }
}
async fn retrieve_payment_attempt(
    State(state): State<AppPaymentState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    Path(payment_attempt_id): Path<String>,
) -> Response {
    let ctx = request_ctx(&request_context);
    match load_payment_record(state, runtime_context, ctx, payment_attempt_id).await {
        Ok(record) => success_item(ctx, map_payment_attempt_record(record)),
        Err(response) => response,
    }
}
async fn retrieve_payment_checkout(
    State(state): State<AppPaymentState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    Path(payment_id): Path<String>,
) -> Response {
    let ctx = request_ctx(&request_context);
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized(ctx, message),
    };
    let query = match PaymentRecordDetailQuery::new(
        &subject.tenant_id,
        subject.organization_id.as_deref(),
        &subject.user_id,
        &payment_id,
    ) {
        Ok(query) => query,
        Err(error) => return validation(ctx, error.message()),
    };
    let record = match state.store.retrieve_payment_record(query).await {
        Ok(record) => record,
        Err(error) => {
            return payment_system_response(
                ctx,
                "payment checkout read model is unavailable",
                error,
            )
        }
    };
    let Some(checkout) = state.checkout.as_ref() else {
        return payment_system_response(
            ctx,
            "payment checkout enrichment is not configured for this router",
            CommerceServiceError::provider_unavailable(
                "payment checkout requires a database pool and provider registry",
            ),
        );
    };
    let enriched = match checkout {
        PaymentCheckoutDeps::Postgres {
            pool,
            registry,
            credentials,
        } => {
            enrich_payment_record_checkout_postgres(
                pool,
                registry,
                credentials,
                &subject.tenant_id,
                subject.organization_id.as_deref(),
                &subject.user_id,
                record,
            )
            .await
        }
    };
    match enriched {
        Ok(outcome) => success_item(ctx, map_checkout_from_outcome(outcome)),
        Err(error) => payment_system_response(ctx, "payment checkout enrichment failed", error),
    }
}
async fn retrieve_payment_status(
    State(state): State<AppPaymentState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    Path(payment_id): Path<String>,
) -> Response {
    let ctx = request_ctx(&request_context);
    match load_payment_record(state, runtime_context, ctx, payment_id).await {
        Ok(record) => success_item(ctx, map_payment_record(record)),
        Err(response) => response,
    }
}
async fn retrieve_payment_status_by_out_trade_no(
    State(state): State<AppPaymentState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    Path(out_trade_no): Path<String>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized(ctx, message),
    };
    let query = match PaymentRecordOutTradeNoQuery::new(
        &subject.tenant_id,
        subject.organization_id.as_deref(),
        &subject.user_id,
        &out_trade_no,
    ) {
        Ok(query) => query,
        Err(error) => return validation(ctx, error.message()),
    };
    match state
        .store
        .retrieve_payment_record_by_out_trade_no(query)
        .await
    {
        Ok(record) => success_item(ctx, map_payment_record(record)),
        Err(error) => {
            payment_system_response(ctx, "payment record read model is unavailable", error)
        }
    }
}
async fn fetch_payment_statistics(
    State(state): State<AppPaymentState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized(ctx, message),
    };
    let query = match PaymentRecordStatisticsQuery::new(
        &subject.tenant_id,
        subject.organization_id.as_deref(),
        &subject.user_id,
    ) {
        Ok(query) => query,
        Err(error) => return validation(ctx, error.message()),
    };
    match state.store.fetch_payment_statistics(query).await {
        Ok(statistics) => success_item(ctx, map_payment_statistics(statistics)),
        Err(error) => {
            payment_system_response(ctx, "payment statistics read model is unavailable", error)
        }
    }
}
async fn load_payment_record(
    state: AppPaymentState,
    runtime_context: Option<Extension<IamAppContext>>,
    ctx: Option<&WebRequestContext>,
    payment_id: String,
) -> Result<PaymentRecordItem, Response> {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return Err(unauthorized(ctx, message)),
    };
    let query = match PaymentRecordDetailQuery::new(
        &subject.tenant_id,
        subject.organization_id.as_deref(),
        &subject.user_id,
        &payment_id,
    ) {
        Ok(query) => query,
        Err(error) => return Err(validation(ctx, error.message())),
    };
    state
        .store
        .retrieve_payment_record(query)
        .await
        .map_err(|error| {
            payment_system_response(ctx, "payment record read model is unavailable", error)
        })
}
async fn reconcile_payment(
    State(state): State<AppPaymentState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    headers: HeaderMap,
    body: Json<ReconcilePaymentRequest>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized(ctx, message),
    };
    let _write_headers = match validate_app_write_payload(
        &headers,
        "payments.reconcile",
        &body.0,
        |idempotency_key| format!("payment-reconcile-{idempotency_key}"),
    ) {
        Ok(headers) => headers,
        Err(response) => return response,
    };
    let order_id = body
        .order_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let out_trade_no = body
        .out_trade_no
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let reconcile_type = body
        .reconcile_type
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            if order_id.is_some() {
                "ORDER_ID"
            } else {
                "OUT_TRADE_NO"
            }
        });
    let record_result = if reconcile_type.eq_ignore_ascii_case("ORDER_ID") {
        let Some(order_id) = order_id else {
            return validation(ctx, "orderId is required for ORDER_ID reconciliation");
        };
        let query = match PaymentRecordOrderListQuery::new(
            &subject.tenant_id,
            subject.organization_id.as_deref(),
            &subject.user_id,
            order_id,
        ) {
            Ok(query) => query.with_paging(0, 1),
            Err(error) => return validation(ctx, error.message()),
        };
        match state.store.list_payment_records_by_order(query).await {
            Ok(page) => {
                if let Some(record) = page.items.into_iter().next() {
                    Ok(record)
                } else {
                    Err(CommerceServiceError::not_found(
                        "payment record was not found",
                    ))
                }
            }
            Err(error) => Err(error),
        }
    } else if out_trade_no.is_some() || reconcile_type.eq_ignore_ascii_case("OUT_TRADE_NO") {
        let Some(out_trade_no) = out_trade_no else {
            return validation(
                ctx,
                "outTradeNo is required for OUT_TRADE_NO reconciliation",
            );
        };
        let query = match PaymentRecordOutTradeNoQuery::new(
            &subject.tenant_id,
            subject.organization_id.as_deref(),
            &subject.user_id,
            out_trade_no,
        ) {
            Ok(query) => query,
            Err(error) => return validation(ctx, error.message()),
        };
        state
            .store
            .retrieve_payment_record_by_out_trade_no(query)
            .await
    } else {
        return validation(ctx, "reconcileType must be ORDER_ID or OUT_TRADE_NO");
    };
    match record_result {
        Ok(record) => success_item(ctx, map_payment_record(record)),
        Err(error) if error.code() == "not-found" => not_found(ctx, "payment record was not found"),
        Err(error) => {
            payment_system_response(ctx, "payment reconcile read model is unavailable", error)
        }
    }
}
async fn create_payment(
    State(state): State<AppPaymentState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    headers: HeaderMap,
    body: Json<CreatePaymentRequest>,
) -> Response {
    let ctx = request_ctx(&request_context);
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized(ctx, message),
    };
    // C9 修复：所有写操作必须校验 Idempotency-Key 与 Sdkwork-Request-Hash 命令头，
    // 保证客户端重试不会触发非幂等副作用。原代码完全跳过此校验。
    let write_headers =
        match validate_app_write_payload(&headers, "payment-create", &body.0, |idempotency_key| {
            format!("payment-create-{idempotency_key}")
        }) {
            Ok(headers) => headers,
            Err(response) => return response,
        };
    let payment_method = body
        .payment_method
        .clone()
        .unwrap_or_else(|| "wechat_native".to_owned());
    let payment_scene = body.payment_scene.clone().or(body.product_type.clone());
    let mut payment_metadata = serde_json::json!({});
    if let Some(client_ip) = body
        .client_ip
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        payment_metadata["client_ip"] = serde_json::json!(client_ip);
    }
    if let Some(openid) = body
        .payer_open_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        payment_metadata["openid"] = serde_json::json!(openid);
    }
    let _ = (
        body.amount.as_deref(),
        body.business_order_id.as_deref(),
        body.business_type.as_deref(),
        body.payment_provider.as_deref(),
    );
    let command = match PayOwnerOrderCommand::new(PayOwnerOrderCommandInput {
        tenant_id: subject.tenant_id.clone(),
        organization_id: subject.organization_id.clone(),
        owner_user_id: subject.user_id.clone(),
        order_id: body.order_id.clone(),
        payment_method,
        payment_scene,
        payment_attempt_callback_payload: Some(
            serde_json::json!({ "paymentMetadata": payment_metadata.clone() }).to_string(),
        ),
        payment_metadata,
        request_no: write_headers.request_no.clone(),
        idempotency_key: write_headers.idempotency_key.clone(),
    }) {
        Ok(command) => command,
        Err(error) => return validation(ctx, error.message()),
    };
    match state.store.pay_owner_order(command).await {
        Ok(outcome) => success_created_item(ctx, map_checkout_from_outcome(outcome)),
        Err(error) => payment_system_response(ctx, "payment create command failed", error),
    }
}
async fn close_payment_record(
    State(state): State<AppPaymentState>,
    runtime_context: Option<Extension<IamAppContext>>,
    request_context: Option<Extension<WebRequestContext>>,
    headers: HeaderMap,
    Path(payment_id): Path<String>,
) -> Response {
    let ctx = request_context.as_ref().map(|Extension(value)| value);
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized(ctx, message),
    };
    let payload = write_payload_with_route_param("paymentId", &payment_id, &serde_json::json!({}));
    let _write_headers =
        match validate_app_write_payload(&headers, "payments.close", &payload, |idempotency_key| {
            format!("payment-close-{payment_id}-{idempotency_key}")
        }) {
            Ok(headers) => headers,
            Err(response) => return response,
        };
    let command = match ClosePaymentRecordCommand::new(
        &subject.tenant_id,
        subject.organization_id.as_deref(),
        &subject.user_id,
        &payment_id,
    ) {
        Ok(command) => command,
        Err(error) => return validation(ctx, error.message()),
    };
    match state.store.close_payment_record(command).await {
        Ok(()) => success_command_accepted(ctx, Some(payment_id)),
        Err(error) => payment_system_response(ctx, "payment close command failed", error),
    }
}
fn request_ctx(ext: &Option<Extension<WebRequestContext>>) -> Option<&WebRequestContext> {
    ext.as_ref().map(|Extension(value)| value)
}
fn map_checkout_from_outcome(value: PayOwnerOrderOutcome) -> CreatePaymentResponse {
    let payment_url = value
        .payment_params
        .get("cashierUrl")
        .cloned()
        .or_else(|| value.payment_params.get("paymentUrl").cloned())
        .or_else(|| value.payment_params.get("qrCodeUrl").cloned());
    let status = map_payment_status_code(&value.status);
    CreatePaymentResponse {
        payment_id: value.payment_id,
        order_id: value.order_id,
        out_trade_no: value.out_trade_no,
        amount: value.amount.as_str().to_owned(),
        payment_method: value.payment_method,
        status: status.to_owned(),
        status_name: format_payment_status_name(status),
        payment_params: Some(value.payment_params),
        payment_url,
    }
}
fn map_payment_method(value: PaymentMethodItem) -> PaymentMethodResponse {
    let product_types =
        sdkwork_payment_service::wire_product_types_from_scene_codes(&value.scene_codes)
            .into_iter()
            .map(|(code, name)| PaymentMethodProductTypeResponse {
                code,
                name,
                available: true,
            })
            .collect();
    PaymentMethodResponse {
        method_id: value.id,
        code: value.method_key,
        method_name: value.display_name,
        available: true,
        sort: value.sort_order,
        product_types,
    }
}
fn map_payment_record(value: PaymentRecordItem) -> PaymentRecordResponse {
    let status = map_payment_status_code(&value.status);
    PaymentRecordResponse {
        payment_id: value.id,
        order_id: value.order_id,
        out_trade_no: value.order_no,
        payment_method: value.method,
        amount: value.amount.as_str().to_owned(),
        created_at: value.date,
        status: status.to_owned(),
        status_name: format_payment_status_name(status),
    }
}
fn map_payment_attempt_record(value: PaymentRecordItem) -> AppCommercePaymentAttemptRecordResponse {
    let status = map_payment_status_code(&value.status);
    AppCommercePaymentAttemptRecordResponse {
        id: value.id,
        order_no: value.order_no,
        method: value.method,
        amount: value.amount.as_str().to_owned(),
        date: value.date,
        status: status.to_owned(),
    }
}
fn map_payment_statistics(value: PaymentRecordStatistics) -> PaymentStatisticsResponse {
    PaymentStatisticsResponse {
        total_payments: value.total_payments,
        pending_payments: value.pending_payments,
        success_payments: value.success_payments,
        failed_payments: value.failed_payments,
        timeout_payments: value.timeout_payments,
        closed_payments: value.closed_payments,
    }
}
fn map_payment_status_code(status: &str) -> &'static str {
    match status.trim().to_ascii_lowercase().as_str() {
        "success" | "succeeded" | "paid" => "SUCCESS",
        "failed" => "FAILED",
        "timeout" => "TIMEOUT",
        "closed" | "canceled" | "cancelled" => "CLOSED",
        _ => "PENDING",
    }
}
fn format_payment_status_name(status: &str) -> String {
    match status {
        "SUCCESS" => "Success".to_owned(),
        "FAILED" => "Failed".to_owned(),
        "TIMEOUT" => "Timeout".to_owned(),
        "CLOSED" => "Closed".to_owned(),
        _ => "Pending".to_owned(),
    }
}
fn payment_system_response(
    context: Option<&WebRequestContext>,
    _label: &str,
    error: CommerceServiceError,
) -> Response {
    map_service_error(context, error)
}
