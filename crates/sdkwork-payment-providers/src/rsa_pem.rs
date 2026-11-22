//! RSA private-key PEM parsing shared by the RSA-signed providers.
//!
//! The PSP consoles and third-party tooling disagree about private-key encoding:
//! the Alipay open platform exports PKCS#1 (`-----BEGIN RSA PRIVATE KEY-----`),
//! WeChat Pay tooling emits PKCS#8, and operators convert between them freely.
//! Accepting both keeps a valid key from being reported as an unconfigured
//! provider, which is indistinguishable from missing credentials at checkout.

use rsa::pkcs1::DecodeRsaPrivateKey;
use rsa::pkcs8::DecodePrivateKey;
use rsa::RsaPrivateKey;

use crate::adapter::PaymentAdapterOperation;
use crate::error::{ProviderError, ProviderResult};

/// Parses an RSA private key in PKCS#8 or PKCS#1 PEM form.
///
/// `provider_label` names the caller in the error so the operator sees which
/// provider's key was rejected.
pub fn parse_rsa_private_key_pem(
    provider_label: &str,
    private_key_pem: &str,
) -> ProviderResult<RsaPrivateKey> {
    if let Ok(private_key) = RsaPrivateKey::from_pkcs8_pem(private_key_pem) {
        return Ok(private_key);
    }
    RsaPrivateKey::from_pkcs1_pem(private_key_pem).map_err(|error| {
        ProviderError::invalid_request(
            PaymentAdapterOperation::CreatePaymentIntent,
            format!(
                "invalid {provider_label} merchant private key (PKCS#8 or PKCS#1 PEM required): {error}"
            ),
        )
    })
}

#[cfg(test)]
mod tests {
    use rsa::pkcs1::EncodeRsaPrivateKey;
    use rsa::pkcs8::EncodePrivateKey;
    use rsa::traits::PublicKeyParts;
    use rsa::RsaPrivateKey;

    use super::*;

    fn test_key() -> RsaPrivateKey {
        RsaPrivateKey::new(&mut rand::thread_rng(), 2048).expect("rsa key")
    }

    #[test]
    fn both_pkcs8_and_pkcs1_pem_are_accepted() {
        let key = test_key();
        let modulus = key.n().clone();
        let pkcs8 = key
            .to_pkcs8_pem(rsa::pkcs8::LineEnding::LF)
            .expect("pkcs8 pem")
            .to_string();
        let pkcs1 = key
            .to_pkcs1_pem(rsa::pkcs1::LineEnding::LF)
            .expect("pkcs1 pem")
            .to_string();
        for (label, pem) in [("pkcs8", pkcs8), ("pkcs1", pkcs1)] {
            let parsed = parse_rsa_private_key_pem("alipay", &pem)
                .unwrap_or_else(|error| panic!("{label} must parse: {error:?}"));
            assert_eq!(parsed.n(), &modulus, "{label} must yield the same key");
        }
    }

    #[test]
    fn a_garbage_key_names_the_provider_in_the_error() {
        let error = parse_rsa_private_key_pem("alipay", "not a key").expect_err("must reject");
        let rendered = format!("{error:?}");
        assert!(rendered.contains("alipay"), "unexpected error: {rendered}");
    }
}
