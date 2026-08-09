//! Re-applies stored webhook payloads for backend admin replay.
//!
//! Payment-side only: updates attempt/intent status from persisted normalized fields.
//! Order settlement after success remains on the order gateway (`payment_confirmations`).
use sdkwork_contract_service::CommerceServiceError;
use sqlx::{Pool, Postgres, Row, };
use crate::payment_attempt_context::{
    load_payment_webhook_attempt_context_by_id_postgres,
    PaymentWebhookAttemptContext,
};
use crate::shared::{current_timestamp_string, store_error, string_cell, StringCellRow};
use crate::webhook_event_payload::{
    parse_stored_webhook_payload, validate_stored_webhook_scope, WEBHOOK_EVENT_STATUS_PROCESSED,
    WEBHOOK_MATCH_STATE_UNMATCHED,
};
pub const WEBHOOK_STORED_REPLAY_MAX_RETRIES: i64 = 5;
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebhookStoredReplayScope {
    pub tenant_id: String,
    pub organization_id: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StoredWebhookReplayResult {
    Applied {
        webhook_event: Box<WebhookEventRow>,
        payment_attempt_context: Option<PaymentWebhookAttemptContext>,
    },
    NotFound,
    LimitExceeded {
        current_retries: i64,
    },
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebhookEventRow {
    pub id: String,
    pub event_id: String,
    pub provider_code: String,
    pub event_type: String,
    pub status: String,
    pub received_at: String,
    pub processed_at: Option<String>,
    pub retries: i64,
}
pub async fn replay_stored_webhook_event_postgres(
    pool: &Pool<Postgres>,
    scope: WebhookStoredReplayScope,
    provider_scoped_event_id: String,
) -> Result<StoredWebhookReplayResult, CommerceServiceError> {
    let now = current_timestamp_string();
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| store_error("failed to begin webhook replay transaction", error))?;
    let row = sqlx::query(
        r#"
        SELECT id, event_id, provider_code, event_type, status,
               to_char(received_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS received_at,
               to_char(processed_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS processed_at,
               retries, payload
        FROM commerce_payment_webhook_event
        WHERE event_id = CAST($1 AS TEXT)
          AND tenant_id = CAST($2 AS TEXT)
          AND (organization_id IS NULL AND $3::text IS NULL OR organization_id = CAST($3 AS TEXT))
        FOR UPDATE
        "#,
    )
    .bind(&provider_scoped_event_id)
    .bind(&scope.tenant_id)
    .bind(scope.organization_id.as_deref())
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| store_error("failed to load webhook event for replay", error))?;
    let Some(row) = row else {
        return Ok(StoredWebhookReplayResult::NotFound);
    };
    let retries: i64 = row.try_get("retries").unwrap_or(0);
    if retries >= WEBHOOK_STORED_REPLAY_MAX_RETRIES {
        return Ok(StoredWebhookReplayResult::LimitExceeded {
            current_retries: retries,
        });
    }
    let internal_id = string_cell(&row, "id");
    let stored_event_id = string_cell(&row, "event_id");
    let provider_code = string_cell(&row, "provider_code");
    let payload: serde_json::Value = row
        .try_get("payload")
        .map_err(|error| CommerceServiceError::storage(format!("webhook payload: {error}")))?;
    let stored = parse_stored_webhook_payload(&payload.to_string())?;
    validate_stored_webhook_scope(
        &stored,
        &stored_event_id,
        &provider_code,
        &scope.tenant_id,
        scope.organization_id.as_deref(),
    )?;
    if stored.match_state == WEBHOOK_MATCH_STATE_UNMATCHED {
        return Err(CommerceServiceError::conflict(
            "unmatched webhook has no exact payment attempt identity to replay",
        ));
    }
    let identity = stored.attempt_identity.as_ref().ok_or_else(|| {
        CommerceServiceError::conflict(
            "stored webhook has no exact payment attempt identity to replay",
        )
    })?;
    let applied_status = crate::postgres_webhook_ingestion::apply_webhook_payment_status_postgres(
        &mut tx,
        identity,
        stored.payment_status.as_deref(),
        &now,
    )
    .await?;
    ensure_replay_target_applied(&stored.payment_status, &applied_status)?;
    let payment_attempt_context = if applied_status.as_deref() == Some("succeeded") {
        load_payment_webhook_attempt_context_by_id_postgres(
            &mut tx,
            &identity.payment_attempt_id,
            &identity.provider_code,
            Some(&identity.tenant_id),
            identity.organization_id.as_deref(),
        )
        .await?
    } else {
        None
    };
    let next_retries = retries + 1;
    sqlx::query(
        r#"
        UPDATE commerce_payment_webhook_event
        SET status = $1, processed_at = $2::timestamptz,
            updated_at = $2::timestamptz, retries = $3, last_error = NULL
        WHERE id = CAST($4 AS TEXT) AND tenant_id = CAST($5 AS TEXT)
          AND ((organization_id = CAST($6 AS TEXT)) OR (organization_id IS NULL AND $6 IS NULL) OR (organization_id = '0' AND $6 IS NULL))
        "#,
    )
    .bind(WEBHOOK_EVENT_STATUS_PROCESSED)
    .bind(&now)
    .bind(next_retries)
    .bind(&internal_id)
    .bind(&scope.tenant_id)
    .bind(scope.organization_id.as_deref())
    .execute(&mut *tx)
    .await
    .map_err(|error| store_error("failed to mark webhook event replayed", error))?;
    tx.commit()
        .await
        .map_err(|error| store_error("failed to commit webhook replay transaction", error))?;
    Ok(StoredWebhookReplayResult::Applied {
        webhook_event: Box::new(map_webhook_event_row(
            &row,
            "processed",
            Some(now),
            next_retries,
        )),
        payment_attempt_context,
    })
}
impl WebhookEventRow {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "eventId": self.event_id,
            "providerCode": self.provider_code,
            "eventType": self.event_type,
            "status": self.status,
            "receivedAt": self.received_at,
            "processedAt": self.processed_at,
            "retries": self.retries,
        })
    }
}
fn map_webhook_event_row<R: StringCellRow>(
    row: &R,
    status: &str,
    processed_at: Option<String>,
    retries: i64,
) -> WebhookEventRow {
    WebhookEventRow {
        id: string_cell(row, "id"),
        event_id: string_cell(row, "event_id"),
        provider_code: string_cell(row, "provider_code"),
        event_type: string_cell(row, "event_type"),
        status: status.to_owned(),
        received_at: string_cell(row, "received_at"),
        processed_at,
        retries,
    }
}
fn ensure_replay_target_applied(
    payment_status: &Option<String>,
    applied_status: &Option<String>,
) -> Result<(), CommerceServiceError> {
    let has_target = payment_status
        .as_ref()
        .is_some_and(|value| !value.trim().is_empty());
    if has_target && applied_status.is_none() {
        return Err(CommerceServiceError::conflict(
            "stored webhook payment status cannot be applied safely",
        ));
    }
    Ok(())
}