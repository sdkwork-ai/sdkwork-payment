//! A provider account and the credentials that make it usable must be one unit.
//!
//! If the credential write could fail on its own, the account would already be
//! committed with secret refs and an active status but nothing to sign with, and
//! every checkout routed through it would fail until an operator noticed. The test
//! installs a trigger that rejects the credential insert, drives the real
//! `POST /backend/v3/api/payments/provider_accounts` route, and then proves no
//! account row survived the rollback.
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

async fn atomicity_pool(label: &str) -> Option<PgPool> {
    let base = test_database_url()?;
    let schema = format!("payment_account_atomicity_{}_{}", std::process::id(), label);
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
        vec!["commerce.payments.provider_accounts.write".to_owned()],
    );
    context.user_surface = IamUserSurface {
        app: true,
        organization_member: true,
    };
    context
}

#[tokio::test]
async fn a_credential_failure_rolls_back_the_provider_account() {
    let Some(pool) = atomicity_pool("rollback").await else {
        return;
    };

    // Simulate a mid-transaction failure on the credential side; the account insert
    // runs before it, so only a shared transaction keeps the account out of the
    // table.
    sqlx::raw_sql(sqlx::AssertSqlSafe(
        r#"
        CREATE OR REPLACE FUNCTION reject_provider_credential() RETURNS trigger AS $$
        BEGIN
            RAISE EXCEPTION 'simulated credential write failure';
        END;
        $$ LANGUAGE plpgsql;
        CREATE TRIGGER reject_provider_credential
            BEFORE INSERT ON commerce_payment_provider_credential
            FOR EACH ROW EXECUTE FUNCTION reject_provider_credential();
        "#
        .to_owned(),
    ))
    .execute(&pool)
    .await
    .expect("install failure trigger");

    let router = backend_payment_admin_router_with_postgres_pool(pool.clone());
    let body = serde_json::json!({
        "accountNo": "acct-atomicity-1",
        "providerCode": "wechat_pay",
        "environment": "development",
        "accountMode": "direct",
        "credentialWrite": { "primarySecret": "sk-atomicity-secret-value" }
    });
    let mut request = Request::builder()
        .method("POST")
        .uri("/backend/v3/api/payments/provider_accounts")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .expect("request");
    request.extensions_mut().insert(backend_member_context());

    let response = router.oneshot(request).await.expect("route");
    assert!(
        response.status().is_server_error(),
        "a rejected credential write must fail the request, got {}",
        response.status()
    );
    let _ = axum::body::to_bytes(response.into_body(), usize::MAX).await;

    let survivors: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM commerce_payment_provider_account WHERE account_no = 'acct-atomicity-1'",
    )
    .fetch_one(&pool)
    .await
    .expect("count accounts");
    assert_eq!(
        0, survivors,
        "the account must not survive a credential failure"
    );
}
