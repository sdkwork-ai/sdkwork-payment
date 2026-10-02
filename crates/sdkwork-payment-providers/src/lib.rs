//! Payment service provider adapters for Stripe, Alipay, and WeChat Pay.

mod adapter;
mod alipay;
mod checkout;
mod credential_cipher;
mod credentials;
mod dev_credentials;
mod error;
mod http;
mod money;
mod operations;
mod paypal;
mod registry;
mod sandbox_webhook;
mod stripe;
mod webhook_peek;
mod wechat_pay;

pub use adapter::{
    normalize_provider_code, PaymentCreateRefundRequest, PaymentProviderAdapter,
    PaymentProviderOperationOutcome, PaymentQueryPaymentIntentRequest, PaymentQueryRefundRequest,
};
pub use adapter::{PaymentNormalizeWebhookRequest, PaymentVerifyWebhookRequest};
pub use checkout::{enrich_pay_owner_order_outcome, CheckoutContext};
pub use money::{
    ensure_currency_supported, money_to_minor, normalized_payment_currency,
    require_positive_minor,
};
pub use credential_cipher::{
    install_payment_credential_cipher, payment_credential_cipher,
    payment_credential_cipher_is_installed, CredentialCipherScope, EncryptedPaymentCredential,
    LocalFilePaymentCredentialCipher, PaymentCredentialCipher, PAYMENT_CREDENTIAL_ALGORITHM,
};
pub use credentials::{
    build_order_payment_webhook_url, resolve_secret_ref, EnvPaymentCredentialResolver,
    ProviderAccountBinding, ProviderCredentialBundle, ORDER_PAYMENT_WEBHOOK_PATH,
};
pub use dev_credentials::{
    generate_development_credentials, has_environment_provider_credentials, DevelopmentCredentials,
};
pub use operations::{
    cancel_provider_payment, create_provider_refund, provider_operation_idempotency_key,
    query_provider_payment_intent, query_provider_refund, ProviderPaymentQueryState,
    ProviderRefundSubmissionState,
};
pub use paypal::{PayPalPaymentProviderAdapter, PayPalPaymentProviderConfig};
pub use registry::{provider_registry_for_account, PaymentProviderRegistry};
pub use sandbox_webhook::SandboxWebhookPaymentProviderAdapter;
pub use webhook_peek::{peek_webhook_routing_fields, WebhookPeekOutcome};
