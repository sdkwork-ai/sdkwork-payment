//! Anonymous-cashier key-epoch settings (支付中心 → 支付设置 → 收银台密钥).
//!
//! The anonymous cashier is authorized by an HMAC capability token, not by a
//! session. Its signing key is derived per tenant from the deployment secret and
//! a **key epoch**:
//!
//! ```text
//! tenant_key = HKDF(deployment_secret, "tenant={tenant};[epoch={n};]purpose=cashier-token")
//! ```
//!
//! Only the epoch is stored here. The root secret stays a deployment secret, so a
//! database dump (or a read-only database account) cannot mint cashier tokens.
//! Rotating a tenant increments its epoch: new tokens carry the new epoch, while
//! tokens issued under the previous epoch keep verifying until their 15-minute
//! expiry. Rotation therefore needs no restart, no key propagation, and no grace
//! table.
//!
//! Resolution order for reads: exact tenant row -> platform `'0'` row -> epoch 1
//! (the legacy derivation). Rotation always writes the exact tenant row.

use sdkwork_contract_service::CommerceServiceError;
use serde::Serialize;
use sqlx::{Pool, Postgres, Row};

use crate::shared::{store_error};

/// Tenant scope of the platform-wide fallback setting.
pub const PLATFORM_CASHIER_TOKEN_TENANT_ID: &str = "0";

/// Epoch used when no settings row exists; matches the legacy derivation.
pub const DEFAULT_CASHIER_TOKEN_EPOCH: i64 = 1;

/// Which scope supplied the effective epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CashierTokenSettingSource {
    /// An exact tenant row.
    Tenant,
    /// The platform `'0'` fallback row.
    Platform,
    /// No row at all: the legacy epoch 1 derivation.
    Default,
}

impl CashierTokenSettingSource {
    /// Wire label for the admin read model.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tenant => "tenant",
            Self::Platform => "platform",
            Self::Default => "default",
        }
    }
}

/// Public read model of the cashier-token key setting.
///
/// Deliberately contains no key material: the admin surface only needs to show
/// which epoch is active and when it was rotated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CashierTokenSettingView {
    /// Tenant scope the setting belongs to.
    pub tenant_id: String,
    /// Active key epoch (>= 1), serialized as an int64 decimal string per
    /// API_SPEC §13.6.
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub key_epoch: i64,
    /// When the epoch was last rotated, RFC 3339 UTC; `None` for a never-rotated
    /// tenant/default.
    pub rotated_at: Option<String>,
    /// Operator that performed the last rotation; empty when unknown.
    pub rotated_by: String,
    /// Last write time, RFC 3339 UTC.
    pub updated_at: Option<String>,
    /// Which scope supplied the effective epoch.
    pub source: CashierTokenSettingSource,
}

impl CashierTokenSettingView {
    /// The effective "no settings row" answer: epoch 1, default source.
    #[must_use]
    pub fn default_for(tenant_id: &str) -> Self {
        Self {
            tenant_id: tenant_id.trim().to_owned(),
            key_epoch: DEFAULT_CASHIER_TOKEN_EPOCH,
            rotated_at: None,
            rotated_by: String::new(),
            updated_at: None,
            source: CashierTokenSettingSource::Default,
        }
    }
}

/// Loads the effective key epoch for a tenant (tenant row -> platform -> 1).
pub async fn load_cashier_token_epoch_postgres(
    pool: &Pool<Postgres>,
    tenant_id: &str,
) -> Result<i64, CommerceServiceError> {
    Ok(retrieve_cashier_token_setting_postgres(pool, tenant_id)
        .await?
        .key_epoch)
}

/// Loads the effective setting view for the admin surface.
pub async fn retrieve_cashier_token_setting_postgres(
    pool: &Pool<Postgres>,
    tenant_id: &str,
) -> Result<CashierTokenSettingView, CommerceServiceError> {
    let tenant_id = normalize_tenant_id(tenant_id);
    if let Some(view) = load_cashier_token_setting_row_postgres(
        pool,
        &tenant_id,
        CashierTokenSettingSource::Tenant,
    )
    .await
    .map_err(|error| store_error("failed to load cashier token setting", error))?
    {
        return Ok(view);
    }
    if tenant_id != PLATFORM_CASHIER_TOKEN_TENANT_ID {
        if let Some(view) = load_cashier_token_setting_row_postgres(
            pool,
            PLATFORM_CASHIER_TOKEN_TENANT_ID,
            CashierTokenSettingSource::Platform,
        )
        .await
        .map_err(|error| store_error("failed to load platform cashier token setting", error))?
        {
            return Ok(CashierTokenSettingView {
                tenant_id,
                ..view
            });
        }
    }
    Ok(CashierTokenSettingView::default_for(&tenant_id))
}

/// Rotates the tenant's key epoch and returns the new setting view.
///
/// The first rotation of a tenant with no row creates epoch 2 (epoch 1 is the
/// legacy derivation, already "issued"). The previous epoch stays valid for
/// already-minted tokens until they expire; there is nothing to keep alive.
pub async fn rotate_cashier_token_epoch_postgres(
    pool: &Pool<Postgres>,
    tenant_id: &str,
    rotated_by: &str,
) -> Result<CashierTokenSettingView, CommerceServiceError> {
    let tenant_id = normalize_tenant_id(tenant_id);
    let rotated_by = rotated_by.trim();
    // First rotation of a tenant that inherits the platform row must continue
    // from the effective platform epoch, not restart at 2. An existing tenant
    // row is advanced by the ON CONFLICT branch instead.
    let row = sqlx::query(
        r#"
        INSERT INTO commerce_payment_cashier_token_setting
            (tenant_id, key_epoch, rotated_at, rotated_by, created_at, updated_at)
        VALUES
            (
                CAST($1 AS TEXT),
                COALESCE(
                    (SELECT key_epoch FROM commerce_payment_cashier_token_setting
                     WHERE tenant_id = '0' LIMIT 1),
                    1
                ) + 1,
                NOW(),
                $2,
                NOW(),
                NOW()
            )
        ON CONFLICT (tenant_id) DO UPDATE
        SET key_epoch = commerce_payment_cashier_token_setting.key_epoch + 1,
            rotated_at = NOW(),
            rotated_by = EXCLUDED.rotated_by,
            updated_at = NOW()
        RETURNING
            tenant_id,
            key_epoch,
            TO_CHAR(rotated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS rotated_at,
            rotated_by,
            TO_CHAR(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at
        "#,
    )
    .bind(&tenant_id)
    .bind(rotated_by)
    .fetch_one(pool)
    .await
    .map_err(|error| store_error("failed to rotate cashier token setting", error))?;

    Ok(CashierTokenSettingView {
        tenant_id: string_cell(&row, "tenant_id"),
        key_epoch: row.try_get::<i64, _>("key_epoch").unwrap_or(2),
        rotated_at: optional_string_cell(&row, "rotated_at"),
        rotated_by: string_cell(&row, "rotated_by"),
        updated_at: optional_string_cell(&row, "updated_at"),
        source: CashierTokenSettingSource::Tenant,
    })
}

/// Seeds the platform tenant with epoch 1 when the installation has no row.
///
/// Idempotent; safe to call on every startup. A missing row already behaves as
/// epoch 1, so this only makes the "install default" visible in the admin surface.
pub async fn seed_cashier_token_setting_postgres(
    pool: &Pool<Postgres>,
) -> Result<(), CommerceServiceError> {
    sqlx::query(
        r#"
        INSERT INTO commerce_payment_cashier_token_setting (tenant_id, key_epoch)
        VALUES (CAST($1 AS TEXT), 1)
        ON CONFLICT (tenant_id) DO NOTHING
        "#,
    )
    .bind(PLATFORM_CASHIER_TOKEN_TENANT_ID)
    .execute(pool)
    .await
    .map_err(|error| store_error("failed to seed cashier token setting", error))?;
    Ok(())
}

async fn load_cashier_token_setting_row_postgres(
    pool: &Pool<Postgres>,
    tenant_id: &str,
    source: CashierTokenSettingSource,
) -> Result<Option<CashierTokenSettingView>, sqlx::Error> {
    let row = sqlx::query(
        r#"
        SELECT
            tenant_id,
            key_epoch,
            TO_CHAR(rotated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS rotated_at,
            rotated_by,
            TO_CHAR(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at
        FROM commerce_payment_cashier_token_setting
        WHERE tenant_id = CAST($1 AS TEXT)
        LIMIT 1
        "#,
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| CashierTokenSettingView {
        tenant_id: string_cell(&row, "tenant_id"),
        key_epoch: row
            .try_get::<i64, _>("key_epoch")
            .unwrap_or(DEFAULT_CASHIER_TOKEN_EPOCH)
            .max(DEFAULT_CASHIER_TOKEN_EPOCH),
        rotated_at: optional_string_cell(&row, "rotated_at"),
        rotated_by: string_cell(&row, "rotated_by"),
        updated_at: optional_string_cell(&row, "updated_at"),
        source,
    }))
}

fn normalize_tenant_id(tenant_id: &str) -> String {
    let trimmed = tenant_id.trim();
    if trimmed.is_empty() {
        PLATFORM_CASHIER_TOKEN_TENANT_ID.to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn string_cell(row: &sqlx::postgres::PgRow, column: &str) -> String {
    row.try_get::<String, _>(column).unwrap_or_default()
}

fn optional_string_cell(row: &sqlx::postgres::PgRow, column: &str) -> Option<String> {
    row.try_get::<Option<String>, _>(column).ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_setting_is_epoch_one() {
        let view = CashierTokenSettingView::default_for(" 100001 ");
        assert_eq!(view.tenant_id, "100001");
        assert_eq!(view.key_epoch, 1);
        assert_eq!(view.source, CashierTokenSettingSource::Default);
        assert_eq!(view.source.as_str(), "default");
    }

    #[test]
    fn blank_tenant_uses_the_platform_scope() {
        assert_eq!(normalize_tenant_id("  "), "0");
        assert_eq!(normalize_tenant_id("100001"), "100001");
    }

    #[test]
    fn setting_view_serializes_the_epoch_as_an_int64_string() {
        let view = CashierTokenSettingView::default_for("100001");
        let json = serde_json::to_value(view).expect("setting json");
        assert_eq!(json["keyEpoch"], serde_json::json!("1"));
        assert_eq!(json["source"], serde_json::json!("default"));
    }
}
