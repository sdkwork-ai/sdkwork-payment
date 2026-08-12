use std::collections::HashMap;
use std::sync::Arc;

use crate::adapter::PaymentProviderAdapter;
use crate::alipay::{AlipayPaymentProviderAdapter, AlipayPaymentProviderConfig, RsaAlipaySigner};
use crate::credentials::{
    build_order_payment_webhook_url, ProviderAccountBinding, ProviderCredentialBundle,
};
use crate::stripe::{StripePaymentProviderAdapter, StripePaymentProviderConfig};
use crate::wechat_pay::{
    WeChatPayProviderAdapter, WeChatPayProviderConfig, WeChatPaySignVerifyMode,
};
use crate::SandboxWebhookPaymentProviderAdapter;

#[derive(Clone)]
pub struct PaymentProviderRegistry {
    adapters: HashMap<String, Arc<dyn PaymentProviderAdapter>>,
    notify_urls: HashMap<String, String>,
}

impl PaymentProviderRegistry {
    pub fn from_env() -> Self {
        Self::from_credentials(ProviderCredentialBundle::from_env())
    }

    pub fn from_credentials(bundle: ProviderCredentialBundle) -> Self {
        let mut registry = Self {
            adapters: HashMap::new(),
            notify_urls: HashMap::new(),
        };
        let webhook_base = bundle.webhook_base_url.clone();
        registry.register_stripe(bundle.stripe);
        registry.register_alipay(bundle.alipay, webhook_base.as_deref());
        registry.register_wechat_pay(bundle.wechat_pay, webhook_base.as_deref());
        // The sandbox webhook adapter accepts UNSIGNED bodies, so it must
        // never be reachable from the public webhook route outside local
        // development: an attacker who knows an `out_trade_no` could settle
        // an order as paid without payment. Registration is therefore gated —
        // on by default only in dev/test environments, overridable with
        // `SDKWORK_PAYMENT_SANDBOX_WEBHOOK_ENABLED` (0/1) in either direction.
        if sandbox_webhook_enabled() {
            registry.adapters.insert(
                "sandbox".to_owned(),
                Arc::new(SandboxWebhookPaymentProviderAdapter::new()),
            );
        }
        registry
    }

    pub fn resolve(&self, provider_code: &str) -> Option<Arc<dyn PaymentProviderAdapter>> {
        self.adapters
            .get(&provider_code.to_ascii_lowercase())
            .cloned()
    }

    pub fn default_notify_url(&self, provider_code: &str) -> Option<String> {
        self.notify_urls
            .get(&provider_code.to_ascii_lowercase())
            .cloned()
    }

    fn register_stripe(&mut self, config: Option<StripePaymentProviderConfig>) {
        let Some(config) = config else {
            return;
        };
        if let Ok(adapter) = StripePaymentProviderAdapter::with_default_http_client(config) {
            self.adapters.insert("stripe".to_owned(), Arc::new(adapter));
        }
    }

    fn register_alipay(
        &mut self,
        config: Option<AlipayRegistryConfig>,
        webhook_base: Option<&str>,
    ) {
        let Some(mut config) = config else {
            return;
        };
        if config.notify_url.is_none() {
            config.notify_url =
                webhook_base.map(|base| build_order_payment_webhook_url(base, "alipay"));
        }
        if let Some(notify_url) = config.notify_url.clone() {
            self.notify_urls.insert("alipay".to_owned(), notify_url);
        }
        if let Ok(signer) =
            RsaAlipaySigner::from_pkcs8_pem(&config.private_key_pem, &config.alipay_public_key_pem)
        {
            let provider_config = AlipayPaymentProviderConfig {
                app_id: config.app_id,
                notify_url: config.notify_url,
                return_url: config.return_url,
                sandbox: config.sandbox,
            };
            if let Ok(adapter) =
                AlipayPaymentProviderAdapter::new(provider_config, Arc::new(signer))
            {
                self.adapters.insert("alipay".to_owned(), Arc::new(adapter));
            }
        }
    }

    fn register_wechat_pay(
        &mut self,
        config: Option<WeChatPayRegistryConfig>,
        webhook_base: Option<&str>,
    ) {
        let Some(mut config) = config else {
            return;
        };
        if config.notify_url.is_none() {
            config.notify_url =
                webhook_base.map(|base| build_order_payment_webhook_url(base, "wechat_pay"));
        }
        if let Some(notify_url) = config.notify_url.clone() {
            self.notify_urls
                .insert("wechat_pay".to_owned(), notify_url.clone());
        }
        let provider_config = WeChatPayProviderConfig {
            app_id: config.app_id,
            mch_id: config.mch_id,
            merchant_serial_no: config.merchant_serial_no,
            merchant_private_key_pem: config.merchant_private_key_pem,
            api_v3_key: config.api_v3_key,
            notify_url: config.notify_url,
            sign_verify_mode: config.sign_verify_mode,
            verification_key_pem: config.verification_key_pem,
            verification_serial_no: config.verification_serial_no,
        };
        if let Ok(adapter) = WeChatPayProviderAdapter::new(provider_config) {
            self.adapters
                .insert("wechat_pay".to_owned(), Arc::new(adapter));
        }
    }
}

/// Whether the unsigned sandbox webhook adapter may be registered.
///
/// Default: dev/test environments only. `SDKWORK_PAYMENT_SANDBOX_WEBHOOK_ENABLED`
/// overrides in either direction (production deployments that insist on the
/// sandbox simulate route must set it explicitly and accept the forgery risk).
fn sandbox_webhook_enabled() -> bool {
    if let Ok(value) = std::env::var("SDKWORK_PAYMENT_SANDBOX_WEBHOOK_ENABLED") {
        return matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        );
    }
    matches!(
        std::env::var("SDKWORK_ENVIRONMENT")
            .ok()
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref(),
        Some("dev" | "development" | "test" | "testing" | "local")
    )
}

/// Builds a tenant-scoped registry when a provider account binding is present.
pub fn provider_registry_for_account(
    base_bundle: &ProviderCredentialBundle,
    account: Option<ProviderAccountBinding>,
) -> PaymentProviderRegistry {
    match account {
        Some(account) => PaymentProviderRegistry::from_credentials(
            base_bundle.clone().with_provider_account(&account),
        ),
        None => PaymentProviderRegistry::from_credentials(base_bundle.clone()),
    }
}

#[derive(Clone)]
pub struct AlipayRegistryConfig {
    pub app_id: String,
    pub private_key_pem: String,
    pub alipay_public_key_pem: String,
    pub notify_url: Option<String>,
    pub return_url: Option<String>,
    pub sandbox: bool,
}

#[derive(Clone)]
pub struct WeChatPayRegistryConfig {
    pub app_id: String,
    pub mch_id: String,
    pub merchant_serial_no: String,
    pub merchant_private_key_pem: String,
    pub api_v3_key: String,
    pub notify_url: Option<String>,
    /// 验签模式：微信支付公钥（默认、官方推荐）或平台证书。
    pub sign_verify_mode: WeChatPaySignVerifyMode,
    /// 验签密钥 PEM（公钥 `pub_key.pem` 或平台证书 `wechatpay_cert.pem`）。
    pub verification_key_pem: Option<String>,
    /// 微信支付公钥 ID（`PUB_KEY_ID_` 前缀）或平台证书序列号。
    pub verification_serial_no: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// The env read is process-global; all env-touching tests share one lock
    /// so parallel tests can never race each other's env state.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn sandbox_webhook_adapter_is_gated_by_environment() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        std::env::remove_var("SDKWORK_PAYMENT_SANDBOX_WEBHOOK_ENABLED");
        std::env::set_var("SDKWORK_ENVIRONMENT", "prod");
        assert!(!sandbox_webhook_enabled());

        std::env::set_var("SDKWORK_ENVIRONMENT", "dev");
        assert!(sandbox_webhook_enabled());

        // Explicit flag overrides in either direction.
        std::env::set_var("SDKWORK_ENVIRONMENT", "dev");
        std::env::set_var("SDKWORK_PAYMENT_SANDBOX_WEBHOOK_ENABLED", "0");
        assert!(!sandbox_webhook_enabled());

        std::env::set_var("SDKWORK_ENVIRONMENT", "prod");
        std::env::set_var("SDKWORK_PAYMENT_SANDBOX_WEBHOOK_ENABLED", "1");
        assert!(sandbox_webhook_enabled());

        std::env::remove_var("SDKWORK_PAYMENT_SANDBOX_WEBHOOK_ENABLED");
        std::env::remove_var("SDKWORK_ENVIRONMENT");
    }

    #[test]
    fn registry_exposes_the_unsigned_sandbox_adapter_only_when_enabled() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let bundle = ProviderCredentialBundle {
            stripe: None,
            alipay: None,
            wechat_pay: None,
            webhook_base_url: None,
        };
        std::env::set_var("SDKWORK_ENVIRONMENT", "prod");
        let registry = PaymentProviderRegistry::from_credentials(bundle.clone());
        assert!(
            registry.resolve("sandbox").is_none(),
            "production registry must not expose the unsigned sandbox webhook adapter"
        );

        std::env::set_var("SDKWORK_PAYMENT_SANDBOX_WEBHOOK_ENABLED", "1");
        let registry = PaymentProviderRegistry::from_credentials(bundle);
        assert!(
            registry.resolve("sandbox").is_some(),
            "explicit enablement must register the sandbox webhook adapter"
        );

        std::env::remove_var("SDKWORK_PAYMENT_SANDBOX_WEBHOOK_ENABLED");
        std::env::remove_var("SDKWORK_ENVIRONMENT");
    }
}
