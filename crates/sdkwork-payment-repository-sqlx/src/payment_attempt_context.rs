use std::collections::BTreeMap;
use sdkwork_contract_service::CommerceServiceError;
use serde_json::json;
use sqlx::{Pool, Postgres, Row, };
use crate::shared::{current_timestamp_string, store_error, string_cell};
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentAttemptProviderContext {
    pub attempt_id: String,
    pub channel_id: Option<String>,
    pub provider_account_id: Option<String>,
    pub provider_code: String,
    pub out_trade_no: String,
    pub amount: String,
    pub tenant_id: String,
    pub idempotency_key: String,
    pub provider_transaction_id: Option<String>,
    pub payment_metadata: serde_json::Value,
}
fn provider_transaction_id_from_callback_payload(payload: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(payload).ok()?;
    let raw = value
        .get("providerTransactionId")
        .or_else(|| value.get("provider_transaction_id"))?;
    let text = raw.as_str()?.trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_owned())
    }
}
fn provider_account_id_from_callback_payload(payload: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(payload).ok()?;
    let text = value.get("providerAccountId")?.as_str()?.trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_owned())
    }
}
fn payment_metadata_from_callback_payload(payload: &str) -> serde_json::Value {
    let value = serde_json::from_str(payload)
        .ok()
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| json!({}));
    value
        .get("paymentMetadata")
        .filter(|metadata| metadata.is_object())
        .cloned()
        .unwrap_or(value)
}
fn provider_enrichment_patch(payment_params: &BTreeMap<String, String>) -> String {
    let mut value = json!({});
    let obj = value
        .as_object_mut()
        .expect("provider enrichment patch is an object");
    if let Some(native_id) = payment_params.get("providerTransactionId") {
        obj.insert("providerTransactionId".to_owned(), json!(native_id));
    }
    if let Some(status) = payment_params.get("providerStatus") {
        obj.insert("providerStatus".to_owned(), json!(status));
    }
    value.to_string()
}
pub async fn persist_attempt_enrichment_postgres(
    pool: &Pool<Postgres>,
    tenant_id: &str,
    attempt_id: &str,
    payment_params: &BTreeMap<String, String>,
) -> Result<(), CommerceServiceError> {
    if payment_params.get("providerTransactionId").is_none()
        && payment_params.get("providerStatus").is_none()
    {
        return Ok(());
    }
    let patch = provider_enrichment_patch(payment_params);
    let now = current_timestamp_string();
    let update = sqlx::query(
        r#"
        UPDATE commerce_payment_attempt
        SET callback_payload = COALESCE(callback_payload, '{}'::jsonb) || $1::jsonb,
            updated_at = $2::timestamptz
        WHERE id = CAST($3 AS TEXT)
          AND tenant_id = CAST($4 AS TEXT)
          AND deleted_at IS NULL
        "#,
    )
    .bind(&patch)
    .bind(&now)
    .bind(attempt_id)
    .bind(tenant_id)
    .execute(pool)
    .await
    .map_err(|error| store_error("failed to persist payment attempt enrichment", error))?;
    ensure_attempt_enrichment_persisted(update.rows_affected())?;
    Ok(())
}
fn ensure_attempt_enrichment_persisted(rows_affected: u64) -> Result<(), CommerceServiceError> {
    if rows_affected == 1 {
        Ok(())
    } else {
        Err(CommerceServiceError::conflict(
            "payment attempt is no longer available for provider enrichment",
        ))
    }
}
pub async fn load_payment_attempt_provider_context_postgres(
    pool: &Pool<Postgres>,
    tenant_id: &str,
    owner_user_id: &str,
    payment_id: &str,
) -> Result<Option<PaymentAttemptProviderContext>, CommerceServiceError> {
    let row = sqlx::query(
        r#"
        SELECT pa.id, pa.channel_id, c.provider_account_id, pa.provider_code, pa.out_trade_no,
               pa.amount, pa.tenant_id, pa.callback_payload, pa.idempotency_key
        FROM commerce_payment_attempt pa
        LEFT JOIN commerce_payment_channel c ON c.id = pa.channel_id
        WHERE pa.tenant_id = CAST($1 AS TEXT)
          AND pa.owner_user_id = CAST($2 AS TEXT)
          AND pa.id = CAST($3 AS TEXT)
          AND pa.deleted_at IS NULL
        "#,
    )
    .bind(tenant_id)
    .bind(owner_user_id)
    .bind(payment_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| store_error("failed to load payment attempt provider context", error))?;
    Ok(row.map(|row| {
        let callback_payload = string_cell(&row, "callback_payload");
        PaymentAttemptProviderContext {
            attempt_id: string_cell(&row, "id"),
            channel_id: row.try_get("channel_id").ok().flatten(),
            provider_account_id: provider_account_id_from_callback_payload(&callback_payload)
                .or_else(|| row.try_get("provider_account_id").ok().flatten()),
            provider_code: string_cell(&row, "provider_code"),
            out_trade_no: string_cell(&row, "out_trade_no"),
            amount: string_cell(&row, "amount"),
            tenant_id: string_cell(&row, "tenant_id"),
            idempotency_key: string_cell(&row, "idempotency_key"),
            provider_transaction_id: provider_transaction_id_from_callback_payload(
                &callback_payload,
            ),
            payment_metadata: payment_metadata_from_callback_payload(&callback_payload),
        }
    }))
}
pub async fn load_payment_attempt_provider_context_by_id_postgres(
    pool: &Pool<Postgres>,
    payment_attempt_id: &str,
) -> Result<Option<PaymentAttemptProviderContext>, CommerceServiceError> {
    let row = sqlx::query(
        r#"
        SELECT pa.id, pa.channel_id, c.provider_account_id, pa.provider_code, pa.out_trade_no,
               pa.amount, pa.tenant_id, pa.callback_payload, pa.idempotency_key
        FROM commerce_payment_attempt pa
        LEFT JOIN commerce_payment_channel c ON c.id = pa.channel_id
        WHERE pa.id = CAST($1 AS TEXT)
          AND pa.deleted_at IS NULL
        "#,
    )
    .bind(payment_attempt_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| {
        store_error(
            "failed to load payment attempt provider context by id",
            error,
        )
    })?;
    Ok(row.map(|row| {
        let callback_payload = string_cell(&row, "callback_payload");
        PaymentAttemptProviderContext {
            attempt_id: string_cell(&row, "id"),
            channel_id: row.try_get("channel_id").ok().flatten(),
            provider_account_id: provider_account_id_from_callback_payload(&callback_payload)
                .or_else(|| row.try_get("provider_account_id").ok().flatten()),
            provider_code: string_cell(&row, "provider_code"),
            out_trade_no: string_cell(&row, "out_trade_no"),
            amount: string_cell(&row, "amount"),
            tenant_id: string_cell(&row, "tenant_id"),
            idempotency_key: string_cell(&row, "idempotency_key"),
            provider_transaction_id: provider_transaction_id_from_callback_payload(
                &callback_payload,
            ),
            payment_metadata: payment_metadata_from_callback_payload(&callback_payload),
        }
    }))
}
/// Payment-attempt context returned after webhook ingest (no order-table join).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentWebhookAttemptContext {
    pub payment_attempt_id: String,
    pub out_trade_no: String,
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub owner_user_id: String,
    pub order_id: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PaymentWebhookAttemptIdentity {
    pub payment_attempt_id: String,
    pub payment_intent_id: String,
    pub provider_code: String,
    pub out_trade_no: String,
    pub attempt_status: String,
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub owner_user_id: String,
    pub order_id: String,
}
pub(crate) async fn load_payment_webhook_attempt_context_by_id_postgres(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    payment_attempt_id: &str,
    provider_code: &str,
    tenant_id: Option<&str>,
    organization_id: Option<&str>,
) -> Result<Option<PaymentWebhookAttemptContext>, CommerceServiceError> {
    let row = sqlx::query(
        r#"
        SELECT id, out_trade_no, tenant_id, organization_id, owner_user_id, order_id
        FROM commerce_payment_attempt
        WHERE id = CAST($1 AS TEXT)
          AND provider_code = CAST($2 AS TEXT)
          AND tenant_id = CAST($3 AS TEXT)
          AND ((organization_id = CAST($4 AS TEXT)) OR (organization_id IS NULL AND $4 IS NULL) OR (organization_id = '0' AND $4 IS NULL))
          AND deleted_at IS NULL
        "#,
    )
    .bind(payment_attempt_id)
    .bind(provider_code)
    .bind(tenant_id)
    .bind(organization_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| store_error("failed to resolve payment webhook attempt context", error))?;
    Ok(row.map(|row| PaymentWebhookAttemptContext {
        payment_attempt_id: string_cell(&row, "id"),
        out_trade_no: string_cell(&row, "out_trade_no"),
        tenant_id: string_cell(&row, "tenant_id"),
        organization_id: row.try_get("organization_id").ok().flatten(),
        owner_user_id: string_cell(&row, "owner_user_id"),
        order_id: string_cell(&row, "order_id"),
    }))
}
pub(crate) async fn load_attempt_by_out_trade_no_postgres(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    provider_code: &str,
    out_trade_no: &str,
    tenant_id: Option<&str>,
    organization_id: Option<&str>,
) -> Result<Option<PaymentWebhookAttemptIdentity>, CommerceServiceError> {
    let rows = sqlx::query(
        r#"
        SELECT id, payment_intent_id, provider_code, out_trade_no, status, tenant_id,
               organization_id, owner_user_id, order_id
        FROM commerce_payment_attempt
        WHERE provider_code = CAST($1 AS TEXT)
          AND out_trade_no = CAST($2 AS TEXT)
          AND ($3::text IS NULL OR tenant_id = CAST($3 AS TEXT))
          AND ($3::text IS NULL OR organization_id = CAST($4 AS TEXT)
               OR (organization_id IS NULL AND $4::text IS NULL) OR (organization_id = '0' AND $4::text IS NULL))
          AND deleted_at IS NULL
        ORDER BY id
        LIMIT 2
        "#,
    )
    .bind(provider_code)
    .bind(out_trade_no)
    .bind(tenant_id)
    .bind(organization_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| store_error("failed to resolve webhook tenant", error))?;
    match rows.as_slice() {
        [] => Ok(None),
        [_first, _second, ..] => Err(CommerceServiceError::conflict(
            "multiple payment attempts match webhook identity",
        )),
        [row] => Ok(Some(map_webhook_attempt_identity(row))),
    }
}
fn map_webhook_attempt_identity<R: crate::shared::StringCellRow>(
    row: &R,
) -> PaymentWebhookAttemptIdentity {
    let organization_id = string_cell(row, "organization_id");
    PaymentWebhookAttemptIdentity {
        payment_attempt_id: string_cell(row, "id"),
        payment_intent_id: string_cell(row, "payment_intent_id"),
        provider_code: string_cell(row, "provider_code"),
        out_trade_no: string_cell(row, "out_trade_no"),
        attempt_status: string_cell(row, "status"),
        tenant_id: string_cell(row, "tenant_id"),
        organization_id: (!organization_id.is_empty()).then_some(organization_id),
        owner_user_id: string_cell(row, "owner_user_id"),
        order_id: string_cell(row, "order_id"),
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebhookAttemptContext {
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub provider_code: String,
}
pub async fn load_webhook_attempt_context_by_out_trade_no_postgres(
    pool: &Pool<Postgres>,
    provider_code: &str,
    out_trade_no: &str,
) -> Result<Option<WebhookAttemptContext>, CommerceServiceError> {
    let rows = sqlx::query(
        r#"
        SELECT tenant_id, organization_id, provider_code
        FROM commerce_payment_attempt
        WHERE provider_code = CAST($1 AS TEXT)
          AND out_trade_no = CAST($2 AS TEXT)
          AND deleted_at IS NULL
        ORDER BY id
        LIMIT 2
        "#,
    )
    .bind(provider_code)
    .bind(out_trade_no)
    .fetch_all(pool)
    .await
    .map_err(|error| store_error("failed to load webhook attempt context", error))?;
    match rows.as_slice() {
        [] => Ok(None),
        [_first, _second, ..] => Err(CommerceServiceError::conflict(
            "multiple payment attempts match webhook identity",
        )),
        [row] => Ok(Some(WebhookAttemptContext {
            tenant_id: string_cell(row, "tenant_id"),
            organization_id: row.try_get("organization_id").ok().flatten(),
            provider_code: string_cell(row, "provider_code"),
        })),
    }
}