-- Development bootstrap is immediately usable through the local sandbox. Real
-- PSP methods remain present in the catalog but cannot be selected until an
-- operator configures their own account and channel.
INSERT INTO commerce_payment_method (
    id, tenant_id, organization_id, method_key, display_name, provider_code,
    status, sort_order, scope, currency_code, country_code, metadata,
    idempotency_key, created_at, updated_at
)
VALUES
    ('bootstrap-payment-method-sandbox-test', '100001', '0', 'sandbox_test', 'Sandbox', 'sandbox', 'active', 900, 'organization', 'CNY', NULL, '{"bootstrap":true,"environment":"development"}', 'bootstrap-payment-method-sandbox-test', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
ON CONFLICT DO NOTHING;

-- Development PSP template accounts mirror the production structure with
-- development-scoped identifiers and the dev platform callback domain. They
-- stay inactive until an operator attaches write-only database credentials,
-- passes the dry-run test, and activates the account.
INSERT INTO commerce_payment_provider_account (
    id, tenant_id, organization_id, account_no, provider_code, merchant_id,
    account_name, environment, settlement_currency, secret_ref, webhook_secret_ref,
    certificate_ref, capabilities, status, metadata, created_at, updated_at
)
VALUES
    ('bootstrap-payment-provider-sandbox', '100001', '0', 'sandbox-primary', 'sandbox', '880100001234567', 'Sandbox Account', 'development', 'CNY', 'database:primary_secret', NULL, NULL, '{"pay":true,"refund":true,"close":true,"query":true}', 'active', '{"bootstrap":true,"environment":"development"}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-payment-provider-stripe-dev', '100001', '0', 'stripe-dev-primary', 'stripe', 'acct_2AbCdEfGhIjKlMnOpQ', 'Stripe Development Account', 'development', 'CNY', 'database:primary_secret', 'database:webhook_secret', NULL, '{"pay":true,"refund":true,"close":true,"query":true}', 'inactive', '{"bootstrap":true,"configureBeforeActivation":true}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-payment-provider-alipay-dev', '100001', '0', 'alipay-dev-primary', 'alipay', '2088123456789013', 'Alipay Development Account', 'development', 'CNY', 'database:primary_secret', NULL, 'database:certificate', '{"pay":true,"refund":true,"close":true,"query":true}', 'inactive', '{"bootstrap":true,"appId":"2021001122668846","configureBeforeActivation":true}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-payment-provider-wechat-pay-dev', '100001', '0', 'wechat-dev-primary', 'wechat_pay', '1900000209', 'WeChat Pay Development Account', 'development', 'CNY', 'database:primary_secret', 'database:webhook_secret', 'database:certificate', '{"pay":true,"refund":true,"close":true,"query":true}', 'inactive', '{"bootstrap":true,"appId":"wx8a9b0c1d2e3f4051","merchantSerialNo":"1A2B3C4D5E6F7A8B9C0D1E2F3A4B5C6D7E8F9A0B1C","notifyUrl":"https://api-dev.sdkwork.com/app/v3/api/orders/payments/webhooks/wechat_pay","configureBeforeActivation":true}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
ON CONFLICT DO NOTHING;

INSERT INTO commerce_payment_channel (
    id, tenant_id, organization_id, channel_no, channel_name, provider_account_id,
    method_id, provider_code, scene_code, currency_code, country_code, status,
    priority, sort_order, metadata, created_at, updated_at
)
VALUES
    ('bootstrap-payment-channel-sandbox-test', '100001', '0', 'sandbox-channel', 'Sandbox', 'bootstrap-payment-provider-sandbox', 'bootstrap-payment-method-sandbox-test', 'sandbox', 'api', 'CNY', NULL, 'active', 900, 900, '{"bootstrap":true,"environment":"development"}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
ON CONFLICT DO NOTHING;
