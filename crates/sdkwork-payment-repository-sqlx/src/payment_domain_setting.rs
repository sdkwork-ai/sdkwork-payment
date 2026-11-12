//! Payment domain settings (支付设置 → 域名设置).
//!
//! The admin payment center configures the H5 payment domain per tenant scope.
//! It is the origin used to build the order cashier URL / QR code payload and
//! the default H5 return URL for redirect-style PSP products. Resolution is
//! exact organization row -> organization `'0'` (platform) row -> the
//! deployment `SDKWORK_COMMERCE_CASHIER_BASE_URL` / platform default (resolved
//! by `sdkwork_utils_rust`, never by this module).
//!
//! The callback (notify) domain keeps its own multi-row table
//! `commerce_payment_notify_domain`; this table stores a single typed settings
//! record per scope.

use sdkwork_contract_service::CommerceServiceError;
use serde::Serialize;
use sqlx::{Pool, Postgres, Row};

use crate::shared::{stable_storage_id, store_error};

/// Resolution order marker for the platform (`'0'`) fallback row.
const PLATFORM_ORGANIZATION_ID: &str = "0";

/// Payment domain settings projection, including the derived base origin and
/// a cashier URL template operators can review before saving.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentDomainSettingView {
    pub id: String,
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub h5_protocol: String,
    pub h5_hostname: String,
    pub h5_port: Option<i32>,
    pub h5_status: String,
    /// Whether a settings row exists for this scope.
    pub configured: bool,
    /// `{protocol}://{hostname}{:port}` (no trailing slash).
    pub h5_base_url: String,
    /// `{h5BaseUrl}/cashier/{orderId}?scene={scene}&outTradeNo={outTradeNo}`.
    pub cashier_url_template: String,
}

/// The "not configured yet" answer for a scope without a settings row. The
/// checkout falls back to the deployment origin; the admin form starts empty.
#[must_use]
pub fn default_payment_domain_setting_view() -> PaymentDomainSettingView {
    PaymentDomainSettingView {
        id: String::new(),
        tenant_id: String::new(),
        organization_id: None,
        h5_protocol: "https".to_owned(),
        h5_hostname: String::new(),
        h5_port: None,
        h5_status: "inactive".to_owned(),
        configured: false,
        h5_base_url: String::new(),
        cashier_url_template: String::new(),
    }
}

/// Command for admin save of the payment domain settings.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct UpsertPaymentDomainSettingCommand {
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub id: Option<String>,
    pub h5_protocol: String,
    pub h5_hostname: String,
    pub h5_port: Option<i32>,
    pub h5_status: String,
    pub request_no: String,
    pub idempotency_key: String,
}

/// Canonical cashier route template appended to the configured H5 domain.
pub const CASHIER_URL_TEMPLATE_SUFFIX: &str =
    "/cashier/{orderId}?scene={scene}&outTradeNo={outTradeNo}";

/// Builds the origin for a configured domain row.
#[must_use]
pub fn build_payment_domain_base_url(protocol: &str, hostname: &str, port: Option<i32>) -> String {
    match port {
        Some(port) => format!("{protocol}://{hostname}:{port}"),
        None => format!("{protocol}://{hostname}"),
    }
}

/// Loads the active payment domain settings for a tenant scope. Resolution:
/// exact organization row first, then the platform `'0'` row.
pub async fn load_payment_domain_setting_postgres(
    pool: &Pool<Postgres>,
    tenant_id: &str,
    organization_id: Option<&str>,
) -> Result<Option<PaymentDomainSettingView>, CommerceServiceError> {
    load_payment_domain_setting_row_postgres(pool, tenant_id, organization_id)
        .await
        .map_err(|error| store_error("failed to load payment domain setting", error))
}

async fn load_payment_domain_setting_row_postgres(
    pool: &Pool<Postgres>,
    tenant_id: &str,
    organization_id: Option<&str>,
) -> Result<Option<PaymentDomainSettingView>, sqlx::Error> {
    let organization_bind = organization_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(PLATFORM_ORGANIZATION_ID);
    let row = sqlx::query(
        r#"
        SELECT id, tenant_id, organization_id, h5_protocol, h5_hostname, h5_port, h5_status
        FROM commerce_payment_domain_setting
        WHERE tenant_id = CAST($1 AS TEXT)
          AND h5_status = 'active'
          AND deleted_at IS NULL
          AND (organization_id = CAST($2 AS TEXT) OR organization_id = '0')
        ORDER BY CASE WHEN organization_id = CAST($2 AS TEXT) THEN 0 ELSE 1 END,
                 id ASC
        LIMIT 1
        "#,
    )
    .bind(tenant_id)
    .bind(organization_bind)
    .fetch_optional(pool)
    .await?;
    Ok(row.as_ref().map(payment_domain_setting_from_row))
}

/// Resolves the configured H5 cashier base origin, or `None` when no active
/// setting exists (the caller falls back to the deployment default).
///
/// A deployment that has not yet applied the domain-settings migration has no
/// table at all; checkout treats that exactly like an unconfigured scope
/// instead of failing every payment, so the feature rolls out safely.
pub async fn resolve_h5_cashier_base_url_postgres(
    pool: &Pool<Postgres>,
    tenant_id: &str,
    organization_id: Option<&str>,
) -> Result<Option<String>, CommerceServiceError> {
    match load_payment_domain_setting_row_postgres(pool, tenant_id, organization_id).await {
        Ok(setting) => Ok(setting.map(|value| value.h5_base_url)),
        Err(error) if is_missing_settings_table(&error) => Ok(None),
        Err(error) => Err(store_error("failed to resolve payment H5 domain", error)),
    }
}

/// Whether the failure only proves the settings table has not been created yet
/// (PostgreSQL undefined-table SQLSTATE 42P01).
fn is_missing_settings_table(error: &sqlx::Error) -> bool {
    matches!(
        error,
        sqlx::Error::Database(database)
            if database.code().is_some_and(|code| code.as_ref() == "42P01")
    )
}

/// Creates or updates the payment domain settings for a scope.
///
/// A scope keeps exactly one settings row: the existing row id is reused so
/// editing the hostname never trips the unique scope index.
pub async fn upsert_payment_domain_setting_postgres(
    pool: &Pool<Postgres>,
    command: UpsertPaymentDomainSettingCommand,
) -> Result<PaymentDomainSettingView, CommerceServiceError> {
    validate_payment_domain_setting(&command.h5_protocol, &command.h5_hostname, command.h5_port)?;
    let protocol = command.h5_protocol.trim().to_ascii_lowercase();
    let hostname = command.h5_hostname.trim().to_owned();
    let status = normalize_payment_domain_status(&command.h5_status)?;
    let organization_bind = command
        .organization_id
        .clone()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| PLATFORM_ORGANIZATION_ID.to_owned());

    let mut tx = pool.begin().await.map_err(|error| {
        store_error("failed to begin payment domain setting transaction", error)
    })?;

    let existing_id = sqlx::query_scalar::<_, String>(
        r#"
        SELECT id
        FROM commerce_payment_domain_setting
        WHERE tenant_id = CAST($1 AS TEXT)
          AND organization_id = CAST($2 AS TEXT)
          AND deleted_at IS NULL
        ORDER BY id ASC
        LIMIT 1
        FOR UPDATE
        "#,
    )
    .bind(&command.tenant_id)
    .bind(&organization_bind)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| store_error("failed to load payment domain setting for update", error))?;

    let id = command
        .id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .or(existing_id)
        .unwrap_or_else(|| {
            stable_storage_id(&[
                "payment-domain-setting",
                &command.tenant_id,
                &organization_bind,
                &hostname,
                &command.idempotency_key,
            ])
        });

    sqlx::query(
        r#"
        INSERT INTO commerce_payment_domain_setting
            (id, tenant_id, organization_id, h5_protocol, h5_hostname, h5_port,
             h5_status, version, created_at, updated_at)
        VALUES
            ($1, $2, $3, $4, $5, $6, $7, 1, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
        ON CONFLICT (id) DO UPDATE SET
            h5_protocol = EXCLUDED.h5_protocol,
            h5_hostname = EXCLUDED.h5_hostname,
            h5_port = EXCLUDED.h5_port,
            h5_status = EXCLUDED.h5_status,
            version = commerce_payment_domain_setting.version + 1,
            updated_at = CURRENT_TIMESTAMP,
            deleted_at = NULL
        RETURNING id, tenant_id, organization_id, h5_protocol, h5_hostname, h5_port, h5_status
        "#,
    )
    .bind(&id)
    .bind(&command.tenant_id)
    .bind(&organization_bind)
    .bind(&protocol)
    .bind(&hostname)
    .bind(command.h5_port)
    .bind(&status)
    .fetch_one(&mut *tx)
    .await
    .map_err(|error| store_error("failed to upsert payment domain setting", error))?;

    tx.commit().await.map_err(|error| {
        store_error("failed to commit payment domain setting transaction", error)
    })?;

    load_payment_domain_setting_for_scope_postgres(pool, &command.tenant_id, &organization_bind)
        .await?
        .ok_or_else(|| CommerceServiceError::storage("payment domain setting was not persisted"))
}

/// Loads the settings row for one exact scope (not the platform fallback).
async fn load_payment_domain_setting_for_scope_postgres(
    pool: &Pool<Postgres>,
    tenant_id: &str,
    organization_id: &str,
) -> Result<Option<PaymentDomainSettingView>, CommerceServiceError> {
    let row = sqlx::query(
        r#"
        SELECT id, tenant_id, organization_id, h5_protocol, h5_hostname, h5_port, h5_status
        FROM commerce_payment_domain_setting
        WHERE tenant_id = CAST($1 AS TEXT)
          AND organization_id = CAST($2 AS TEXT)
          AND deleted_at IS NULL
        ORDER BY id ASC
        LIMIT 1
        "#,
    )
    .bind(tenant_id)
    .bind(organization_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| store_error("failed to load payment domain setting", error))?;
    Ok(row.as_ref().map(payment_domain_setting_from_row))
}

fn validate_payment_domain_setting(
    protocol: &str,
    hostname: &str,
    port: Option<i32>,
) -> Result<(), CommerceServiceError> {
    let protocol = protocol.trim().to_ascii_lowercase();
    if !matches!(protocol.as_str(), "https" | "http") {
        return Err(CommerceServiceError::validation(
            "payment H5 domain protocol must be https or http",
        ));
    }
    // Plain-text H5 payment pages expose payer return links to interception and
    // tampering; only local development/test keep http for loopback testing.
    if protocol == "http" && !http_payment_protocol_allowed() {
        return Err(CommerceServiceError::validation(
            "payment H5 domain protocol must be https outside local development",
        ));
    }
    let hostname = hostname.trim();
    if hostname.is_empty()
        || hostname.len() > 255
        || hostname.starts_with('/')
        || hostname.contains('/')
        || hostname.contains(':')
        || hostname.chars().any(char::is_whitespace)
        || hostname.starts_with('.')
        || hostname.ends_with('.')
    {
        return Err(CommerceServiceError::validation(
            "payment H5 domain hostname must be a bare hostname without scheme, port, or path",
        ));
    }
    if port.is_some_and(|port| !(1..=65535).contains(&port)) {
        return Err(CommerceServiceError::validation(
            "payment H5 domain port must be between 1 and 65535",
        ));
    }
    Ok(())
}

fn normalize_payment_domain_status(status: &str) -> Result<String, CommerceServiceError> {
    match status.trim().to_ascii_lowercase().as_str() {
        "active" => Ok("active".to_owned()),
        "inactive" => Ok("inactive".to_owned()),
        "" => Ok("active".to_owned()),
        _ => Err(CommerceServiceError::validation(
            "payment H5 domain status must be active or inactive",
        )),
    }
}

/// Whether plain-text `http` payment domains may be registered. Mirrors the
/// notify-domain posture: only explicitly non-production environments allow it.
fn http_payment_protocol_allowed() -> bool {
    let environment = [
        "SDKWORK_ENVIRONMENT",
        "SDKWORK_PAYMENT_ENVIRONMENT",
        "PAYMENT_ENVIRONMENT",
        "SDKWORK_ENV",
    ]
    .iter()
    .find_map(|key| std::env::var(key).ok())
    .unwrap_or_else(|| "development".to_owned());
    matches!(
        environment.trim().to_ascii_lowercase().as_str(),
        "development" | "dev" | "local" | "test" | "testing"
    )
}

fn payment_domain_setting_from_row(row: &sqlx::postgres::PgRow) -> PaymentDomainSettingView {
    let h5_protocol = row
        .try_get::<String, _>("h5_protocol")
        .unwrap_or_else(|_| "https".to_owned());
    let h5_hostname = row.try_get::<String, _>("h5_hostname").unwrap_or_default();
    let h5_port = row.try_get::<Option<i32>, _>("h5_port").ok().flatten();
    let h5_base_url = build_payment_domain_base_url(&h5_protocol, &h5_hostname, h5_port);
    PaymentDomainSettingView {
        id: row.try_get::<String, _>("id").unwrap_or_default(),
        tenant_id: row.try_get::<String, _>("tenant_id").unwrap_or_default(),
        organization_id: row
            .try_get::<Option<String>, _>("organization_id")
            .ok()
            .flatten(),
        h5_protocol,
        h5_hostname,
        h5_port,
        h5_status: row
            .try_get::<String, _>("h5_status")
            .unwrap_or_else(|_| "inactive".to_owned()),
        configured: true,
        cashier_url_template: format!("{h5_base_url}{CASHIER_URL_TEMPLATE_SUFFIX}"),
        h5_base_url,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_url_includes_the_configured_port() {
        assert_eq!(
            "https://h5.example.com",
            build_payment_domain_base_url("https", "h5.example.com", None)
        );
        assert_eq!(
            "http://127.0.0.1:5182",
            build_payment_domain_base_url("http", "127.0.0.1", Some(5182))
        );
    }

    #[test]
    fn cashier_url_template_uses_the_canonical_h5_route() {
        let base = build_payment_domain_base_url("https", "h5.example.com", None);
        assert_eq!(
            "https://h5.example.com/cashier/{orderId}?scene={scene}&outTradeNo={outTradeNo}",
            format!("{base}{CASHIER_URL_TEMPLATE_SUFFIX}")
        );
    }

    #[test]
    fn validation_rejects_bad_protocols_hostnames_and_ports() {
        for (protocol, hostname, port) in [
            ("ftp", "h5.example.com", None),
            ("https", "", None),
            ("https", "https://h5.example.com", None),
            ("https", "h5.example.com/cashier", None),
            ("https", "h5.example.com:443", None),
            ("https", "h5 example.com", None),
            ("https", ".example.com", None),
            ("https", "example.com.", None),
            ("https", "h5.example.com", Some(0)),
            ("https", "h5.example.com", Some(70000)),
        ] {
            assert!(
                validate_payment_domain_setting(protocol, hostname, port).is_err(),
                "{protocol}://{hostname}:{port:?} must be rejected"
            );
        }
        assert!(validate_payment_domain_setting("https", "h5.example.com", None).is_ok());
        assert!(validate_payment_domain_setting("https", "127.0.0.1", Some(5182)).is_ok());
        // `http` is a local-development-only allowance (SDKWORK_ENVIRONMENT
        // driven); production must always reject it.
        if !http_payment_protocol_allowed() {
            assert!(validate_payment_domain_setting("http", "127.0.0.1", Some(5182)).is_err());
        }
    }

    #[test]
    fn status_normalization_accepts_only_active_or_inactive() {
        assert_eq!("active", normalize_payment_domain_status("").unwrap());
        assert_eq!(
            "inactive",
            normalize_payment_domain_status("INACTIVE").unwrap()
        );
        assert!(normalize_payment_domain_status("disabled").is_err());
    }
}
