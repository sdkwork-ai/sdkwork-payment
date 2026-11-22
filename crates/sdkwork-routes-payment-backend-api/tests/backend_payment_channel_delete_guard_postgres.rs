//! A payment channel with money in flight must not be deletable.
//!
//! Deleting it strands pending attempts (the payer's checkout can no longer
//! resolve the channel's provider) and breaks reconciliation and refunds
//! afterwards. The admin delete must answer 409 while live attempts exist, and
//! succeed once they are terminal.
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

async fn channel_pool(label: &str) -> Option<PgPool> {
    let base = test_database_url()?;
    let schema = format!("payment_channel_delete_{}_{}", std::process::id(), label);
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
    sqlx::raw_sql(sqlx::AssertSqlSafe(
        r#"
        INSERT INTO commerce_payment_provider_account
            (id, tenant_id, organization_id, account_no, provider_code, merchant_id, secret_ref, status)
        VALUES ('pa-1', '100001', '0', 'A1', 'wechat_pay', 'M1', '', 'active');

        INSERT INTO commerce_payment_channel
            (id, tenant_id, organization_id, channel_no, provider_account_id, provider_code, status)
        VALUES ('ch-1', '100001', '0', 'CH1', 'pa-1', 'wechat_pay', 'active');

        INSERT INTO commerce_payment_attempt
            (id, tenant_id, owner_user_id, payment_intent_id, order_id, payment_method,
             provider_code, channel_id, idempotency_key, status)
        VALUES ('attempt-live', '100001', 'user-1', 'intent-1', 'order-1', 'wechat_jsapi',
                'wechat_pay', 'ch-1', 'idem-live', 'pending');
        "#
        .to_owned(),
    ))
    .execute(&pool)
    .await
    .expect("seed channel and live attempt");
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
            "commerce.payments.channels.write".to_owned(),
            "commerce.payments.channels.read".to_owned(),
        ],
    );
    context.user_surface = IamUserSurface {
        app: true,
        organization_member: true,
    };
    context
}

async fn delete_channel(
    router: axum::Router,
    headers: &[(&str, &str)],
) -> (StatusCode, String) {
    let mut builder = Request::builder()
        .method("DELETE")
        .uri("/backend/v3/api/payments/channels/ch-1");
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let mut request = builder.body(Body::empty()).expect("request");
    request.extensions_mut().insert(backend_member_context());
    let response = router.oneshot(request).await.expect("route");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    (status, String::from_utf8_lossy(&body).to_string())
}

#[tokio::test]
async fn a_channel_with_in_flight_payments_is_not_deletable() {
    let Some(pool) = channel_pool("inflight").await else {
        return;
    };
    let write_headers = [
        ("Idempotency-Key", "idem-channel-delete"),
        ("X-Request-Id", "req-channel-delete"),
    ];

    let (status, body) = delete_channel(
        backend_payment_admin_router_with_postgres_pool(pool.clone()),
        &write_headers,
    )
    .await;
    assert_eq!(
        StatusCode::CONFLICT,
        status,
        "a channel with a pending attempt must answer 409, got {status}: {body}"
    );
    assert!(
        body.contains("in-flight"),
        "the conflict must explain the in-flight payments: {body}"
    );

    // Once the attempt is terminal the channel is disposable again.
    sqlx::query(sqlx::AssertSqlSafe(
        "UPDATE commerce_payment_attempt SET status = 'succeeded' WHERE id = 'attempt-live'"
            .to_owned(),
    ))
    .execute(&pool)
    .await
    .expect("settle attempt");

    let (status, body) = delete_channel(
        backend_payment_admin_router_with_postgres_pool(pool.clone()),
        &write_headers,
    )
    .await;
    assert!(
        status.is_success(),
        "a settled attempt must not block deletion, got {status}: {body}"
    );
    let deleted: Option<String> = sqlx::query_scalar(
        "SELECT CAST(deleted_at AS TEXT) FROM commerce_payment_channel WHERE id = 'ch-1'",
    )
    .fetch_one(&pool)
    .await
    .expect("read channel");
    assert!(deleted.is_some(), "the channel must be soft deleted");

    // Deleting a channel is a privileged change and must be attributed.
    let audited: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM commerce_payment_admin_audit_log \
         WHERE action = 'payment_channel.delete' AND resource_id = 'ch-1'",
    )
    .fetch_one(&pool)
    .await
    .expect("read audit trail");
    assert_eq!(1, audited, "the deletion must be recorded in the audit trail");
}
