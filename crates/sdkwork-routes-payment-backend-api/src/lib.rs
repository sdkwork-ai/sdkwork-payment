pub mod api_response;
pub mod backend_payment_admin_router;
pub mod backend_payment_integration_router;
pub mod backend_payment_intent_router;
pub mod backend_payment_refund_router;
pub mod command_headers;
pub mod http_route_manifest;
pub mod routes;
pub mod subject;
pub mod web_bootstrap;

pub use backend_payment_admin_router::{
    backend_payment_admin_router_with_postgres_pool, build_backend_payment_admin_router,
    BackendPaymentMethodListQuery, CommerceBackendPaymentAdminStore,
};
pub use backend_payment_intent_router::{
    backend_payment_intent_router_with_postgres_pool, build_backend_payment_intent_router,
    CommerceBackendPaymentIntentStore,
};
pub use backend_payment_refund_router::backend_payment_refund_router_with_postgres_pool;
pub use routes::build_payment_backend_router_with_framework;
pub use web_bootstrap::{wrap_router_with_web_framework, wrap_router_with_web_framework_from_env};

use axum::Router;
use sdkwork_payment_service_host::PaymentServiceHost;
use sdkwork_web_core::HttpRouteManifest;
use std::sync::Arc;

#[cfg(test)]
pub(crate) fn ensure_test_payment_credential_cipher() {
    use sdkwork_payment_providers::{
        install_payment_credential_cipher, payment_credential_cipher_is_installed,
        LocalFilePaymentCredentialCipher,
    };

    if payment_credential_cipher_is_installed() {
        return;
    }
    let cipher = LocalFilePaymentCredentialCipher::from_key_material(vec![11; 64])
        .expect("test credential cipher");
    let _ = install_payment_credential_cipher(Arc::new(cipher));
    assert!(payment_credential_cipher_is_installed());
}

/// C17 修复：网关装配入口，构造 payment backend-api 的完整 framework router。
pub async fn gateway_mount(host: Arc<PaymentServiceHost>) -> Router {
    build_payment_backend_router_with_framework(host).await
}

/// Business-only assembly entrypoint. Infrastructure probes remain listener-owned.
pub async fn gateway_mount_business(host: Arc<PaymentServiceHost>) -> Router {
    routes::build_payment_backend_router(host)
}

/// C17 修复：导出 route manifest，满足 `WEB_BACKEND_SPEC.md` §4.2 的导出契约。
/// 物化器与网关通过此函数获取 payment backend-api 的路由契约元数据。
pub fn gateway_route_manifest() -> HttpRouteManifest {
    http_route_manifest::backend_route_manifest()
}
