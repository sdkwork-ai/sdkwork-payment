use sdkwork_api_payment_assembly::assemble_api_router_from_env;
use sdkwork_iam_web_adapter::{
    build_web_framework_builder, iam_web_request_context_resolver_from_env,
};
use sdkwork_web_bootstrap::{infra_public_path_prefixes, ApiModuleRegistry, ComposedApiAssembly};
use std::time::Duration;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

/// C13/H1/C23 修复：API server 生产级启动配置。
///
/// - C13: CORS 由 `PAYMENT_API_CORS_ORIGINS` 环境变量驱动（逗号分隔），默认拒绝跨域，
/// - H1:  接入 graceful shutdown、请求超时（30s）、请求体大小限制（1 MiB）。
/// - C23: 接入 TraceLayer 结构化请求追踪（含 span、URI、状态码、耗时）。
#[tokio::main]
async fn main() {
    // 结构化日志输出，生产环境应配合 OTel exporter（后续 P1 阶段接入）。
    tracing_subscriber::fmt::init();

    let assembly = assemble_api_router_from_env()
        .await
        .expect("payment API assembly failed");
    let framework = build_web_framework_builder(
        iam_web_request_context_resolver_from_env().await,
        assembly.route_manifest.clone(),
        infra_public_path_prefixes(),
    );
    let mut module_registry = ApiModuleRegistry::new();
    module_registry.add_modules(vec![assembly]);
    let app = module_registry
        .try_compose("SDKWork Payment API")
        .expect("payment API composition failed")
        .into_hosted(framework)
        .router
        .layer(RequestBodyLimitLayer::new(1024 * 1024)) // 1 MiB，支付请求体不会超过
        .layer(TimeoutLayer::new(Duration::from_secs(30))) // 30s 超时，防止慢 SQL 拖垮线程池
        .layer(TraceLayer::new_for_http());
    let addr = std::env::var("PAYMENT_API_BIND").unwrap_or_else(|_| "0.0.0.0:18094".to_owned());
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind");

    tracing::info!(bind = %addr, "payment api server starting");

    // H1 修复：graceful shutdown，收到 SIGINT/Ctrl+C 后停止接受新连接，
    // 等待在途请求完成（最多 30s），避免 K8s 滚动更新断连。
    let shutdown = async {
        let ctrl_c = async {
            tokio::signal::ctrl_c()
                .await
                .expect("failed to install Ctrl+C handler");
        };

        #[cfg(unix)]
        let terminate = async {
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("failed to install signal handler")
                .recv()
                .await;
        };

        #[cfg(not(unix))]
        let terminate = std::future::pending::<()>();

        tokio::select! {
            _ = ctrl_c => {},
            _ = terminate => {},
        }

        tracing::info!("payment api server shutdown signal received, draining in-flight requests");
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
        .expect("serve");
}
