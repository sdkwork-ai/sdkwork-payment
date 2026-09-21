//! Regression cover for the backend refund read surface.
//!
//! `list_refunds` and `retrieve_refund` join `commerce_payment_attempt` and used
//! to read `provider_account_id` straight off it. That column lives on
//! `commerce_payment_channel`, not on the attempt — the attempt only carries
//! `channel_id`. The query therefore failed at parse time with
//! `column a.provider_account_id does not exist`, which the gateway masked into
//! an opaque `500 errors.result.50001` for
//! `GET /backend/v3/api/payments/refunds` and
//! `GET /backend/v3/api/payments/refunds/{refundId}`.
//!
//! These tests drive the **real router** against a real PostgreSQL in an
//! isolated schema, so reading the column off the wrong table turns them red.
//!
//! Requires `SDKWORK_DATABASE_TEST_POSTGRES_URL`; skips cleanly when unset.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Extension;
use http_body_util::BodyExt;
use sdkwork_iam_context_service::{
    AuthLevel, DeploymentMode, Environment, IamAppContext, IamUserSurface,
};
use sdkwork_routes_payment_backend_api::backend_payment_refund_router_with_postgres_pool;
use sqlx::postgres::{PgPool, PgPoolOptions};
use tower::ServiceExt;

fn test_database_url() -> Option<String> {
    std::env::var("SDKWORK_DATABASE_TEST_POSTGRES_URL").ok()
}

async fn refund_pool(label: &str) -> Option<PgPool> {
    let base = test_database_url()?;
    // Tests in one binary share a PID, so the label must disambiguate them.
    let schema = format!("payment_refund_refunds_{}_{}", std::process::id(), label);
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

    // Minimal shape of the three tables the refund read path touches. The point
    // is column placement: provider_account_id lives on the channel.
    sqlx::raw_sql(sqlx::AssertSqlSafe(
        r#"
        CREATE TABLE commerce_payment_provider_account (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL
        );
        CREATE TABLE commerce_payment_channel (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            channel_no TEXT NOT NULL,
            provider_account_id TEXT,
            provider_code TEXT NOT NULL DEFAULT '',
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );
        CREATE TABLE commerce_payment_attempt (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            organization_id TEXT NOT NULL DEFAULT '0',
            payment_intent_id TEXT NOT NULL,
            order_id TEXT NOT NULL,
            channel_id TEXT,
            provider_code TEXT NOT NULL DEFAULT '',
            amount NUMERIC(18,2) NOT NULL DEFAULT 0,
            currency_code TEXT NOT NULL DEFAULT 'CNY',
            status TEXT NOT NULL DEFAULT 'pending',
            deleted_at TIMESTAMPTZ NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );
        CREATE TABLE commerce_refund (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            organization_id TEXT NOT NULL DEFAULT '0',
            refund_no TEXT NOT NULL,
            order_id TEXT NOT NULL,
            payment_attempt_id TEXT NOT NULL,
            amount NUMERIC(18,2) NOT NULL DEFAULT 0,
            currency_code TEXT NOT NULL DEFAULT 'CNY',
            status TEXT NOT NULL DEFAULT 'pending',
            refund_reason_code TEXT,
            requested_by_type TEXT,
            requested_by TEXT,
            deleted_at TIMESTAMPTZ NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );

        INSERT INTO commerce_payment_provider_account (id, tenant_id)
        VALUES ('acct-1', '100001');
        INSERT INTO commerce_payment_channel
            (id, tenant_id, channel_no, provider_account_id, provider_code)
        VALUES ('ch-1', '100001', 'CH1', 'acct-1', 'wechat');
        INSERT INTO commerce_payment_attempt
            (id, tenant_id, payment_intent_id, order_id, channel_id, provider_code, amount, status)
        VALUES ('pa-1', '100001', 'pi-1', 'o-1', 'ch-1', 'wechat', 100.00, 'succeeded');
        INSERT INTO commerce_refund
            (id, tenant_id, refund_no, order_id, payment_attempt_id, amount, status)
        VALUES ('r-1', '100001', 'RF1', 'o-1', 'pa-1', 100.00, 'succeeded');
        "#
        .to_string(),
    ))
    .execute(&pool)
    .await
    .expect("seed refund fixtures");

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
        vec!["payment.refunds.read".to_owned()],
    );
    context.user_surface = IamUserSurface {
        app: true,
        organization_member: true,
    };
    context
}

/// Production route `GET /backend/v3/api/payments/refunds`.
///
/// Before the fix this returned a masked 500 because the joined
/// `commerce_payment_attempt` has no `provider_account_id`.
#[tokio::test]
async fn list_refunds_returns_rows_from_the_production_route() {
    let Some(pool) = refund_pool("list").await else {
        eprintln!("skipping: SDKWORK_DATABASE_TEST_POSTGRES_URL is not set");
        return;
    };

    let router = backend_payment_refund_router_with_postgres_pool(pool)
        .layer(Extension(backend_member_context()));

    let response = router
        .oneshot(
            Request::builder()
                .uri("/backend/v3/api/payments/refunds")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router must answer");

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "list refunds must not fail on the provider_account_id join"
    );

    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        body.pointer("/data/items/0/refundNo"),
        Some(&serde_json::json!("RF1")),
        "the seeded refund must be listed"
    );
}

/// Production route `GET /backend/v3/api/payments/refunds/{refundId}`.
#[tokio::test]
async fn retrieve_refund_returns_the_row_from_the_production_route() {
    let Some(pool) = refund_pool("retrieve").await else {
        eprintln!("skipping: SDKWORK_DATABASE_TEST_POSTGRES_URL is not set");
        return;
    };

    let router = backend_payment_refund_router_with_postgres_pool(pool)
        .layer(Extension(backend_member_context()));

    let response = router
        .oneshot(
            Request::builder()
                .uri("/backend/v3/api/payments/refunds/r-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router must answer");

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "retrieve refund must not fail on the provider_account_id join"
    );

    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        body.pointer("/data/item/id"),
        Some(&serde_json::json!("r-1"))
    );
}
