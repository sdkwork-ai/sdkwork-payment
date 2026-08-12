use sdkwork_database_sqlx::DatabasePool;
use sdkwork_payment_database_host::{bootstrap_payment_database_from_env, PaymentDatabaseHost};
use sdkwork_payment_providers::{
    install_payment_credential_cipher, payment_credential_cipher_is_installed,
    LocalFilePaymentCredentialCipher, PaymentCredentialCipher,
};
use sdkwork_payment_repository_sqlx::ensure_development_provider_credentials_postgres;
use sdkwork_web_core::WebEnvironment;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const PAYMENT_CREDENTIAL_MASTER_KEY_FILE_ENV: &str = "SDKWORK_PAYMENT_CREDENTIAL_MASTER_KEY_FILE";
const PAYMENT_ENVIRONMENT_KEYS: &[&str] = &[
    "SDKWORK_ENVIRONMENT",
    "SDKWORK_PAYMENT_ENVIRONMENT",
    "PAYMENT_ENVIRONMENT",
    "SDKWORK_ENV",
];

pub struct PaymentServiceHost {
    database: PaymentDatabaseHost,
}

impl PaymentServiceHost {
    pub async fn new() -> Self {
        Self::from_env()
            .await
            .expect("payment service host bootstrap failed")
    }

    pub async fn from_env() -> Result<Self, String> {
        ensure_payment_credential_cipher_from_env()?;
        let database = bootstrap_payment_database_from_env().await?;
        ensure_bootstrap_provider_credentials(&database).await?;
        Ok(Self { database })
    }

    /// Build the payment service host against a caller-provided database pool so
    /// the platform cloud gateway can share its process-wide PostgreSQL pool.
    pub async fn from_pool(pool: DatabasePool) -> Result<Self, String> {
        ensure_payment_credential_cipher_from_env()?;
        let database = sdkwork_payment_database_host::bootstrap_payment_database(pool).await?;
        ensure_bootstrap_provider_credentials(&database).await?;
        Ok(Self { database })
    }

    pub fn database_pool(&self) -> &DatabasePool {
        self.database.pool()
    }

    pub fn database_module(&self) -> std::sync::Arc<sdkwork_database_spi::DefaultDatabaseModule> {
        self.database.module()
    }
}

/// Fills bootstrap provider accounts with real-format test credentials so the
/// one-cent test payment (and any checkout) drives the real provider adapters
/// end to end without an activation gate. No-op on non-PostgreSQL pools; the
/// repository function is idempotent and skips accounts that already carry
/// operator-configured credentials or complete environment credentials.
async fn ensure_bootstrap_provider_credentials(database: &PaymentDatabaseHost) -> Result<(), String> {
    let Some(pool) = database.pool().as_postgres() else {
        return Ok(());
    };
    ensure_development_provider_credentials_postgres(pool)
        .await
        .map_err(|error| format!("payment bootstrap credential fill failed: {}", error.message()))
}

fn ensure_payment_credential_cipher_from_env() -> Result<(), String> {    if payment_credential_cipher_is_installed() {
        return Ok(());
    }

    let environment = sdkwork_web_bootstrap::web_environment_from_env(PAYMENT_ENVIRONMENT_KEYS);
    let production_like = environment == WebEnvironment::Prod;
    let key_path = resolve_payment_credential_master_key_path(
        nonempty_env_path(PAYMENT_CREDENTIAL_MASTER_KEY_FILE_ENV),
        user_home_dir(),
        production_like,
    )?;
    let cipher: Arc<dyn PaymentCredentialCipher> = if production_like {
        Arc::new(LocalFilePaymentCredentialCipher::load(&key_path)?)
    } else {
        Arc::new(LocalFilePaymentCredentialCipher::load_or_create(&key_path)?)
    };

    match install_payment_credential_cipher(cipher) {
        Ok(()) => Ok(()),
        Err(_) if payment_credential_cipher_is_installed() => Ok(()),
        Err(error) => Err(error),
    }
}

fn nonempty_env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn user_home_dir() -> Option<PathBuf> {
    let keys: &[&str] = if cfg!(windows) {
        &["USERPROFILE", "HOME"]
    } else {
        &["HOME", "USERPROFILE"]
    };
    keys.iter().find_map(|key| nonempty_env_path(key))
}

fn resolve_payment_credential_master_key_path(
    configured_path: Option<PathBuf>,
    user_home: Option<PathBuf>,
    production_like: bool,
) -> Result<PathBuf, String> {
    let path = match configured_path {
        Some(path) => path,
        None if production_like => {
            return Err(format!(
                "{PAYMENT_CREDENTIAL_MASTER_KEY_FILE_ENV} is required unless the host installs a payment credential cipher"
            ));
        }
        None => user_home
            .ok_or_else(|| "payment credential user-private directory is unavailable".to_owned())?
            .join(".sdkwork")
            .join("commerce")
            .join("secrets")
            .join("payment-credential-master.key"),
    };

    if !path.is_absolute() {
        return Err(format!(
            "{PAYMENT_CREDENTIAL_MASTER_KEY_FILE_ENV} must be an absolute path"
        ));
    }
    if is_inside_source_checkout(&path) {
        return Err("payment credential key storage must be outside a source checkout".to_owned());
    }
    Ok(path)
}

fn is_inside_source_checkout(path: &Path) -> bool {
    path.ancestors()
        .any(|ancestor| ancestor.join(".git").exists())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn development_key_uses_the_canonical_user_private_secret_directory() {
        let home = std::env::temp_dir().join("sdkwork-payment-home-contract");
        let path = resolve_payment_credential_master_key_path(None, Some(home.clone()), false)
            .expect("development key path");
        assert_eq!(
            path,
            home.join(".sdkwork")
                .join("commerce")
                .join("secrets")
                .join("payment-credential-master.key")
        );
    }

    #[test]
    fn production_requires_an_explicit_key_file() {
        let error = resolve_payment_credential_master_key_path(None, None, true)
            .expect_err("production must reject implicit local keys");
        assert!(error.contains(PAYMENT_CREDENTIAL_MASTER_KEY_FILE_ENV));
    }

    #[test]
    fn configured_key_file_must_be_absolute() {
        let error = resolve_payment_credential_master_key_path(
            Some(PathBuf::from("credential-master.key")),
            None,
            false,
        )
        .expect_err("relative key path must fail");
        assert!(error.contains("absolute path"));
    }
}
