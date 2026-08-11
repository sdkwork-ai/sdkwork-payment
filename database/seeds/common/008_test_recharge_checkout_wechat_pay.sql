-- Test/CI uses the isolated sandbox account for the same checkout key exposed
-- by the points-recharge API and PC dialog.
INSERT INTO commerce_payment_method (
    id, tenant_id, organization_id, method_key, display_name, provider_code,
    status, sort_order, scope, currency_code, country_code, metadata,
    idempotency_key, created_at, updated_at
)
VALUES
    ('bootstrap-payment-method-recharge-wechat-pay', '100001', '0', 'wechat_pay', 'WeChat Pay Recharge', 'sandbox', 'active', 305, 'organization', 'CNY', NULL, '{"bootstrap":true,"checkoutAliasFor":"wechat_native","environment":"test"}', 'bootstrap-payment-method-recharge-wechat-pay', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
ON CONFLICT DO NOTHING;

INSERT INTO commerce_payment_channel (
    id, tenant_id, organization_id, channel_no, channel_name, provider_account_id,
    method_id, provider_code, scene_code, currency_code, country_code, status,
    priority, sort_order, metadata, created_at, updated_at
)
VALUES
    ('bootstrap-payment-channel-recharge-wechat-pay', '100001', '0', 'recharge-wechat-pay', 'WeChat Pay Recharge', 'bootstrap-payment-provider-sandbox', 'bootstrap-payment-method-recharge-wechat-pay', 'sandbox', 'api', 'CNY', NULL, 'active', 305, 305, '{"bootstrap":true,"checkoutAliasFor":"wechat_native","environment":"test"}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
ON CONFLICT DO NOTHING;
