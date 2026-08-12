//! Real-format development credential generation.
//!
//! Development bootstrap provider accounts are seeded active (see
//! `database/seeds/common/003_development_templates.sql`); the payment service
//! host fills them with real-format test credentials on first boot so the
//! one-cent test payment drives the real provider adapters — real HTTP calls
//! to the PSP — without a manual Test → Activate gate.
//!
//! The generated values are structurally valid (Stripe `sk_test_…` keys,
//! parseable RSA-2048 PKCS#8 PEM private keys, a 32-char WeChat API v3 key),
//! but they target the template test merchant identifiers, so a real PSP call
//! either succeeds — after the operator replaces them with real credentials in
//! Provider Accounts — or fails with the PSP's own authentic error. Nothing
//! here is a mock: the adapter, the HTTP call, and the error are all real.

use rsa::pkcs8::{EncodePrivateKey, EncodePublicKey, LineEnding};
use rsa::{RsaPrivateKey, RsaPublicKey};
use uuid::Uuid;

/// Development credential set generated for a bootstrap provider account.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DevelopmentCredentials {
    pub primary_secret: String,
    pub webhook_secret: Option<String>,
    pub certificate: Option<String>,
    /// WeChat Pay public key ID (`PUB_KEY_ID_` prefix, WeChat Pay public key
    /// mode) so bootstrap accounts exercise the official recommended
    /// verification credential system end to end.
    pub wechatpay_public_key_id: Option<String>,
}

/// Generates real-format test credentials for `provider_code`
/// (`stripe` / `alipay` / `wechat_pay`).
pub fn generate_development_credentials(
    provider_code: &str,
) -> Result<DevelopmentCredentials, String> {
    match provider_code.trim().to_ascii_lowercase().as_str() {
        "stripe" => Ok(DevelopmentCredentials {
            primary_secret: format!("sk_test_{}", random_hex(48)),
            webhook_secret: Some(format!("whsec_{}", random_hex(32))),
            certificate: None,
            wechatpay_public_key_id: None,
        }),
        "alipay" => {
            let (private_pem, public_pem) = generate_rsa_keypair_pem()?;
            Ok(DevelopmentCredentials {
                primary_secret: private_pem,
                webhook_secret: None,
                certificate: Some(public_pem),
                wechatpay_public_key_id: None,
            })
        }
        "wechat_pay" | "wechat-pay" => {
            let (private_pem, public_pem) = generate_rsa_keypair_pem()?;
            Ok(DevelopmentCredentials {
                primary_secret: private_pem,
                // WeChat API v3 key: 32 arbitrary characters (hex form).
                webhook_secret: Some(random_hex(32)),
                // The verification key slot accepts a SPKI public key PEM
                // (`pub_key.pem` equivalent), which the adapter uses to verify
                // webhook and response signatures in WeChat Pay public key mode.
                certificate: Some(public_pem),
                // WeChat Pay public key ID (PUB_KEY_ID_ prefix) carried by the
                // `Wechatpay-Serial` response/webhook header in public key mode.
                wechatpay_public_key_id: Some(format!("PUB_KEY_ID_{}", random_hex(32))),
            })
        }
        _ => Err(format!(
            "provider {provider_code} has no development credential template"
        )),
    }
}

/// Returns true when the runtime environment already carries the complete
/// credential set for `provider_code` (`STRIPE_SECRET_KEY`, `ALIPAY_*`,
/// `WECHAT_PAY_*`). The host skips auto-filling database credentials for such
/// accounts so operator-provided environment credentials keep taking effect.
pub fn has_environment_provider_credentials(provider_code: &str) -> bool {
    match provider_code.trim().to_ascii_lowercase().as_str() {
        "stripe" => env_set("STRIPE_SECRET_KEY"),
        "alipay" => {
            env_set("ALIPAY_APP_ID")
                && env_set("ALIPAY_PRIVATE_KEY_PEM")
                && env_set("ALIPAY_PUBLIC_KEY_PEM")
        }
        "wechat_pay" | "wechat-pay" => {
            env_set("WECHAT_PAY_APP_ID")
                && env_set("WECHAT_PAY_MCH_ID")
                && env_set("WECHAT_PAY_MERCHANT_SERIAL_NO")
                && env_set("WECHAT_PAY_PRIVATE_KEY_PEM")
                && env_set("WECHAT_PAY_API_V3_KEY")
        }
        _ => false,
    }
}

fn env_set(key: &str) -> bool {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_owned())
        .is_some_and(|value| !value.is_empty())
}

fn generate_rsa_keypair_pem() -> Result<(String, String), String> {
    let mut rng = rand::thread_rng();
    let private_key = RsaPrivateKey::new(&mut rng, 2048)
        .map_err(|error| format!("development RSA key generation failed: {error}"))?;
    let private_pem = private_key
        .to_pkcs8_pem(LineEnding::LF)
        .map_err(|error| format!("development RSA private key encoding failed: {error}"))?
        .to_string();
    let public_pem = RsaPublicKey::from(&private_key)
        .to_public_key_pem(LineEnding::LF)
        .map_err(|error| format!("development RSA public key encoding failed: {error}"))?;
    Ok((private_pem, public_pem))
}

fn random_hex(len: usize) -> String {
    let mut value = String::with_capacity(len);
    while value.len() < len {
        value.push_str(&Uuid::new_v4().simple().to_string());
    }
    value.truncate(len);
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsa::pkcs8::DecodePrivateKey;
    use rsa::pkcs8::DecodePublicKey;
    use rsa::traits::PrivateKeyParts;
    use rsa::traits::PublicKeyParts;

    #[test]
    fn stripe_credentials_use_real_test_key_format() {
        let credentials =
            generate_development_credentials("stripe").expect("stripe template must exist");
        assert!(credentials.primary_secret.starts_with("sk_test_"));
        assert_eq!(credentials.primary_secret.len(), "sk_test_".len() + 48);
        assert!(credentials
            .webhook_secret
            .as_deref()
            .is_some_and(|value| value.starts_with("whsec_")));
        assert!(credentials.certificate.is_none());
    }

    #[test]
    fn rsa_credentials_are_parseable_pkcs8_pem_keys() {
        for provider_code in ["alipay", "wechat_pay", "wechat-pay"] {
            let credentials = generate_development_credentials(provider_code)
                .unwrap_or_else(|error| panic!("{provider_code} template must exist: {error}"));
            let private_key = RsaPrivateKey::from_pkcs8_pem(&credentials.primary_secret)
                .unwrap_or_else(|error| {
                    panic!("{provider_code} private key must parse as PKCS#8 PEM: {error}")
                });
            assert_eq!(
                private_key.size() * 8,
                2048,
                "{provider_code} must be RSA-2048"
            );
            assert!(credentials
                .primary_secret
                .starts_with("-----BEGIN PRIVATE KEY-----"));
            let public_key = credentials
                .certificate
                .as_deref()
                .expect("rsa providers carry a certificate slot");
            let parsed = RsaPublicKey::from_public_key_pem(public_key)
                .unwrap_or_else(|error| panic!("{provider_code} public key must parse: {error}"));
            assert_eq!(
                parsed.n(),
                private_key.n(),
                "{provider_code} keypair must match"
            );
        }
    }

    #[test]
    fn wechat_api_v3_key_is_32_characters() {
        let credentials =
            generate_development_credentials("wechat_pay").expect("wechat template must exist");
        assert_eq!(
            credentials.webhook_secret.as_deref().map(str::len),
            Some(32),
            "WeChat API v3 key must be exactly 32 characters"
        );
        assert!(credentials
            .webhook_secret
            .as_deref()
            .is_some_and(|value| value.chars().all(|c| c.is_ascii_hexdigit())));
    }

    #[test]
    fn generated_credentials_are_randomized() {
        let first = generate_development_credentials("stripe").expect("stripe");
        let second = generate_development_credentials("stripe").expect("stripe");
        assert_ne!(first.primary_secret, second.primary_secret);
        let first = generate_development_credentials("wechat_pay").expect("wechat");
        let second = generate_development_credentials("wechat_pay").expect("wechat");
        assert_ne!(first.primary_secret, second.primary_secret);
    }

    #[test]
    fn unknown_provider_has_no_template() {
        assert!(generate_development_credentials("sandbox").is_err());
        assert!(generate_development_credentials("unknown").is_err());
    }
}
