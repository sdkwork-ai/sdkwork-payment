//! Compensation worker claim queries (补偿轮询认领).
//!
//! The payment compensation worker scans payments stuck in pending/processing
//! and refunds stuck in submitted/processing, queries the PSP, and re-enters
//! the notify processing framework with a synthetic event. Claims use
//! `FOR UPDATE SKIP LOCKED` so multiple worker instances never process the
//! same row; the scan window bounds PSP query load.

use sdkwork_contract_service::CommerceServiceError;
use serde_json::Value;
use sqlx::{Pool, Postgres, Row};

use crate::shared::store_error;

/// A claimed payment attempt awaiting PSP status query.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ClaimedPaymentAttempt {
    pub id: String,
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub owner_user_id: String,
    pub order_id: String,
    pub payment_intent_id: String,
    pub provider_code: String,
    pub out_trade_no: String,
    pub channel_id: Option<String>,
    pub provider_transaction_id: Option<String>,
    pub provider_account_id: Option<String>,
    pub amount: String,
}

/// A claimed refund awaiting PSP status query.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ClaimedRefund {
    pub id: String,
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub refund_no: String,
    pub payment_attempt_id: String,
    pub status: String,
}

/// Claims due payment attempts: status pending/processing, created between
/// `min_age_seconds` and `max_age_seconds` ago, not expired. Rows are locked
/// for the claiming transaction and skipped when another worker holds them.
pub async fn claim_due_payment_attempts_postgres(
    pool: &Pool<Postgres>,
    tenant_id: &str,
    organization_id: Option<&str>,
    limit: i64,
    now_seconds: i64,
    min_age_seconds: i64,
    max_age_seconds: i64,
) -> Result<Vec<ClaimedPaymentAttempt>, CommerceServiceError> {
    let min_age = now_seconds - min_age_seconds;
    let max_age = now_seconds - max_age_seconds;
    let rows = sqlx::query(
        r#"
        SELECT pa.id, pa.tenant_id, pa.organization_id, pa.owner_user_id,
               pa.order_id, pa.payment_intent_id, pa.provider_code, pa.out_trade_no,
               pa.channel_id, pa.provider_transaction_id,
               COALESCE(NULLIF(pa.callback_payload->>'providerAccountId', ''), NULL) AS provider_account_id,
               CAST(COALESCE(pa.amount, 0) AS BIGINT)::TEXT AS amount
        FROM commerce_payment_attempt pa
        WHERE pa.tenant_id = CAST($1 AS TEXT)
          AND ((pa.organization_id = CAST($2 AS TEXT)) OR (pa.organization_id IS NULL AND $2 IS NULL) OR (pa.organization_id = '0' AND $2 IS NULL))
          AND pa.status IN ('pending', 'processing')
          AND (pa.expires_at IS NULL OR EXTRACT(EPOCH FROM pa.expires_at) > $3)
          AND EXTRACT(EPOCH FROM pa.created_at) <= $4
          AND EXTRACT(EPOCH FROM pa.created_at) >= $5
          AND pa.deleted_at IS NULL
        ORDER BY pa.created_at ASC, pa.id ASC
        LIMIT $6
        FOR UPDATE SKIP LOCKED
        "#,
    )
    .bind(tenant_id)
    .bind(organization_id)
    .bind(now_seconds)
    .bind(min_age)
    .bind(max_age)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|error| store_error("failed to claim due payment attempts", error))?;
    Ok(rows
        .iter()
        .map(|row| ClaimedPaymentAttempt {
            id: string_cell(row, "id"),
            tenant_id: string_cell(row, "tenant_id"),
            organization_id: optional_string_cell(row, "organization_id"),
            owner_user_id: string_cell(row, "owner_user_id"),
            order_id: string_cell(row, "order_id"),
            payment_intent_id: string_cell(row, "payment_intent_id"),
            provider_code: string_cell(row, "provider_code"),
            out_trade_no: string_cell(row, "out_trade_no"),
            channel_id: optional_string_cell(row, "channel_id"),
            provider_transaction_id: optional_string_cell(row, "provider_transaction_id"),
            provider_account_id: optional_string_cell(row, "provider_account_id"),
            amount: string_cell(row, "amount"),
        })
        .collect())
}

/// Claims due refunds: status submitted/processing within the age window.
/// Refunds have no expires_at column, so the window is created_at-bounded.
pub async fn claim_due_refunds_postgres(
    pool: &Pool<Postgres>,
    tenant_id: &str,
    organization_id: Option<&str>,
    limit: i64,
    now_seconds: i64,
    min_age_seconds: i64,
    max_age_seconds: i64,
) -> Result<Vec<ClaimedRefund>, CommerceServiceError> {
    let min_age = now_seconds - min_age_seconds;
    let max_age = now_seconds - max_age_seconds;
    let rows = sqlx::query(
        r#"
        SELECT id, tenant_id, organization_id, refund_no, payment_attempt_id, status
        FROM commerce_refund
        WHERE tenant_id = CAST($1 AS TEXT)
          AND ((organization_id = CAST($2 AS TEXT)) OR (organization_id IS NULL AND $2 IS NULL) OR (organization_id = '0' AND $2 IS NULL))
          AND status IN ('submitted', 'processing')
          AND EXTRACT(EPOCH FROM created_at) <= $3
          AND EXTRACT(EPOCH FROM created_at) >= $4
          AND deleted_at IS NULL
        ORDER BY created_at ASC, id ASC
        LIMIT $5
        FOR UPDATE SKIP LOCKED
        "#,
    )
    .bind(tenant_id)
    .bind(organization_id)
    .bind(min_age)
    .bind(max_age)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|error| store_error("failed to claim due refunds", error))?;
    Ok(rows
        .iter()
        .map(|row| ClaimedRefund {
            id: string_cell(row, "id"),
            tenant_id: string_cell(row, "tenant_id"),
            organization_id: optional_string_cell(row, "organization_id"),
            refund_no: string_cell(row, "refund_no"),
            payment_attempt_id: string_cell(row, "payment_attempt_id"),
            status: string_cell(row, "status"),
        })
        .collect())
}

/// Loads the provider context for a claimed payment attempt (channel,
/// account id, provider code, out-trade-no, native transaction id).
pub async fn load_claim_attempt_provider_context_postgres(
    pool: &Pool<Postgres>,
    attempt_id: &str,
) -> Result<Option<ClaimAttemptProviderContext>, CommerceServiceError> {
    let row = sqlx::query(
        r#"
        SELECT pa.id, pa.tenant_id, pa.organization_id, pa.provider_code, pa.out_trade_no,
               pa.channel_id, pa.provider_transaction_id,
               COALESCE(NULLIF(pa.callback_payload->>'providerAccountId', ''), NULL) AS provider_account_id,
               CAST(COALESCE(pa.amount, 0) AS BIGINT)::TEXT AS amount,
               pa.callback_payload
        FROM commerce_payment_attempt pa
        WHERE pa.id = CAST($1 AS TEXT)
          AND pa.deleted_at IS NULL
        LIMIT 1
        "#,
    )
    .bind(attempt_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| store_error("failed to load claimed attempt provider context", error))?;
    let Some(row) = row else {
        return Ok(None);
    };
    let payload: Value = row
        .try_get("callback_payload")
        .unwrap_or_else(|_| Value::Null);
    let provider_transaction_id =
        optional_string_cell(&row, "provider_transaction_id").or_else(|| {
            payload
                .get("providerTransactionId")
                .and_then(Value::as_str)
                .map(str::to_owned)
        });
    Ok(Some(ClaimAttemptProviderContext {
        attempt_id: string_cell(&row, "id"),
        tenant_id: string_cell(&row, "tenant_id"),
        organization_id: optional_string_cell(&row, "organization_id"),
        provider_code: string_cell(&row, "provider_code"),
        out_trade_no: string_cell(&row, "out_trade_no"),
        channel_id: optional_string_cell(&row, "channel_id"),
        provider_transaction_id,
        provider_account_id: optional_string_cell(&row, "provider_account_id"),
        amount: string_cell(&row, "amount"),
    }))
}

/// Provider context of a claimed payment attempt, sufficient to resolve the
/// provider account and build the query.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ClaimAttemptProviderContext {
    pub attempt_id: String,
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub provider_code: String,
    pub out_trade_no: String,
    pub channel_id: Option<String>,
    pub provider_transaction_id: Option<String>,
    pub provider_account_id: Option<String>,
    pub amount: String,
}

fn optional_string_cell(row: &sqlx::postgres::PgRow, column: &str) -> Option<String> {
    row.try_get::<Option<String>, _>(column).ok().flatten()
}

fn string_cell(row: &sqlx::postgres::PgRow, column: &str) -> String {
    optional_string_cell(row, column).unwrap_or_default()
}
