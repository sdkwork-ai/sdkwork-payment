//! Payment notify domain configuration (支付通知域名配置).
//!
//! The admin payment center configures one or more notify domains per tenant
//! scope; exactly one active domain is the default. Checkout resolves the
//! default domain (exact organization row → organization `'0'` platform row
//! → `ORDER_PAYMENT_WEBHOOK_BASE_URL` env) and builds the order payment
//! webhook URL from it.

use sdkwork_contract_service::CommerceServiceError;
use serde::Serialize;
use sqlx::{Pool, Postgres, Row};

use crate::shared::{stable_storage_id, store_error};

/// Canonical order payment webhook path (HTTP-owned by sdkwork-order).
pub const ORDER_PAYMENT_WEBHOOK_PATH: &str = "/app/v3/api/orders/payments/webhooks/{providerCode}";
/// Canonical order refund webhook path (HTTP-owned by sdkwork-order).
pub const ORDER_REFUND_WEBHOOK_PATH: &str = "/app/v3/api/orders/refunds/webhooks/{providerCode}";

/// Notify domain row projection with the full notify URL templates.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyDomainView {
    pub id: String,
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub protocol: String,
    pub hostname: String,
    pub port: Option<i32>,
    pub is_default: bool,
    pub status: String,
    pub sort_order: i32,
    /// `{protocol}://{hostname}{:port}` + payment webhook path template.
    pub payment_notify_url: String,
    /// `{protocol}://{hostname}{:port}` + refund webhook path template.
    pub refund_notify_url: String,
}

/// Command for admin create/update of a notify domain.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct UpsertNotifyDomainCommand {
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub id: Option<String>,
    pub protocol: String,
    pub hostname: String,
    pub port: Option<i32>,
    pub is_default: bool,
    pub status: String,
    pub sort_order: i32,
    pub request_no: String,
    pub idempotency_key: String,
}

pub fn build_notify_domain_urls(
    protocol: &str,
    hostname: &str,
    port: Option<i32>,
) -> (String, String) {
    let base = match port {
        Some(port) => format!("{protocol}://{hostname}:{port}"),
        None => format!("{protocol}://{hostname}"),
    };
    (
        format!("{base}{ORDER_PAYMENT_WEBHOOK_PATH}"),
        format!("{base}{ORDER_REFUND_WEBHOOK_PATH}"),
    )
}

/// Loads the active default notify domain for a tenant scope. Resolution:
/// exact organization row first, then the platform `'0'` row.
pub async fn load_default_notify_domain_postgres(
    pool: &Pool<Postgres>,
    tenant_id: &str,
    organization_id: Option<&str>,
) -> Result<Option<NotifyDomainView>, CommerceServiceError> {
    let rows = sqlx::query(
        r#"
        SELECT id, tenant_id, organization_id, protocol, hostname, port,
               is_default, status, sort_order
        FROM commerce_payment_notify_domain
        WHERE tenant_id = CAST($1 AS TEXT)
          AND is_default
          AND status = 'active'
          AND ((organization_id = CAST($2 AS TEXT)) OR (organization_id = '0' AND $2 IS NULL))
          AND deleted_at IS NULL
        ORDER BY CASE WHEN organization_id = $2 THEN 0 ELSE 1 END,
                 organization_id = '0' DESC,
                 sort_order ASC,
                 id ASC
        LIMIT 1
        "#,
    )
    .bind(tenant_id)
    .bind(organization_id.unwrap_or("0"))
    .fetch_all(pool)
    .await
    .map_err(|error| store_error("failed to load default notify domain", error))?;
    let Some(row) = rows.first() else {
        return Ok(None);
    };
    Ok(Some(notify_domain_from_row(row)))
}

/// Lists active notify domains for the admin surface (exact org + platform).
pub async fn list_notify_domains_postgres(
    pool: &Pool<Postgres>,
    tenant_id: &str,
    organization_id: Option<&str>,
) -> Result<Vec<NotifyDomainView>, CommerceServiceError> {
    let rows = sqlx::query(
        r#"
        SELECT id, tenant_id, organization_id, protocol, hostname, port,
               is_default, status, sort_order
        FROM commerce_payment_notify_domain
        WHERE tenant_id = CAST($1 AS TEXT)
          AND ((organization_id = CAST($2 AS TEXT)) OR (organization_id = '0' AND $2 IS NULL))
          AND deleted_at IS NULL
        ORDER BY organization_id = '0' DESC, sort_order ASC, id ASC
        "#,
    )
    .bind(tenant_id)
    .bind(organization_id.unwrap_or("0"))
    .fetch_all(pool)
    .await
    .map_err(|error| store_error("failed to list notify domains", error))?;
    Ok(rows.iter().map(notify_domain_from_row).collect())
}

/// Creates or updates a notify domain. Setting a domain default clears the
/// default flag on sibling rows of the same scope; setting it inactive clears
/// the default flag too.
pub async fn upsert_notify_domain_postgres(
    pool: &Pool<Postgres>,
    command: UpsertNotifyDomainCommand,
) -> Result<NotifyDomainView, CommerceServiceError> {
    validate_notify_domain(&command.protocol, &command.hostname, command.port)?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| store_error("failed to begin notify domain transaction", error))?;
    let organization_bind = command
        .organization_id
        .clone()
        .unwrap_or_else(|| "0".to_owned());
    let id = command.id.clone().unwrap_or_else(|| {
        stable_storage_id(&[
            "notify-domain",
            &command.tenant_id,
            &organization_bind,
            &command.protocol,
            &command.hostname,
            &command.idempotency_key,
        ])
    });

    if command.is_default {
        sqlx::query(
            r#"
            UPDATE commerce_payment_notify_domain
            SET is_default = FALSE, updated_at = CURRENT_TIMESTAMP
            WHERE tenant_id = CAST($1 AS TEXT)
              AND ((organization_id = CAST($2 AS TEXT)) OR (organization_id = '0' AND $2 IS NULL))
              AND is_default
              AND id <> CAST($3 AS TEXT)
              AND deleted_at IS NULL
            "#,
        )
        .bind(&command.tenant_id)
        .bind(command.organization_id.as_deref().unwrap_or("0"))
        .bind(&id)
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error("failed to clear sibling notify domain defaults", error))?;
    }

    sqlx::query(
        r#"
        INSERT INTO commerce_payment_notify_domain
            (id, tenant_id, organization_id, protocol, hostname, port,
             is_default, status, sort_order, version, created_at, updated_at)
        VALUES
            ($1, $2, $3, $4, $5, $6, $7, $8, $9, 1, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
        ON CONFLICT (id) DO UPDATE SET
            protocol = EXCLUDED.protocol,
            hostname = EXCLUDED.hostname,
            port = EXCLUDED.port,
            is_default = EXCLUDED.is_default,
            status = EXCLUDED.status,
            sort_order = EXCLUDED.sort_order,
            version = commerce_payment_notify_domain.version + 1,
            updated_at = CURRENT_TIMESTAMP,
            deleted_at = NULL
        RETURNING id, tenant_id, organization_id, protocol, hostname, port,
                  is_default, status, sort_order
        "#,
    )
    .bind(&id)
    .bind(&command.tenant_id)
    .bind(&organization_bind)
    .bind(&command.protocol)
    .bind(&command.hostname)
    .bind(command.port)
    .bind(command.is_default)
    .bind(&command.status)
    .bind(command.sort_order)
    .fetch_one(&mut *tx)
    .await
    .map_err(|error| store_error("failed to upsert notify domain", error))?;

    if !command.is_default {
        sqlx::query(
            r#"
            UPDATE commerce_payment_notify_domain
            SET is_default = FALSE, updated_at = CURRENT_TIMESTAMP
            WHERE id = CAST($1 AS TEXT)
              AND NOT EXISTS (
                  SELECT 1 FROM commerce_payment_notify_domain d
                  WHERE d.tenant_id = commerce_payment_notify_domain.tenant_id
                    AND ((d.organization_id = commerce_payment_notify_domain.organization_id)
                         OR (d.organization_id = '0' AND commerce_payment_notify_domain.organization_id IS NULL))
                    AND d.is_default AND d.deleted_at IS NULL
              )
            "#,
        )
        .bind(&id)
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error("failed to demote orphan notify domain default", error))?;
    }

    tx.commit()
        .await
        .map_err(|error| store_error("failed to commit notify domain transaction", error))?;
    load_notify_domain_by_id_postgres(pool, &command.tenant_id, &id).await
}

/// Soft-deletes a notify domain.
pub async fn delete_notify_domain_postgres(
    pool: &Pool<Postgres>,
    tenant_id: &str,
    domain_id: &str,
) -> Result<(), CommerceServiceError> {
    let result = sqlx::query(
        r#"
        UPDATE commerce_payment_notify_domain
        SET deleted_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP,
            is_default = FALSE
        WHERE id = CAST($1 AS TEXT)
          AND tenant_id = CAST($2 AS TEXT)
          AND deleted_at IS NULL
        "#,
    )
    .bind(domain_id)
    .bind(tenant_id)
    .execute(pool)
    .await
    .map_err(|error| store_error("failed to delete notify domain", error))?;
    if result.rows_affected() != 1 {
        return Err(CommerceServiceError::not_found(
            "payment notify domain was not found",
        ));
    }
    Ok(())
}

async fn load_notify_domain_by_id_postgres(
    pool: &Pool<Postgres>,
    tenant_id: &str,
    domain_id: &str,
) -> Result<NotifyDomainView, CommerceServiceError> {
    let row = sqlx::query(
        r#"
        SELECT id, tenant_id, organization_id, protocol, hostname, port,
               is_default, status, sort_order
        FROM commerce_payment_notify_domain
        WHERE id = CAST($1 AS TEXT)
          AND tenant_id = CAST($2 AS TEXT)
          AND deleted_at IS NULL
        LIMIT 1
        "#,
    )
    .bind(domain_id)
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| store_error("failed to load notify domain", error))?
    .ok_or_else(|| CommerceServiceError::not_found("payment notify domain was not found"))?;
    Ok(notify_domain_from_row(&row))
}

fn validate_notify_domain(
    protocol: &str,
    hostname: &str,
    port: Option<i32>,
) -> Result<(), CommerceServiceError> {
    if !matches!(
        protocol.trim().to_ascii_lowercase().as_str(),
        "https" | "http"
    ) {
        return Err(CommerceServiceError::validation(
            "payment notify domain protocol must be https or http",
        ));
    }
    let hostname = hostname.trim();
    if hostname.is_empty()
        || hostname.len() > 255
        || hostname.starts_with('/')
        || hostname.contains('/')
    {
        return Err(CommerceServiceError::validation(
            "payment notify domain hostname must be a bare hostname without scheme or path",
        ));
    }
    if port.is_some_and(|port| !(1..=65535).contains(&port)) {
        return Err(CommerceServiceError::validation(
            "payment notify domain port must be between 1 and 65535",
        ));
    }
    Ok(())
}

fn notify_domain_from_row(row: &sqlx::postgres::PgRow) -> NotifyDomainView {
    let protocol = string_cell(row, "protocol");
    let hostname = string_cell(row, "hostname");
    let port = row.try_get::<Option<i32>, _>("port").ok().flatten();
    let (payment_notify_url, refund_notify_url) =
        build_notify_domain_urls(&protocol, &hostname, port);
    NotifyDomainView {
        id: string_cell(row, "id"),
        tenant_id: string_cell(row, "tenant_id"),
        organization_id: optional_string_cell(row, "organization_id"),
        protocol,
        hostname,
        port,
        is_default: row.try_get::<bool, _>("is_default").unwrap_or(false),
        status: string_cell(row, "status"),
        sort_order: row.try_get::<i32, _>("sort_order").unwrap_or(0),
        payment_notify_url,
        refund_notify_url,
    }
}

fn optional_string_cell(row: &sqlx::postgres::PgRow, column: &str) -> Option<String> {
    row.try_get::<Option<String>, _>(column).ok().flatten()
}

fn string_cell(row: &sqlx::postgres::PgRow, column: &str) -> String {
    optional_string_cell(row, column).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notify_url_templates_use_canonical_order_paths() {
        let (payment, refund) = build_notify_domain_urls("https", "pay.example.com", None);
        assert_eq!(
            "https://pay.example.com/app/v3/api/orders/payments/webhooks/{providerCode}",
            payment
        );
        assert_eq!(
            "https://pay.example.com/app/v3/api/orders/refunds/webhooks/{providerCode}",
            refund
        );
        let (payment, _) = build_notify_domain_urls("http", "127.0.0.1", Some(3905));
        assert_eq!(
            "http://127.0.0.1:3905/app/v3/api/orders/payments/webhooks/{providerCode}",
            payment
        );
    }

    #[test]
    fn validation_rejects_bad_protocols_hostnames_and_ports() {
        for (protocol, hostname, port) in [
            ("ftp", "pay.example.com", None),
            ("https", "", None),
            ("https", "https://pay.example.com", None),
            ("https", "pay.example.com/x", None),
            ("https", "pay.example.com", Some(0)),
            ("https", "pay.example.com", Some(70000)),
        ] {
            assert!(
                validate_notify_domain(protocol, hostname, port).is_err(),
                "{protocol}://{hostname}:{port:?} must be rejected"
            );
        }
    }
}
