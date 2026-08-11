//! Read-only `commerce_order` snapshots for payment validation.
//!
//! Payment must not depend on `sdkwork-order` crates. These queries are foreign-key
//! lookups only; order lifecycle mutations remain in the order capability.
use chrono::{DateTime, NaiveDateTime, Utc};
use sdkwork_contract_service::{CommerceMoney, CommerceServiceError};
use sdkwork_payment_service::OrderPaymentReferenceQuery;
use sdkwork_payment_service::OrderPaymentReferenceSnapshot;
use sqlx::postgres::PgRow;
use sqlx::{Postgres, Row, Transaction};
use crate::shared::{store_error, string_cell, StringCellRow};
pub(crate) async fn load_order_payment_reference_postgres(
    tx: &mut Transaction<'_, Postgres>,
    query: &OrderPaymentReferenceQuery,
) -> Result<Option<OrderPaymentReferenceSnapshot>, CommerceServiceError> {
    let row = sqlx::query(
        r#"
        SELECT
            o.id AS order_id,
            o.order_no AS order_sn,
            o.subject AS order_subject,
            o.status,
            -- Timestamp boundaries are normalized to a UTC RFC3339 text
            -- representation on the SQL side so the Rust reader never has to
            -- decode the raw column type: deployments may store them as TEXT
            -- (order baseline) or TIMESTAMPTZ, and a silent decode failure
            -- would turn every boundary into NULL (order "not pending
            -- payment").
            CASE WHEN o.expired_at IS NULL OR o.expired_at = '' THEN NULL
                 ELSE to_char(CAST(o.expired_at AS TIMESTAMPTZ) AT TIME ZONE 'UTC',
                              'YYYY-MM-DD"T"HH24:MI:SS"Z"')
            END AS expires_at,
            CASE WHEN o.paid_at IS NULL OR o.paid_at = '' THEN NULL
                 ELSE to_char(CAST(o.paid_at AS TIMESTAMPTZ) AT TIME ZONE 'UTC',
                              'YYYY-MM-DD"T"HH24:MI:SS"Z"')
            END AS pay_time,
            COALESCE(
                (
                    SELECT b.payable_amount
                    FROM commerce_order_amount_breakdown b
                    WHERE b.tenant_id = o.tenant_id
                      AND b.order_id = o.id
                      AND b.allocation_type = 'order_total'
                    LIMIT 1
                ),
                '0'
            ) AS total_amount
        FROM commerce_order o
        WHERE o.id = CAST($1 AS TEXT)
          AND o.tenant_id = CAST($2 AS TEXT)
          AND ((o.organization_id = CAST($3 AS TEXT)) OR (o.organization_id IS NULL AND $3 IS NULL) OR (o.organization_id = '0' AND $3 IS NULL))
          AND o.owner_user_id = CAST($4 AS TEXT)
        "#,
    )
    .bind(&query.order_id)
    .bind(&query.tenant_id)
    .bind(query.organization_id.as_deref())
    .bind(&query.owner_user_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| store_error("failed to load order payment reference", error))?;
    Ok(row.map(map_postgres_order_payment_reference_row))
}
fn map_postgres_order_payment_reference_row(row: PgRow) -> OrderPaymentReferenceSnapshot {
    map_order_payment_reference_row(
        &row,
        optional_postgres_string_cell(&row, "expires_at"),
        optional_postgres_string_cell(&row, "order_subject"),
        optional_postgres_string_cell(&row, "pay_time"),
    )
}
fn map_order_payment_reference_row<R: StringCellRow>(
    row: &R,
    expires_at: Option<String>,
    order_subject: Option<String>,
    pay_time: Option<String>,
) -> OrderPaymentReferenceSnapshot {
    OrderPaymentReferenceSnapshot {
        expires_at,
        order_id: string_cell(row, "order_id"),
        order_sn: string_cell(row, "order_sn"),
        order_subject,
        status: string_cell(row, "status"),
        total_amount: CommerceMoney::new(&string_cell(row, "total_amount"))
            .unwrap_or_else(|_| CommerceMoney::new("0").expect("zero amount")),
        pay_time,
    }
}
fn optional_postgres_string_cell(row: &PgRow, column: &str) -> Option<String> {
    row.try_get::<Option<String>, _>(column).ok().flatten()
}
pub(crate) fn order_status_is_payable(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        "draft" | "pending" | "pending_payment" | "unpaid" | "wait_pay" | "created"
    )
}
pub(crate) fn order_payment_reference_is_payable(
    reference: &OrderPaymentReferenceSnapshot,
) -> bool {
    order_status_is_payable(&reference.status)
        && order_expiration_is_payable(reference.expires_at.as_deref())
}
pub(crate) fn order_expiration_is_payable(expires_at: Option<&str>) -> bool {
    let Some(expires_at) = expires_at.map(str::trim).filter(|value| !value.is_empty()) else {
        return false;
    };
    parse_utc_timestamp(expires_at)
        .map(|value| value > Utc::now())
        .unwrap_or(false)
}
fn parse_utc_timestamp(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
                .ok()
                .map(|value| value.and_utc())
        })
}
pub(crate) fn order_status_is_refundable(status: &str, pay_time: Option<&str>) -> bool {
    let normalized = status.trim().to_ascii_lowercase();
    let paid = matches!(
        normalized.as_str(),
        "paid" | "succeeded" | "success" | "completed" | "finished"
    );
    paid && pay_time
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}
#[cfg(test)]
mod tests {
    use super::order_expiration_is_payable;
    use chrono::{SecondsFormat, Utc};

    #[test]
    fn order_expiration_fails_closed_for_missing_expired_or_invalid_values() {
        assert!(!order_expiration_is_payable(None));
        assert!(!order_expiration_is_payable(Some("")));
        assert!(order_expiration_is_payable(Some("2099-01-01T00:00:00Z")));
        assert!(!order_expiration_is_payable(Some("2020-01-01T00:00:00Z")));
        assert!(!order_expiration_is_payable(Some("not-a-timestamp")));
    }

    #[test]
    fn millisecond_rfc3339_boundaries_from_the_test_payment_flow_are_payable() {
        // The one-cent test payment writes its 15-minute expiry with the
        // `sdkwork-utils-rust` default pattern (%Y-%m-%dT%H:%M:%S%.3fZ); the
        // payable check must accept that exact shape while it is in the future.
        let boundary = (Utc::now() + chrono::Duration::minutes(15))
            .to_rfc3339_opts(SecondsFormat::Millis, true);
        assert!(boundary.contains('.'), "expected millisecond RFC3339");
        assert!(order_expiration_is_payable(Some(&boundary)));
        assert!(order_expiration_is_payable(Some(
            &(Utc::now() + chrono::Duration::minutes(15)).to_rfc3339()
        )));
    }
}