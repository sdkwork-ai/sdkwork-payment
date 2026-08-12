#![allow(clippy::too_many_arguments)]

use sdkwork_contract_service::{CommerceMoney, CommerceServiceError};
use sdkwork_payment_service::{
    CreateOwnerRefundCommand, OrderPaymentReferenceQuery, RefundDetailQuery, RefundListPage,
    RefundListQuery, RefundView,
};
use sqlx::{PgPool, Postgres, Row, Transaction};

use crate::order_reference::{load_order_payment_reference_postgres, order_status_is_refundable};
use crate::shared::{
    current_timestamp_string, ensure_refund_idempotency_replay_matches,
    ensure_refund_requester_idempotency_replay_matches, money_to_minor_units,
    normalize_stored_money_amount, organization_scope_bind, resolve_refund_amount,
    stable_storage_id, store_error, validate_refund_bounds,
};

#[derive(Debug, Clone)]
pub struct PostgresCommerceRefundStore {
    pool: PgPool,
}

impl PostgresCommerceRefundStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

impl PostgresCommerceRefundStore {
    pub async fn create_owner_refund(
        &self,
        command: CreateOwnerRefundCommand,
    ) -> Result<RefundView, CommerceServiceError> {
        if let Some(existing) = self.find_refund_by_idempotency(&command).await? {
            ensure_refund_idempotency_replay_matches(&command, &existing)?;
            return Ok(existing);
        }

        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|error| store_error("failed to begin refund transaction", error))?;

        sqlx::query(
            r#"
            SELECT id
            FROM commerce_order
            WHERE tenant_id = CAST($1 AS TEXT)
              AND id = CAST($2 AS TEXT)
            FOR UPDATE
            "#,
        )
        .bind(&command.tenant_id)
        .bind(&command.order_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| store_error("failed to lock order for refund", error))?;

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
        if !order_status_is_refundable(&order_ref.status, order_ref.pay_time.as_deref()) {
            return Err(CommerceServiceError::conflict(
                "order is not eligible for refund",
            ));
        }

        let payment_attempt = find_succeeded_payment_attempt_in_tx(&mut tx, &command)
            .await?
            .ok_or_else(|| CommerceServiceError::not_found("payment attempt was not found"))?;
        let (payment_attempt_id, paid_amount, paid_currency_code) = payment_attempt;

        // The refundable bound is the actually paid amount (the succeeded
        // attempt amount), not the order total — PSP refunds (e.g. WeChat
        // `amount.total`) are anchored to the paid amount, so an order whose
        // payment settled below its total must not allow refunding more.
        let paid_amount = normalize_stored_money_amount(&paid_amount)?;
        let paid_money = CommerceMoney::new(&paid_amount).map_err(CommerceServiceError::storage)?;
        let refund_amount = resolve_refund_amount(&command, &paid_money)?;
        let paid_minor = money_to_minor_units(&paid_amount)?;
        let refund_minor = money_to_minor_units(&refund_amount)?;
        validate_refund_bounds(refund_minor, paid_minor)?;
        let already_refunded_minor =
            sum_refunded_amount_in_tx(&mut tx, &command, &paid_currency_code).await?;
        if refund_minor > paid_minor.saturating_sub(already_refunded_minor) {
            return Err(CommerceServiceError::conflict(
                "refund amount exceeds remaining refundable amount",
            ));
        }

        let now = current_timestamp_string();
        let refund_id = refund_id(&command);
        let refund_no = format!("RF-{}", command.request_no);
        crate::shared::ensure_refund_status_transition(None, "submitted")?;

        let insert_result = sqlx::query(
            r#"
            INSERT INTO commerce_refund
                (id, tenant_id, organization_id, order_id, payment_attempt_id, refund_no,
                 amount, currency_code, status, refund_reason_code, requested_by_type,
                 requested_by, request_no, idempotency_key, created_at, updated_at)
            VALUES
                ($1, $2, $3, $4, $5, $6, $7::numeric, $8, 'submitted', $9, $10, $11, $12, $13, $14::timestamptz, $15::timestamptz)
            ON CONFLICT (id) DO NOTHING
            "#,
        )
        .bind(&refund_id)
        .bind(&command.tenant_id)
        .bind(organization_scope_bind(&command.organization_id))
        .bind(&command.order_id)
        .bind(&payment_attempt_id)
        .bind(&refund_no)
        .bind(&refund_amount)
        .bind(&command.currency_code)
        .bind(command.reason_code.as_deref())
        .bind(&command.requested_by_type)
        .bind(&command.requested_by)
        .bind(&command.request_no)
        .bind(&command.idempotency_key)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error("failed to insert refund", error))?;

        if insert_result.rows_affected() == 0 {
            if let Some(existing) = find_refund_by_idempotency_in_tx(&mut tx, &command).await? {
                ensure_refund_idempotency_replay_matches(&command, &existing)?;
                tx.commit().await.map_err(|error| {
                    store_error("failed to commit refund idempotency replay", error)
                })?;
                return Ok(existing);
            }
            return Err(CommerceServiceError::conflict(
                "refund idempotency identity could not be resolved",
            ));
        }

        insert_refund_event(
            &mut tx,
            &command.tenant_id,
            command.organization_id.as_deref(),
            &refund_id,
            "created",
            None,
            "submitted",
            &command.requested_by_type,
            Some(&command.requested_by),
            &command.request_no,
            &command.idempotency_key,
            &now,
        )
        .await?;

        tx.commit()
            .await
            .map_err(|error| store_error("failed to commit refund transaction", error))?;

        Ok(RefundView {
            refund_id,
            refund_no,
            order_id: command.order_id,
            payment_attempt_id,
            amount: CommerceMoney::new(&refund_amount).map_err(CommerceServiceError::storage)?,
            currency_code: command.currency_code,
            status: "submitted".to_owned(),
            reason_code: command.reason_code,
        })
    }

    pub async fn list_owner_refunds(
        &self,
        query: RefundListQuery,
    ) -> Result<RefundListPage, CommerceServiceError> {
        let mut sql = String::from(
            r#"
            SELECT r.id, r.refund_no, r.order_id, r.payment_attempt_id,
                   CAST(r.amount AS BIGINT)::TEXT AS amount, r.currency_code, r.status, r.refund_reason_code,
                   COUNT(*) OVER() AS total_count
            FROM commerce_refund r
            INNER JOIN commerce_order o
                ON o.tenant_id = r.tenant_id
               AND o.id = r.order_id
            WHERE r.tenant_id = CAST($1 AS TEXT)
              AND ((r.organization_id = CAST($2 AS TEXT)) OR (r.organization_id IS NULL AND $3 IS NULL) OR (r.organization_id = '0' AND $3 IS NULL))
              AND o.owner_user_id = CAST($4 AS TEXT)
              AND r.deleted_at IS NULL
            "#,
        );
        if query.status.is_some() {
            sql.push_str(" AND r.status = CAST($5 AS TEXT)");
            sql.push_str(" ORDER BY r.created_at DESC, r.id DESC LIMIT $6 OFFSET $7");
        } else {
            sql.push_str(" ORDER BY r.created_at DESC, r.id DESC LIMIT $5 OFFSET $6");
        }

        let mut db_query = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(&query.tenant_id)
            .bind(query.organization_id.as_deref())
            .bind(query.organization_id.as_deref())
            .bind(&query.owner_user_id);
        if let Some(status) = query.status.as_deref() {
            db_query = db_query.bind(status);
        }
        db_query = db_query.bind(query.limit).bind(query.offset);

        let rows = db_query
            .fetch_all(self.pool())
            .await
            .map_err(|error| store_error("failed to list owner refunds", error))?;

        let total_items = rows
            .first()
            .and_then(|row| row.try_get::<i64, _>("total_count").ok())
            .unwrap_or(0);
        let items = rows
            .into_iter()
            .map(map_refund_row)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(RefundListPage { items, total_items })
    }

    pub async fn retrieve_owner_refund(
        &self,
        query: RefundDetailQuery,
    ) -> Result<Option<RefundView>, CommerceServiceError> {
        let row = sqlx::query(
            r#"
            SELECT r.id, r.refund_no, r.order_id, r.payment_attempt_id,
                   CAST(r.amount AS BIGINT)::TEXT AS amount, r.currency_code, r.status, r.refund_reason_code
            FROM commerce_refund r
            INNER JOIN commerce_order o
                ON o.tenant_id = r.tenant_id
               AND o.id = r.order_id
            WHERE r.tenant_id = CAST($1 AS TEXT)
              AND ((r.organization_id = CAST($2 AS TEXT)) OR (r.organization_id IS NULL AND $3 IS NULL) OR (r.organization_id = '0' AND $3 IS NULL))
              AND o.owner_user_id = CAST($4 AS TEXT)
              AND r.id = CAST($5 AS TEXT)
              AND r.deleted_at IS NULL
            LIMIT 1
           "#,
        )
        .bind(&query.tenant_id)
        .bind(query.organization_id.as_deref())
        .bind(query.organization_id.as_deref())
        .bind(&query.owner_user_id)
        .bind(&query.refund_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|error| store_error("failed to retrieve owner refund", error))?;

        row.map(map_refund_row).transpose()
    }

    pub async fn mark_owner_refund_provider_submission_failed(
        &self,
        tenant_id: &str,
        organization_id: Option<&str>,
        refund_id: &str,
        actor_type: &str,
        actor_id: Option<&str>,
        request_no: &str,
        idempotency_key: &str,
    ) -> Result<RefundView, CommerceServiceError> {
        self.mark_owner_refund_provider_submission_terminal(
            tenant_id,
            organization_id,
            refund_id,
            actor_type,
            actor_id,
            request_no,
            idempotency_key,
            "failed",
        )
        .await
    }

    pub async fn mark_owner_refund_provider_submission_succeeded(
        &self,
        tenant_id: &str,
        organization_id: Option<&str>,
        refund_id: &str,
        actor_type: &str,
        actor_id: Option<&str>,
        request_no: &str,
        idempotency_key: &str,
    ) -> Result<RefundView, CommerceServiceError> {
        self.mark_owner_refund_provider_submission_terminal(
            tenant_id,
            organization_id,
            refund_id,
            actor_type,
            actor_id,
            request_no,
            idempotency_key,
            "succeeded",
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn mark_owner_refund_provider_submission_terminal(
        &self,
        tenant_id: &str,
        organization_id: Option<&str>,
        refund_id: &str,
        actor_type: &str,
        actor_id: Option<&str>,
        request_no: &str,
        idempotency_key: &str,
        terminal_status: &'static str,
    ) -> Result<RefundView, CommerceServiceError> {
        let now = current_timestamp_string();
        let mut tx =
            self.pool.begin().await.map_err(|error| {
                store_error("failed to begin refund terminal transaction", error)
            })?;
        let row = sqlx::query(
            r#"
            SELECT id, refund_no, order_id, payment_attempt_id,
                   CAST(amount AS BIGINT)::TEXT AS amount, currency_code, status, refund_reason_code
            FROM commerce_refund
            WHERE tenant_id = CAST($1 AS TEXT)
              AND ((organization_id = CAST($2 AS TEXT)) OR (organization_id IS NULL AND $3 IS NULL) OR (organization_id = '0' AND $3 IS NULL))
              AND id = CAST($4 AS TEXT)
              AND deleted_at IS NULL
            LIMIT 1
            FOR UPDATE
            "#,
        )
        .bind(tenant_id)
        .bind(organization_id)
        .bind(organization_id)
        .bind(refund_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| store_error("failed to load refund for terminal transition", error))?;
        let Some(row) = row else {
            return Err(CommerceServiceError::not_found("refund was not found"));
        };
        let current_status = string_cell(&row, "status");
        crate::shared::ensure_refund_status_transition(Some(&current_status), terminal_status)?;
        let result = sqlx::query(
            r#"
            UPDATE commerce_refund
            SET status = $1, updated_at = $2::timestamptz, version = version + 1
            WHERE tenant_id = CAST($3 AS TEXT)
              AND ((organization_id = CAST($4 AS TEXT)) OR (organization_id IS NULL AND $5 IS NULL) OR (organization_id = '0' AND $5 IS NULL))
              AND id = CAST($6 AS TEXT)
              AND status IN ('submitted', 'processing')
              AND deleted_at IS NULL
            "#,
        )
        .bind(terminal_status)
        .bind(&now)
        .bind(tenant_id)
        .bind(organization_id)
        .bind(organization_id)
        .bind(refund_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error("failed to mark refund terminal status", error))?;
        if result.rows_affected() != 1 {
            return Err(CommerceServiceError::conflict(
                "refund terminal transition lost a concurrent status change",
            ));
        }
        insert_refund_event(
            &mut tx,
            tenant_id,
            organization_id,
            refund_id,
            terminal_status,
            Some(&current_status),
            terminal_status,
            actor_type,
            actor_id,
            request_no,
            idempotency_key,
            &now,
        )
        .await?;
        tx.commit()
            .await
            .map_err(|error| store_error("failed to commit refund terminal transaction", error))?;
        map_refund_row(row).map(|mut view| {
            view.status = terminal_status.to_owned();
            view
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn mark_owner_refund_provider_submission_processing(
        &self,
        tenant_id: &str,
        organization_id: Option<&str>,
        refund_id: &str,
        actor_type: &str,
        actor_id: Option<&str>,
        request_no: &str,
        idempotency_key: &str,
    ) -> Result<RefundView, CommerceServiceError> {
        let now = current_timestamp_string();
        let mut tx =
            self.pool.begin().await.map_err(|error| {
                store_error("failed to begin refund submission transaction", error)
            })?;
        let row = sqlx::query(
            r#"
            SELECT id, refund_no, order_id, payment_attempt_id,
                   CAST(amount AS BIGINT)::TEXT AS amount, currency_code, status, refund_reason_code
            FROM commerce_refund
            WHERE tenant_id = CAST($1 AS TEXT)
              AND ((organization_id = CAST($2 AS TEXT)) OR (organization_id IS NULL AND $3 IS NULL) OR (organization_id = '0' AND $3 IS NULL))
              AND id = CAST($4 AS TEXT)
              AND deleted_at IS NULL
            LIMIT 1
            FOR UPDATE
            "#,
        )
        .bind(tenant_id)
        .bind(organization_id)
        .bind(organization_id)
        .bind(refund_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| store_error("failed to load refund for provider submission", error))?;
        let Some(row) = row else {
            return Err(CommerceServiceError::not_found("refund was not found"));
        };
        let current_status = string_cell(&row, "status");
        crate::shared::ensure_refund_status_transition(Some(&current_status), "processing")?;
        let result = sqlx::query(
            r#"
            UPDATE commerce_refund
            SET status = 'processing', updated_at = $1::timestamptz, version = version + 1
            WHERE tenant_id = CAST($2 AS TEXT)
              AND ((organization_id = CAST($3 AS TEXT)) OR (organization_id IS NULL AND $4 IS NULL) OR (organization_id = '0' AND $4 IS NULL))
              AND id = CAST($5 AS TEXT)
              AND status IN ('submitted', 'failed')
            "#,
        )
        .bind(&now)
        .bind(tenant_id)
        .bind(organization_id)
        .bind(organization_id)
        .bind(refund_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            store_error(
                "failed to mark refund provider submission processing",
                error,
            )
        })?;
        if result.rows_affected() != 1 {
            return Err(CommerceServiceError::conflict(
                "refund is already processing or is not retryable",
            ));
        }
        insert_refund_event(
            &mut tx,
            tenant_id,
            organization_id,
            refund_id,
            "status_changed",
            Some(&current_status),
            "processing",
            actor_type,
            actor_id,
            request_no,
            idempotency_key,
            &now,
        )
        .await?;
        tx.commit().await.map_err(|error| {
            store_error("failed to commit refund submission transaction", error)
        })?;
        map_refund_row(row).map(|mut view| {
            view.status = "processing".to_owned();
            view
        })
    }

    async fn find_refund_by_idempotency(
        &self,
        command: &CreateOwnerRefundCommand,
    ) -> Result<Option<RefundView>, CommerceServiceError> {
        let row = sqlx::query(
            r#"
            SELECT r.id AS id, r.refund_no, r.order_id, r.payment_attempt_id,
                   CAST(r.amount AS BIGINT)::TEXT AS amount, r.currency_code, r.status,
                   r.refund_reason_code, r.requested_by_type, r.requested_by
            FROM commerce_refund r
            INNER JOIN commerce_order o
                ON o.tenant_id = r.tenant_id
               AND o.id = r.order_id
            WHERE r.tenant_id = CAST($1 AS TEXT)
              AND r.order_id = CAST($2 AS TEXT)
              AND r.idempotency_key = CAST($3 AS TEXT)
              AND ((r.organization_id = CAST($4 AS TEXT)) OR (r.organization_id IS NULL AND $4 IS NULL) OR (r.organization_id = '0' AND $4 IS NULL))
              AND o.owner_user_id = CAST($5 AS TEXT)
              AND r.deleted_at IS NULL
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
        .map_err(|error| store_error("failed to load refund idempotency replay", error))?;

        let Some(row) = row else {
            return Ok(None);
        };
        ensure_refund_requester_idempotency_replay_matches(
            command,
            &string_cell(&row, "requested_by_type"),
            &string_cell(&row, "requested_by"),
        )?;
        map_refund_row(row).map(Some)
    }
}

async fn find_succeeded_payment_attempt_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    command: &CreateOwnerRefundCommand,
) -> Result<Option<(String, String, String)>, CommerceServiceError> {
    let row = sqlx::query(
        r#"
        SELECT id, CAST(amount AS BIGINT)::TEXT AS amount, currency_code
        FROM commerce_payment_attempt
        WHERE tenant_id = CAST($1 AS TEXT)
          AND ((organization_id = CAST($2 AS TEXT)) OR (organization_id IS NULL AND $2 IS NULL) OR (organization_id = '0' AND $2 IS NULL))
          AND owner_user_id = CAST($3 AS TEXT)
          AND order_id = CAST($4 AS TEXT)
          AND currency_code = CAST($5 AS TEXT)
          AND ($6::text IS NULL OR id = CAST($6 AS TEXT))
          AND LOWER(COALESCE(status, '')) IN ('succeeded', 'success', 'paid')
          AND deleted_at IS NULL
        ORDER BY created_at DESC, id DESC
        LIMIT 1
       "#,
    )
    .bind(&command.tenant_id)
    .bind(command.organization_id.as_deref())
    .bind(&command.owner_user_id)
    .bind(&command.order_id)
    .bind(&command.currency_code)
    .bind(command.payment_attempt_id.as_deref())
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| store_error("failed to load payment attempt for refund", error))?;

    Ok(row.map(|row| {
        (
            string_cell(&row, "id"),
            string_cell(&row, "amount"),
            string_cell(&row, "currency_code"),
        )
    }))
}

async fn sum_refunded_amount_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    command: &CreateOwnerRefundCommand,
    currency_code: &str,
) -> Result<i64, CommerceServiceError> {
    let rows = sqlx::query(
        r#"
        SELECT CAST(amount AS BIGINT)::TEXT AS amount
        FROM commerce_refund
        WHERE tenant_id = CAST($1 AS TEXT)
          AND order_id = CAST($2 AS TEXT)
          AND ((organization_id = CAST($3 AS TEXT)) OR (organization_id IS NULL AND $3 IS NULL) OR (organization_id = '0' AND $3 IS NULL))
          AND currency_code = CAST($4 AS TEXT)
          AND LOWER(COALESCE(status, '')) IN ('submitted', 'processing', 'succeeded')
          AND deleted_at IS NULL
        "#,
    )
    .bind(&command.tenant_id)
    .bind(&command.order_id)
    .bind(command.organization_id.as_deref())
    .bind(currency_code)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| store_error("failed to sum refunded amount", error))?;

    rows.iter().try_fold(0_i64, |acc, row| {
        let amount = string_cell(row, "amount");
        let minor = money_to_minor_units(&amount)?;
        acc.checked_add(minor)
            .ok_or_else(|| CommerceServiceError::validation("refunded amount sum overflows i64"))
    })
}

async fn find_refund_by_idempotency_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    command: &CreateOwnerRefundCommand,
) -> Result<Option<RefundView>, CommerceServiceError> {
    let row = sqlx::query(
        r#"
        SELECT r.id AS id, r.refund_no, r.order_id, r.payment_attempt_id,
               CAST(r.amount AS BIGINT)::TEXT AS amount, r.currency_code, r.status,
               r.refund_reason_code, r.requested_by_type, r.requested_by
        FROM commerce_refund r
        INNER JOIN commerce_order o
            ON o.tenant_id = r.tenant_id
           AND o.id = r.order_id
        WHERE r.tenant_id = CAST($1 AS TEXT)
          AND r.order_id = CAST($2 AS TEXT)
          AND r.idempotency_key = CAST($3 AS TEXT)
          AND ((r.organization_id = CAST($4 AS TEXT)) OR (r.organization_id IS NULL AND $4 IS NULL) OR (r.organization_id = '0' AND $4 IS NULL))
          AND o.owner_user_id = CAST($5 AS TEXT)
          AND r.deleted_at IS NULL
        LIMIT 1
        "#,
    )
    .bind(&command.tenant_id)
    .bind(&command.order_id)
    .bind(&command.idempotency_key)
    .bind(command.organization_id.as_deref())
    .bind(&command.owner_user_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| store_error("failed to load refund idempotency replay in tx", error))?;

    let Some(row) = row else {
        return Ok(None);
    };
    ensure_refund_requester_idempotency_replay_matches(
        command,
        &string_cell(&row, "requested_by_type"),
        &string_cell(&row, "requested_by"),
    )?;
    map_refund_row(row).map(Some)
}

pub(crate) async fn insert_refund_event(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: &str,
    organization_id: Option<&str>,
    refund_id: &str,
    event_type: &str,
    from_status: Option<&str>,
    to_status: &str,
    actor_type: &str,
    actor_id: Option<&str>,
    request_no: &str,
    idempotency_key: &str,
    now: &str,
) -> Result<(), CommerceServiceError> {
    let event_id = stable_storage_id(&[
        "refund-event",
        tenant_id,
        refund_id,
        event_type,
        idempotency_key,
    ]);
    let event_no = format!("RFE-{event_type}-{request_no}");
    sqlx::query(
        r#"
        INSERT INTO commerce_refund_event
            (id, tenant_id, organization_id, event_no, refund_id, event_type,
             from_status, to_status, actor_type, actor_id, request_id, idempotency_key, created_at)
        VALUES
            ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13::timestamptz)
        ON CONFLICT (id) DO NOTHING
       "#,
    )
    .bind(&event_id)
    .bind(tenant_id)
    .bind(organization_scope_bind(&organization_id.map(str::to_owned)))
    .bind(&event_no)
    .bind(refund_id)
    .bind(event_type)
    .bind(from_status)
    .bind(to_status)
    .bind(actor_type)
    .bind(actor_id)
    .bind(request_no)
    .bind(idempotency_key)
    .bind(now)
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error("failed to insert refund event", error))?;
    Ok(())
}

fn map_refund_row(row: sqlx::postgres::PgRow) -> Result<RefundView, CommerceServiceError> {
    let amount = normalize_stored_money_amount(&string_cell(&row, "amount"))?;
    Ok(RefundView {
        refund_id: string_cell(&row, "id"),
        refund_no: string_cell(&row, "refund_no"),
        order_id: string_cell(&row, "order_id"),
        payment_attempt_id: string_cell(&row, "payment_attempt_id"),
        amount: CommerceMoney::new(&amount).map_err(CommerceServiceError::storage)?,
        currency_code: string_cell(&row, "currency_code"),
        status: string_cell(&row, "status"),
        reason_code: optional_string_cell(&row, "refund_reason_code"),
    })
}

fn refund_id(command: &CreateOwnerRefundCommand) -> String {
    stable_storage_id(&[
        "refund",
        &command.tenant_id,
        &command.order_id,
        &command.idempotency_key,
    ])
}

fn optional_string_cell(row: &sqlx::postgres::PgRow, column: &str) -> Option<String> {
    row.try_get::<Option<String>, _>(column).ok().flatten()
}

fn string_cell(row: &sqlx::postgres::PgRow, column: &str) -> String {
    optional_string_cell(row, column).unwrap_or_default()
}
