-- Test mirror of 009_production_recharge_checkout_alipay.sql: the order checkout
-- surfaces send the stable `alipay` key, so the test catalog needs the same
-- compatibility method/channel. Wired to the Alipay sandbox account so automated
-- checkout runs stay isolated from live merchant credentials.
INSERT INTO commerce_payment_method (
    id, tenant_id, organization_id, method_key, display_name, provider_code,
    status, sort_order, scope, currency_code, country_code, metadata,
    idempotency_key, created_at, updated_at
)
VALUES
    ('bootstrap-payment-method-recharge-alipay', '100001', '0', 'alipay', 'Alipay', 'alipay', 'active', 205, 'organization', 'CNY', 'CN', '{"bootstrap":true,"checkoutAliasFor":"alipay_qr","environment":"test"}', 'bootstrap-payment-method-recharge-alipay', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
ON CONFLICT DO NOTHING;

INSERT INTO commerce_payment_channel (
    id, tenant_id, organization_id, channel_no, channel_name, provider_account_id,
    method_id, provider_code, scene_code, currency_code, country_code, status,
    priority, sort_order, metadata, created_at, updated_at
)
VALUES
    ('bootstrap-payment-channel-recharge-alipay', '100001', '0', 'recharge-alipay', 'Alipay Recharge', 'bootstrap-payment-provider-alipay-sandbox', 'bootstrap-payment-method-recharge-alipay', 'alipay', 'web', 'CNY', 'CN', 'active', 205, 205, '{"bootstrap":true,"checkoutAliasFor":"alipay_qr","environment":"test"}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
ON CONFLICT DO NOTHING;
