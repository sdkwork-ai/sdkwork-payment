use crate::shared::current_timestamp_string;
use sdkwork_contract_service::{CommerceMoney, CommercePaymentStatus, CommerceServiceError};
use sdkwork_payment_service::{
    ClosePaymentRecordCommand, PaymentRecordDetailQuery, PaymentRecordItem, PaymentRecordListPage,
    PaymentRecordListQuery, PaymentRecordOrderListPage, PaymentRecordOrderListQuery,
    PaymentRecordOutTradeNoQuery, PaymentRecordStatistics, PaymentRecordStatisticsQuery,
};
use sqlx::{PgPool, Row};

const PAYMENT_INTENT_JOIN: &str = r#"
LEFT JOIN commerce_payment_intent pi
    ON pi.id = (
        SELECT pi2.id
        FROM commerce_payment_intent pi2
        WHERE pi2.tenant_id = o.tenant_id
          AND pi2.order_id = o.id
          AND pi2.owner_user_id = o.owner_user_id
          AND pi2.deleted_at IS NULL
          AND (
                (pi2.organization_id = o.organization_id)
             OR (pi2.organization_id IS NULL AND o.organization_id = '0')
          )
        ORDER BY pi2.created_at DESC, pi2.id DESC
        LIMIT 1
    )
"#;

const PAYMENT_ATTEMPT_JOIN: &str = r#"
LEFT JOIN commerce_payment_attempt pa
    ON pa.id = (
        SELECT pa2.id
        FROM commerce_payment_attempt pa2
        WHERE pa2.tenant_id = o.tenant_id
          AND pa2.order_id = o.id
          AND pa2.owner_user_id = o.owner_user_id
          AND pa2.deleted_at IS NULL
          AND (
                (pa2.organization_id = o.organization_id)
             OR (pa2.organization_id IS NULL AND o.organization_id = '0')
          )
        ORDER BY pa2.created_at DESC, pa2.id DESC
        LIMIT 1
    )
"#;

const LIST_PAYMENT_RECORDS: &str = r#"
SELECT
    CAST(o.id AS TEXT) AS order_id,
    CAST(COALESCE(pa.id, pi.id, o.id) AS TEXT) AS id,
    CAST(pi.id AS TEXT) AS payment_id,
    CAST(pa.id AS TEXT) AS payment_attempt_id,
    COALESCE(NULLIF(pa.out_trade_no, ''), NULLIF(o.order_no, ''), '-') AS order_no,
    COALESCE(NULLIF(pa.payment_method, ''), NULLIF(pi.payment_method, ''), '-') AS method,
    CAST(COALESCE(NULLIF(pa.amount, ''), NULLIF(pi.amount, ''), '0') AS TEXT) AS amount,
    CAST(COALESCE(pa.paid_at, pa.created_at, o.paid_at, o.created_at) AS TEXT) AS date,
    o.status AS order_status,
    pi.status AS payment_status,
    pa.status AS payment_attempt_status,
    COUNT(*) OVER() AS total_count
FROM commerce_order o
"#;

const LIST_PAYMENT_RECORDS_BY_ORDER: &str = r#"
SELECT
    CAST(o.id AS TEXT) AS order_id,
    CAST(COALESCE(pa.id, pi.id, o.id) AS TEXT) AS id,
    CAST(pi.id AS TEXT) AS payment_id,
    CAST(pa.id AS TEXT) AS payment_attempt_id,
    COALESCE(NULLIF(pa.out_trade_no, ''), NULLIF(o.order_no, ''), '-') AS order_no,
    COALESCE(NULLIF(pa.payment_method, ''), NULLIF(pi.payment_method, ''), '-') AS method,
    CAST(COALESCE(NULLIF(pa.amount, ''), NULLIF(pi.amount, ''), '0') AS TEXT) AS amount,
    CAST(COALESCE(pa.paid_at, pa.created_at, o.paid_at, o.created_at) AS TEXT) AS date,
    o.status AS order_status,
    pi.status AS payment_status,
    pa.status AS payment_attempt_status,
    COUNT(*) OVER() AS total_count
FROM commerce_order o
"#;

const RETRIEVE_PAYMENT_RECORD: &str = r#"
SELECT
    CAST(o.id AS TEXT) AS order_id,
    CAST(COALESCE(pa.id, pi.id, o.id) AS TEXT) AS id,
    CAST(pi.id AS TEXT) AS payment_id,
    CAST(pa.id AS TEXT) AS payment_attempt_id,
    COALESCE(NULLIF(pa.out_trade_no, ''), NULLIF(o.order_no, ''), '-') AS order_no,
    COALESCE(NULLIF(pa.payment_method, ''), NULLIF(pi.payment_method, ''), '-') AS method,
    CAST(COALESCE(NULLIF(pa.amount, ''), NULLIF(pi.amount, ''), '0') AS TEXT) AS amount,
    CAST(COALESCE(pa.paid_at, pa.created_at, o.paid_at, o.created_at) AS TEXT) AS date,
    o.status AS order_status,
    pi.status AS payment_status,
    pa.status AS payment_attempt_status
FROM commerce_order o
"#;

const RETRIEVE_PAYMENT_RECORD_BY_OUT_TRADE_NO: &str = r#"
SELECT
    CAST(o.id AS TEXT) AS order_id,
    CAST(COALESCE(pa.id, pi.id, o.id) AS TEXT) AS id,
    CAST(pi.id AS TEXT) AS payment_id,
    CAST(pa.id AS TEXT) AS payment_attempt_id,
    COALESCE(NULLIF(pa.out_trade_no, ''), NULLIF(o.order_no, ''), '-') AS order_no,
    COALESCE(NULLIF(pa.payment_method, ''), NULLIF(pi.payment_method, ''), '-') AS method,
    CAST(COALESCE(NULLIF(pa.amount, ''), NULLIF(pi.amount, ''), '0') AS TEXT) AS amount,
    CAST(COALESCE(pa.paid_at, pa.created_at, o.paid_at, o.created_at) AS TEXT) AS date,
    o.status AS order_status,
    pi.status AS payment_status,
    pa.status AS payment_attempt_status
FROM commerce_order o
"#;

const FETCH_PAYMENT_STATISTICS: &str = r#"
SELECT
    COUNT(*)::BIGINT AS total_payments,
    COALESCE(SUM(CASE WHEN record_status = 'pending' THEN 1 ELSE 0 END), 0)::BIGINT AS pending_payments,
    COALESCE(SUM(CASE WHEN record_status = 'success' THEN 1 ELSE 0 END), 0)::BIGINT AS success_payments,
    COALESCE(SUM(CASE WHEN record_status = 'failed' THEN 1 ELSE 0 END), 0)::BIGINT AS failed_payments,
    COALESCE(SUM(CASE WHEN record_status = 'timeout' THEN 1 ELSE 0 END), 0)::BIGINT AS timeout_payments,
    COALESCE(SUM(CASE WHEN record_status = 'closed' THEN 1 ELSE 0 END), 0)::BIGINT AS closed_payments
FROM (
    SELECT
        CASE
            WHEN LOWER(COALESCE(pa.status, '')) = 'timeout'
              OR LOWER(COALESCE(pi.status, '')) = 'timeout' THEN 'timeout'
            WHEN LOWER(COALESCE(pa.status, '')) = 'closed'
              OR LOWER(COALESCE(pi.status, '')) = 'closed' THEN 'closed'
            WHEN LOWER(TRIM(COALESCE(o.status, ''))) IN ('closed', 'cancelled', 'canceled', 'failed')
              OR (pi.id IS NOT NULL AND LOWER(TRIM(COALESCE(pi.status, ''))) IN ('failed', 'canceled', 'cancelled'))
              OR (pa.id IS NOT NULL AND LOWER(TRIM(COALESCE(pa.status, ''))) IN ('failed', 'canceled', 'cancelled')) THEN 'failed'
            WHEN (pa.id IS NOT NULL AND LOWER(TRIM(COALESCE(pa.status, ''))) IN ('succeeded', 'success', 'paid'))
              OR (pi.id IS NOT NULL AND LOWER(TRIM(COALESCE(pi.status, ''))) IN ('succeeded', 'success', 'paid'))
              OR LOWER(TRIM(COALESCE(o.status, ''))) IN ('paid', 'fulfilled', 'success', 'completed') THEN 'success'
            ELSE 'pending'
        END AS record_status
    FROM commerce_order o
"#;

const CLOSE_PAYMENT_ATTEMPT: &str = r#"
UPDATE commerce_payment_attempt
SET status = $1, updated_at = $2::timestamptz
WHERE tenant_id = CAST($3 AS TEXT)
  AND owner_user_id = CAST($4 AS TEXT)
  AND id = CAST($5 AS TEXT)
  AND LOWER(COALESCE(status, '')) IN ('created', 'pending', 'processing')
"#;

const CLOSE_PARENT_PAYMENT_INTENT: &str = r#"
UPDATE commerce_payment_intent
SET status = $1, updated_at = $2::timestamptz
WHERE tenant_id = CAST($3 AS TEXT)
  AND owner_user_id = CAST($4 AS TEXT)
  AND id = (
      SELECT payment_intent_id
      FROM commerce_payment_attempt
      WHERE tenant_id = CAST($3 AS TEXT)
        AND owner_user_id = CAST($4 AS TEXT)
        AND id = CAST($5 AS TEXT)
      LIMIT 1
  )
  AND LOWER(COALESCE(status, '')) IN ('created', 'pending', 'processing')
"#;

const CLOSE_PAYMENT_INTENT: &str = r#"
UPDATE commerce_payment_intent
SET status = $1, updated_at = $2::timestamptz
WHERE tenant_id = CAST($3 AS TEXT)
  AND owner_user_id = CAST($4 AS TEXT)
  AND id = CAST($5 AS TEXT)
  AND LOWER(COALESCE(status, '')) IN ('created', 'pending', 'processing')
"#;

#[derive(Debug, Clone)]
pub struct PostgresCommercePaymentRecordStore {
    pool: PgPool,
}

impl PostgresCommercePaymentRecordStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn list_payment_records(
        &self,
        query: PaymentRecordListQuery,
    ) -> Result<PaymentRecordListPage, CommerceServiceError> {
        let sql = format!(
            "{LIST_PAYMENT_RECORDS}{PAYMENT_INTENT_JOIN}{PAYMENT_ATTEMPT_JOIN}
WHERE o.tenant_id = CAST($1 AS TEXT)
  AND ((o.organization_id = CAST($2 AS TEXT)) OR (o.organization_id IS NULL AND $2 IS NULL) OR (o.organization_id = '0' AND $2 IS NULL))
  AND o.owner_user_id = CAST($3 AS TEXT)
ORDER BY COALESCE(pa.paid_at, pa.created_at, o.paid_at, o.created_at) DESC NULLS LAST, o.id DESC
LIMIT $4 OFFSET $5"
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(&query.tenant_id)
            .bind(query.organization_id.as_deref())
            .bind(&query.owner_user_id)
            .bind(query.limit)
            .bind(query.offset)
            .fetch_all(&self.pool)
            .await
            .or_else(empty_rows_when_read_model_is_missing)
            .map_err(|error| store_error("failed to list payment records", error))?;

        let total_items = rows
            .first()
            .and_then(|row| row.try_get::<i64, _>("total_count").ok())
            .unwrap_or(0);

        let items = rows
            .iter()
            .map(payment_record_from_row)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(PaymentRecordListPage { items, total_items })
    }

    pub async fn list_payment_records_by_order(
        &self,
        query: PaymentRecordOrderListQuery,
    ) -> Result<PaymentRecordOrderListPage, CommerceServiceError> {
        let sql = format!(
            "{LIST_PAYMENT_RECORDS_BY_ORDER}{PAYMENT_INTENT_JOIN}{PAYMENT_ATTEMPT_JOIN}
WHERE o.tenant_id = CAST($1 AS TEXT)
  AND ((o.organization_id = CAST($2 AS TEXT)) OR (o.organization_id IS NULL AND $2 IS NULL) OR (o.organization_id = '0' AND $2 IS NULL))
  AND o.owner_user_id = CAST($3 AS TEXT)
  AND o.id = CAST($4 AS TEXT)
ORDER BY COALESCE(pa.paid_at, pa.created_at, o.paid_at, o.created_at) DESC NULLS LAST, o.id DESC
LIMIT $5 OFFSET $6"
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(&query.tenant_id)
            .bind(query.organization_id.as_deref())
            .bind(&query.owner_user_id)
            .bind(&query.order_id)
            .bind(query.limit)
            .bind(query.offset)
            .fetch_all(&self.pool)
            .await
            .or_else(empty_rows_when_read_model_is_missing)
            .map_err(|error| store_error("failed to list payment records by order", error))?;

        let total_items = rows
            .first()
            .and_then(|row| row.try_get::<i64, _>("total_count").ok())
            .unwrap_or(0);
        let items = rows
            .iter()
            .map(payment_record_from_row)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(PaymentRecordOrderListPage { items, total_items })
    }

    pub async fn retrieve_payment_record(
        &self,
        query: PaymentRecordDetailQuery,
    ) -> Result<PaymentRecordItem, CommerceServiceError> {
        let sql = format!(
            "{RETRIEVE_PAYMENT_RECORD}{PAYMENT_INTENT_JOIN}{PAYMENT_ATTEMPT_JOIN}
WHERE o.tenant_id = CAST($1 AS TEXT)
  AND ((o.organization_id = CAST($2 AS TEXT)) OR (o.organization_id IS NULL AND $2 IS NULL) OR (o.organization_id = '0' AND $2 IS NULL))
  AND o.owner_user_id = CAST($3 AS TEXT)
  AND (pa.id = CAST($4 AS TEXT) OR pi.id = CAST($4 AS TEXT) OR o.id = CAST($4 AS TEXT))
LIMIT 1"
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(&query.tenant_id)
            .bind(query.organization_id.as_deref())
            .bind(&query.owner_user_id)
            .bind(&query.payment_id)
            .fetch_optional(&self.pool)
            .await
            .or_else(none_when_read_model_is_missing)
            .map_err(|error| store_error("failed to retrieve payment record", error))?;

        row.as_ref()
            .map(payment_record_from_row)
            .transpose()?
            .ok_or_else(|| CommerceServiceError::not_found("payment record was not found"))
    }

    pub async fn retrieve_payment_record_by_out_trade_no(
        &self,
        query: PaymentRecordOutTradeNoQuery,
    ) -> Result<PaymentRecordItem, CommerceServiceError> {
        let sql = format!(
            "{RETRIEVE_PAYMENT_RECORD_BY_OUT_TRADE_NO}{PAYMENT_INTENT_JOIN}{PAYMENT_ATTEMPT_JOIN}
WHERE o.tenant_id = CAST($1 AS TEXT)
  AND ((o.organization_id = CAST($2 AS TEXT)) OR (o.organization_id IS NULL AND $2 IS NULL) OR (o.organization_id = '0' AND $2 IS NULL))
  AND o.owner_user_id = CAST($3 AS TEXT)
  AND COALESCE(NULLIF(pa.out_trade_no, ''), NULLIF(o.order_no, '')) = CAST($4 AS TEXT)
LIMIT 1"
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(&query.tenant_id)
            .bind(query.organization_id.as_deref())
            .bind(&query.owner_user_id)
            .bind(&query.out_trade_no)
            .fetch_optional(&self.pool)
            .await
            .or_else(none_when_read_model_is_missing)
            .map_err(|error| {
                store_error("failed to retrieve payment record by out trade no", error)
            })?;

        row.as_ref()
            .map(payment_record_from_row)
            .transpose()?
            .ok_or_else(|| CommerceServiceError::not_found("payment record was not found"))
    }

    pub async fn fetch_payment_statistics(
        &self,
        query: PaymentRecordStatisticsQuery,
    ) -> Result<PaymentRecordStatistics, CommerceServiceError> {
        let sql = format!(
            "{FETCH_PAYMENT_STATISTICS}{PAYMENT_INTENT_JOIN}{PAYMENT_ATTEMPT_JOIN}
WHERE o.tenant_id = CAST($1 AS TEXT)
  AND ((o.organization_id = CAST($2 AS TEXT)) OR (o.organization_id IS NULL AND $2 IS NULL) OR (o.organization_id = '0' AND $2 IS NULL))
  AND o.owner_user_id = CAST($3 AS TEXT)
) stats"
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(&query.tenant_id)
            .bind(query.organization_id.as_deref())
            .bind(&query.owner_user_id)
            .fetch_optional(&self.pool)
            .await
            .or_else(none_when_read_model_is_missing)
            .map_err(|error| store_error("failed to fetch payment statistics", error))?;

        let Some(row) = row else {
            return Ok(PaymentRecordStatistics {
                total_payments: 0,
                pending_payments: 0,
                success_payments: 0,
                failed_payments: 0,
                timeout_payments: 0,
                closed_payments: 0,
            });
        };

        Ok(PaymentRecordStatistics {
            total_payments: row.try_get("total_payments").unwrap_or(0),
            pending_payments: row.try_get("pending_payments").unwrap_or(0),
            success_payments: row.try_get("success_payments").unwrap_or(0),
            failed_payments: row.try_get("failed_payments").unwrap_or(0),
            timeout_payments: row.try_get("timeout_payments").unwrap_or(0),
            closed_payments: row.try_get("closed_payments").unwrap_or(0),
        })
    }

    pub async fn close_payment_record(
        &self,
        command: ClosePaymentRecordCommand,
    ) -> Result<(), CommerceServiceError> {
        crate::shared::ensure_payment_status_transition(
            "pending",
            CommercePaymentStatus::Canceled.as_str(),
        )?;
        let now = current_timestamp_string();
        let mut tx = self.pool.begin().await.map_err(|error| {
            store_error("failed to begin close payment record transaction", error)
        })?;

        let attempt = sqlx::query(CLOSE_PAYMENT_ATTEMPT)
            .bind(CommercePaymentStatus::Canceled.as_str())
            .bind(&now)
            .bind(&command.tenant_id)
            .bind(&command.owner_user_id)
            .bind(&command.payment_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error("failed to close payment attempt", error))?;

        if attempt.rows_affected() > 0 {
            sqlx::query(CLOSE_PARENT_PAYMENT_INTENT)
                .bind(CommercePaymentStatus::Canceled.as_str())
                .bind(&now)
                .bind(&command.tenant_id)
                .bind(&command.owner_user_id)
                .bind(&command.payment_id)
                .execute(&mut *tx)
                .await
                .map_err(|error| store_error("failed to close parent payment intent", error))?;

            tx.commit().await.map_err(|error| {
                store_error("failed to commit close payment record transaction", error)
            })?;
            return Ok(());
        }

        let intent = sqlx::query(CLOSE_PAYMENT_INTENT)
            .bind(CommercePaymentStatus::Canceled.as_str())
            .bind(&now)
            .bind(&command.tenant_id)
            .bind(&command.owner_user_id)
            .bind(&command.payment_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error("failed to close payment intent", error))?;

        if intent.rows_affected() == 0 {
            return Err(CommerceServiceError::conflict(
                "payment record is not closable or was not found",
            ));
        }

        tx.commit().await.map_err(|error| {
            store_error("failed to commit close payment record transaction", error)
        })?;

        Ok(())
    }
}

fn payment_record_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<PaymentRecordItem, CommerceServiceError> {
    PaymentRecordItem::new(
        &string_cell(row, "id"),
        &string_cell(row, "order_id"),
        &string_cell(row, "order_no"),
        &string_cell(row, "method"),
        commerce_money_cell(row, "amount", "payment record amount")?,
        &string_cell(row, "date"),
        payment_record_status(row)?,
    )
}

fn payment_record_status(
    row: &sqlx::postgres::PgRow,
) -> Result<&'static str, CommerceServiceError> {
    let order_status =
        owner_order_status_wire_label(&required_status_cell(row, "order_status", "order")?)?;
    let payment_status = related_status_cell(row, "payment_id", "payment_status", "payment")?
        .map(|status| payment_status_label(&status))
        .transpose()?
        .unwrap_or("pending");
    let payment_attempt_status = related_status_cell(
        row,
        "payment_attempt_id",
        "payment_attempt_status",
        "payment attempt",
    )?
    .map(|status| payment_status_label(&status))
    .transpose()?
    .unwrap_or("pending");

    Ok(payment_record_status_label(
        order_status,
        payment_status,
        payment_attempt_status,
    ))
}

fn payment_record_status_label(
    order_status: &str,
    payment_status: &str,
    payment_attempt_status: &str,
) -> &'static str {
    if order_status == "failed" {
        "failed"
    } else if payment_attempt_status == "success"
        || payment_status == "success"
        || order_status == "success"
    {
        "success"
    } else if payment_attempt_status == "failed" || payment_status == "failed" {
        "failed"
    } else {
        "pending"
    }
}

fn owner_order_status_wire_label(value: &str) -> Result<&'static str, CommerceServiceError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "draft" | "pending_payment" | "unpaid" | "wait_pay" | "pending" => Ok("pending"),
        "paid" | "success" | "completed" | "fulfilled" => Ok("success"),
        "cancelled" | "canceled" | "closed" | "failed" | "expired" => Ok("failed"),
        status => Err(CommerceServiceError::storage(format!(
            "unsupported payment record order status: {status}"
        ))),
    }
}

fn payment_status_label(value: &str) -> Result<&'static str, CommerceServiceError> {
    match value.trim().to_ascii_lowercase().as_str() {
        status if status == CommercePaymentStatus::Pending.as_str() => Ok("pending"),
        status if status == CommercePaymentStatus::Succeeded.as_str() => Ok("success"),
        status if status == CommercePaymentStatus::Failed.as_str() => Ok("failed"),
        status if status == CommercePaymentStatus::Canceled.as_str() => Ok("failed"),
        status => Err(CommerceServiceError::storage(format!(
            "unsupported payment record payment status: {status}"
        ))),
    }
}

fn optional_string_cell(row: &sqlx::postgres::PgRow, column: &str) -> Option<String> {
    row.try_get::<Option<String>, _>(column).ok().flatten()
}

fn string_cell(row: &sqlx::postgres::PgRow, column: &str) -> String {
    optional_string_cell(row, column).unwrap_or_default()
}

fn required_status_cell(
    row: &sqlx::postgres::PgRow,
    column: &str,
    source: &str,
) -> Result<String, CommerceServiceError> {
    optional_string_cell(row, column)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| missing_payment_record_status_error(source))
}

fn related_status_cell(
    row: &sqlx::postgres::PgRow,
    relation_column: &str,
    status_column: &str,
    source: &str,
) -> Result<Option<String>, CommerceServiceError> {
    if optional_string_cell(row, relation_column)
        .map(|value| value.trim().is_empty())
        .unwrap_or(true)
    {
        return Ok(None);
    }
    required_status_cell(row, status_column, source).map(Some)
}

fn missing_payment_record_status_error(source: &str) -> CommerceServiceError {
    CommerceServiceError::storage(format!(
        "missing payment record {source} status from database row"
    ))
}

fn commerce_money_cell(
    row: &sqlx::postgres::PgRow,
    column: &str,
    field_name: &str,
) -> Result<CommerceMoney, CommerceServiceError> {
    let value = string_cell(row, column);
    if value.trim().is_empty()
        || !value
            .trim()
            .chars()
            .all(|character| character.is_ascii_digit())
    {
        return Err(CommerceServiceError::storage(format!(
            "invalid {field_name}: {value}"
        )));
    }
    CommerceMoney::new(value.trim())
        .map_err(|message| CommerceServiceError::storage(format!("{message}: {value}")))
}

fn empty_rows_when_read_model_is_missing(
    error: sqlx::Error,
) -> Result<Vec<sqlx::postgres::PgRow>, sqlx::Error> {
    if is_missing_postgres_read_model(&error) {
        Ok(Vec::new())
    } else {
        Err(error)
    }
}

fn none_when_read_model_is_missing(
    error: sqlx::Error,
) -> Result<Option<sqlx::postgres::PgRow>, sqlx::Error> {
    if is_missing_postgres_read_model(&error) {
        Ok(None)
    } else {
        Err(error)
    }
}

fn is_missing_postgres_read_model(error: &sqlx::Error) -> bool {
    if matches!(error, sqlx::Error::ColumnNotFound(_)) {
        return true;
    }
    error
        .as_database_error()
        .and_then(|database_error| database_error.code())
        .map(|code| matches!(code.as_ref(), "42P01" | "42703"))
        .unwrap_or(false)
}

fn store_error(context: &str, error: sqlx::Error) -> CommerceServiceError {
    CommerceServiceError::storage(format!("{context}: {error}"))
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;

    use super::{
        current_timestamp_string, CLOSE_PARENT_PAYMENT_INTENT, CLOSE_PAYMENT_ATTEMPT,
        CLOSE_PAYMENT_INTENT,
    };

    #[test]
    fn close_payment_record_uses_rfc3339_timestamptz_writes() {
        let now = current_timestamp_string();
        DateTime::parse_from_rfc3339(&now).expect("current timestamp must be RFC3339");

        for sql in [
            CLOSE_PAYMENT_ATTEMPT,
            CLOSE_PARENT_PAYMENT_INTENT,
            CLOSE_PAYMENT_INTENT,
        ] {
            assert!(sql.contains("updated_at = $2::timestamptz"));
        }
    }
}
