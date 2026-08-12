//! Refund webhook ingestion（退款通知入库，payment 域）。
//!
//! Provider refund notifications (WeChat `REFUND.*`, Stripe `charge.refunded`,
//! Alipay refund async notify) are ingested through this independent flow:
//! the event is persisted idempotently, the provider refund facts are parsed
//! from the payload, the exact `commerce_refund` is resolved by `refund_no`
//! (the order gateway submits `refund_no` as the provider `out_refund_no`),
//! and the refund status machine is advanced. The order domain consumes the
//! outcome to link the order/after-sales state.
//!
//! This module mirrors `postgres_webhook_ingestion` but is a separate flow
//! system: payment notifications settle payments, refund notifications
//! settle refunds. They share only the event persistence helpers.

use sdkwork_contract_service::CommerceServiceError;
use serde_json::Value;
use sqlx::{Pool, Postgres, Row, Transaction};

use crate::payment_attempt_context::{
    load_attempt_by_out_trade_no_postgres, PaymentWebhookAttemptContext,
    PaymentWebhookAttemptIdentity,
};
use crate::postgres_refund::insert_refund_event;
use crate::postgres_webhook_ingestion::{
    ingest_provider_webhook_postgres, persist_webhook_event_postgres, IngestProviderWebhookCommand,
    IngestProviderWebhookOutcome,
};
use crate::shared::{current_timestamp_string, store_error, string_cell};
use crate::webhook_event_payload::{
    build_stored_webhook_payload, provider_scoped_webhook_event_id, webhook_event_storage_id,
    WebhookEventInsert, WebhookEventPayloadInput, WEBHOOK_EVENT_STATUS_FAILED,
    WEBHOOK_EVENT_STATUS_PROCESSED, WEBHOOK_EVENT_STATUS_QUEUED,
};
use crate::webhook_status::map_provider_refund_status;

/// Refund event type recorded on `commerce_refund_event` for webhook-driven
/// transitions (matches the table CHECK constraint).
const REFUND_EVENT_TYPE_STATUS_CHANGED: &str = "status_changed";

/// Canonical commerce refund terminal statuses that a webhook must never
/// overwrite (a succeeded refund is final; a failed refund can retry).
const REFUND_STATUS_SUCCEEDED: &str = "succeeded";
const REFUND_STATUS_PROCESSING: &str = "processing";
const REFUND_STATUS_FAILED: &str = "failed";
const REFUND_STATUS_CANCELED: &str = "canceled";

/// Refund facts parsed from a provider refund notification payload.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ParsedRefundNotifyFacts {
    pub refund_no: Option<String>,
    pub refund_status: Option<String>,
    pub refund_amount: Option<String>,
}

/// Resolved refund context returned after the refund status machine applied
/// the provider notification.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct IngestedRefundContext {
    pub refund_id: String,
    pub refund_no: String,
    pub order_id: String,
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub status: String,
    pub amount: String,
}

/// Outcome of refund webhook ingestion, projected for the order domain.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct IngestProviderRefundWebhookOutcome {
    pub webhook_event_id: String,
    pub replayed: bool,
    pub refund: Option<IngestedRefundContext>,
    pub payment_attempt_context: Option<PaymentWebhookAttemptContext>,
}

/// Ingests a provider refund notification idempotently and advances the
/// refund status machine. The event is persisted with the same identity rules
/// as payment webhooks; unmatched refunds (missing/unknown refund_no) are
/// stored as failed events for forensics and acked without side effects.
pub async fn ingest_provider_refund_webhook_postgres(
    pool: &Pool<Postgres>,
    command: IngestProviderWebhookCommand,
) -> Result<IngestProviderRefundWebhookOutcome, CommerceServiceError> {
    let provider_code = command.provider_code.trim().to_ascii_lowercase();
    if provider_code.is_empty() {
        return Err(CommerceServiceError::validation(
            "refund webhook provider code is required",
        ));
    }
    let provider_event_id = command.provider_event_id.trim();
    if provider_event_id.is_empty() {
        return Err(CommerceServiceError::validation(
            "refund webhook provider event id is required",
        ));
    }
    let now = current_timestamp_string();
    let mut tx = pool.begin().await.map_err(|error| {
        store_error(
            "failed to begin refund webhook ingestion transaction",
            error,
        )
    })?;

    let out_trade_no = command
        .out_trade_no
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let command_tenant_id = command
        .tenant_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let command_organization_id = command
        .organization_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if command_tenant_id.is_none() && command_organization_id.is_some() {
        return Err(CommerceServiceError::validation(
            "refund webhook organization scope requires tenant scope",
        ));
    }

    let attempt_identity = match out_trade_no {
        Some(out_trade_no) => {
            load_attempt_by_out_trade_no_postgres(
                &mut tx,
                &provider_code,
                out_trade_no,
                command_tenant_id,
                command_organization_id,
            )
            .await?
        }
        None => None,
    };
    let tenant_id = attempt_identity
        .as_ref()
        .map(|identity| identity.tenant_id.clone())
        .or_else(|| command_tenant_id.map(str::to_owned))
        .ok_or_else(|| {
            CommerceServiceError::validation(
                "refund webhook tenant scope could not be resolved safely",
            )
        })?;
    let organization_id = attempt_identity
        .as_ref()
        .and_then(|identity| identity.organization_id.clone())
        .or_else(|| command_organization_id.map(str::to_owned));

    let facts = parse_refund_notify_facts(&command.payload);
    let refund_no = facts.refund_no.as_deref();
    let applied_refund = match refund_no {
        Some(refund_no) => {
            apply_webhook_refund_status_postgres(
                &mut tx,
                &tenant_id,
                organization_id.as_deref(),
                refund_no,
                facts.refund_status.as_deref(),
                &now,
            )
            .await?
        }
        None => None,
    };

    let provider_scoped_event_id =
        provider_scoped_webhook_event_id(&provider_code, provider_event_id);
    let internal_id = webhook_event_storage_id(&tenant_id, &provider_scoped_event_id);
    let unmatched_reason = if applied_refund.is_none() {
        Some(if refund_no.is_some() {
            "refund_not_found"
        } else {
            "refund_no_missing"
        })
    } else {
        None
    };
    let payload_json = build_stored_webhook_payload(WebhookEventPayloadInput {
        provider_code: &provider_code,
        provider_event_id,
        provider_scoped_event_id: &provider_scoped_event_id,
        event_type: command.event_type.as_deref(),
        out_trade_no,
        payment_status: command.payment_status.as_deref(),
        provider_payload: &command.payload,
        attempt_identity: attempt_identity.as_ref(),
        unmatched_reason,
    })?;
    let proposed_event_status = if applied_refund.is_some() {
        WEBHOOK_EVENT_STATUS_QUEUED
    } else {
        WEBHOOK_EVENT_STATUS_FAILED
    };
    let insert = WebhookEventInsert {
        internal_id: &internal_id,
        tenant_id: &tenant_id,
        organization_id: organization_id.as_deref(),
        provider_scoped_event_id: &provider_scoped_event_id,
        event_type: command.event_type.as_deref().unwrap_or("refund"),
        provider_code: &provider_code,
        payload_json: &payload_json,
        status: proposed_event_status,
        last_error: unmatched_reason,
        now: &now,
    };
    let inserted = persist_webhook_event_postgres(&mut tx, &insert).await?;
    let refund = if inserted {
        applied_refund
    } else {
        // A redelivered refund notification replays the status application
        // against the stored provider payload; the status machine is
        // terminal-idempotent so this is safe.
        let stored_payload = load_stored_webhook_payload_json(
            &mut tx,
            &tenant_id,
            organization_id.as_deref(),
            &provider_scoped_event_id,
        )
        .await?;
        let stored_facts = stored_payload
            .as_ref()
            .and_then(|payload| payload.get("providerPayload"))
            .map(parse_refund_notify_facts);
        match stored_facts {
            Some(facts) => match (facts.refund_no.as_deref(), facts.refund_status.as_deref()) {
                (Some(refund_no), Some(raw_status)) => {
                    apply_webhook_refund_status_postgres(
                        &mut tx,
                        &tenant_id,
                        organization_id.as_deref(),
                        refund_no,
                        Some(raw_status),
                        &now,
                    )
                    .await?
                }
                _ => applied_refund,
            },
            None => applied_refund,
        }
    };

    if let Some(context) = refund.as_ref() {
        sqlx::query(
            r#"
            UPDATE commerce_payment_webhook_event
            SET status = $1, processed_at = $2::timestamptz, updated_at = $2::timestamptz,
                last_error = NULL
            WHERE id = CAST($3 AS TEXT)
              AND tenant_id = CAST($4 AS TEXT)
              AND ((organization_id = CAST($5 AS TEXT)) OR (organization_id IS NULL AND $5 IS NULL) OR (organization_id = '0' AND $5 IS NULL))
            "#,
        )
        .bind(WEBHOOK_EVENT_STATUS_PROCESSED)
        .bind(&now)
        .bind(&internal_id)
        .bind(&tenant_id)
        .bind(organization_id.as_deref())
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error("failed to mark refund webhook event processed", error))?;
        let _ = &context;
    }

    let payment_attempt_context = match attempt_identity.as_ref() {
        Some(identity) => {
            load_payment_webhook_attempt_context_by_identity(&mut tx, identity, &provider_code)
                .await
        }
        None => None,
    };

    tx.commit().await.map_err(|error| {
        store_error(
            "failed to commit refund webhook ingestion transaction",
            error,
        )
    })?;

    Ok(IngestProviderRefundWebhookOutcome {
        webhook_event_id: internal_id,
        replayed: !inserted,
        refund,
        payment_attempt_context,
    })
}

/// Parses the standard refund facts from a provider refund notification
/// payload. Key aliases cover WeChat/Stripe/Alipay field naming, including the
/// provider-specific envelopes:
///
/// - WeChat: the decrypted resource lives under `resource_plaintext`
///   (`out_refund_no`/`refund_status`/`refund_amount`);
/// - Stripe: the refund object lives under `data.object` (`id`, `status`,
///   `amount`, and the local `refund_no` carried as `metadata.refund_no`);
/// - Alipay / sandbox: flat top-level fields.
pub fn parse_refund_notify_facts(payload: &Value) -> ParsedRefundNotifyFacts {
    // Candidate objects searched in order, each with layer-appropriate keys:
    // the top-level `id` of a WeChat envelope is the *event* id and must not
    // be treated as a refund id; Stripe's refund id lives on `data.object`.
    let top_level_keys: &[&str] = &[
        "out_refund_no",
        "outRefundNo",
        "refund_no",
        "refundNo",
        "out_biz_no",
        "outBizNo",
        "refund_id",
        "refundId",
    ];
    let object_level_keys: &[&str] = &[
        "out_refund_no",
        "outRefundNo",
        "refund_no",
        "refundNo",
        "refund_id",
        "refundId",
        "id",
    ];
    let plaintext = payload.get("resource_plaintext");
    let object = payload.get("data").and_then(|data| data.get("object"));
    let refund_no = top_level_keys
        .iter()
        .find_map(|key| json_string(payload, &[*key]))
        .or_else(|| plaintext.and_then(|candidate| json_string(candidate, top_level_keys)))
        .or_else(|| {
            object.and_then(|candidate| {
                // Stripe carries the local refund_no as metadata.refund_no;
                // the object `id` is only a fallback when metadata is absent.
                candidate
                    .get("metadata")
                    .and_then(|metadata| json_string(metadata, &["refund_no", "refundNo"]))
                    .or_else(|| json_string(candidate, object_level_keys))
            })
        });
    let refund_status = layered_json_string(
        payload,
        plaintext,
        object,
        &[
            "refund_status",
            "refundStatus",
            "refund_state",
            "refundState",
            "status",
        ],
    );
    let refund_amount = layered_json_string(
        payload,
        plaintext,
        object,
        &[
            "refund_amount",
            "refundAmount",
            "refund_fee",
            "refundFee",
            "amount",
            "total_amount",
            "totalAmount",
        ],
    );
    ParsedRefundNotifyFacts {
        refund_no: refund_no
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty()),
        refund_status: refund_status
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty()),
        refund_amount: refund_amount
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty()),
    }
}

fn json_string(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| match value.get(*key) {
        Some(Value::String(value)) => Some(value.clone()),
        Some(Value::Number(value)) => Some(value.to_string()),
        _ => None,
    })
}

/// Looks up a field across the top-level payload, the WeChat decrypted
/// `resource_plaintext`, and the Stripe `data.object`, in that order.
fn layered_json_string(
    payload: &Value,
    plaintext: Option<&Value>,
    object: Option<&Value>,
    keys: &[&str],
) -> Option<String> {
    json_string(payload, keys)
        .or_else(|| plaintext.and_then(|candidate| json_string(candidate, keys)))
        .or_else(|| object.and_then(|candidate| json_string(candidate, keys)))
}

/// Resolves the exact refund by `refund_no` and advances the refund status
/// machine from the provider notification. Terminal states are preserved:
/// a succeeded refund is never overwritten; a failed refund may retry to
/// processing. Returns `None` when no refund matches the refund_no.
async fn apply_webhook_refund_status_postgres(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: &str,
    organization_id: Option<&str>,
    refund_no: &str,
    raw_status: Option<&str>,
    now: &str,
) -> Result<Option<IngestedRefundContext>, CommerceServiceError> {
    let row = sqlx::query(
        r#"
        SELECT id, order_id, payment_attempt_id, status,
               CAST(COALESCE(amount, 0) AS BIGINT)::TEXT AS amount
        FROM commerce_refund
        WHERE tenant_id = CAST($1 AS TEXT)
          AND refund_no = CAST($2 AS TEXT)
          AND ((organization_id = CAST($3 AS TEXT)) OR (organization_id IS NULL AND $3 IS NULL) OR (organization_id = '0' AND $3 IS NULL))
          AND deleted_at IS NULL
        LIMIT 1
        FOR UPDATE
        "#,
    )
    .bind(tenant_id)
    .bind(refund_no)
    .bind(organization_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| store_error("failed to load refund for webhook", error))?;
    let Some(row) = row else {
        return Ok(None);
    };
    let refund_id = string_cell(&row, "id");
    let order_id = string_cell(&row, "order_id");
    let amount = string_cell(&row, "amount");
    let current_status = string_cell(&row, "status");

    let Some(target_status) = map_provider_refund_status_raw(raw_status) else {
        return Ok(Some(IngestedRefundContext {
            refund_id,
            refund_no: refund_no.to_owned(),
            order_id,
            tenant_id: tenant_id.to_owned(),
            organization_id: organization_id.map(str::to_owned),
            status: current_status,
            amount,
        }));
    };
    let Some((event_type, from_status)) = refund_status_transition(&current_status, target_status)
    else {
        // Terminal-preserved replay (e.g. succeeded → succeeded) reports the
        // current state without rewriting history.
        return Ok(Some(IngestedRefundContext {
            refund_id,
            refund_no: refund_no.to_owned(),
            order_id,
            tenant_id: tenant_id.to_owned(),
            organization_id: organization_id.map(str::to_owned),
            status: current_status,
            amount,
        }));
    };

    sqlx::query(
        r#"
        UPDATE commerce_refund
        SET status = $1,
            version = version + 1,
            updated_at = $2::timestamptz
        WHERE id = $3
          AND tenant_id = $4
          AND status = $5
        "#,
    )
    .bind(target_status)
    .bind(now)
    .bind(&refund_id)
    .bind(tenant_id)
    .bind(&current_status)
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error("failed to advance refund webhook status", error))?;

    // The webhook audit key includes the transition so distinct status
    // changes on one refund each produce their own event; replays of the same
    // transition still deduplicate (same from→to).
    let webhook_event_key = format!("webhook:{refund_no}:{from_status}:{target_status}");
    insert_refund_event(
        tx,
        tenant_id,
        organization_id,
        &refund_id,
        event_type,
        Some(from_status.as_str()),
        target_status,
        "system",
        Some("refund-webhook"),
        &webhook_event_key,
        &webhook_event_key,
        now,
    )
    .await?;

    Ok(Some(IngestedRefundContext {
        refund_id,
        refund_no: refund_no.to_owned(),
        order_id,
        tenant_id: tenant_id.to_owned(),
        organization_id: organization_id.map(str::to_owned),
        status: target_status.to_owned(),
        amount,
    }))
}

/// Maps the raw provider refund status (refund_status field value or event
/// type suffix) to a commerce refund status; `None` keeps the refund
/// untouched but still reports its current state.
fn map_provider_refund_status_raw(raw_status: Option<&str>) -> Option<&'static str> {
    let raw_status = raw_status
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    map_provider_refund_status("wechat_pay", raw_status)
        .or_else(|| map_provider_refund_status("stripe", raw_status))
        .or_else(|| map_provider_refund_status("alipay", raw_status))
        .or_else(|| map_provider_refund_status("sandbox", raw_status))
        .or_else(|| {
            // Event-type suffix inference: REFUND.SUCCESS / REFUND.CLOSED /
            // REFUND.ABNORMAL carry the outcome when no status field exists.
            match raw_status.to_ascii_uppercase().as_str() {
                "REFUND.SUCCESS" | "REFUND_SUCCESS" => Some(REFUND_STATUS_SUCCEEDED),
                "REFUND.CLOSED" | "REFUND.ABNORMAL" | "REFUND_FAILED" => Some(REFUND_STATUS_FAILED),
                "REFUND.PROCESSING" => Some(REFUND_STATUS_PROCESSING),
                _ => None,
            }
        })
}

/// Validated refund state transition: returns the event type and from-status
/// when the transition is allowed, or `None` for terminal-preserved replays.
fn refund_status_transition(current: &str, target: &str) -> Option<(&'static str, String)> {
    match (current, target) {
        (REFUND_STATUS_SUCCEEDED, _) => None,
        (REFUND_STATUS_CANCELED, _) => None,
        (_, REFUND_STATUS_SUCCEEDED) => {
            Some((REFUND_EVENT_TYPE_STATUS_CHANGED, current.to_owned()))
        }
        (_, REFUND_STATUS_CANCELED) => Some((REFUND_EVENT_TYPE_STATUS_CHANGED, current.to_owned())),
        (REFUND_STATUS_FAILED, REFUND_STATUS_PROCESSING) => {
            Some((REFUND_EVENT_TYPE_STATUS_CHANGED, current.to_owned()))
        }
        (REFUND_STATUS_FAILED, REFUND_STATUS_FAILED) => None,
        (_, REFUND_STATUS_FAILED) => Some((REFUND_EVENT_TYPE_STATUS_CHANGED, current.to_owned())),
        (REFUND_STATUS_PROCESSING, REFUND_STATUS_PROCESSING) => None,
        (_, REFUND_STATUS_PROCESSING) => {
            Some((REFUND_EVENT_TYPE_STATUS_CHANGED, current.to_owned()))
        }
        _ => None,
    }
}

/// Loads the stored webhook event payload JSON (for replay forensics and
/// re-application) within the ingestion transaction.
async fn load_stored_webhook_payload_json(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: &str,
    organization_id: Option<&str>,
    provider_scoped_event_id: &str,
) -> Result<Option<Value>, CommerceServiceError> {
    let row = sqlx::query(
        r#"
        SELECT payload
        FROM commerce_payment_webhook_event
        WHERE tenant_id = CAST($1 AS TEXT)
          AND event_id = CAST($2 AS TEXT)
          AND ((organization_id = CAST($3 AS TEXT)) OR (organization_id IS NULL AND $3 IS NULL) OR (organization_id = '0' AND $3 IS NULL))
        FOR UPDATE
        "#,
    )
    .bind(tenant_id)
    .bind(provider_scoped_event_id)
    .bind(organization_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| store_error("failed to load stored refund webhook payload", error))?;
    let Some(row) = row else {
        return Ok(None);
    };
    let payload: Value = row
        .try_get("payload")
        .map_err(|error| store_error("failed to decode stored refund webhook payload", error))?;
    Ok(Some(payload))
}

async fn load_payment_webhook_attempt_context_by_identity(
    tx: &mut Transaction<'_, Postgres>,
    identity: &PaymentWebhookAttemptIdentity,
    provider_code: &str,
) -> Option<PaymentWebhookAttemptContext> {
    crate::payment_attempt_context::load_payment_webhook_attempt_context_by_id_postgres(
        tx,
        &identity.payment_attempt_id,
        provider_code,
        Some(&identity.tenant_id),
        identity.organization_id.as_deref(),
    )
    .await
    .ok()
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_wechat_refund_facts() {
        let facts = parse_refund_notify_facts(&json!({
            "out_refund_no": "RF-123",
            "refund_status": "SUCCESS",
            "refund_amount": 100,
        }));
        assert_eq!(Some("RF-123".to_owned()), facts.refund_no);
        assert_eq!(Some("SUCCESS".to_owned()), facts.refund_status);
        assert_eq!(Some("100".to_owned()), facts.refund_amount);
    }

    #[test]
    fn parses_stripe_refund_facts_with_camel_case() {
        let facts = parse_refund_notify_facts(&json!({
            "refundId": "re_123",
            "status": "succeeded",
            "amount": "50",
        }));
        assert_eq!(Some("re_123".to_owned()), facts.refund_no);
        assert_eq!(Some("succeeded".to_owned()), facts.refund_status);
        assert_eq!(Some("50".to_owned()), facts.refund_amount);
    }

    #[test]
    fn parses_alipay_out_biz_no() {
        let facts = parse_refund_notify_facts(&json!({
            "out_biz_no": "RF-456",
            "refund_status": "refund_success",
        }));
        assert_eq!(Some("RF-456".to_owned()), facts.refund_no);
        assert_eq!(Some("refund_success".to_owned()), facts.refund_status);
    }

    #[test]
    fn parses_wechat_refund_envelope_under_resource_plaintext() {
        let facts = parse_refund_notify_facts(&json!({
            "id": "evt-1",
            "event_type": "REFUND.SUCCESS",
            "resource_plaintext": {
                "out_refund_no": "RF-789",
                "refund_status": "SUCCESS",
                "refund_amount": 880,
            },
        }));
        assert_eq!(Some("RF-789".to_owned()), facts.refund_no);
        assert_eq!(Some("SUCCESS".to_owned()), facts.refund_status);
        assert_eq!(Some("880".to_owned()), facts.refund_amount);
    }

    #[test]
    fn parses_stripe_refund_object_with_metadata_refund_no() {
        let facts = parse_refund_notify_facts(&json!({
            "type": "charge.refunded",
            "data": {
                "object": {
                    "id": "re_456",
                    "status": "succeeded",
                    "amount": 500,
                    "metadata": { "refund_no": "RF-456" },
                }
            },
        }));
        assert_eq!(Some("RF-456".to_owned()), facts.refund_no);
        assert_eq!(Some("succeeded".to_owned()), facts.refund_status);
        assert_eq!(Some("500".to_owned()), facts.refund_amount);
    }

    #[test]
    fn stripe_object_id_falls_back_when_metadata_missing() {
        let facts = parse_refund_notify_facts(&json!({
            "data": { "object": { "id": "re_999", "status": "failed" } },
        }));
        assert_eq!(Some("re_999".to_owned()), facts.refund_no);
        assert_eq!(Some("failed".to_owned()), facts.refund_status);
    }

    #[test]
    fn empty_payload_yields_no_facts() {
        let facts = parse_refund_notify_facts(&json!({}));
        assert_eq!(None, facts.refund_no);
        assert_eq!(None, facts.refund_status);
        assert_eq!(None, facts.refund_amount);
    }

    #[test]
    fn status_mapping_prefers_status_field_and_infers_from_event_type() {
        assert_eq!(
            Some(REFUND_STATUS_SUCCEEDED),
            map_provider_refund_status_raw(Some("SUCCESS"))
        );
        assert_eq!(
            Some(REFUND_STATUS_SUCCEEDED),
            map_provider_refund_status_raw(Some("REFUND.SUCCESS"))
        );
        assert_eq!(
            Some(REFUND_STATUS_FAILED),
            map_provider_refund_status_raw(Some("REFUND.CLOSED"))
        );
        assert_eq!(
            Some(REFUND_STATUS_PROCESSING),
            map_provider_refund_status_raw(Some("processing"))
        );
        assert_eq!(None, map_provider_refund_status_raw(None));
        assert_eq!(None, map_provider_refund_status_raw(Some("mystery")));
    }

    #[test]
    fn refund_transition_guards_terminal_states() {
        assert!(refund_status_transition("processing", "succeeded").is_some());
        assert!(refund_status_transition("submitted", "failed").is_some());
        assert!(refund_status_transition("failed", "processing").is_some());
        assert_eq!(None, refund_status_transition("succeeded", "failed"));
        assert_eq!(None, refund_status_transition("succeeded", "succeeded"));
        assert_eq!(None, refund_status_transition("canceled", "succeeded"));
        assert_eq!(None, refund_status_transition("failed", "failed"));
    }
}
