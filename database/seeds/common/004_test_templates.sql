-- Test/CI bootstrap is isolated from development while retaining a fully
-- operable local provider for end-to-end payment flows.
INSERT INTO commerce_payment_method (
    id, tenant_id, organization_id, method_key, display_name, provider_code,
    status, sort_order, scope, currency_code, country_code, metadata,
    idempotency_key, created_at, updated_at
)
VALUES
    ('bootstrap-payment-method-sandbox-test', '100001', '0', 'sandbox_test', 'Sandbox', 'sandbox', 'active', 900, 'organization', 'CNY', 'CN', '{"bootstrap":true,"environment":"test"}', 'bootstrap-payment-method-sandbox-test', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
ON CONFLICT DO NOTHING;

-- Test-environment PSP template accounts mirror the production structure with
-- test-scoped identifiers and the test platform callback domain. They are
-- active from the start; the payment service host fills real-format test
-- credentials on first boot when no credentials row exists yet.
INSERT INTO commerce_payment_provider_account (
    id, tenant_id, organization_id, account_no, provider_code, merchant_id,
    account_name, environment, settlement_currency, secret_ref, webhook_secret_ref,
    certificate_ref, capabilities, status, metadata, created_at, updated_at
)
VALUES
    ('bootstrap-payment-provider-sandbox', '100001', '0', 'sandbox-primary', 'sandbox', '312624127505056', 'Sandbox Account', 'sandbox', 'CNY', 'database:primary_secret', NULL, NULL, '{"pay":true,"refund":true,"close":true,"query":true}', 'active', '{"bootstrap":true,"environment":"test"}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-payment-provider-stripe-sandbox', '100001', '0', 'stripe-sandbox-primary', 'stripe', 'acct_lq87FC704mTOHl', 'Stripe Sandbox Account', 'sandbox', 'CNY', 'database:primary_secret', 'database:webhook_secret', NULL, '{"pay":true,"refund":true,"close":true,"query":true}', 'active', '{"bootstrap":true,"environment":"test"}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-payment-provider-alipay-sandbox', '100001', '0', 'alipay-sandbox-primary', 'alipay', '2088221181292725', 'Alipay Sandbox Account', 'sandbox', 'CNY', 'database:primary_secret', NULL, 'database:certificate', '{"pay":true,"refund":true,"close":true,"query":true}', 'active', '{"bootstrap":true,"appId":"2427777880481494","environment":"test"}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-payment-provider-wechat-pay-sandbox', '100001', '0', 'wechat-sandbox-primary', 'wechat_pay', '1900931628', 'WeChat Pay Sandbox Account', 'sandbox', 'CNY', 'database:primary_secret', 'database:webhook_secret', 'database:certificate', '{"pay":true,"refund":true,"close":true,"query":true}', 'active', '{"bootstrap":true,"appId":"wx7130382b0911e5af","merchantSerialNo":"C4EEA242F503F28DC66537D2A38E023047F4FB27","notifyUrl":"https://api-test.sdkwork.com/app/v3/api/orders/payments/webhooks/wechat_pay","environment":"test"}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
ON CONFLICT DO NOTHING;

INSERT INTO commerce_payment_channel (
    id, tenant_id, organization_id, channel_no, channel_name, provider_account_id,
    method_id, provider_code, scene_code, currency_code, country_code, status,
    priority, sort_order, metadata, created_at, updated_at
)
VALUES
    ('bootstrap-payment-channel-sandbox-test', '100001', '0', 'sandbox-channel', 'Sandbox', 'bootstrap-payment-provider-sandbox', 'bootstrap-payment-method-sandbox-test', 'sandbox', 'api', 'CNY', 'CN', 'active', 900, 900, '{"bootstrap":true,"environment":"test"}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
ON CONFLICT DO NOTHING;
