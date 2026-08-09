use sdkwork_contract_service::CommerceServiceError;
use sdkwork_payment_providers::{
    payment_credential_cipher, CredentialCipherScope, EncryptedPaymentCredential,
};
use sqlx::{PgPool, Row, };
const PRIMARY_SECRET: &str = "primary_secret";
const WEBHOOK_SECRET: &str = "webhook_secret";
const CERTIFICATE: &str = "certificate";
#[derive(Clone, Default)]
pub struct ProviderCredentialWrite {
    pub primary_secret: Option<String>,
    pub webhook_secret: Option<String>,
    pub certificate: Option<String>,
}
#[derive(Clone, Default)]
pub struct ProviderCredentialSet {
    pub primary_secret: Option<String>,
    pub webhook_secret: Option<String>,
    pub certificate: Option<String>,
}
impl ProviderCredentialWrite {
    fn normalized(self) -> Vec<(&'static str, String)> {
        [
            (PRIMARY_SECRET, self.primary_secret),
            (WEBHOOK_SECRET, self.webhook_secret),
            (CERTIFICATE, self.certificate),
        ]
        .into_iter()
        .filter_map(|(kind, value)| {
            value
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
                .map(|value| (kind, value))
        })
        .collect()
    }
}
pub async fn rotate_provider_credentials_postgres(
    pool: &PgPool,
    tenant_id: &str,
    organization_id: Option<&str>,
    provider_account_id: &str,
    write: ProviderCredentialWrite,
) -> Result<(), CommerceServiceError> {
    let encrypted = encrypt_writes(tenant_id, provider_account_id, write)?;
    if encrypted.is_empty() {
        return Ok(());
    }
    let mut transaction = pool.begin().await.map_err(store_error)?;
    ensure_account_postgres(
        &mut transaction,
        tenant_id,
        organization_id,
        provider_account_id,
    )
    .await?;
    for (kind, envelope) in encrypted {
        let version = sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(MAX(version), 0) + 1 FROM commerce_payment_provider_credential WHERE tenant_id = CAST($1 AS TEXT) AND provider_account_id = CAST($2 AS TEXT) AND credential_kind = $3",
        )
        .bind(tenant_id).bind(provider_account_id).bind(kind)
        .fetch_one(&mut *transaction).await.map_err(store_error)?;
        sqlx::query("UPDATE commerce_payment_provider_credential SET status = 'superseded', updated_at = CURRENT_TIMESTAMP WHERE tenant_id = CAST($1 AS TEXT) AND provider_account_id = CAST($2 AS TEXT) AND credential_kind = $3 AND status = 'active' AND deleted_at IS NULL")
            .bind(tenant_id).bind(provider_account_id).bind(kind)
            .execute(&mut *transaction).await.map_err(store_error)?;
        sqlx::query("INSERT INTO commerce_payment_provider_credential (id, tenant_id, organization_id, provider_account_id, credential_kind, ciphertext, encryption_key_id, encryption_algorithm, fingerprint_sha256, status, version) VALUES (CAST($1 AS TEXT), CAST($2 AS TEXT), CAST($3 AS TEXT), CAST($4 AS TEXT), $5, $6, $7, $8, $9, 'active', $10)")
            .bind(uuid::Uuid::new_v4().to_string()).bind(tenant_id).bind(organization_id)
            .bind(provider_account_id).bind(kind).bind(envelope.ciphertext)
            .bind(envelope.encryption_key_id).bind(envelope.encryption_algorithm)
            .bind(envelope.fingerprint_sha256).bind(version)
            .execute(&mut *transaction).await.map_err(store_error)?;
        update_legacy_marker_postgres(&mut transaction, provider_account_id, kind).await?;
    }
    transaction.commit().await.map_err(store_error)
}
pub async fn load_provider_credentials_postgres(
    pool: &PgPool,
    tenant_id: &str,
    organization_id: Option<&str>,
    provider_account_id: &str,
) -> Result<ProviderCredentialSet, CommerceServiceError> {
    let rows = sqlx::query("SELECT credential_kind, ciphertext, encryption_key_id, encryption_algorithm FROM commerce_payment_provider_credential WHERE tenant_id = CAST($1 AS TEXT) AND ((organization_id = CAST($2 AS TEXT)) OR (organization_id IS NULL AND $2 IS NULL) OR (organization_id = '0' AND $2 IS NULL)) AND provider_account_id = CAST($3 AS TEXT) AND status = 'active' AND deleted_at IS NULL")
        .bind(tenant_id).bind(organization_id).bind(provider_account_id)
        .fetch_all(pool).await.map_err(store_error)?;
    decrypt_rows(
        tenant_id,
        provider_account_id,
        rows.iter().map(|row| {
            (
                row.try_get::<String, _>("credential_kind")
                    .unwrap_or_default(),
                row.try_get::<String, _>("ciphertext").unwrap_or_default(),
                row.try_get::<String, _>("encryption_key_id")
                    .unwrap_or_default(),
                row.try_get::<String, _>("encryption_algorithm")
                    .unwrap_or_default(),
            )
        }),
    )
}
fn encrypt_writes(
    tenant_id: &str,
    provider_account_id: &str,
    write: ProviderCredentialWrite,
) -> Result<Vec<(&'static str, EncryptedPaymentCredential)>, CommerceServiceError> {
    let cipher = payment_credential_cipher().map_err(credential_error)?;
    write
        .normalized()
        .into_iter()
        .map(|(kind, value)| {
            cipher
                .encrypt(
                    CredentialCipherScope {
                        tenant_id,
                        provider_account_id,
                        credential_kind: kind,
                    },
                    &value,
                )
                .map(|encrypted| (kind, encrypted))
                .map_err(credential_error)
        })
        .collect()
}
fn decrypt_rows(
    tenant_id: &str,
    provider_account_id: &str,
    rows: impl Iterator<Item = (String, String, String, String)>,
) -> Result<ProviderCredentialSet, CommerceServiceError> {
    let cipher = payment_credential_cipher().map_err(credential_error)?;
    let mut set = ProviderCredentialSet::default();
    for (kind, ciphertext, key_id, algorithm) in rows {
        let plaintext = cipher
            .decrypt(
                CredentialCipherScope {
                    tenant_id,
                    provider_account_id,
                    credential_kind: &kind,
                },
                &ciphertext,
                &key_id,
                &algorithm,
            )
            .map_err(credential_error)?;
        match kind.as_str() {
            PRIMARY_SECRET => set.primary_secret = Some(plaintext),
            WEBHOOK_SECRET => set.webhook_secret = Some(plaintext),
            CERTIFICATE => set.certificate = Some(plaintext),
            _ => return Err(credential_error("unsupported credential kind")),
        }
    }
    Ok(set)
}
async fn ensure_account_postgres(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: &str,
    organization_id: Option<&str>,
    provider_account_id: &str,
) -> Result<(), CommerceServiceError> {
    let found = sqlx::query_scalar::<_, i64>("SELECT 1 FROM commerce_payment_provider_account WHERE id = CAST($1 AS TEXT) AND tenant_id = CAST($2 AS TEXT) AND ((organization_id = CAST($3 AS TEXT)) OR (organization_id IS NULL AND $3 IS NULL) OR (organization_id = '0' AND $3 IS NULL)) AND deleted_at IS NULL")
        .bind(provider_account_id).bind(tenant_id).bind(organization_id)
        .fetch_optional(&mut **transaction).await.map_err(store_error)?;
    if found.is_none() {
        return Err(CommerceServiceError::not_found(
            "payment provider account was not found",
        ));
    }
    Ok(())
}
async fn update_legacy_marker_postgres(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    account_id: &str,
    kind: &str,
) -> Result<(), CommerceServiceError> {
    let column = credential_marker_column(kind)?;
    let sql = format!("UPDATE commerce_payment_provider_account SET {column} = $1, version = version + 1, updated_at = CURRENT_TIMESTAMP WHERE id = CAST($2 AS TEXT)");
    sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(format!("database:{kind}"))
        .bind(account_id)
        .execute(&mut **transaction)
        .await
        .map_err(store_error)?;
    Ok(())
}
fn credential_marker_column(kind: &str) -> Result<&'static str, CommerceServiceError> {
    match kind {
        PRIMARY_SECRET => Ok("secret_ref"),
        WEBHOOK_SECRET => Ok("webhook_secret_ref"),
        CERTIFICATE => Ok("certificate_ref"),
        _ => Err(credential_error("unsupported credential kind")),
    }
}
fn store_error(error: sqlx::Error) -> CommerceServiceError {
    CommerceServiceError::storage(format!("payment provider credential store failed: {error}"))
}
fn credential_error(_error: impl std::fmt::Display) -> CommerceServiceError {
    CommerceServiceError::storage("payment provider credential operation failed")
}