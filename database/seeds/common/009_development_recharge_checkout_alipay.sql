-- Development mirror of 009_production_recharge_checkout_alipay.sql: the order
-- checkout surfaces send the stable `alipay` key, so the development catalog
-- needs the same compatibility method/channel or local purchases cannot pay by
-- Alipay at all. Wired to the development Alipay account (not the sandbox
-- account) so signing exercises the real Alipay client path.
INSERT INTO commerce_payment_method (
    id, tenant_id, organization_id, method_key, display_name, provider_code,
    status, sort_order, scope, currency_code, country_code, metadata,
    idempotency_key, created_at, updated_at
)
VALUES
    ('bootstrap-payment-method-recharge-alipay', '100001', '0', 'alipay', 'Alipay', 'alipay', 'active', 205, 'organization', 'CNY', 'CN', '{"bootstrap":true,"checkoutAliasFor":"alipay_qr","environment":"development"}', 'bootstrap-payment-method-recharge-alipay', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
ON CONFLICT DO NOTHING;

INSERT INTO commerce_payment_channel (
    id, tenant_id, organization_id, channel_no, channel_name, provider_account_id,
    method_id, provider_code, scene_code, currency_code, country_code, status,
    priority, sort_order, metadata, created_at, updated_at
)
VALUES
    ('bootstrap-payment-channel-recharge-alipay', '100001', '0', 'recharge-alipay', 'Alipay Recharge', 'bootstrap-payment-provider-alipay-dev', 'bootstrap-payment-method-recharge-alipay', 'alipay', 'web', 'CNY', 'CN', 'active', 205, 205, '{"bootstrap":true,"checkoutAliasFor":"alipay_qr","environment":"development"}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
ON CONFLICT DO NOTHING;
