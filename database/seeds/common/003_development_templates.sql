-- Development bootstrap is immediately usable: the sandbox method is active
-- out of the box, and the real PSP template accounts (stripe/alipay/wechat_pay)
-- start active with real-format test credentials that the payment service host
-- generates and encrypts on first boot (see
-- `ensure_development_provider_credentials`). This lets one-cent test payments
-- drive the real provider adapters (real HTTP calls to the PSP) in development
-- without a manual Test → Activate gate. Production templates (002) still
-- require operator-configured real merchant credentials.
INSERT INTO commerce_payment_method (
    id, tenant_id, organization_id, method_key, display_name, provider_code,
    status, sort_order, scope, currency_code, country_code, metadata,
    idempotency_key, created_at, updated_at
)
VALUES
    ('bootstrap-payment-method-sandbox-test', '100001', '0', 'sandbox_test', 'Sandbox', 'sandbox', 'active', 900, 'organization', 'CNY', 'CN', '{"bootstrap":true,"environment":"development"}', 'bootstrap-payment-method-sandbox-test', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
ON CONFLICT DO NOTHING;

-- Development PSP template accounts mirror the production structure with
-- development-scoped identifiers and the dev platform callback domain. They
-- are active from the start; the payment service host fills real-format test
-- credentials on first boot when no credentials row exists yet.
INSERT INTO commerce_payment_provider_account (
    id, tenant_id, organization_id, account_no, provider_code, merchant_id,
    account_name, environment, settlement_currency, secret_ref, webhook_secret_ref,
    certificate_ref, capabilities, status, metadata, created_at, updated_at
)
VALUES
    ('bootstrap-payment-provider-sandbox', '100001', '0', 'sandbox-primary', 'sandbox', '454576126750169', 'Sandbox Account', 'development', 'CNY', 'database:primary_secret', NULL, NULL, '{"pay":true,"refund":true,"close":true,"query":true}', 'active', '{"bootstrap":true,"environment":"development"}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-payment-provider-stripe-dev', '100001', '0', 'stripe-dev-primary', 'stripe', 'acct_qwYxPWO1l8YO07', 'Stripe Development Account', 'development', 'CNY', 'database:primary_secret', 'database:webhook_secret', NULL, '{"pay":true,"refund":true,"close":true,"query":true}', 'active', '{"bootstrap":true,"environment":"development"}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-payment-provider-alipay-dev', '100001', '0', 'alipay-dev-primary', 'alipay', '2088717271184084', 'Alipay Development Account', 'development', 'CNY', 'database:primary_secret', NULL, 'database:certificate', '{"pay":true,"refund":true,"close":true,"query":true}', 'active', '{"bootstrap":true,"appId":"9341938871096707","environment":"development"}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-payment-provider-wechat-pay-dev', '100001', '0', 'wechat-dev-primary', 'wechat_pay', '1900948059', 'WeChat Pay Development Account', 'development', 'CNY', 'database:primary_secret', 'database:webhook_secret', 'database:certificate', '{"pay":true,"refund":true,"close":true,"query":true}', 'active', '{"bootstrap":true,"appId":"wx0f25e86fc88a446c","merchantSerialNo":"6EB892196BEAA85D5E59B06F077C8A2903683649","signVerifyMode":"wechatpay_public_key","wechatpayPublicKeyId":"PUB_KEY_ID_00000000000000000000000000000001","notifyUrl":"https://api-dev.sdkwork.com/app/v3/api/orders/payments/webhooks/wechat_pay","environment":"development"}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
ON CONFLICT DO NOTHING;

INSERT INTO commerce_payment_channel (
    id, tenant_id, organization_id, channel_no, channel_name, provider_account_id,
    method_id, provider_code, scene_code, currency_code, country_code, status,
    priority, sort_order, metadata, created_at, updated_at
)
VALUES
    ('bootstrap-payment-channel-sandbox-test', '100001', '0', 'sandbox-channel', 'Sandbox', 'bootstrap-payment-provider-sandbox', 'bootstrap-payment-method-sandbox-test', 'sandbox', 'api', 'CNY', 'CN', 'active', 900, 900, '{"bootstrap":true,"environment":"development"}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
ON CONFLICT DO NOTHING;
