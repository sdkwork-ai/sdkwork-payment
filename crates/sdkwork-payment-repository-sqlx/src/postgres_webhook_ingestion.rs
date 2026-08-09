use sdkwork_contract_service::CommerceServiceError;
use sqlx::{Pool, Postgres, Row};
use crate::payment_attempt_context::{
    load_attempt_by_out_trade_no_postgres, load_payment_webhook_attempt_context_by_id_postgres,
    PaymentWebhookAttemptContext, PaymentWebhookAttemptIdentity,
};
use crate::shared::{current_timestamp_string, store_error, string_cell};
use crate::webhook_event_payload::{
    build_stored_webhook_payload, parse_stored_webhook_payload, provider_scoped_webhook_event_id,
    validate_stored_webhook_scope, webhook_event_storage_id, StoredWebhookPayload,
    WebhookEventInsert, WebhookEventPayloadInput, WEBHOOK_EVENT_STATUS_FAILED,
    WEBHOOK_EVENT_STATUS_PROCESSED, WEBHOOK_EVENT_STATUS_QUEUED,
};
use crate::webhook_status::map_provider_payment_status;


#[derive(Clone, Debug)]
pub struct IngestProviderWebhookCommand {
    pub provider_code: String,
    pub provider_event_id: String,
    pub event_type: Option<String>,
    pub out_trade_no: Option<String>,
    pub payment_status: Option<String>,
    pub payload: serde_json::Value,
    /// Scope for persisting unmatched events when `out_trade_no` cannot be resolved.
    pub tenant_id: Option<String>,
    pub organization_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IngestProviderWebhookOutcome {
    pub webhook_event_id: String,
    pub replayed: bool,
    pub payment_attempt_id: Option<String>,
    pub applied_status: Option<String>,
    pub payment_attempt_context: Option<PaymentWebhookAttemptContext>,
}

pub fn empty_ingest_outcome(
    webhook_event_id: String,
    replayed: bool,
) -> IngestProviderWebhookOutcome {
    IngestProviderWebhookOutcome {
        webhook_event_id,
        replayed,
        payment_attempt_id: None,
        applied_status: None,
        payment_attempt_context: None,
    }
}

async fn persist_webhook_event_postgres(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    insert: &WebhookEventInsert<'_>,
) -> Result<bool, CommerceServiceError> {
    let result = sqlx::query(
        r#"
        INSERT INTO commerce_payment_webhook_event
            (id, tenant_id, organization_id, event_id, event_type, provider_code, payload, status,
             last_error, received_at, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7::jsonb, $8, $9,
                $10::timestamptz, $10::timestamptz, $10::timestamptz)
        ON CONFLICT (tenant_id, event_id) DO NOTHING
        "#,
    )
    .bind(insert.internal_id)
    .bind(insert.tenant_id)
    .bind(insert.organization_id)
    .bind(insert.provider_scoped_event_id)
    .bind(insert.event_type)
    .bind(insert.provider_code)
    .bind(insert.payload_json)
    .bind(insert.status)
    .bind(insert.last_error)
    .bind(insert.now)
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error("failed to insert webhook event", error))?;
    Ok(result.rows_affected() == 1)
}
pub async fn ingest_provider_webhook_postgres(
    pool: &Pool<Postgres>,
    command: IngestProviderWebhookCommand,
) -> Result<IngestProviderWebhookOutcome, CommerceServiceError> {
    let provider_code = command.provider_code.trim().to_ascii_lowercase();
    if provider_code.is_empty() {
        return Err(CommerceServiceError::validation(
            "payment webhook provider code is required",
        ));
    }
    let provider_event_id = command.provider_event_id.trim();
    if provider_event_id.is_empty() {
        return Err(CommerceServiceError::validation(
            "payment webhook provider event id is required",
        ));
    }
    let now = current_timestamp_string();
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| store_error("failed to begin webhook ingestion transaction", error))?;
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
            "payment webhook organization scope requires tenant scope",
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
            CommerceServiceError::conflict(
                "payment webhook tenant scope could not be resolved safely",
            )
        })?;
    let organization_id = attempt_identity
        .as_ref()
        .and_then(|identity| identity.organization_id.clone())
        .or_else(|| command_organization_id.map(str::to_owned));
    let provider_scoped_event_id =
        provider_scoped_webhook_event_id(&provider_code, provider_event_id);
    let internal_id = webhook_event_storage_id(&tenant_id, &provider_scoped_event_id);
    let unmatched_reason = if attempt_identity.is_none() {
        Some(if out_trade_no.is_some() {
            "payment_attempt_not_found"
        } else {
            "out_trade_no_missing"
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
    let proposed_event_status = if attempt_identity.is_some() {
        WEBHOOK_EVENT_STATUS_QUEUED
    } else {
        WEBHOOK_EVENT_STATUS_FAILED
    };
    let insert = WebhookEventInsert {
        internal_id: &internal_id,
        tenant_id: &tenant_id,
        organization_id: organization_id.as_deref(),
        provider_scoped_event_id: &provider_scoped_event_id,
        event_type: command.event_type.as_deref().unwrap_or("payment"),
        provider_code: &provider_code,
        payload_json: &payload_json,
        status: proposed_event_status,
        last_error: unmatched_reason,
        now: &now,
    };
    let inserted = persist_webhook_event_postgres(&mut tx, &insert).await?;
    let (attempt_identity, payment_status) = if inserted {
        (attempt_identity, command.payment_status.clone())
    } else {
        let stored = load_existing_webhook_event_postgres(
            &mut tx,
            &tenant_id,
            organization_id.as_deref(),
            &provider_scoped_event_id,
        )
        .await?;
        (stored.attempt_identity, stored.payment_status)
    };
    let Some(attempt_identity) = attempt_identity else {
        tx.commit()
            .await
            .map_err(|error| store_error("failed to commit unmatched webhook event", error))?;
        return Ok(empty_ingest_outcome(internal_id, !inserted));
    };
    let applied_status = apply_webhook_payment_status_postgres(
        &mut tx,
        &attempt_identity,
        payment_status.as_deref(),
        &now,
    )
    .await?;
    let payment_attempt_id = Some(attempt_identity.payment_attempt_id.clone());
    let payment_attempt_context = if applied_status.as_deref() == Some("succeeded") {
        load_payment_webhook_attempt_context_by_id_postgres(
            &mut tx,
            &attempt_identity.payment_attempt_id,
            &attempt_identity.provider_code,
            Some(&attempt_identity.tenant_id),
            attempt_identity.organization_id.as_deref(),
        )
        .await?
    } else {
        None
    };
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
    .map_err(|error| store_error("failed to mark webhook event processed", error))?;
    tx.commit()
        .await
        .map_err(|error| store_error("failed to commit webhook ingestion transaction", error))?;
    Ok(IngestProviderWebhookOutcome {
        webhook_event_id: internal_id,
        replayed: !inserted,
        payment_attempt_id,
        applied_status,
        payment_attempt_context,
    })
}
async fn load_existing_webhook_event_postgres(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    tenant_id: &str,
    organization_id: Option<&str>,
    provider_scoped_event_id: &str,
) -> Result<StoredWebhookPayload, CommerceServiceError> {
    let row = sqlx::query(
        r#"
        SELECT provider_code, payload
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
    .map_err(|error| store_error("failed to load existing webhook event", error))?
    .ok_or_else(|| {
        CommerceServiceError::conflict(
            "webhook idempotency identity conflicts with another organization scope",
        )
    })?;
    let provider_code = string_cell(&row, "provider_code");
    let payload: serde_json::Value = row
        .try_get("payload")
        .map_err(|error| store_error("failed to decode stored webhook payload", error))?;
    let stored = parse_stored_webhook_payload(&payload.to_string())?;
    validate_stored_webhook_scope(
        &stored,
        provider_scoped_event_id,
        &provider_code,
        tenant_id,
        organization_id,
    )?;
    Ok(stored)
}
pub(crate) async fn apply_webhook_payment_status_postgres(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    identity: &PaymentWebhookAttemptIdentity,
    payment_status: Option<&str>,
    now: &str,
) -> Result<Option<String>, CommerceServiceError> {
    let row = sqlx::query(
        r#"
        SELECT id, payment_intent_id, tenant_id, organization_id, owner_user_id, order_id, status
        FROM commerce_payment_attempt
        WHERE id = CAST($1 AS TEXT)
          AND payment_intent_id = CAST($2 AS TEXT)
          AND provider_code = CAST($3 AS TEXT)
          AND out_trade_no = CAST($4 AS TEXT)
          AND tenant_id = CAST($5 AS TEXT)
          AND ((organization_id = CAST($6 AS TEXT)) OR (organization_id IS NULL AND $7 IS NULL) OR (organization_id = '0' AND $7 IS NULL))
          AND owner_user_id = CAST($8 AS TEXT)
          AND order_id = CAST($9 AS TEXT)
          AND deleted_at IS NULL
        "#,
    )
    .bind(&identity.payment_attempt_id)
    .bind(&identity.payment_intent_id)
    .bind(&identity.provider_code)
    .bind(&identity.out_trade_no)
    .bind(&identity.tenant_id)
    .bind(identity.organization_id.as_deref())
    .bind(identity.organization_id.as_deref())
    .bind(&identity.owner_user_id)
    .bind(&identity.order_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| store_error("failed to load exact payment attempt for webhook", error))?
    .ok_or_else(|| {
        CommerceServiceError::conflict(
            "stored webhook payment attempt identity no longer exists",
        )
    })?;
    let attempt_id = string_cell(&row, "id");
    let payment_intent_id = string_cell(&row, "payment_intent_id");
    let resolved_tenant_id = string_cell(&row, "tenant_id");
    let resolved_organization_id: Option<String> = row.try_get("organization_id").ok().flatten();
    let owner_user_id = string_cell(&row, "owner_user_id");
    let order_id = string_cell(&row, "order_id");
    let Some(raw_status) = payment_status.filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };
    let Some(target_status) = map_provider_payment_status(&identity.provider_code, raw_status)
    else {
        return Ok(None);
    };
    let order_row = sqlx::query(
        r#"
        SELECT id
        FROM commerce_order
        WHERE tenant_id = CAST($1 AS TEXT)
          AND ((organization_id = CAST($2 AS TEXT)) OR (organization_id IS NULL AND $3 IS NULL) OR (organization_id = '0' AND $3 IS NULL))
          AND owner_user_id = CAST($4 AS TEXT)
          AND id = CAST($5 AS TEXT)
        FOR UPDATE
        "#,
    )
    .bind(&resolved_tenant_id)
    .bind(resolved_organization_id.as_deref())
    .bind(resolved_organization_id.as_deref())
    .bind(&owner_user_id)
    .bind(&order_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| store_error("failed to lock webhook owner order", error))?;
    if order_row.is_none() {
        return Err(CommerceServiceError::storage(
            "payment webhook attempt references a missing or deleted order",
        ));
    }
    let identity_rows = sqlx::query(
        r#"
        SELECT id, payment_intent_id
        FROM commerce_payment_attempt
        WHERE provider_code = CAST($1 AS TEXT)
          AND out_trade_no = CAST($2 AS TEXT)
          AND tenant_id = CAST($3 AS TEXT)
          AND ((organization_id = CAST($4 AS TEXT)) OR (organization_id IS NULL AND $5 IS NULL) OR (organization_id = '0' AND $5 IS NULL))
          AND deleted_at IS NULL
        ORDER BY id
        LIMIT 2
        "#,
    )
    .bind(&identity.provider_code)
    .bind(&identity.out_trade_no)
    .bind(&resolved_tenant_id)
    .bind(resolved_organization_id.as_deref())
    .bind(resolved_organization_id.as_deref())
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| store_error("failed to revalidate webhook identity", error))?;
    match identity_rows.as_slice() {
        [identity]
            if string_cell(identity, "id") == attempt_id
                && string_cell(identity, "payment_intent_id") == payment_intent_id => {}
        [] => {
            return Err(CommerceServiceError::storage(
                "payment webhook attempt disappeared during settlement",
            ));
        }
        _ => {
            return Err(CommerceServiceError::conflict(
                "multiple payment attempts match webhook identity",
            ));
        }
    }
    let intent_row = sqlx::query(
        r#"
        SELECT status
        FROM commerce_payment_intent
        WHERE id = CAST($1 AS TEXT)
          AND tenant_id = CAST($2 AS TEXT)
          AND ((organization_id = CAST($3 AS TEXT)) OR (organization_id IS NULL AND $4 IS NULL) OR (organization_id = '0' AND $4 IS NULL))
          AND owner_user_id = CAST($5 AS TEXT)
          AND order_id = CAST($6 AS TEXT)
          AND deleted_at IS NULL
        FOR UPDATE
        "#,
    )
    .bind(&payment_intent_id)
    .bind(&resolved_tenant_id)
    .bind(resolved_organization_id.as_deref())
    .bind(resolved_organization_id.as_deref())
    .bind(&owner_user_id)
    .bind(&order_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| store_error("failed to lock webhook payment intent", error))?
    .ok_or_else(|| {
        CommerceServiceError::storage(
            "payment webhook attempt references a missing or deleted payment intent",
        )
    })?;
    let intent_status = string_cell(&intent_row, "status");
    let attempt_row = sqlx::query(
        r#"
        SELECT status
        FROM commerce_payment_attempt
        WHERE id = CAST($1 AS TEXT)
          AND payment_intent_id = CAST($2 AS TEXT)
          AND provider_code = CAST($3 AS TEXT)
          AND out_trade_no = CAST($4 AS TEXT)
          AND tenant_id = CAST($5 AS TEXT)
          AND ((organization_id = CAST($6 AS TEXT)) OR (organization_id IS NULL AND $7 IS NULL) OR (organization_id = '0' AND $7 IS NULL))
          AND owner_user_id = CAST($8 AS TEXT)
          AND order_id = CAST($9 AS TEXT)
          AND deleted_at IS NULL
        FOR UPDATE
        "#,
    )
    .bind(&attempt_id)
    .bind(&payment_intent_id)
    .bind(&identity.provider_code)
    .bind(&identity.out_trade_no)
    .bind(&resolved_tenant_id)
    .bind(resolved_organization_id.as_deref())
    .bind(resolved_organization_id.as_deref())
    .bind(&owner_user_id)
    .bind(&order_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| store_error("failed to lock webhook payment attempt", error))?
    .ok_or_else(|| {
        CommerceServiceError::storage("payment webhook attempt changed identity during settlement")
    })?;
    let attempt_status = string_cell(&attempt_row, "status");
    if !intent_status.eq_ignore_ascii_case(target_status) {
        crate::shared::ensure_payment_status_transition(&intent_status, target_status)?;
        let updated = sqlx::query(
            r#"
            UPDATE commerce_payment_intent
            SET status = $1, updated_at = $2::timestamptz
            WHERE id = CAST($3 AS TEXT)
              AND tenant_id = CAST($4 AS TEXT)
              AND deleted_at IS NULL
            "#,
        )
        .bind(target_status)
        .bind(now)
        .bind(&payment_intent_id)
        .bind(&resolved_tenant_id)
        .execute(&mut **tx)
        .await
        .map_err(|error| store_error("failed to update payment intent from webhook", error))?;
        if updated.rows_affected() != 1 {
            return Err(CommerceServiceError::storage(
                "webhook payment intent update did not affect exactly one row",
            ));
        }
    }
    if !attempt_status.eq_ignore_ascii_case(target_status) {
        crate::shared::ensure_payment_status_transition(&attempt_status, target_status)?;
        let updated = sqlx::query(
            r#"
            UPDATE commerce_payment_attempt
            SET status = $1,
                updated_at = $2::timestamptz,
                paid_at = CASE
                    WHEN $1 = 'succeeded' THEN COALESCE(paid_at, $2::timestamptz)
                    ELSE paid_at
                END
            WHERE id = CAST($3 AS TEXT)
              AND payment_intent_id = CAST($4 AS TEXT)
              AND tenant_id = CAST($5 AS TEXT)
              AND deleted_at IS NULL
            "#,
        )
        .bind(target_status)
        .bind(now)
        .bind(&attempt_id)
        .bind(&payment_intent_id)
        .bind(&resolved_tenant_id)
        .execute(&mut **tx)
        .await
        .map_err(|error| store_error("failed to update payment attempt from webhook", error))?;
        if updated.rows_affected() != 1 {
            return Err(CommerceServiceError::storage(
                "webhook payment attempt update did not affect exactly one row",
            ));
        }
    }
    Ok(Some(target_status.to_owned()))
}