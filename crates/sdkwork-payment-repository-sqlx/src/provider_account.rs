use sdkwork_contract_service::CommerceServiceError;
use serde_json::Value;
use sqlx::{Pool, Postgres, Row, };
use crate::provider_credential::{
    load_provider_credentials_postgres,
};
use crate::shared::{store_error, string_cell};
#[derive(Clone, Eq, PartialEq)]
pub struct PaymentProviderAccountRecord {
    pub id: String,
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub provider_code: String,
    pub merchant_id: Option<String>,
    pub environment: String,
    pub secret_ref: String,
    pub webhook_secret_ref: Option<String>,
    pub certificate_ref: Option<String>,
    pub primary_secret: Option<String>,
    pub webhook_secret: Option<String>,
    pub certificate: Option<String>,
    pub metadata: Value,
}
fn organization_scope_rank(requested: Option<&str>, actual: Option<&str>) -> u8 {
    if requested.is_some() && actual == requested {
        0
    } else if actual == Some("0") {
        1
    } else {
        2
    }
}
impl std::fmt::Debug for PaymentProviderAccountRecord {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PaymentProviderAccountRecord")
            .field("id", &self.id)
            .field("tenant_id", &self.tenant_id)
            .field("organization_id", &self.organization_id)
            .field("provider_code", &self.provider_code)
            .field("merchant_id", &self.merchant_id)
            .field("environment", &self.environment)
            .field("has_primary_secret", &self.primary_secret.is_some())
            .field("has_webhook_secret", &self.webhook_secret.is_some())
            .field("has_certificate", &self.certificate.is_some())
            .finish_non_exhaustive()
    }
}
pub fn ensure_provider_account_matches(
    account: Option<&PaymentProviderAccountRecord>,
    provider_code: &str,
) -> Result<(), CommerceServiceError> {
    if account.is_some_and(|account| !account.provider_code.eq_ignore_ascii_case(provider_code)) {
        return Err(CommerceServiceError::conflict(
            "payment provider account does not match the persisted provider snapshot",
        ));
    }
    Ok(())
}
fn metadata_from_row(raw: Result<String, sqlx::Error>) -> Value {
    match raw {
        Ok(text) => serde_json::from_str(&text).unwrap_or(Value::Object(Default::default())),
        Err(_) => Value::Object(Default::default()),
    }
}
fn metadata_from_jsonb(raw: Result<Value, sqlx::Error>) -> Value {
    raw.unwrap_or(Value::Object(Default::default()))
}
pub async fn load_active_provider_account_postgres(
    pool: &Pool<Postgres>,
    tenant_id: &str,
    organization_id: Option<&str>,
    provider_code: &str,
) -> Result<Option<PaymentProviderAccountRecord>, CommerceServiceError> {
    let rows = sqlx::query(
        r#"
        SELECT id, tenant_id, organization_id, provider_code, merchant_id, environment, secret_ref,
               webhook_secret_ref, certificate_ref, metadata
        FROM commerce_payment_provider_account
        WHERE tenant_id = CAST($1 AS TEXT)
          AND (organization_id = CAST($2 AS TEXT) OR organization_id = '0' OR organization_id = '0')
          AND LOWER(provider_code) = LOWER(CAST($3 AS TEXT))
          AND status = 'active'
          AND deleted_at IS NULL
        ORDER BY CASE
                     WHEN organization_id = CAST($2 AS TEXT) THEN 0
                     WHEN organization_id = '0' THEN 1
                     ELSE 2
                 END,
                 updated_at DESC,
                 id DESC
        LIMIT 2
        "#,
    )
    .bind(tenant_id)
    .bind(organization_id)
    .bind(provider_code)
    .fetch_all(pool)
    .await
    .map_err(|error| store_error("failed to load payment provider account", error))?;
    match rows.as_slice() {
        [] => Ok(None),
        [row] => Ok(Some(
            hydrate_postgres(pool, map_provider_account_row_postgres(row)).await?,
        )),
        [first, second, ..] => {
            let first = map_provider_account_row_postgres(first);
            let second = map_provider_account_row_postgres(second);
            if organization_scope_rank(organization_id, first.organization_id.as_deref())
                < organization_scope_rank(organization_id, second.organization_id.as_deref())
            {
                Ok(Some(hydrate_postgres(pool, first).await?))
            } else {
                Err(CommerceServiceError::conflict(
                    "multiple active payment provider accounts require deterministic channel routing",
                ))
            }
        }
    }
}
pub async fn load_active_provider_account_by_id_postgres(
    pool: &Pool<Postgres>,
    tenant_id: &str,
    organization_id: Option<&str>,
    provider_account_id: &str,
) -> Result<Option<PaymentProviderAccountRecord>, CommerceServiceError> {
    let row = sqlx::query(
        r#"
        SELECT id, tenant_id, organization_id, provider_code, merchant_id, environment, secret_ref,
               webhook_secret_ref, certificate_ref, metadata
        FROM commerce_payment_provider_account
        WHERE id = CAST($1 AS TEXT)
          AND tenant_id = CAST($2 AS TEXT)
          AND (organization_id = CAST($3 AS TEXT) OR organization_id = '0' OR organization_id = '0')
          AND status = 'active'
          AND deleted_at IS NULL
        LIMIT 1
        "#,
    )
    .bind(provider_account_id)
    .bind(tenant_id)
    .bind(organization_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| store_error("failed to load payment provider account by id", error))?;
    match row {
        Some(row) => Ok(Some(
            hydrate_postgres(pool, map_provider_account_row_postgres(&row)).await?,
        )),
        None => Ok(None),
    }
}
pub async fn load_provider_account_for_existing_payment_postgres(
    pool: &Pool<Postgres>,
    tenant_id: &str,
    organization_id: Option<&str>,
    provider_account_id: &str,
) -> Result<Option<PaymentProviderAccountRecord>, CommerceServiceError> {
    let row = sqlx::query(
        r#"
        SELECT id, tenant_id, organization_id, provider_code, merchant_id, environment, secret_ref,
               webhook_secret_ref, certificate_ref, metadata
        FROM commerce_payment_provider_account
        WHERE id = CAST($1 AS TEXT)
          AND tenant_id = CAST($2 AS TEXT)
          AND (organization_id = CAST($3 AS TEXT) OR organization_id = '0' OR organization_id = '0')
          AND status IN ('active', 'inactive', 'deprecated')
          AND deleted_at IS NULL
        LIMIT 1
        "#,
    )
    .bind(provider_account_id)
    .bind(tenant_id)
    .bind(organization_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| store_error("failed to load historical payment provider account", error))?;
    match row {
        Some(row) => Ok(Some(
            hydrate_postgres(pool, map_provider_account_row_postgres(&row)).await?,
        )),
        None => Ok(None),
    }
}
pub async fn load_active_provider_account_for_channel_postgres(
    pool: &Pool<Postgres>,
    tenant_id: &str,
    organization_id: Option<&str>,
    channel_id: &str,
    provider_code: &str,
) -> Result<Option<PaymentProviderAccountRecord>, CommerceServiceError> {
    let row = sqlx::query(
        r#"
        SELECT provider_account_id
        FROM commerce_payment_channel
        WHERE id = CAST($1 AS TEXT)
          AND tenant_id = CAST($2 AS TEXT)
          AND (organization_id = CAST($3 AS TEXT) OR organization_id = '0' OR organization_id = '0')
          AND LOWER(provider_code) = LOWER(CAST($4 AS TEXT))
          AND status = 'active'
          AND deleted_at IS NULL
        LIMIT 1
        "#,
    )
    .bind(channel_id)
    .bind(tenant_id)
    .bind(organization_id)
    .bind(provider_code)
    .fetch_optional(pool)
    .await
    .map_err(|error| store_error("failed to load payment channel account binding", error))?
    .ok_or_else(|| CommerceServiceError::conflict("payment channel is no longer active"))?;
    let provider_account_id: Option<String> = row.try_get("provider_account_id").ok().flatten();
    let Some(provider_account_id) = provider_account_id else {
        return Ok(None);
    };
    load_active_provider_account_by_id_postgres(
        pool,
        tenant_id,
        organization_id,
        &provider_account_id,
    )
    .await?
    .map(Some)
    .ok_or_else(|| {
        CommerceServiceError::conflict("payment channel provider account is no longer active")
    })
}
pub async fn load_active_provider_account_by_merchant_id_postgres(
    pool: &Pool<Postgres>,
    provider_code: &str,
    merchant_id: &str,
) -> Result<Option<PaymentProviderAccountRecord>, CommerceServiceError> {
    let rows = sqlx::query(
        r#"
        SELECT id, tenant_id, organization_id, provider_code, merchant_id, environment, secret_ref,
               webhook_secret_ref, certificate_ref, metadata
        FROM commerce_payment_provider_account
        WHERE LOWER(provider_code) = LOWER(CAST($1 AS TEXT))
          AND merchant_id = CAST($2 AS TEXT)
          AND status = 'active'
          AND deleted_at IS NULL
        ORDER BY updated_at DESC, id DESC
        LIMIT 2
        "#,
    )
    .bind(provider_code)
    .bind(merchant_id)
    .fetch_all(pool)
    .await
    .map_err(|error| store_error("failed to load payment provider account by merchant", error))?;
    match rows.as_slice() {
        [] => Ok(None),
        [row] => Ok(Some(
            hydrate_postgres(pool, map_provider_account_row_postgres(row)).await?,
        )),
        _ => Err(CommerceServiceError::conflict(
            "multiple active payment provider accounts match merchant identity",
        )),
    }
}
fn map_provider_account_row_postgres(row: &sqlx::postgres::PgRow) -> PaymentProviderAccountRecord {
    let metadata = metadata_from_jsonb(row.try_get("metadata"));
    PaymentProviderAccountRecord {
        id: string_cell(row, "id"),
        tenant_id: string_cell(row, "tenant_id"),
        organization_id: row.try_get("organization_id").ok().flatten(),
        provider_code: string_cell(row, "provider_code"),
        merchant_id: row.try_get("merchant_id").ok().flatten(),
        environment: string_cell(row, "environment"),
        secret_ref: string_cell(row, "secret_ref"),
        webhook_secret_ref: row.try_get("webhook_secret_ref").ok().flatten(),
        certificate_ref: row.try_get("certificate_ref").ok().flatten(),
        primary_secret: None,
        webhook_secret: None,
        certificate: None,
        metadata,
    }
}
fn uses_database_credentials(record: &PaymentProviderAccountRecord) -> bool {
    record.secret_ref.starts_with("database:")
        || record
            .webhook_secret_ref
            .as_deref()
            .is_some_and(|value| value.starts_with("database:"))
        || record
            .certificate_ref
            .as_deref()
            .is_some_and(|value| value.starts_with("database:"))
}
async fn hydrate_postgres(
    pool: &Pool<Postgres>,
    mut record: PaymentProviderAccountRecord,
) -> Result<PaymentProviderAccountRecord, CommerceServiceError> {
    if uses_database_credentials(&record) {
        let credentials = load_provider_credentials_postgres(
            pool,
            &record.tenant_id,
            record.organization_id.as_deref(),
            &record.id,
        )
        .await?;
        record.primary_secret = credentials.primary_secret;
        record.webhook_secret = credentials.webhook_secret;
        record.certificate = credentials.certificate;
    }
    Ok(record)
}