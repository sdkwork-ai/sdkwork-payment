use sdkwork_api_payment_assembly::{assemble_api_router, PaymentServiceHost};
use sdkwork_iam_web_adapter::{
    build_web_framework_builder, iam_web_request_context_resolver_from_env,
};
use sdkwork_web_bootstrap::{infra_public_path_prefixes, ApiModuleRegistry, ComposedApiAssembly};
use std::time::Duration;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

mod observability;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const BINARY_NAME: &str = "sdkwork-api-payment-standalone-gateway";

/// Information commands run before any runtime initialization
/// (PACKAGING_SPEC.md §5.2: `--help`/`--version` must have no durable or
/// external side effects — no database, no network, no tracing exporter).
fn handle_info_args(args: &[String]) -> Option<i32> {
    let first = args.first()?.as_str();
    match first {
        "--version" | "-V" => {
            println!("{BINARY_NAME} {VERSION}");
            Some(0)
        }
        "--help" | "-h" => {
            println!(
                "{BINARY_NAME} {VERSION}\n\nSDKWork payment standalone API gateway.\n\n\
                 USAGE:\n    {BINARY_NAME} [--version] [--help]\n\n\
                 The gateway is configured through environment variables only\n\
                 (see .env.postgres.example and docs/architecture/tech/TECH_ARCHITECTURE.md):\n\
                 \x20 SDKWORK_DATABASE_*              authoritative PostgreSQL connection\n\
                 \x20 PAYMENT_API_BIND                ingress bind address (default 0.0.0.0:18094)\n\
                 \x20 SDKWORK_PAYMENT_APPLICATION_PUBLIC_INGRESS_BIND\n\
                 \x20                                 topology-contract bind key (wins over PAYMENT_API_BIND)\n\
                 \x20 SDKWORK_CORS_ALLOWED_ORIGINS    browser origin allow-list\n\
                 \x20 SDKWORK_PAYMENT_CREDENTIAL_MASTER_KEY_FILE\n\
                 \x20                                 provider credential master key (required in production)\n\
                 \x20 OTEL_EXPORTER_OTLP_ENDPOINT     enables OTLP/HTTP span export when set\n\n\
                 Running without arguments starts the HTTP server."
            );
            Some(0)
        }
        other => {
            eprintln!("{BINARY_NAME}: unrecognized argument '{other}' (run with --help)");
            Some(2)
        }
    }
}

/// Standalone payment API gateway.
///
/// - CORS is allow-list driven (`SDKWORK_CORS_ALLOWED_ORIGINS`, deny by default).
/// - Graceful shutdown (SIGINT/SIGTERM), 30s request timeout, 1 MiB body cap.
/// - The host-owned compensation worker reconciles lost-PSP-callback attempts
///   and stuck refunds on a bounded, jittered cadence; it drains with the
///   same shutdown signal as the HTTP server.
/// - Span export goes through OTLP/HTTP when `OTEL_EXPORTER_OTLP_ENDPOINT`
///   is set (see `observability.rs`).
#[tokio::main]
async fn main() {
    if let Some(code) = handle_info_args(&std::env::args().skip(1).collect::<Vec<_>>()) {
        std::process::exit(code);
    }

    let otel_guard = observability::init_tracing();

    // The host owns the authoritative pool; the compensation worker shares it
    // (the checkout lock gate sizes itself from this pool's capacity).
    let host = std::sync::Arc::new(
        PaymentServiceHost::from_env()
            .await
            .expect("payment service host bootstrap failed"),
    );
    let assembly = assemble_api_router(host.clone())
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
    // Bind precedence: the topology contract key wins, the legacy payment
    // key stays accepted, and the default matches the deployed nginx
    // (reverse-proxy) expectations.
    let addr = std::env::var("SDKWORK_PAYMENT_APPLICATION_PUBLIC_INGRESS_BIND")
        .or_else(|_| std::env::var("PAYMENT_API_BIND"))
        .unwrap_or_else(|_| "0.0.0.0:18094".to_owned());
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind");

    tracing::info!(bind = %addr, version = VERSION, "payment api server starting");

    // Graceful shutdown: SIGINT/SIGTERM stops accepting new connections and
    // drains in-flight requests (bounded by the 30s timeout layer).
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

    let serve = axum::serve(listener, app).with_graceful_shutdown(shutdown);
    if let Err(error) = serve.await {
        eprintln!("payment api server error: {error}");
    }

    // The 30s timeout layer bounds in-flight requests; the host-owned
    // compensation worker pass is bounded by the PSP client timeouts, so this
    // drain terminates.
    host.drain_payment_compensation_worker().await;
    otel_guard.shutdown();
}
