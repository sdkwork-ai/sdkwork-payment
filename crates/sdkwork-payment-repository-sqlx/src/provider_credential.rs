use sdkwork_contract_service::CommerceServiceError;
use sdkwork_payment_providers::{
    generate_development_credentials, has_environment_provider_credentials,
    payment_credential_cipher, CredentialCipherScope, EncryptedPaymentCredential,
};
use sqlx::{PgPool, Row};
const PRIMARY_SECRET: &str = "primary_secret";
const WEBHOOK_SECRET: &str = "webhook_secret";
const CERTIFICATE: &str = "certificate";
/// Existence probe for the active provider account under the subject scope.
/// The literal is explicitly typed `1::bigint` so sqlx decodes it into the
/// `i64` scalar: an untyped `SELECT 1` literal is `INT4` and fails decoding
/// with "Rust type `i64` (as SQL type `INT8`) is not compatible with SQL
/// type `INT4`" under sqlx 0.9.
const ENSURE_ACCOUNT_POSTGRES_SQL: &str = "SELECT 1::bigint FROM commerce_payment_provider_account WHERE id = CAST($1 AS TEXT) AND tenant_id = CAST($2 AS TEXT) AND ((organization_id = CAST($3 AS TEXT)) OR (organization_id IS NULL AND $3 IS NULL) OR (organization_id = '0' AND $3 IS NULL)) AND deleted_at IS NULL";
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
    // Platform rows persist the sentinel organization scope (`"0"`) so that
    // personal-login (no-org) sessions never write NULL into the NOT NULL
    // `organization_id` column (DATABASE_SPEC DB090).
    let organization_id = Some(
        organization_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("0"),
    );
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
/// Idempotently fills bootstrap provider accounts with real-format test
/// credentials.
///
/// Bootstrap accounts (`metadata.bootstrap=true`) are seeded active so every
/// checkout path runs end to end against the real provider adapters. This
/// host-side bootstrap generates real-format test credentials (parseable RSA
/// key pairs, Stripe `sk_test_` keys, a 32-char WeChat API v3 key), encrypts
/// them through the same credential cipher as admin writes, and marks the
/// account tested. Accounts that already carry active credentials
/// (operator-configured) or whose provider credentials are fully provided
/// through environment variables are skipped untouched.
pub async fn ensure_development_provider_credentials_postgres(
    pool: &PgPool,
) -> Result<(), CommerceServiceError> {
    let accounts = sqlx::query(
        "SELECT id, tenant_id, organization_id, provider_code FROM commerce_payment_provider_account WHERE deleted_at IS NULL AND status = 'active' AND LOWER(provider_code) IN ('stripe', 'alipay', 'wechat_pay') AND CAST(metadata AS TEXT) LIKE '%bootstrap%' ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .map_err(store_error)?;
    for row in accounts {
        let provider_account_id: String = row.try_get("id").unwrap_or_default();
        let tenant_id: String = row.try_get("tenant_id").unwrap_or_default();
        let organization_id: Option<String> = row.try_get("organization_id").ok().flatten();
        let provider_code: String = row.try_get("provider_code").unwrap_or_default();
        if provider_account_id.is_empty() || tenant_id.is_empty() {
            continue;
        }
        let has_active_credentials = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*)::bigint FROM commerce_payment_provider_credential WHERE provider_account_id = CAST($1 AS TEXT) AND status = 'active' AND deleted_at IS NULL",
        )
        .bind(&provider_account_id)
        .fetch_one(pool)
        .await
        .map_err(store_error)?;
        if has_active_credentials > 0 {
            continue;
        }
        if has_environment_provider_credentials(&provider_code) {
            continue;
        }
        let generated = generate_development_credentials(&provider_code).map_err(|message| {
            CommerceServiceError::storage(format!(
                "payment provider development credential generation failed: {message}"
            ))
        })?;
        rotate_provider_credentials_postgres(
            pool,
            &tenant_id,
            organization_id.as_deref(),
            &provider_account_id,
            ProviderCredentialWrite {
                primary_secret: Some(generated.primary_secret),
                webhook_secret: generated.webhook_secret,
                certificate: generated.certificate,
            },
        )
        .await?;
        // WeChat Pay bootstrap accounts use the official recommended public key
        // mode: patch the verification mode and the generated public key ID
        // (`Wechatpay-Serial` matching) into the account metadata.
        if provider_code.eq_ignore_ascii_case("wechat_pay") {
            if let Some(public_key_id) = generated.wechatpay_public_key_id.as_deref() {
                sqlx::query(
                    "UPDATE commerce_payment_provider_account SET metadata = COALESCE(metadata, '{}'::jsonb) || jsonb_build_object('signVerifyMode', 'wechatpay_public_key', 'wechatpayPublicKeyId', $2), updated_at = CURRENT_TIMESTAMP WHERE id = CAST($1 AS TEXT)",
                )
                .bind(&provider_account_id)
                .bind(public_key_id)
                .execute(pool)
                .await
                .map_err(store_error)?;
            }
        }
        sqlx::query("UPDATE commerce_payment_provider_account SET last_test_status = 'success', last_tested_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP WHERE id = CAST($1 AS TEXT)")
            .bind(&provider_account_id)
            .execute(pool)
            .await
            .map_err(store_error)?;
    }
    Ok(())
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
    let found = sqlx::query_scalar::<_, i64>(ENSURE_ACCOUNT_POSTGRES_SQL)
        .bind(provider_account_id)
        .bind(tenant_id)
        .bind(organization_id)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(store_error)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_account_probe_literal_is_bigint_typed() {
        // Regression guard: an untyped `SELECT 1` literal is INT4, which sqlx
        // 0.9 refuses to decode into the `i64` scalar (`query_scalar::<_, i64>`)
        // and turned provider credential rotation into a 50001 internal error.
        assert!(
            ENSURE_ACCOUNT_POSTGRES_SQL.starts_with("SELECT 1::bigint FROM"),
            "existence probe must use a bigint-typed literal for i64 decoding"
        );
    }

    #[test]
    fn ensure_account_probe_covers_tenant_and_organization_scopes() {
        assert!(ENSURE_ACCOUNT_POSTGRES_SQL.contains("tenant_id = CAST($2 AS TEXT)"));
        assert!(ENSURE_ACCOUNT_POSTGRES_SQL.contains("organization_id IS NULL AND $3 IS NULL"));
        assert!(ENSURE_ACCOUNT_POSTGRES_SQL.contains("organization_id = '0' AND $3 IS NULL"));
        assert!(ENSURE_ACCOUNT_POSTGRES_SQL.contains("deleted_at IS NULL"));
    }
}
