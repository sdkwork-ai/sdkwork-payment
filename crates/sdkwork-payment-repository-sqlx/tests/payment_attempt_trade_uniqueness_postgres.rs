//! The merchant trade number a payment provider echoes back must identify exactly
//! one attempt: a duplicate lets one channel callback settle or refund the wrong
//! attempt, which is unrecoverable once money has moved.
//!
//! Executes the shipped baseline DDL in an isolated schema, so the test verifies
//! the *deployed* constraint rather than a local copy of it.
//!
//! Requires `SDKWORK_DATABASE_TEST_POSTGRES_URL`; skips cleanly when unset.

use sqlx::postgres::{PgPool, PgPoolOptions};

fn test_database_url() -> Option<String> {
    std::env::var("SDKWORK_DATABASE_TEST_POSTGRES_URL").ok()
}

async fn attempt_pool(label: &str) -> Option<PgPool> {
    let base = test_database_url()?;
    let schema = format!("payment_attempt_trade_{}_{}", std::process::id(), label);
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

async fn insert_attempt(
    pool: &PgPool,
    id: &str,
    provider_code: &str,
    out_trade_no: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(sqlx::AssertSqlSafe(
        "INSERT INTO commerce_payment_attempt \
         (id, tenant_id, owner_user_id, payment_intent_id, order_id, payment_method, \
          provider_code, out_trade_no, idempotency_key) \
         VALUES ($1, '100001', 'user-1', 'intent-1', 'order-1', 'wechat_jsapi', $2, $3, $4)"
            .to_owned(),
    ))
    .bind(id)
    .bind(provider_code)
    .bind(out_trade_no)
    .bind(format!("idem-{id}"))
    .execute(pool)
    .await
    .map(|_| ())
}

#[tokio::test]
async fn a_tenant_provider_trade_number_identifies_exactly_one_attempt() {
    let Some(pool) = attempt_pool("unique").await else {
        return;
    };

    insert_attempt(&pool, "attempt-1", "wechat_pay", "OT-1")
        .await
        .expect("first attempt");

    // Same provider, same trade number, another attempt: the provider could not
    // tell them apart, so its callback would settle the wrong row.
    assert!(
        insert_attempt(&pool, "attempt-2", "wechat_pay", "OT-1")
            .await
            .is_err(),
        "a duplicate (tenant, provider, out_trade_no) must be rejected"
    );

    // A different provider may legitimately reuse the merchant trade number.
    insert_attempt(&pool, "attempt-3", "alipay", "OT-1")
        .await
        .expect("another provider may reuse the trade number");

    // Soft-deleted attempts release the trade number for reuse.
    sqlx::query(sqlx::AssertSqlSafe(
        "UPDATE commerce_payment_attempt SET deleted_at = NOW() WHERE id = 'attempt-1'".to_owned(),
    ))
    .execute(&pool)
    .await
    .expect("soft delete");
    insert_attempt(&pool, "attempt-4", "wechat_pay", "OT-1")
        .await
        .expect("a soft-deleted attempt must not hold the trade number");
}
