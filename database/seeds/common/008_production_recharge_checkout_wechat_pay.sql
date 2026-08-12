-- Points recharge exposes the stable `wechat_pay` checkout key while the
-- payment catalog keeps provider-product methods such as `wechat_native`.
-- Production wires that compatibility key to WeChat Native; the bootstrap
-- provider account is active with real-format test credentials filled by the
-- payment service host on first boot, so the checkout path runs end to end.
INSERT INTO commerce_payment_method (
    id, tenant_id, organization_id, method_key, display_name, provider_code,
    status, sort_order, scope, currency_code, country_code, metadata,
    idempotency_key, created_at, updated_at
)
VALUES
    ('bootstrap-payment-method-recharge-wechat-pay', '100001', '0', 'wechat_pay', 'WeChat Pay Recharge', 'wechat_pay', 'active', 305, 'organization', 'CNY', 'CN', '{"bootstrap":true,"checkoutAliasFor":"wechat_native"}', 'bootstrap-payment-method-recharge-wechat-pay', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
ON CONFLICT DO NOTHING;

INSERT INTO commerce_payment_channel (
    id, tenant_id, organization_id, channel_no, channel_name, provider_account_id,
    method_id, provider_code, scene_code, currency_code, country_code, status,
    priority, sort_order, metadata, created_at, updated_at
)
VALUES
    ('bootstrap-payment-channel-recharge-wechat-pay', '100001', '0', 'recharge-wechat-pay', 'WeChat Pay Recharge', 'bootstrap-payment-provider-wechat-pay', 'bootstrap-payment-method-recharge-wechat-pay', 'wechat_pay', 'api', 'CNY', 'CN', 'active', 305, 305, '{"bootstrap":true,"checkoutAliasFor":"wechat_native"}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
ON CONFLICT DO NOTHING;
