use crate::registry::{AlipayRegistryConfig, WeChatPayRegistryConfig};
use crate::stripe::StripePaymentProviderConfig;

/// Canonical PSP notify path (HTTP owned by sdkwork-order).
pub const ORDER_PAYMENT_WEBHOOK_PATH: &str = "/app/v3/api/orders/payments/webhooks/{providerCode}";

/// Builds the order-gateway PSP notify URL for `provider_code`.
pub fn build_order_payment_webhook_url(base: &str, provider_code: &str) -> String {
    format!(
        "{}{}",
        base.trim_end_matches('/'),
        ORDER_PAYMENT_WEBHOOK_PATH.replace("{providerCode}", provider_code)
    )
}

/// Resolves a legacy `secret_ref` pointer to a runtime secret.
///
/// New provider credentials are database-backed and bypass this compatibility path.
pub fn resolve_secret_ref(secret_ref: &str) -> Option<String> {
    if secret_ref.starts_with("database:") {
        return None;
    }
    env_required(secret_ref)
}

/// Tenant-scoped provider account binding (from `commerce_payment_provider_account`).
#[derive(Clone, Eq, PartialEq)]
pub struct ProviderAccountBinding {
    pub provider_code: String,
    pub merchant_id: Option<String>,
    pub environment: String,
    pub secret_ref: String,
    pub webhook_secret_ref: Option<String>,
    pub certificate_ref: Option<String>,
    pub primary_secret: Option<String>,
    pub webhook_secret: Option<String>,
    pub certificate: Option<String>,
    pub metadata: serde_json::Value,
}

#[derive(Clone)]
pub struct ProviderCredentialBundle {
    pub stripe: Option<StripePaymentProviderConfig>,
    pub alipay: Option<AlipayRegistryConfig>,
    pub wechat_pay: Option<WeChatPayRegistryConfig>,
    pub webhook_base_url: Option<String>,
}

impl ProviderCredentialBundle {
    pub fn from_env() -> Self {
        Self {
            stripe: load_stripe(),
            alipay: load_alipay(),
            wechat_pay: load_wechat_pay(),
            webhook_base_url: load_webhook_base_url(),
        }
    }

    pub fn provider_notify_url(&self, provider_code: &str) -> Option<String> {
        let base = self.webhook_base_url.as_deref()?;
        Some(build_order_payment_webhook_url(base, provider_code))
    }

    /// Merges tenant-scoped `commerce_payment_provider_account` credentials.
    ///
    /// Account rows override env defaults for the matching `provider_code` when secrets resolve.
    pub fn with_provider_account(mut self, account: &ProviderAccountBinding) -> Self {
        let provider_code = account.provider_code.to_ascii_lowercase();
        match provider_code.as_str() {
            "stripe" => merge_stripe_account(&mut self, account),
            "alipay" => merge_alipay_account(&mut self, account),
            "wechat_pay" => merge_wechat_account(&mut self, account),
            _ => {}
        }
        self
    }
}

fn load_stripe() -> Option<StripePaymentProviderConfig> {
    let secret_key = env_required("STRIPE_SECRET_KEY")?;
    Some(StripePaymentProviderConfig {
        secret_key,
        webhook_secret: env_optional("STRIPE_WEBHOOK_SECRET"),
        publishable_key: env_optional("STRIPE_PUBLISHABLE_KEY"),
    })
}

fn load_alipay() -> Option<AlipayRegistryConfig> {
    Some(AlipayRegistryConfig {
        app_id: env_required("ALIPAY_APP_ID")?,
        private_key_pem: env_required("ALIPAY_PRIVATE_KEY_PEM")?,
        alipay_public_key_pem: env_required("ALIPAY_PUBLIC_KEY_PEM")?,
        notify_url: env_optional("ALIPAY_NOTIFY_URL"),
        return_url: env_optional("ALIPAY_RETURN_URL"),
        sandbox: env_optional("ALIPAY_SANDBOX")
            .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            .unwrap_or(false),
    })
}

fn load_wechat_pay() -> Option<WeChatPayRegistryConfig> {
    Some(WeChatPayRegistryConfig {
        app_id: env_required("WECHAT_PAY_APP_ID")?,
        mch_id: env_required("WECHAT_PAY_MCH_ID")?,
        merchant_serial_no: env_required("WECHAT_PAY_MERCHANT_SERIAL_NO")?,
        merchant_private_key_pem: env_required("WECHAT_PAY_PRIVATE_KEY_PEM")?,
        api_v3_key: env_required("WECHAT_PAY_API_V3_KEY")?,
        notify_url: env_optional("WECHAT_PAY_NOTIFY_URL"),
        platform_public_key_pem: env_optional("WECHAT_PAY_PLATFORM_PUBLIC_KEY_PEM"),
    })
}

fn env_required(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn env_optional(key: &str) -> Option<String> {
    env_required(key)
}

fn load_webhook_base_url() -> Option<String> {
    env_optional("ORDER_PAYMENT_WEBHOOK_BASE_URL")
}

pub struct EnvPaymentCredentialResolver;

impl EnvPaymentCredentialResolver {
    pub fn load() -> ProviderCredentialBundle {
        ProviderCredentialBundle::from_env()
    }
}

fn merge_stripe_account(bundle: &mut ProviderCredentialBundle, account: &ProviderAccountBinding) {
    let Some(secret_key) = account
        .primary_secret
        .clone()
        .or_else(|| resolve_secret_ref(&account.secret_ref))
    else {
        return;
    };
    bundle.stripe = Some(StripePaymentProviderConfig {
        secret_key,
        webhook_secret: account.webhook_secret.clone().or_else(|| {
            account
                .webhook_secret_ref
                .as_ref()
                .and_then(|value| resolve_secret_ref(value))
        }),
        publishable_key: metadata_string(&account.metadata, "publishableKey"),
    });
}

fn merge_alipay_account(bundle: &mut ProviderCredentialBundle, account: &ProviderAccountBinding) {
    let app_id = account
        .merchant_id
        .clone()
        .or_else(|| metadata_string(&account.metadata, "appId"));
    let Some(app_id) = app_id.filter(|value| !value.trim().is_empty()) else {
        return;
    };
    let Some(private_key_pem) = account
        .primary_secret
        .clone()
        .or_else(|| resolve_secret_ref(&account.secret_ref))
    else {
        return;
    };
    let Some(alipay_public_key_pem) = account.certificate.clone().or_else(|| {
        account
            .certificate_ref
            .as_ref()
            .and_then(|value| resolve_secret_ref(value))
    }) else {
        return;
    };
    bundle.alipay = Some(AlipayRegistryConfig {
        app_id,
        private_key_pem,
        alipay_public_key_pem,
        notify_url: metadata_string(&account.metadata, "notifyUrl"),
        return_url: metadata_string(&account.metadata, "returnUrl"),
        sandbox: account.environment.eq_ignore_ascii_case("sandbox"),
    });
}

fn merge_wechat_account(bundle: &mut ProviderCredentialBundle, account: &ProviderAccountBinding) {
    let Some(app_id) = metadata_string(&account.metadata, "appId") else {
        return;
    };
    let Some(mch_id) = account
        .merchant_id
        .clone()
        .filter(|value| !value.trim().is_empty())
    else {
        return;
    };
    let Some(merchant_serial_no) = metadata_string(&account.metadata, "merchantSerialNo") else {
        return;
    };
    let Some(merchant_private_key_pem) = account
        .primary_secret
        .clone()
        .or_else(|| resolve_secret_ref(&account.secret_ref))
    else {
        return;
    };
    let Some(api_v3_key) = account.webhook_secret.clone().or_else(|| {
        account
            .webhook_secret_ref
            .as_ref()
            .and_then(|value| resolve_secret_ref(value))
    }) else {
        return;
    };
    let Some(platform_public_key_pem) = account.certificate.clone().or_else(|| {
        account
            .certificate_ref
            .as_ref()
            .and_then(|value| resolve_secret_ref(value))
    }) else {
        return;
    };
    bundle.wechat_pay = Some(WeChatPayRegistryConfig {
        app_id,
        mch_id,
        merchant_serial_no,
        merchant_private_key_pem,
        api_v3_key,
        notify_url: metadata_string(&account.metadata, "notifyUrl"),
        platform_public_key_pem: Some(platform_public_key_pem),
    });
}

fn metadata_string(metadata: &serde_json::Value, key: &str) -> Option<String> {
    metadata
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wechat_account_uses_database_values_including_notify_url() {
        let bundle = ProviderCredentialBundle {
            stripe: None,
            alipay: None,
            wechat_pay: None,
            webhook_base_url: None,
        };
        let account = ProviderAccountBinding {
            provider_code: "wechat_pay".to_owned(),
            merchant_id: Some("1900000109".to_owned()),
            environment: "production".to_owned(),
            secret_ref: "database:primary_secret".to_owned(),
            webhook_secret_ref: Some("database:webhook_secret".to_owned()),
            certificate_ref: Some("database:certificate".to_owned()),
            primary_secret: Some("merchant-private-key".to_owned()),
            webhook_secret: Some("12345678901234567890123456789012".to_owned()),
            certificate: Some("platform-certificate".to_owned()),
            metadata: serde_json::json!({
                "appId": "wx-app-id",
                "merchantSerialNo": "merchant-serial",
                "notifyUrl": "https://pay.example.com/app/v3/api/orders/payments/webhooks/wechat_pay"
            }),
        };

        let resolved = bundle.with_provider_account(&account);
        let wechat = resolved.wechat_pay.expect("WeChat database credentials");
        assert_eq!(wechat.mch_id, "1900000109");
        assert_eq!(
            wechat.notify_url.as_deref(),
            Some("https://pay.example.com/app/v3/api/orders/payments/webhooks/wechat_pay")
        );
    }
}
