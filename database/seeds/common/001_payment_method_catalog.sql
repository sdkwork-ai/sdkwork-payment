-- Shared payment-method catalog for the platform bootstrap tenant. Profile
-- templates add the sandbox method and provider/channel records appropriate to
-- their environment. Existing administrator-owned records are never updated.
INSERT INTO commerce_payment_method (
    id, tenant_id, organization_id, method_key, display_name, provider_code,
    status, sort_order, scope, currency_code, country_code, metadata,
    idempotency_key, created_at, updated_at
)
VALUES
    ('bootstrap-payment-method-stripe-card', '100001', '0', 'stripe_card', 'Credit / Debit Card', 'stripe', 'active', 100, 'organization', 'CNY', 'CN', '{"bootstrap":true,"provider":"stripe"}', 'bootstrap-payment-method-stripe-card', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-payment-method-stripe-apple-pay', '100001', '0', 'stripe_apple_pay', 'Apple Pay', 'stripe', 'active', 110, 'organization', 'CNY', 'CN', '{"bootstrap":true,"provider":"stripe"}', 'bootstrap-payment-method-stripe-apple-pay', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-payment-method-stripe-google-pay', '100001', '0', 'stripe_google_pay', 'Google Pay', 'stripe', 'active', 120, 'organization', 'CNY', 'CN', '{"bootstrap":true,"provider":"stripe"}', 'bootstrap-payment-method-stripe-google-pay', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-payment-method-stripe-alipay', '100001', '0', 'stripe_alipay', 'Alipay (cross-border)', 'stripe', 'active', 130, 'organization', 'CNY', 'CN', '{"bootstrap":true,"provider":"stripe"}', 'bootstrap-payment-method-stripe-alipay', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-payment-method-stripe-wechat-pay', '100001', '0', 'stripe_wechat_pay', 'WeChat Pay (cross-border)', 'stripe', 'active', 140, 'organization', 'CNY', 'CN', '{"bootstrap":true,"provider":"stripe"}', 'bootstrap-payment-method-stripe-wechat-pay', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-payment-method-alipay-qr', '100001', '0', 'alipay_qr', 'Alipay In-store QR', 'alipay', 'active', 200, 'organization', 'CNY', 'CN', '{"bootstrap":true,"provider":"alipay"}', 'bootstrap-payment-method-alipay-qr', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-payment-method-alipay-pc', '100001', '0', 'alipay_pc', 'Alipay PC Website', 'alipay', 'active', 210, 'organization', 'CNY', 'CN', '{"bootstrap":true,"provider":"alipay"}', 'bootstrap-payment-method-alipay-pc', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-payment-method-alipay-wap', '100001', '0', 'alipay_wap', 'Alipay WAP', 'alipay', 'active', 220, 'organization', 'CNY', 'CN', '{"bootstrap":true,"provider":"alipay"}', 'bootstrap-payment-method-alipay-wap', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-payment-method-alipay-app', '100001', '0', 'alipay_app', 'Alipay App', 'alipay', 'active', 230, 'organization', 'CNY', 'CN', '{"bootstrap":true,"provider":"alipay"}', 'bootstrap-payment-method-alipay-app', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-payment-method-alipay-jsapi', '100001', '0', 'alipay_jsapi', 'Alipay JSAPI', 'alipay', 'active', 240, 'organization', 'CNY', 'CN', '{"bootstrap":true,"provider":"alipay"}', 'bootstrap-payment-method-alipay-jsapi', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-payment-method-wechat-native', '100001', '0', 'wechat_native', 'WeChat Pay Native', 'wechat_pay', 'active', 300, 'organization', 'CNY', 'CN', '{"bootstrap":true,"provider":"wechat_pay"}', 'bootstrap-payment-method-wechat-native', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-payment-method-wechat-jsapi', '100001', '0', 'wechat_jsapi', 'WeChat Pay JSAPI', 'wechat_pay', 'active', 310, 'organization', 'CNY', 'CN', '{"bootstrap":true,"provider":"wechat_pay"}', 'bootstrap-payment-method-wechat-jsapi', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-payment-method-wechat-h5', '100001', '0', 'wechat_h5', 'WeChat Pay H5', 'wechat_pay', 'active', 320, 'organization', 'CNY', 'CN', '{"bootstrap":true,"provider":"wechat_pay"}', 'bootstrap-payment-method-wechat-h5', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-payment-method-wechat-app', '100001', '0', 'wechat_app', 'WeChat Pay App', 'wechat_pay', 'active', 330, 'organization', 'CNY', 'CN', '{"bootstrap":true,"provider":"wechat_pay"}', 'bootstrap-payment-method-wechat-app', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
ON CONFLICT DO NOTHING;
