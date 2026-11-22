//! Privileged payment-admin writes must leave an audit record.
//!
//! `INTEGRATION_SPEC` requires audit events for credential create/rotate/revoke.
//! The record must share the mutation's transaction (so a committed change can
//! never be missing from the trail) and must never carry secret material.
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

async fn audit_pool(label: &str) -> Option<PgPool> {
    let base = test_database_url()?;
    let schema = format!("payment_admin_audit_{}_{}", std::process::id(), label);
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
async fn creating_an_account_with_credentials_records_who_changed_what() {
    let Some(pool) = audit_pool("create").await else {
        return;
    };

    // The host installs the credential cipher at boot; a test process must do the
    // same or every credential write answers 500 ("cipher is not initialized").
    let _ = sdkwork_payment_providers::install_payment_credential_cipher(std::sync::Arc::new(
        sdkwork_payment_providers::LocalFilePaymentCredentialCipher::from_key_material(
            b"payment-audit-test-master-key-material".to_vec(),
        )
        .expect("credential cipher"),
    ));    const SECRET: &str = "sk-audit-secret-do-not-record";
    let router = backend_payment_admin_router_with_postgres_pool(pool.clone());
    let body = serde_json::json!({
        "accountNo": "acct-audit-1",
        "providerCode": "stripe",
        "merchantId": "acct_merchant",
        "accountName": "Audited account",
        "environment": "development",
        "countryCode": "US",
        "settlementCurrency": "USD",
        "accountMode": "direct",
        "primarySecret": SECRET,
    });
    let mut request = Request::builder()
        .method("POST")
        .uri("/backend/v3/api/payments/provider_accounts")
        .header("content-type", "application/json")
        .header("Idempotency-Key", "idem-audit-1")
        .header("X-Request-Id", "req-audit-1")
        .body(Body::from(body.to_string()))
        .expect("request");
    request.extensions_mut().insert(backend_member_context());

    let response = router.oneshot(request).await.expect("route");
    let status = response.status();
    let payload = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    assert_eq!(
        StatusCode::CREATED,
        status,
        "create failed: {}",
        String::from_utf8_lossy(&payload)
    );

    let rows = sqlx::query(
        "SELECT action, actor_user_id, resource_type, resource_id, detail::text AS detail \
         FROM commerce_payment_admin_audit_log WHERE tenant_id = '100001' ORDER BY action",
    )
    .fetch_all(&pool)
    .await
    .expect("read audit trail");
    let recorded: Vec<(String, String, String, String, String)> = rows
        .iter()
        .map(|row| {
            use sqlx::Row;
            (
                row.get::<String, _>("action"),
                row.get::<String, _>("actor_user_id"),
                row.get::<String, _>("resource_type"),
                row.get::<String, _>("resource_id"),
                row.get::<String, _>("detail"),
            )
        })
        .collect();

    assert_eq!(
        vec![
            "payment_provider_credential.rotate".to_owned(),
            "provider_account.create".to_owned()
        ],
        recorded
            .iter()
            .map(|(action, ..)| action.clone())
            .collect::<Vec<_>>(),
        "account creation and credential rotation must both be audited"
    );
    for (_, actor, resource_type, resource_id, detail) in &recorded {
        assert_eq!("1", actor, "the acting user must be recorded");
        assert_eq!("provider_account", resource_type);
        assert!(!resource_id.is_empty(), "the audited resource must be named");
        assert!(
            !detail.contains(SECRET),
            "the audit trail must never carry secret material"
        );
    }
    assert!(
        recorded
            .iter()
            .any(|(action, _, _, _, detail)| action == "payment_provider_credential.rotate"
                && detail.contains("primary_secret")),
        "the rotation record must name the changed credential kind"
    );
}
