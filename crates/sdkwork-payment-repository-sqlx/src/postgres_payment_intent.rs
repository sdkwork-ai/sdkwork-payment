#![allow(clippy::too_many_arguments)]

use sdkwork_contract_service::{CommerceMoney, CommercePaymentStatus, CommerceServiceError};
use sdkwork_payment_service::{
    CancelOwnerPaymentIntentCommand, CreateOwnerPaymentAttemptCommand,
    CreateOwnerPaymentAttemptOutcome, CreateOwnerPaymentIntentCommand, OrderPaymentReferenceQuery,
    OrderPaymentReferenceSnapshot, PaymentIntentDetailQuery, PaymentIntentView,
};
use sqlx::{PgPool, Postgres, Row, Transaction};

use crate::order_reference::{
    load_order_payment_reference_postgres, order_payment_reference_is_payable,
};
use crate::payment_channel::select_payment_channel_postgres;
use crate::shared::{
    ensure_payment_attempt_idempotency_replay_matches,
    ensure_payment_intent_idempotency_replay_matches, normalize_stored_money_amount,
    organization_scope_bind,
};

#[derive(Debug, Clone)]
struct ResolvedPaymentMethod {
    method_key: String,
    provider_code: String,
}

/// Resolves the order currency for intent persistence, defaulting to `CNY`
/// when the order row carries none.
fn order_currency_code(order_ref: &OrderPaymentReferenceSnapshot) -> &str {
    order_ref
        .currency_code
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("CNY")
}

#[derive(Debug, Clone)]
pub struct PostgresCommercePaymentIntentStore {
    pool: PgPool,
}

impl PostgresCommercePaymentIntentStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

impl PostgresCommercePaymentIntentStore {
    pub async fn create_owner_payment_intent(
        &self,
        command: CreateOwnerPaymentIntentCommand,
    ) -> Result<PaymentIntentView, CommerceServiceError> {
        if let Some(existing) = self
            .find_owner_payment_intent_by_idempotency(&command)
            .await?
        {
            ensure_payment_intent_idempotency_replay_matches(&command, &existing)?;
            return Ok(existing);
        }

        let mut tx =
            self.pool().begin().await.map_err(|error| {
                store_error("failed to begin payment intent transaction", error)
            })?;
        let reference_query = OrderPaymentReferenceQuery::new(
            &command.tenant_id,
            command.organization_id.as_deref(),
            &command.owner_user_id,
            &command.order_id,
        )?;
        let Some(order_ref) =
            load_order_payment_reference_postgres(&mut tx, &reference_query).await?
        else {
            return Err(CommerceServiceError::not_found("order was not found"));
        };
        if !order_payment_reference_is_payable(&order_ref) {
            return Err(CommerceServiceError::conflict(
                "order is not pending payment",
            ));
        }
        let method = load_payment_method_for_intent(&mut tx, &command).await?;
        let now = current_timestamp_string();
        let payment_intent_id = payment_intent_id(&command);
        let status = CommercePaymentStatus::Pending.as_str();

        // The deterministic ID turns concurrent retries into one insert and one replay lookup.
        let inserted = sqlx::query(
            r#"
            INSERT INTO commerce_payment_intent
                (id, tenant_id, organization_id, owner_user_id, order_id, payment_intent_no,
                 payment_method, provider_code, amount, currency_code, status, request_no,
                 idempotency_key, created_at, updated_at)
            VALUES
                ($1, $2, $3, $4, $5, $6, $7, $8, $9::numeric, $10, $11, $12, $13, $14::timestamptz, $15::timestamptz)
            ON CONFLICT (id) DO NOTHING
           "#,
        )
        .bind(&payment_intent_id)
        .bind(&command.tenant_id)
        .bind(organization_scope_bind(&command.organization_id))
        .bind(&command.owner_user_id)
        .bind(&command.order_id)
        .bind(format!("PAY-{}", order_ref.order_sn))
        .bind(&method.method_key)
        .bind(&method.provider_code)
        .bind(order_ref.total_amount.as_str())
        .bind(order_currency_code(&order_ref))
        .bind(status)
        .bind(&command.request_no)
        .bind(&command.idempotency_key)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error("failed to insert payment intent", error))?;

        if inserted.rows_affected() == 0 {
            // 并发竞态下另一请求已写入，回查并返回已有记录，保证幂等语义。
            tx.commit().await.map_err(|error| {
                store_error(
                    "failed to commit payment intent transaction after conflict",
                    error,
                )
            })?;
            if let Some(existing) = self
                .find_owner_payment_intent_by_idempotency(&command)
                .await?
            {
                ensure_payment_intent_idempotency_replay_matches(&command, &existing)?;
                return Ok(existing);
            }
            return Err(CommerceServiceError::conflict(
                "payment intent idempotency conflict: existing record not found",
            ));
        }

        tx.commit()
            .await
            .map_err(|error| store_error("failed to commit payment intent transaction", error))?;

        Ok(PaymentIntentView {
            payment_intent_id,
            order_id: command.order_id,
            payment_intent_no: command.request_no,
            payment_method: method.method_key,
            provider_code: method.provider_code,
            currency_code: order_currency_code(&order_ref).to_owned(),
            amount: order_ref.total_amount,
            status: status.to_owned(),
        })
    }

    pub async fn retrieve_owner_payment_intent(
        &self,
        query: PaymentIntentDetailQuery,
    ) -> Result<Option<PaymentIntentView>, CommerceServiceError> {
        let row = sqlx::query(
            r#"
            SELECT id, order_id, payment_intent_no, payment_method, provider_code,
                   CAST(amount AS BIGINT)::TEXT AS amount, currency_code, status
            FROM commerce_payment_intent
            WHERE tenant_id = CAST($1 AS TEXT)
              AND ((organization_id = CAST($2 AS TEXT)) OR (organization_id IS NULL AND $3 IS NULL) OR (organization_id = '0' AND $3 IS NULL))
              AND owner_user_id = CAST($4 AS TEXT)
              AND id = CAST($5 AS TEXT)
            LIMIT 1
           "#,
        )
        .bind(&query.tenant_id)
        .bind(query.organization_id.as_deref())
        .bind(query.organization_id.as_deref())
        .bind(&query.owner_user_id)
        .bind(&query.payment_intent_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|error| store_error("failed to retrieve payment intent", error))?;

        row.map(map_payment_intent_row).transpose()
    }

    pub async fn cancel_owner_payment_intent(
        &self,
        command: CancelOwnerPaymentIntentCommand,
    ) -> Result<PaymentIntentView, CommerceServiceError> {
        let query = PaymentIntentDetailQuery::new(
            &command.tenant_id,
            command.organization_id.as_deref(),
            &command.owner_user_id,
            &command.payment_intent_id,
        )?;
        let Some(intent) = self.retrieve_owner_payment_intent(query).await? else {
            return Err(CommerceServiceError::not_found(
                "payment intent was not found",
            ));
        };
        if matches!(
            intent.status.to_ascii_lowercase().as_str(),
            "succeeded" | "closed" | "canceled" | "cancelled"
        ) {
            return Err(CommerceServiceError::conflict(
                "payment intent is not cancelable",
            ));
        }

        let now = current_timestamp_string();
        let canceled = CommercePaymentStatus::Canceled.as_str();
        crate::shared::ensure_payment_status_transition(&intent.status, canceled)?;
        sqlx::query(
            r#"
            UPDATE commerce_payment_intent
            SET status = $1, updated_at = $2::timestamptz
            WHERE tenant_id = CAST($3 AS TEXT)
              AND owner_user_id = CAST($4 AS TEXT)
              AND id = CAST($5 AS TEXT)
           "#,
        )
        .bind(canceled)
        .bind(&now)
        .bind(&command.tenant_id)
        .bind(&command.owner_user_id)
        .bind(&command.payment_intent_id)
        .execute(self.pool())
        .await
        .map_err(|error| store_error("failed to cancel payment intent", error))?;

        sqlx::query(
            r#"
            UPDATE commerce_payment_attempt
            SET status = $1, updated_at = $2::timestamptz
            WHERE tenant_id = CAST($3 AS TEXT)
              AND owner_user_id = CAST($4 AS TEXT)
              AND payment_intent_id = CAST($5 AS TEXT)
              AND LOWER(COALESCE(status, '')) IN ('created', 'pending', 'processing')
           "#,
        )
        .bind(canceled)
        .bind(&now)
        .bind(&command.tenant_id)
        .bind(&command.owner_user_id)
        .bind(&command.payment_intent_id)
        .execute(self.pool())
        .await
        .map_err(|error| store_error("failed to cancel payment attempts", error))?;

        Ok(PaymentIntentView {
            status: canceled.to_owned(),
            ..intent
        })
    }

    pub async fn create_owner_payment_attempt(
        &self,
        command: CreateOwnerPaymentAttemptCommand,
    ) -> Result<CreateOwnerPaymentAttemptOutcome, CommerceServiceError> {
        if let Some(existing) = self
            .find_owner_payment_attempt_by_idempotency(&command)
            .await?
        {
            ensure_payment_attempt_idempotency_replay_matches(&command, &existing)?;
            return Ok(existing);
        }

        let intent_query = PaymentIntentDetailQuery::new(
            &command.tenant_id,
            command.organization_id.as_deref(),
            &command.owner_user_id,
            &command.payment_intent_id,
        )?;
        let Some(intent) = self.retrieve_owner_payment_intent(intent_query).await? else {
            return Err(CommerceServiceError::not_found(
                "payment intent was not found",
            ));
        };
        if matches!(
            intent.status.to_ascii_lowercase().as_str(),
            "succeeded" | "closed" | "canceled" | "cancelled"
        ) {
            return Err(CommerceServiceError::conflict(
                "payment intent is not attemptable",
            ));
        }

        if let Some(existing) = load_reusable_payment_attempt(self.pool(), &command).await? {
            return Ok(existing);
        }

        let reference_query = OrderPaymentReferenceQuery::new(
            &command.tenant_id,
            command.organization_id.as_deref(),
            &command.owner_user_id,
            &intent.order_id,
        )?;
        let mut tx =
            self.pool().begin().await.map_err(|error| {
                store_error("failed to begin payment attempt transaction", error)
            })?;
        let Some(order_ref) =
            load_order_payment_reference_postgres(&mut tx, &reference_query).await?
        else {
            return Err(CommerceServiceError::not_found("order was not found"));
        };
        if !order_payment_reference_is_payable(&order_ref) {
            return Err(CommerceServiceError::conflict("order is not payable"));
        }

        let channel = select_payment_channel_postgres(
            &mut tx,
            &command.tenant_id,
            command.organization_id.as_deref(),
            &intent.payment_method,
            &intent.currency_code,
            intent.amount.as_str(),
            None,
        )
        .await?;

        let now = current_timestamp_string();
        let attempt_id = payment_attempt_id(&command);
        let out_trade_no = crate::shared::provider_out_trade_no(
            &command.tenant_id,
            &intent.order_id,
            &command.idempotency_key,
        );
        let pending = CommercePaymentStatus::Pending.as_str();
        let callback_payload =
            crate::shared::payment_attempt_callback_payload(channel.provider_account_id.as_deref());

        let insert = sqlx::query(
            r#"
            INSERT INTO commerce_payment_attempt
                (id, tenant_id, organization_id, owner_user_id, payment_intent_id, order_id,
                 payment_method, provider_code, channel_id, out_trade_no, amount, currency_code, status,
                 callback_payload, request_no, idempotency_key, expires_at, created_at, paid_at, updated_at)
            VALUES
                ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11::numeric, $12, $13, $14::jsonb, $15, $16, $17::timestamptz, $18::timestamptz, NULL, $19::timestamptz)
            ON CONFLICT (id) DO NOTHING
           "#,
        )
        .bind(&attempt_id)
        .bind(&command.tenant_id)
        .bind(organization_scope_bind(&command.organization_id))
        .bind(&command.owner_user_id)
        .bind(&command.payment_intent_id)
        .bind(&intent.order_id)
        .bind(&intent.payment_method)
        .bind(&channel.provider_code)
        .bind(channel.channel_id.as_deref())
        .bind(&out_trade_no)
        .bind(intent.amount.as_str())
        .bind(&intent.currency_code)
        .bind(pending)
        .bind(&callback_payload)
        .bind(&command.request_no)
        .bind(&command.idempotency_key)
        .bind(order_ref.expires_at.as_deref())
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error("failed to insert payment attempt", error))?;

        if insert.rows_affected() == 0 {
            let row = sqlx::query(
                r#"
                SELECT id, payment_intent_id, order_id, out_trade_no, CAST(amount AS BIGINT)::TEXT AS amount,
                       payment_method, provider_code, channel_id, status
                FROM commerce_payment_attempt
                WHERE tenant_id = CAST($1 AS TEXT)
                  AND owner_user_id = CAST($2 AS TEXT)
                  AND id = CAST($3 AS TEXT)
                LIMIT 1
               "#,
            )
            .bind(&command.tenant_id)
            .bind(&command.owner_user_id)
            .bind(&attempt_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|error| store_error("failed to load payment attempt idempotency replay", error))?;

            tx.commit().await.map_err(|error| {
                store_error("failed to commit payment attempt idempotency replay", error)
            })?;
            return row
                .map(map_payment_attempt_row)
                .transpose()?
                .ok_or_else(|| {
                    CommerceServiceError::storage("payment attempt idempotency replay failed")
                });
        }

        sqlx::query(
            r#"
            UPDATE commerce_payment_intent
            SET status = $1, updated_at = $2::timestamptz
            WHERE tenant_id = CAST($3 AS TEXT)
              AND owner_user_id = CAST($4 AS TEXT)
              AND id = CAST($5 AS TEXT)
              AND LOWER(COALESCE(status, '')) IN ('created', 'pending', 'processing')
           "#,
        )
        .bind(pending)
        .bind(&now)
        .bind(&command.tenant_id)
        .bind(&command.owner_user_id)
        .bind(&command.payment_intent_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error("failed to update payment intent status", error))?;

        tx.commit()
            .await
            .map_err(|error| store_error("failed to commit payment attempt transaction", error))?;

        let mut payment_params = std::collections::BTreeMap::new();
        if let Some(channel_id) = channel.channel_id {
            payment_params.insert("channelId".to_owned(), channel_id);
        }
        Ok(CreateOwnerPaymentAttemptOutcome {
            attempt_id,
            payment_intent_id: command.payment_intent_id,
            order_id: intent.order_id,
            out_trade_no,
            amount: intent.amount,
            payment_method: intent.payment_method,
            provider_code: channel.provider_code,
            status: pending.to_owned(),
            payment_params,
        })
    }

    async fn find_owner_payment_intent_by_idempotency(
        &self,
        command: &CreateOwnerPaymentIntentCommand,
    ) -> Result<Option<PaymentIntentView>, CommerceServiceError> {
        let row = sqlx::query(
            r#"
            SELECT id, order_id, payment_intent_no, payment_method, provider_code,
                   CAST(amount AS BIGINT)::TEXT AS amount, currency_code, status
            FROM commerce_payment_intent
            WHERE tenant_id = CAST($1 AS TEXT)
              AND order_id = CAST($2 AS TEXT)
              AND idempotency_key = CAST($3 AS TEXT)
              AND ((organization_id = CAST($4 AS TEXT)) OR (organization_id IS NULL AND $4 IS NULL) OR (organization_id = '0' AND $4 IS NULL))
              AND owner_user_id = CAST($5 AS TEXT)
              AND deleted_at IS NULL
            LIMIT 1
           "#,
        )
        .bind(&command.tenant_id)
        .bind(&command.order_id)
        .bind(&command.idempotency_key)
        .bind(command.organization_id.as_deref())
        .bind(&command.owner_user_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|error| store_error("failed to load payment intent idempotency replay", error))?;

        row.map(map_payment_intent_row).transpose()
    }

    async fn find_owner_payment_attempt_by_idempotency(
        &self,
        command: &CreateOwnerPaymentAttemptCommand,
    ) -> Result<Option<CreateOwnerPaymentAttemptOutcome>, CommerceServiceError> {
        let attempt_id = payment_attempt_id(command);
        let row = sqlx::query(
            r#"
            SELECT id, payment_intent_id, order_id, out_trade_no, CAST(amount AS BIGINT)::TEXT AS amount,
                   payment_method, provider_code, channel_id, status
            FROM commerce_payment_attempt
            WHERE tenant_id = CAST($1 AS TEXT)
              AND ((organization_id = CAST($2 AS TEXT)) OR (organization_id IS NULL AND $2 IS NULL) OR (organization_id = '0' AND $2 IS NULL))
              AND owner_user_id = CAST($3 AS TEXT)
              AND id = CAST($4 AS TEXT)
              AND deleted_at IS NULL
            LIMIT 1
           "#,
        )
        .bind(&command.tenant_id)
        .bind(command.organization_id.as_deref())
        .bind(&command.owner_user_id)
        .bind(&attempt_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|error| store_error("failed to load payment attempt idempotency replay", error))?;

        row.map(map_payment_attempt_row).transpose()
    }
}

async fn load_payment_method_for_intent(
    tx: &mut Transaction<'_, Postgres>,
    command: &CreateOwnerPaymentIntentCommand,
) -> Result<ResolvedPaymentMethod, CommerceServiceError> {
    let row = sqlx::query(
        r#"
        SELECT method_key, provider_code
        FROM commerce_payment_method
        WHERE method_key = $1
          AND status = 'active'
          AND (
                NOT EXISTS (
                    SELECT 1
                    FROM commerce_payment_channel c0
                    WHERE c0.tenant_id = commerce_payment_method.tenant_id
                      AND (c0.method_id = commerce_payment_method.id
                           OR (c0.method_id IS NULL AND c0.provider_code = commerce_payment_method.provider_code))
                      AND c0.deleted_at IS NULL
                )
                OR EXISTS (
                    SELECT 1
                    FROM commerce_payment_channel c
                    LEFT JOIN commerce_payment_provider_account a
                      ON a.id = c.provider_account_id AND a.deleted_at IS NULL
                    WHERE c.tenant_id = commerce_payment_method.tenant_id
                      AND (c.method_id = commerce_payment_method.id
                           OR (c.method_id IS NULL AND c.provider_code = commerce_payment_method.provider_code))
                      AND c.status = 'active'
                      AND c.deleted_at IS NULL
                      AND (c.provider_account_id IS NULL OR (
                          a.status = 'active'
                          AND LOWER(a.provider_code) = LOWER(commerce_payment_method.provider_code)
                          AND a.tenant_id = commerce_payment_method.tenant_id
                          AND (a.organization_id IS NULL
                               OR commerce_payment_method.organization_id IS NULL
                               OR a.organization_id = commerce_payment_method.organization_id)
                      ))
                )
          )
          AND (
                (tenant_id = CAST($2 AS TEXT) AND organization_id = CAST($3 AS TEXT))
             OR (tenant_id = CAST($4 AS TEXT) AND organization_id = '0')
          )
        ORDER BY CASE WHEN tenant_id = CAST($5 AS TEXT) THEN 0 ELSE 1 END, sort_order ASC
        LIMIT 1
       "#,
    )
    .bind(&command.payment_method)
    .bind(&command.tenant_id)
    .bind(command.organization_id.as_deref())
    .bind(&command.tenant_id)
    .bind(&command.tenant_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| store_error("failed to load payment method for intent", error))?;

    row.map(|row| ResolvedPaymentMethod {
        method_key: string_cell(&row, "method_key"),
        provider_code: string_cell(&row, "provider_code"),
    })
    .ok_or_else(|| CommerceServiceError::conflict("payment method is unavailable"))
}

async fn load_reusable_payment_attempt(
    pool: &sqlx::PgPool,
    command: &CreateOwnerPaymentAttemptCommand,
) -> Result<Option<CreateOwnerPaymentAttemptOutcome>, CommerceServiceError> {
    let row = sqlx::query(
        r#"
        SELECT pa.id, pa.payment_intent_id, pa.order_id, pa.out_trade_no, CAST(pa.amount AS BIGINT)::TEXT AS amount,
               pa.payment_method, pa.provider_code, pa.channel_id, pa.status
        FROM commerce_payment_attempt pa
        INNER JOIN commerce_order o
            ON o.tenant_id = pa.tenant_id
           AND o.id = pa.order_id
           AND o.owner_user_id = pa.owner_user_id
        WHERE pa.tenant_id = CAST($1 AS TEXT)
          AND ((pa.organization_id = CAST($2 AS TEXT)) OR (pa.organization_id IS NULL AND $2 IS NULL) OR (pa.organization_id = '0' AND $2 IS NULL))
          AND pa.owner_user_id = CAST($3 AS TEXT)
          AND pa.payment_intent_id = CAST($4 AS TEXT)
          AND LOWER(COALESCE(pa.status, '')) IN ('created', 'pending', 'processing')
          AND pa.deleted_at IS NULL
          AND pa.expires_at IS NOT NULL
          AND pa.expires_at > NOW()
          AND o.expired_at IS NOT NULL
          AND NULLIF(o.expired_at, '')::timestamptz > NOW()
        ORDER BY pa.created_at DESC, pa.id DESC
        LIMIT 1
       "#,
    )
    .bind(&command.tenant_id)
    .bind(command.organization_id.as_deref())
    .bind(&command.owner_user_id)
    .bind(&command.payment_intent_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| store_error("failed to load reusable payment attempt", error))?;

    row.map(map_payment_attempt_row).transpose()
}

fn map_payment_intent_row(
    row: sqlx::postgres::PgRow,
) -> Result<PaymentIntentView, CommerceServiceError> {
    let amount = normalize_stored_money_amount(&string_cell(&row, "amount"))?;
    Ok(PaymentIntentView {
        payment_intent_id: string_cell(&row, "id"),
        order_id: string_cell(&row, "order_id"),
        payment_intent_no: string_cell(&row, "payment_intent_no"),
        payment_method: string_cell(&row, "payment_method"),
        provider_code: string_cell(&row, "provider_code"),
        amount: CommerceMoney::new(&amount).map_err(CommerceServiceError::storage)?,
        currency_code: string_cell(&row, "currency_code"),
        status: string_cell(&row, "status"),
    })
}

fn map_payment_attempt_row(
    row: sqlx::postgres::PgRow,
) -> Result<CreateOwnerPaymentAttemptOutcome, CommerceServiceError> {
    let mut payment_params = std::collections::BTreeMap::new();
    let channel_id = string_cell(&row, "channel_id");
    if !channel_id.trim().is_empty() {
        payment_params.insert("channelId".to_owned(), channel_id);
    }
    let amount = normalize_stored_money_amount(&string_cell(&row, "amount"))?;
    Ok(CreateOwnerPaymentAttemptOutcome {
        attempt_id: string_cell(&row, "id"),
        payment_intent_id: string_cell(&row, "payment_intent_id"),
        order_id: string_cell(&row, "order_id"),
        out_trade_no: string_cell(&row, "out_trade_no"),
        amount: CommerceMoney::new(&amount).map_err(CommerceServiceError::storage)?,
        payment_method: string_cell(&row, "payment_method"),
        provider_code: string_cell(&row, "provider_code"),
        status: string_cell(&row, "status"),
        payment_params,
    })
}

fn payment_intent_id(command: &CreateOwnerPaymentIntentCommand) -> String {
    stable_storage_id(&[
        "payment-intent",
        &command.tenant_id,
        &command.order_id,
        &command.idempotency_key,
    ])
}

fn payment_attempt_id(command: &CreateOwnerPaymentAttemptCommand) -> String {
    stable_storage_id(&[
        "payment-attempt",
        &command.tenant_id,
        &command.payment_intent_id,
        &command.idempotency_key,
    ])
}

fn stable_storage_id(parts: &[&str]) -> String {
    parts
        .iter()
        .map(|part| {
            part.chars()
                .map(|character| {
                    if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                        character
                    } else {
                        '-'
                    }
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("-")
}

fn store_error(message: &str, error: impl std::fmt::Display) -> CommerceServiceError {
    CommerceServiceError::storage(format!("{message}: {error}"))
}

fn current_timestamp_string() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn optional_string_cell(row: &sqlx::postgres::PgRow, column: &str) -> Option<String> {
    row.try_get::<Option<String>, _>(column).ok().flatten()
}

fn string_cell(row: &sqlx::postgres::PgRow, column: &str) -> String {
    optional_string_cell(row, column).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::order_currency_code;
    use sdkwork_contract_service::CommerceMoney;
    use sdkwork_payment_service::OrderPaymentReferenceSnapshot;

    fn snapshot_with_currency(currency_code: Option<String>) -> OrderPaymentReferenceSnapshot {
        OrderPaymentReferenceSnapshot {
            expires_at: None,
            order_id: "order-1".to_owned(),
            order_sn: "SN-1".to_owned(),
            order_subject: None,
            status: "pending_payment".to_owned(),
            total_amount: CommerceMoney::new("1").expect("amount"),
            currency_code,
            pay_time: None,
        }
    }

    #[test]
    fn intent_currency_follows_the_order_currency() {
        assert_eq!(
            order_currency_code(&snapshot_with_currency(Some("USD".to_owned()))),
            "USD"
        );
        assert_eq!(
            order_currency_code(&snapshot_with_currency(Some("CNY".to_owned()))),
            "CNY"
        );
    }

    #[test]
    fn intent_currency_defaults_to_cny_when_order_has_none() {
        assert_eq!(order_currency_code(&snapshot_with_currency(None)), "CNY");
        assert_eq!(
            order_currency_code(&snapshot_with_currency(Some("  ".to_owned()))),
            "CNY"
        );
    }
}
