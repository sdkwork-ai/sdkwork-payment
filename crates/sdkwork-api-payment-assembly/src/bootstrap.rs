//! API assembly bootstrap for sdkwork-payment.

use axum::Router;
use sdkwork_payment_service_host::PaymentServiceHost;
use sdkwork_web_bootstrap::{
    ApiAssemblyContribution, ContractFallbackConfig, DatabasePoolReadinessCheck,
};
use sdkwork_web_core::HttpRouteManifest;
use std::sync::Arc;

pub type ApiAssembly = ApiAssemblyContribution;

pub struct BusinessRouterAssembly {
    pub router: Router,
}

pub async fn assemble_api_router(host: Arc<PaymentServiceHost>) -> Result<ApiAssembly, String> {
    let router = assemble_business_routes(host.clone()).await.router;
    let mut routes = Vec::new();
    routes.extend_from_slice(sdkwork_routes_payment_app_api::gateway_route_manifest().routes());
    routes.extend_from_slice(sdkwork_routes_payment_backend_api::gateway_route_manifest().routes());
    ApiAssemblyContribution::from_manifest(
        "sdkwork-payment",
        "SDKWork Payment API",
        router,
        HttpRouteManifest::from_owned_routes(routes),
        Vec::new(),
        Arc::new(DatabasePoolReadinessCheck::new(
            host.database_pool().clone(),
        )),
    )
}

pub async fn assemble_business_routes(host: Arc<PaymentServiceHost>) -> BusinessRouterAssembly {
    let mut router = Router::new();
    router =
        router.merge(sdkwork_routes_payment_app_api::gateway_mount_business(host.clone()).await);
    router = router.merge(sdkwork_routes_payment_backend_api::gateway_mount_business(host).await);
    BusinessRouterAssembly { router }
}

pub async fn assemble_backend_business_router(
    host: Arc<PaymentServiceHost>,
) -> BusinessRouterAssembly {
    BusinessRouterAssembly {
        router: sdkwork_routes_payment_backend_api::gateway_mount_business(host).await,
    }
}

pub async fn assemble_app_api_contribution(
    host: Arc<PaymentServiceHost>,
) -> Result<ApiAssemblyContribution, String> {
    let router = sdkwork_routes_payment_app_api::gateway_mount_business(host.clone()).await;
    ApiAssemblyContribution::from_manifest(
        "sdkwork-payment",
        "SDKWork Payment App API",
        router,
        sdkwork_routes_payment_app_api::gateway_route_manifest(),
        Vec::new(),
        Arc::new(DatabasePoolReadinessCheck::new(
            host.database_pool().clone(),
        )),
    )
}

/// Builds the Payment App API contribution for a host that also composes Order routes.
pub async fn assemble_federated_app_api_contribution(
    host: Arc<PaymentServiceHost>,
) -> Result<ApiAssemblyContribution, String> {
    let router =
        sdkwork_routes_payment_app_api::gateway_mount_federated_business(host.clone()).await;
    ApiAssemblyContribution::from_manifest(
        "sdkwork-payment",
        "SDKWork Payment Federated App API",
        router,
        sdkwork_routes_payment_app_api::federated_gateway_route_manifest(),
        Vec::new(),
        Arc::new(DatabasePoolReadinessCheck::new(
            host.database_pool().clone(),
        )),
    )
}

pub fn federated_app_route_manifest() -> HttpRouteManifest {
    sdkwork_routes_payment_app_api::federated_gateway_route_manifest()
}

pub fn gateway_contract_fallback_config() -> ContractFallbackConfig {
    let app_manifest = sdkwork_routes_payment_app_api::gateway_route_manifest();
    let backend_manifest = sdkwork_routes_payment_backend_api::gateway_route_manifest();

    let mut config = ContractFallbackConfig::from_manifest(&app_manifest);
    config
        .manifest_paths
        .extend(ContractFallbackConfig::from_manifest(&backend_manifest).manifest_paths);
    config
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assembly_contract_fallback_contains_both_api_surfaces() {
        let expected_route_count = sdkwork_routes_payment_app_api::gateway_route_manifest()
            .routes()
            .len()
            + sdkwork_routes_payment_backend_api::gateway_route_manifest()
                .routes()
                .len();
        let config = gateway_contract_fallback_config();
        assert_eq!(expected_route_count, config.manifest_paths.len());
        assert!(config.contains("POST", "/app/v3/api/payments/intents"));
        assert!(config.contains("GET", "/backend/v3/api/payments/certificates"));
    }
}
