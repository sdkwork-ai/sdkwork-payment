//! Compensation worker claim queries (补偿轮询认领).
//!
//! The payment compensation worker scans attempts stuck in pending/processing
//! and refunds stuck in submitted/processing, queries the PSP, and re-enters
//! the notify processing framework with a synthetic event.
//!
//! Claim guarantees, stated honestly:
//! - The scan runs in a real transaction with `FOR UPDATE SKIP LOCKED`, so
//!   workers sweeping at the same moment receive disjoint batches and never
//!   both flip the same fresh row.
//! - A claimed `pending` attempt or `submitted` refund is flipped to
//!   `processing` inside the claiming transaction with a status guard, which
//!   keeps it out of the next sweep while the worker queries the PSP.
//! - Rows already in `processing` are re-claimed by later sweeps: their PSP
//!   poll repeats, which is safe because every downstream apply is a
//!   status-guarded, idempotent transition that fails closed on races. There
//!   is deliberately no fake cross-sweep row lock — a pool-level `SKIP LOCKED`
//!   outside a transaction releases at statement end and excludes nothing.
//! - There is no upper age bound: a row past any window is still claimed, so
//!   a refund cannot stay reserved forever just because a backlog delayed its
//!   first sweep. `min_age_seconds` still keeps just-created rows (whose PSP
//!   call may still be in flight) out of the scan.

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

/// A claimed refund awaiting PSP submission retry or status query.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ClaimedRefund {
    pub id: String,
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub order_id: String,
    pub provider_code: String,
    pub refund_no: String,
    pub payment_attempt_id: String,
    pub status: String,
    pub amount: String,
    pub currency_code: String,
    pub request_no: String,
    pub idempotency_key: String,
}

/// Tenants that currently hold rows due for compensation, so the worker can
/// run the per-tenant claim scans (which are index-servable) without a
/// cross-tenant seq scan. Bounded; repeat passes cover overflow.
pub async fn list_due_compensation_tenants_postgres(
    pool: &Pool<Postgres>,
    min_age_seconds: i64,
    limit: i64,
) -> Result<Vec<String>, CommerceServiceError> {
    let min_age = chrono::Utc::now().timestamp() - min_age_seconds;
    let rows = sqlx::query(
        r#"
        SELECT DISTINCT tenant_id FROM (
            SELECT pa.tenant_id
            FROM commerce_payment_attempt pa
            WHERE pa.status IN ('pending', 'processing')
              AND EXTRACT(EPOCH FROM pa.created_at) <= $1
              AND (pa.expires_at IS NULL OR EXTRACT(EPOCH FROM pa.expires_at) > $1)
              AND pa.deleted_at IS NULL
            UNION
            SELECT r.tenant_id
            FROM commerce_refund r
            WHERE r.status IN ('submitted', 'processing')
              AND EXTRACT(EPOCH FROM r.created_at) <= $1
              AND r.deleted_at IS NULL
        ) due
        ORDER BY tenant_id
        LIMIT $2
        "#,
    )
    .bind(min_age)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|error| store_error("failed to list compensation tenants", error))?;
    Ok(rows
        .iter()
        .filter_map(|row| optional_string_cell(row, "tenant_id"))
        .collect())
}

/// Claims due payment attempts: status pending/processing, created at least
/// `min_age_seconds` ago, not expired. Locked `FOR UPDATE SKIP LOCKED` inside
/// one transaction; freshly `pending` claims flip to `processing` before
/// commit so later sweeps skip them while the PSP is being queried.
pub async fn claim_due_payment_attempts_postgres(
    pool: &Pool<Postgres>,
    tenant_id: &str,
    organization_id: Option<&str>,
    limit: i64,
    now_seconds: i64,
    min_age_seconds: i64,
) -> Result<Vec<ClaimedPaymentAttempt>, CommerceServiceError> {
    let min_age = now_seconds - min_age_seconds;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| store_error("failed to begin payment attempt claim", error))?;
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
          AND pa.deleted_at IS NULL
        ORDER BY pa.created_at ASC, pa.id ASC
        LIMIT $5
        FOR UPDATE SKIP LOCKED
        "#,
    )
    .bind(tenant_id)
    .bind(organization_id)
    .bind(now_seconds)
    .bind(min_age)
    .bind(limit)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| store_error("failed to claim due payment attempts", error))?;
    let claimed = rows
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
        .collect::<Vec<_>>();
    let fresh_ids: Vec<String> = claimed.iter().map(|claim| claim.id.clone()).collect();
    if !fresh_ids.is_empty() {
        sqlx::query(
            r#"
            UPDATE commerce_payment_attempt
            SET status = 'processing', updated_at = NOW()
            WHERE id = ANY($1)
              AND LOWER(COALESCE(status, '')) = 'pending'
            "#,
        )
        .bind(&fresh_ids)
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error("failed to mark claimed payment attempts", error))?;
    }
    tx.commit()
        .await
        .map_err(|error| store_error("failed to commit payment attempt claim", error))?;
    Ok(claimed)
}

/// Claims due refunds: status submitted/processing, created at least
/// `min_age_seconds` ago. Locked `FOR UPDATE SKIP LOCKED` inside one
/// transaction; `submitted` claims flip to `processing` before commit so
/// later sweeps skip them while the PSP submission is retried.
pub async fn claim_due_refunds_postgres(
    pool: &Pool<Postgres>,
    tenant_id: &str,
    organization_id: Option<&str>,
    limit: i64,
    now_seconds: i64,
    min_age_seconds: i64,
) -> Result<Vec<ClaimedRefund>, CommerceServiceError> {
    let min_age = now_seconds - min_age_seconds;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| store_error("failed to begin refund claim", error))?;
    let rows = sqlx::query(
        r#"
        SELECT id, tenant_id, organization_id, order_id, provider_code, refund_no,
               payment_attempt_id, status,
               CAST(COALESCE(amount, 0) AS BIGINT)::TEXT AS amount,
               currency_code, request_no, idempotency_key
        FROM commerce_refund
        WHERE tenant_id = CAST($1 AS TEXT)
          AND ((organization_id = CAST($2 AS TEXT)) OR (organization_id IS NULL AND $2 IS NULL) OR (organization_id = '0' AND $2 IS NULL))
          AND status IN ('submitted', 'processing')
          AND EXTRACT(EPOCH FROM created_at) <= $3
          AND deleted_at IS NULL
        ORDER BY created_at ASC, id ASC
        LIMIT $4
        FOR UPDATE SKIP LOCKED
        "#,
    )
    .bind(tenant_id)
    .bind(organization_id)
    .bind(min_age)
    .bind(limit)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| store_error("failed to claim due refunds", error))?;
    let claimed = rows
        .iter()
        .map(|row| ClaimedRefund {
            id: string_cell(row, "id"),
            tenant_id: string_cell(row, "tenant_id"),
            organization_id: optional_string_cell(row, "organization_id"),
            order_id: string_cell(row, "order_id"),
            provider_code: string_cell(row, "provider_code"),
            refund_no: string_cell(row, "refund_no"),
            payment_attempt_id: string_cell(row, "payment_attempt_id"),
            status: string_cell(row, "status"),
            amount: string_cell(row, "amount"),
            currency_code: string_cell(row, "currency_code"),
            request_no: string_cell(row, "request_no"),
            idempotency_key: string_cell(row, "idempotency_key"),
        })
        .collect::<Vec<_>>();
    let submitted_ids: Vec<String> = claimed
        .iter()
        .filter(|claim| claim.status.eq_ignore_ascii_case("submitted"))
        .map(|claim| claim.id.clone())
        .collect();
    if !submitted_ids.is_empty() {
        sqlx::query(
            r#"
            UPDATE commerce_refund
            SET status = 'processing', updated_at = NOW()
            WHERE id = ANY($1)
              AND LOWER(COALESCE(status, '')) = 'submitted'
            "#,
        )
        .bind(&submitted_ids)
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error("failed to mark claimed refunds", error))?;
    }
    tx.commit()
        .await
        .map_err(|error| store_error("failed to commit refund claim", error))?;
    Ok(claimed)
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
