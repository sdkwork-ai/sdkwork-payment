//! Regression cover for the remaining filtered payment admin lists.
//!
//! The provider-account list fix (`backend_payment_provider_accounts_postgres.rs`)
//! corrected the shared placeholder numbering used by every filtered list, but
//! each of these endpoints owns its own SQL and table, so each one needs to be
//! driven against a real PostgreSQL:
//!
//! * `route_rules`, `attempts` and `reconciliation_runs` failed on the same
//!   `$5 LIMIT / $6 OFFSET` versus four-bind mismatch.
//! * `webhook_events` also selected `deleted_at`, a column the
//!   `commerce_payment_webhook_event` table never had, so it kept failing with
//!   `column "deleted_at" does not exist` after the bind fix.
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

async fn list_pool(label: &str) -> Option<PgPool> {
    let base = test_database_url()?;
    let schema = format!("payment_filtered_list_{}_{}", std::process::id(), label);
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
        vec!["commerce.payments.provider_accounts.read".to_owned()],
    );
    context.user_surface = IamUserSurface {
        app: true,
        organization_member: true,
    };
    context
}

async fn get_list(pool: PgPool, uri: &str) -> (StatusCode, serde_json::Value) {
    let router = backend_payment_admin_router_with_postgres_pool(pool)
        .layer(Extension(backend_member_context()));
    let response = router
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .expect("router must answer");
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, body)
}

#[tokio::test]
async fn list_route_rules_returns_rows_from_the_production_route() {
    let Some(pool) = list_pool("route_rules").await else {
        eprintln!("skipping: SDKWORK_DATABASE_TEST_POSTGRES_URL is not set");
        return;
    };
    sqlx::raw_sql(sqlx::AssertSqlSafe(
        r#"
        CREATE TABLE commerce_payment_route_rule (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            organization_id TEXT NOT NULL DEFAULT '0',
            rule_no TEXT NOT NULL,
            priority INTEGER NOT NULL DEFAULT 0,
            purchase_type TEXT,
            country_code TEXT,
            currency_code TEXT,
            client_platform TEXT,
            amount_min NUMERIC,
            amount_max NUMERIC,
            user_segment TEXT,
            risk_level TEXT,
            channel_id TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'active',
            starts_at TIMESTAMPTZ,
            ends_at TIMESTAMPTZ,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            deleted_at TIMESTAMPTZ
        );
        INSERT INTO commerce_payment_route_rule
            (id, tenant_id, organization_id, rule_no, priority, channel_id, status)
        VALUES ('rr-1', '100001', '0', 'RR1', 10, 'ch-1', 'active');
        "#
        .to_string(),
    ))
    .execute(&pool)
    .await
    .expect("seed route rule fixture");

    let (status, body) = get_list(pool, "/backend/v3/api/payments/route_rules").await;
    assert_eq!(
        status,
        StatusCode::OK,
        "route rules list must not fail on its SQL: {body}"
    );
    assert_eq!(
        body.pointer("/data/items/0/ruleNo"),
        Some(&serde_json::json!("RR1"))
    );
}

#[tokio::test]
async fn list_attempts_returns_rows_from_the_production_route() {
    let Some(pool) = list_pool("attempts").await else {
        eprintln!("skipping: SDKWORK_DATABASE_TEST_POSTGRES_URL is not set");
        return;
    };
    sqlx::raw_sql(sqlx::AssertSqlSafe(
        r#"
        CREATE TABLE commerce_payment_attempt (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            organization_id TEXT NOT NULL DEFAULT '0',
            payment_intent_id TEXT NOT NULL,
            attempt_no TEXT,
            provider_code TEXT NOT NULL DEFAULT '',
            channel_id TEXT,
            amount NUMERIC NOT NULL DEFAULT 0,
            currency_code TEXT NOT NULL DEFAULT 'CNY',
            status TEXT NOT NULL DEFAULT 'pending',
            provider_transaction_id TEXT,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            deleted_at TIMESTAMPTZ
        );
        INSERT INTO commerce_payment_attempt
            (id, tenant_id, organization_id, payment_intent_id, attempt_no, provider_code,
             channel_id, amount, status, provider_transaction_id)
        VALUES ('pa-1', '100001', '0', 'pi-1', '1', 'wechat', 'ch-1', 100.00, 'succeeded', 'tx-1');
        "#
        .to_string(),
    ))
    .execute(&pool)
    .await
    .expect("seed attempt fixture");

    let (status, body) = get_list(pool, "/backend/v3/api/payments/attempts").await;
    assert_eq!(
        status,
        StatusCode::OK,
        "attempts list must not fail on its SQL: {body}"
    );
    assert_eq!(
        body.pointer("/data/items/0/id"),
        Some(&serde_json::json!("pa-1"))
    );
}

#[tokio::test]
async fn list_webhook_events_returns_rows_from_the_production_route() {
    let Some(pool) = list_pool("webhook_events").await else {
        eprintln!("skipping: SDKWORK_DATABASE_TEST_POSTGRES_URL is not set");
        return;
    };
    // Faithful to `0001_payment_baseline.sql`: this table has no `deleted_at`.
    sqlx::raw_sql(sqlx::AssertSqlSafe(
        r#"
        CREATE TABLE commerce_payment_webhook_event (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            organization_id TEXT NOT NULL DEFAULT '0',
            event_id TEXT NOT NULL,
            event_type TEXT NOT NULL,
            provider_code TEXT,
            payload JSONB NOT NULL DEFAULT '{}'::jsonb,
            status TEXT NOT NULL DEFAULT 'queued',
            retries INTEGER NOT NULL DEFAULT 0,
            last_error TEXT,
            received_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            processed_at TIMESTAMPTZ,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );
        INSERT INTO commerce_payment_webhook_event
            (id, tenant_id, organization_id, event_id, event_type, provider_code, status)
        VALUES ('we-1', '100001', '0', 'evt-1', 'payment.succeeded', 'wechat', 'processed');
        "#
        .to_string(),
    ))
    .execute(&pool)
    .await
    .expect("seed webhook event fixture");

    let (status, body) = get_list(pool, "/backend/v3/api/payments/webhook_events").await;
    assert_eq!(
        status,
        StatusCode::OK,
        "webhook events list must not select a deleted_at column the table never had: {body}"
    );
    assert_eq!(
        body.pointer("/data/items/0/eventId"),
        Some(&serde_json::json!("evt-1"))
    );
}

#[tokio::test]
async fn list_reconciliation_runs_returns_rows_from_the_production_route() {
    let Some(pool) = list_pool("reconciliation_runs").await else {
        eprintln!("skipping: SDKWORK_DATABASE_TEST_POSTGRES_URL is not set");
        return;
    };
    sqlx::raw_sql(sqlx::AssertSqlSafe(
        r#"
        CREATE TABLE commerce_payment_reconciliation_run (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            organization_id TEXT NOT NULL DEFAULT '0',
            run_no TEXT NOT NULL,
            provider_code TEXT,
            provider_account_id TEXT,
            reconciliation_type TEXT NOT NULL DEFAULT 'daily',
            period_start TIMESTAMPTZ NOT NULL,
            period_end TIMESTAMPTZ NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending',
            matched_count INTEGER NOT NULL DEFAULT 0,
            mismatched_count INTEGER NOT NULL DEFAULT 0,
            currency_code TEXT NOT NULL DEFAULT 'CNY',
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            deleted_at TIMESTAMPTZ
        );
        INSERT INTO commerce_payment_reconciliation_run
            (id, tenant_id, organization_id, run_no, provider_code, provider_account_id,
             reconciliation_type, period_start, period_end, status, matched_count, mismatched_count)
        VALUES ('run-1', '100001', '0', 'RUN1', 'wechat', 'acc-1', 'daily', NOW(), NOW(),
                'succeeded', 3, 1);
        "#
        .to_string(),
    ))
    .execute(&pool)
    .await
    .expect("seed reconciliation run fixture");

    let (status, body) = get_list(pool, "/backend/v3/api/payments/reconciliation_runs").await;
    assert_eq!(
        status,
        StatusCode::OK,
        "reconciliation runs list must not fail on its SQL: {body}"
    );
    assert_eq!(
        body.pointer("/data/items/0/runNo"),
        Some(&serde_json::json!("RUN1"))
    );
}
