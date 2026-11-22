//! Operators manage the checkout scene policy through the admin API.
//!
//! Creating a rule, listing it, resolving against it and seeing it in the audit
//! trail must all work against the shipped schema — this is the configuration
//! surface that lets a new platform ship without a release.
//!
//! Requires `SDKWORK_DATABASE_TEST_POSTGRES_URL`; skips cleanly when unset.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Extension;
use http_body_util::BodyExt;
use sdkwork_iam_context_service::{
    AuthLevel, DeploymentMode, Environment, IamAppContext, IamUserSurface,
};
use sdkwork_routes_payment_backend_api::backend_payment_admin_router_with_postgres_pool;
use sqlx::postgres::{PgPool, PgPoolOptions};
use tower::ServiceExt;

fn test_database_url() -> Option<String> {
    std::env::var("SDKWORK_DATABASE_TEST_POSTGRES_URL").ok()
}

async fn policy_pool(label: &str) -> Option<PgPool> {
    let base = test_database_url()?;
    let schema = format!("payment_scene_policy_api_{}_{}", std::process::id(), label);
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .after_connect({
            let schema = schema.clone();
            move |connection, _| {
                let schema = schema.clone();
                Box::pin(async move {
                    sqlx::query(sqlx::AssertSqlSafe(format!(
                        "SET search_path TO {schema}, public"
                    )))
                    .execute(connection)
                    .await?;
                    Ok(())
                })
            }
        })
        .connect(&base)
        .await
        .expect("connect test postgres");
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP SCHEMA IF EXISTS {schema} CASCADE"
    )))
    .execute(&pool)
    .await
    .expect("drop stale schema");
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&pool)
        .await
        .expect("create schema");
    sqlx::raw_sql(sqlx::AssertSqlSafe(include_str!(
        "../../../database/ddl/baseline/postgres/0001_payment_baseline.sql"
    )))
    .execute(&pool)
    .await
    .expect("apply payment baseline");
    Some(pool)
}

fn backend_member_context() -> IamAppContext {
    let mut context = IamAppContext::new(
        "100001",
        None,
        "1",
        "session-1",
        "sdkwork-payment",
        Environment::Dev,
        DeploymentMode::Saas,
        AuthLevel::Password,
        vec!["tenant:100001".to_owned()],
        vec![
            "commerce.payments.checkout_scene_policy.read".to_owned(),
            "commerce.payments.checkout_scene_policy.write".to_owned(),
        ],
    );
    context.user_surface = IamUserSurface {
        app: true,
        organization_member: true,
    };
    context
}

async fn send(router: axum::Router, request: Request<Body>) -> (StatusCode, String) {
    let response = router.oneshot(request).await.expect("route");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    (status, String::from_utf8_lossy(&body).to_string())
}

#[tokio::test]
async fn a_rule_can_be_created_listed_resolved_and_audited() {
    let Some(pool) = policy_pool("crud").await else {
        return;
    };

    // Create an iPad rule: the platform a bare deployment would otherwise leave
    // unconfigured.
    let body = serde_json::json!({
        "scene": "checkout",
        "appArchitecture": "pad",
        "osFamily": "ipados",
        "paymentMethod": "WeChat",
        "productCode": "wechat_app",
        "status": "active",
        "priority": 10
    });
    let mut request = Request::builder()
        .method("PUT")
        .uri("/backend/v3/api/payments/checkout_scene_policies/brand-pad")
        .header("content-type", "application/json")
        .header("Idempotency-Key", "idem-scene-pad")
        .header("X-Request-Id", "req-scene-pad")
        .body(Body::from(body.to_string()))
        .expect("request");
    request.extensions_mut().insert(backend_member_context());
    let (status, payload) = send(
        backend_payment_admin_router_with_postgres_pool(pool.clone()),
        request,
    )
    .await;
    assert!(status.is_success(), "create failed: {status} {payload}");
    assert!(
        payload.contains("wechat_app"),
        "the saved rule must be returned: {payload}"
    );

    // The list must show it.
    let mut request = Request::builder()
        .method("GET")
        .uri("/backend/v3/api/payments/checkout_scene_policies?page=1&page_size=50")
        .body(Body::empty())
        .expect("request");
    request.extensions_mut().insert(backend_member_context());
    let (status, listing) = send(
        backend_payment_admin_router_with_postgres_pool(pool.clone()),
        request,
    )
    .await;
    assert_eq!(StatusCode::OK, status, "list failed: {listing}");
    assert!(
        listing.contains("brand-pad") && listing.contains("wechat_app"),
        "the list must carry the new rule: {listing}"
    );

    // Codes were normalized, so resolution with a differently-cased request still
    // finds it.
    let mut request = Request::builder()
        .method("GET")
        .uri("/backend/v3/api/payments/checkout_scene_policy/resolve?scene=checkout&app_architecture=pad&os_family=ipados&payment_method=WECHAT")
        .body(Body::empty())
        .expect("request");
    request.extensions_mut().insert(backend_member_context());
    let (status, resolved) = send(
        backend_payment_admin_router_with_postgres_pool(pool.clone()),
        request,
    )
    .await;
    assert_eq!(StatusCode::OK, status, "resolve failed: {resolved}");
    assert!(
        resolved.contains("wechat_app") && resolved.contains("\"configured\":true"),
        "the new rule must resolve: {resolved}"
    );

    // Saving is a privileged configuration change and must be attributed.
    let audited: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM commerce_payment_admin_audit_log \
         WHERE action = 'payment_checkout_scene_policy.upsert' AND resource_id = 'brand-pad'",
    )
    .fetch_one(&pool)
    .await
    .expect("read audit trail");
    assert_eq!(1, audited, "the rule change must be audited");
}
