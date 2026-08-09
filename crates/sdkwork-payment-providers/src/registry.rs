use std::collections::HashMap;
use std::sync::Arc;

use crate::adapter::PaymentProviderAdapter;
use crate::alipay::{AlipayPaymentProviderAdapter, AlipayPaymentProviderConfig, RsaAlipaySigner};
use crate::credentials::{
    build_order_payment_webhook_url, ProviderAccountBinding, ProviderCredentialBundle,
};
use crate::stripe::{StripePaymentProviderAdapter, StripePaymentProviderConfig};
use crate::wechat_pay::{WeChatPayProviderAdapter, WeChatPayProviderConfig};
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
        // The sandbox webhook adapter is always available so local development
        // can simulate a PSP payment-success callback through the order
        // webhook route (`/app/v3/api/orders/payments/webhooks/sandbox`).
        // Its create/cancel/refund operations are unsupported; those are
        // handled as no-ops by `operations.rs` before any adapter lookup.
        registry
            .adapters
            .insert("sandbox".to_owned(), Arc::new(SandboxWebhookPaymentProviderAdapter::new()));
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
        };
        if let Ok(adapter) =
            WeChatPayProviderAdapter::new(provider_config, config.platform_public_key_pem)
        {
            self.adapters
                .insert("wechat_pay".to_owned(), Arc::new(adapter));
        }
    }
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
    pub platform_public_key_pem: Option<String>,
}
