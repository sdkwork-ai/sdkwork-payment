//! Append-only audit trail for privileged payment-admin mutations.
//!
//! `INTEGRATION_SPEC` requires audit events for credential create/rotate/revoke
//! and for other security-relevant configuration changes. The trail records *who*
//! changed *what* in *which request* — never the secret material itself — so an
//! incident can be reconstructed without the audit log becoming a second secret
//! store.

use sdkwork_contract_service::CommerceServiceError;
use serde_json::Value;
use sqlx::{PgPool, Postgres};

/// One recorded admin action.
#[derive(Debug, Clone)]
pub struct AdminAuditEvent<'a> {
    pub tenant_id: &'a str,
    pub organization_id: Option<&'a str>,
    pub actor_user_id: &'a str,
    /// Stable verb such as `provider_account.upsert` or
    /// `payment_provider_credential.rotate`.
    pub action: &'a str,
    /// Resource family, e.g. `provider_account`.
    pub resource_type: &'a str,
    pub resource_id: &'a str,
    /// Write-command request number, when the caller accepted one.
    pub request_no: Option<&'a str>,
    /// Non-secret context (which credential kinds were rotated, changed fields).
    pub detail: Value,
}

/// Records an audit event inside a caller-owned transaction.
///
/// Sharing the mutation's transaction is the point: an audited change and its
/// record commit together, so a successful write can never be missing from the
/// trail.
pub async fn record_admin_audit_event_in_transaction(
    transaction: &mut sqlx::Transaction<'_, Postgres>,
    event: &AdminAuditEvent<'_>,
) -> Result<(), CommerceServiceError> {
    sqlx::query(
        "INSERT INTO commerce_payment_admin_audit_log \
         (id, tenant_id, organization_id, actor_user_id, action, resource_type, resource_id, request_no, detail) \
         VALUES (CAST($1 AS TEXT), CAST($2 AS TEXT), CAST($3 AS TEXT), CAST($4 AS TEXT), $5, $6, CAST($7 AS TEXT), $8, CAST($9 AS JSONB))",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(event.tenant_id)
    .bind(organization_id_or_platform(event.organization_id))
    .bind(event.actor_user_id)
    .bind(event.action)
    .bind(event.resource_type)
    .bind(event.resource_id)
    .bind(event.request_no)
    .bind(event.detail.to_string())
    .execute(&mut **transaction)
    .await
    .map(|_| ())
    .map_err(|error| {
        CommerceServiceError::storage(format!("failed to record payment admin audit event: {error}"))
    })
}

/// Records an audit event in a transaction of its own.
pub async fn record_admin_audit_event_postgres(
    pool: &PgPool,
    event: &AdminAuditEvent<'_>,
) -> Result<(), CommerceServiceError> {
    let mut transaction = pool.begin().await.map_err(|error| {
        CommerceServiceError::storage(format!("failed to begin payment audit transaction: {error}"))
    })?;
    record_admin_audit_event_in_transaction(&mut transaction, event).await?;
    transaction.commit().await.map_err(|error| {
        CommerceServiceError::storage(format!("failed to commit payment audit event: {error}"))
    })
}

/// Most recent audit events for a tenant, newest first.
pub async fn list_admin_audit_events_postgres(
    pool: &PgPool,
    tenant_id: &str,
    limit: i64,
) -> Result<Vec<Value>, CommerceServiceError> {
    let rows = sqlx::query(
        "SELECT id, tenant_id, organization_id, actor_user_id, action, resource_type, resource_id, \
                request_no, detail::text AS detail, \
                TO_CHAR(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"') AS created_at \
         FROM commerce_payment_admin_audit_log \
         WHERE tenant_id = CAST($1 AS TEXT) \
         ORDER BY created_at DESC, id DESC \
         LIMIT $2",
    )
    .bind(tenant_id)
    .bind(limit.clamp(1, 200))
    .fetch_all(pool)
    .await
    .map_err(|error| {
        CommerceServiceError::storage(format!("failed to read payment admin audit events: {error}"))
    })?;
    Ok(rows
        .into_iter()
        .map(|row| {
            use sqlx::Row;
            let detail = row
                .try_get::<Option<String>, _>("detail")
                .ok()
                .flatten()
                .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
                .unwrap_or_else(|| Value::Object(serde_json::Map::new()));
            serde_json::json!({
                "id": row.try_get::<Option<String>, _>("id").ok().flatten(),
                "tenantId": row.try_get::<Option<String>, _>("tenant_id").ok().flatten(),
                "organizationId": row.try_get::<Option<String>, _>("organization_id").ok().flatten(),
                "actorUserId": row.try_get::<Option<String>, _>("actor_user_id").ok().flatten(),
                "action": row.try_get::<Option<String>, _>("action").ok().flatten(),
                "resourceType": row.try_get::<Option<String>, _>("resource_type").ok().flatten(),
                "resourceId": row.try_get::<Option<String>, _>("resource_id").ok().flatten(),
                "requestNo": row.try_get::<Option<String>, _>("request_no").ok().flatten(),
                "detail": detail,
                "createdAt": row.try_get::<Option<String>, _>("created_at").ok().flatten(),
            })
        })
        .collect())
}

fn organization_id_or_platform<'a>(organization_id: Option<&'a str>) -> &'a str {
    // Platform rows persist the sentinel scope so postgres-login sessions never
    // write NULL into the NOT NULL column (DATABASE_SPEC DB090).
    organization_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("0")
}

/// Number of recorded admin events for a tenant.
pub async fn count_admin_audit_events_postgres(
    pool: &PgPool,
    tenant_id: &str,
) -> Result<i64, CommerceServiceError> {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM commerce_payment_admin_audit_log WHERE tenant_id = CAST($1 AS TEXT)",
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .map_err(|error| {
        CommerceServiceError::storage(format!("failed to count payment admin audit events: {error}"))
    })
}