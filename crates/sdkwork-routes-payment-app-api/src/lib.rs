pub mod api_response;
pub mod command_headers;
pub mod http_route_manifest;
pub mod payment_intent_router;
pub mod payment_router;
pub mod refund_router;
pub mod routes;
pub mod subject;
pub mod web_bootstrap;
pub mod webhook_router;

pub use payment_intent_router::{
    app_payment_intent_router_with_postgres_pool, build_app_payment_intent_router,
    CommercePaymentIntentFuture, CommercePaymentIntentStore,
};
pub use payment_router::{
    app_payment_router_with_postgres_pool, build_app_payment_router, CommercePaymentFuture,
    CommercePaymentStore,
};
pub use refund_router::{
    app_refund_router_with_postgres_pool, build_app_refund_router, CommerceRefundFuture,
    CommerceRefundStore,
};
pub use routes::{build_payment_app_router_with_framework, build_payment_federated_app_router};
pub use web_bootstrap::{wrap_router_with_web_framework, wrap_router_with_web_framework_from_env};
pub use webhook_router::payment_webhook_router_deprecated;

use axum::Router;
use sdkwork_payment_service_host::PaymentServiceHost;
use sdkwork_web_core::HttpRouteManifest;
use std::sync::Arc;

/// C17 修复：网关装配入口，构造 payment app-api 的完整 framework router。
pub async fn gateway_mount(host: Arc<PaymentServiceHost>) -> Router {
    build_payment_app_router_with_framework(host).await
}

/// Business-only assembly entrypoint. Infrastructure probes remain listener-owned.
pub async fn gateway_mount_business(host: Arc<PaymentServiceHost>) -> Router {
    routes::build_payment_app_router(host)
}

/// Business-only entrypoint for a listener that also composes the Order App API.
pub async fn gateway_mount_federated_business(host: Arc<PaymentServiceHost>) -> Router {
    routes::build_payment_federated_app_router(host)
}

/// C17 修复：导出 route manifest，满足 `WEB_BACKEND_SPEC.md` §4.2 的导出契约。
/// 物化器与网关通过此函数获取 payment app-api 的路由契约元数据。
pub fn gateway_route_manifest() -> HttpRouteManifest {
    http_route_manifest::app_route_manifest()
}

/// Manifest paired with [`gateway_mount_federated_business`].
pub fn federated_gateway_route_manifest() -> HttpRouteManifest {
    http_route_manifest::federated_app_route_manifest()
}
