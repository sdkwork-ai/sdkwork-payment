//! Regression cover for the backend payment provider-account list surface.
//!
//! `list_provider_accounts` assembles its SQL with an optional-filter fragment
//! whose placeholder numbering starts at `$5`, but it bound `LIMIT`/`OFFSET`
//! before the filters (`$3`/`$4`) and left `$3`/`$4` unreferenced. PostgreSQL
//! therefore required six parameters while the statement supplied four, so
//! every `GET /backend/v3/api/payments/provider_accounts` request failed with
//! the client-safe `500 errors.result.50001` problem in production.
//!
//! The test drives the **real router** against a real PostgreSQL in an isolated
//! schema (same harness as `backend_payment_refund_postgres.rs`), so the
//! bind-count mismatch turns it red.
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

async fn provider_account_pool(label: &str) -> Option<PgPool> {
    let base = test_database_url()?;
    let schema = format!("payment_provider_accounts_{}_{}", std::process::id(), label);
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

    // Minimal shape of the two tables the list query reads: the account row and
    // the credential-kind existence probes.
    sqlx::raw_sql(sqlx::AssertSqlSafe(
        r#"
        CREATE TABLE commerce_payment_provider_account (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            organization_id TEXT NOT NULL DEFAULT '0',
            account_no TEXT NOT NULL,
            provider_code TEXT NOT NULL,
            merchant_id TEXT NOT NULL DEFAULT '',
            account_name TEXT,
            account_name_i18n JSONB,
            account_mode TEXT NOT NULL DEFAULT 'direct',
            partner_provider_account_id TEXT,
            environment TEXT NOT NULL DEFAULT 'production',
            country_code TEXT NOT NULL DEFAULT 'CN',
            settlement_currency TEXT NOT NULL DEFAULT 'CNY',
            secret_ref TEXT,
            webhook_secret_ref TEXT,
            certificate_ref TEXT,
            capabilities JSONB,
            metadata JSONB,
            status TEXT NOT NULL DEFAULT 'active',
            last_tested_at TIMESTAMPTZ,
            last_test_status TEXT,
            certificate_expires_at TIMESTAMPTZ,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            deleted_at TIMESTAMPTZ
        );
        CREATE TABLE commerce_payment_provider_credential (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            provider_account_id TEXT NOT NULL,
            credential_kind TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'active',
            deleted_at TIMESTAMPTZ
        );

        INSERT INTO commerce_payment_provider_account
            (id, tenant_id, organization_id, account_no, provider_code, merchant_id, account_name, status)
        VALUES ('pa-1', '100001', '0', 'A1', 'wechat', 'M1', 'Main account', 'active');
        INSERT INTO commerce_payment_provider_credential
            (id, tenant_id, provider_account_id, credential_kind, status)
        VALUES ('cred-1', '100001', 'pa-1', 'primary_secret', 'active');
        "#
        .to_string(),
    ))
    .execute(&pool)
    .await
    .expect("seed provider account fixtures");

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

/// Production route `GET /backend/v3/api/payments/provider_accounts`.
#[tokio::test]
async fn list_provider_accounts_returns_rows_from_the_production_route() {
    let Some(pool) = provider_account_pool("list").await else {
        eprintln!("skipping: SDKWORK_DATABASE_TEST_POSTGRES_URL is not set");
        return;
    };

    let router = backend_payment_admin_router_with_postgres_pool(pool)
        .layer(Extension(backend_member_context()));

    let response = router
        .oneshot(
            Request::builder()
                .uri("/backend/v3/api/payments/provider_accounts")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router must answer");

    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(
        status,
        StatusCode::OK,
        "list provider accounts must not fail on its SQL bind count: {}",
        String::from_utf8_lossy(&bytes)
    );

    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        body.pointer("/data/items/0/accountNo"),
        Some(&serde_json::json!("A1")),
        "the seeded provider account must be listed"
    );
    assert_eq!(
        body.pointer("/data/items/0/hasPrimarySecret"),
        Some(&serde_json::json!(true)),
        "the primary credential must be reported"
    );
}

/// The optional filters take the first dynamic placeholders (`$3`..), so a
/// filtered request proves the numbering is right when the filter list is not
/// empty — the case the previous offset-by-two bug broke hardest.
#[tokio::test]
async fn list_provider_accounts_applies_optional_filters() {
    let Some(pool) = provider_account_pool("filtered").await else {
        eprintln!("skipping: SDKWORK_DATABASE_TEST_POSTGRES_URL is not set");
        return;
    };

    let router = backend_payment_admin_router_with_postgres_pool(pool)
        .layer(Extension(backend_member_context()));

    let response = router
        .oneshot(
            Request::builder()
                .uri(
                    "/backend/v3/api/payments/provider_accounts?provider_code=wechat&status=active",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router must answer");

    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(
        status,
        StatusCode::OK,
        "filtered provider account list must bind its filters and window: {}",
        String::from_utf8_lossy(&bytes)
    );

    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        body.pointer("/data/items/0/accountNo"),
        Some(&serde_json::json!("A1")),
        "the filtered provider account must be listed"
    );
    assert_eq!(
        body.pointer("/data/items/0/providerCode"),
        Some(&serde_json::json!("wechat"))
    );
}
