-- Provider catalog: the mainstream PSP inventory read by the admin payment
-- center (GET /backend/v3/api/payments/providers). Providers with a runtime
-- adapter (sdkwork-payment-providers registry) are active; future mainstream
-- providers are inactive placeholders until their adapter exists. The read
-- contract hardcodes capabilities, so they are not stored per row.
INSERT INTO commerce_payment_provider (
    id, tenant_id, organization_id, provider_code, display_name, provider_type,
    supported_countries, supported_currencies, status, sort_order,
    created_at, updated_at
)
VALUES
    ('bootstrap-provider-stripe', '100001', '0', 'stripe', 'Stripe', 'card', '["US","GB","CA","AU","HK","SG","JP","DE","FR","CN"]', '["USD","CNY","EUR","GBP","CAD","AUD","HKD","SGD","JPY"]', 'active', 100, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-provider-alipay', '100001', '0', 'alipay', 'Alipay', 'wallet', '["CN","HK","SG","MY","JP","US"]', '["CNY","USD","HKD","SGD","JPY"]', 'active', 200, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-provider-wechat-pay', '100001', '0', 'wechat_pay', 'WeChat Pay', 'wallet', '["CN","HK","SG","US"]', '["CNY","USD","HKD","SGD"]', 'active', 300, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-provider-paypal', '100001', '0', 'paypal', 'PayPal', 'wallet', '["US","GB","DE","FR","CA","AU"]', '["USD","EUR","GBP","CAD","AUD"]', 'inactive', 400, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-provider-apple-pay', '100001', '0', 'apple_pay', 'Apple Pay', 'wallet', '["US","GB","CA","AU","JP","CN"]', '["USD","CNY","GBP","CAD","AUD","JPY"]', 'inactive', 500, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-provider-google-pay', '100001', '0', 'google_pay', 'Google Pay', 'wallet', '["US","GB","CA","AU","JP"]', '["USD","GBP","CAD","AUD","JPY"]', 'inactive', 600, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-provider-sandbox', '100001', '0', 'sandbox', 'Sandbox', 'sandbox', '["CN"]', '["CNY"]', 'active', 900, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
ON CONFLICT DO NOTHING;

-- External PSP accounts are active from the start; the payment service host
-- fills real-format test credentials on first boot when no credentials row
-- exists yet, so every code path runs end to end (real HTTP calls to the PSP)
-- regardless of the configured data. Operators replace the write-only database
-- credentials with real merchant credentials in the admin workspace at any
-- time; the replacement takes effect immediately with no activation gate.
INSERT INTO commerce_payment_provider_account (
    id, tenant_id, organization_id, account_no, provider_code, merchant_id,
    account_name, environment, settlement_currency, secret_ref, webhook_secret_ref,
    certificate_ref, capabilities, status, metadata, created_at, updated_at
)
VALUES
    ('bootstrap-payment-provider-stripe', '100001', '0', 'stripe-live-primary', 'stripe', 'acct_y8yk2pWrAxGf27', 'Stripe Global Production Account', 'production', 'CNY', 'database:primary_secret', 'database:webhook_secret', NULL, '{"pay":true,"refund":true,"close":true,"query":true}', 'active', '{"bootstrap":true}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-payment-provider-alipay', '100001', '0', 'alipay-prod-primary', 'alipay', '2088138651154610', 'Alipay Global Production Account', 'production', 'CNY', 'database:primary_secret', NULL, 'database:certificate', '{"pay":true,"refund":true,"close":true,"query":true}', 'active', '{"bootstrap":true,"appId":"8826820674017219"}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-payment-provider-wechat-pay', '100001', '0', 'wechat-prod-primary', 'wechat_pay', '1900977762', 'WeChat Pay Global Production Account', 'production', 'CNY', 'database:primary_secret', 'database:webhook_secret', 'database:certificate', '{"pay":true,"refund":true,"close":true,"query":true}', 'active', '{"bootstrap":true,"appId":"wxf82c8051283ea5cf","merchantSerialNo":"2BBB5DA90616A3B93D9AA0EBCF2D1EF1BBCEAC70","notifyUrl":"https://api.sdkwork.com/app/v3/api/orders/payments/webhooks/wechat_pay"}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
ON CONFLICT DO NOTHING;

INSERT INTO commerce_payment_channel (
    id, tenant_id, organization_id, channel_no, channel_name, provider_account_id,
    method_id, provider_code, scene_code, currency_code, country_code, status,
    priority, sort_order, metadata, created_at, updated_at
)
VALUES
    ('bootstrap-payment-channel-stripe-card', '100001', '0', 'stripe-card', 'Stripe Card', 'bootstrap-payment-provider-stripe', 'bootstrap-payment-method-stripe-card', 'stripe', 'web', 'CNY', 'CN', 'active', 100, 100, '{"bootstrap":true}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-payment-channel-stripe-apple-pay', '100001', '0', 'stripe-apple-pay', 'Stripe Apple Pay', 'bootstrap-payment-provider-stripe', 'bootstrap-payment-method-stripe-apple-pay', 'stripe', 'web', 'CNY', 'CN', 'active', 110, 110, '{"bootstrap":true}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-payment-channel-stripe-google-pay', '100001', '0', 'stripe-google-pay', 'Stripe Google Pay', 'bootstrap-payment-provider-stripe', 'bootstrap-payment-method-stripe-google-pay', 'stripe', 'web', 'CNY', 'CN', 'active', 120, 120, '{"bootstrap":true}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-payment-channel-stripe-alipay', '100001', '0', 'stripe-alipay', 'Stripe Alipay', 'bootstrap-payment-provider-stripe', 'bootstrap-payment-method-stripe-alipay', 'stripe', 'web', 'CNY', 'CN', 'active', 130, 130, '{"bootstrap":true}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-payment-channel-stripe-wechat-pay', '100001', '0', 'stripe-wechat-pay', 'Stripe WeChat Pay', 'bootstrap-payment-provider-stripe', 'bootstrap-payment-method-stripe-wechat-pay', 'stripe', 'web', 'CNY', 'CN', 'active', 140, 140, '{"bootstrap":true}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-payment-channel-alipay-qr', '100001', '0', 'alipay-qr', 'Alipay QR', 'bootstrap-payment-provider-alipay', 'bootstrap-payment-method-alipay-qr', 'alipay', 'api', 'CNY', 'CN', 'active', 200, 200, '{"bootstrap":true}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-payment-channel-alipay-pc', '100001', '0', 'alipay-pc', 'Alipay PC', 'bootstrap-payment-provider-alipay', 'bootstrap-payment-method-alipay-pc', 'alipay', 'web', 'CNY', 'CN', 'active', 210, 210, '{"bootstrap":true}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-payment-channel-alipay-wap', '100001', '0', 'alipay-wap', 'Alipay WAP', 'bootstrap-payment-provider-alipay', 'bootstrap-payment-method-alipay-wap', 'alipay', 'web', 'CNY', 'CN', 'active', 220, 220, '{"bootstrap":true}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-payment-channel-alipay-app', '100001', '0', 'alipay-app', 'Alipay App', 'bootstrap-payment-provider-alipay', 'bootstrap-payment-method-alipay-app', 'alipay', 'app', 'CNY', 'CN', 'active', 230, 230, '{"bootstrap":true}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-payment-channel-alipay-jsapi', '100001', '0', 'alipay-jsapi', 'Alipay JSAPI', 'bootstrap-payment-provider-alipay', 'bootstrap-payment-method-alipay-jsapi', 'alipay', 'mini_program', 'CNY', 'CN', 'active', 240, 240, '{"bootstrap":true}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-payment-channel-wechat-native', '100001', '0', 'wechat-native', 'WeChat Pay Native', 'bootstrap-payment-provider-wechat-pay', 'bootstrap-payment-method-wechat-native', 'wechat_pay', 'api', 'CNY', 'CN', 'active', 300, 300, '{"bootstrap":true}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-payment-channel-wechat-jsapi', '100001', '0', 'wechat-jsapi', 'WeChat Pay JSAPI', 'bootstrap-payment-provider-wechat-pay', 'bootstrap-payment-method-wechat-jsapi', 'wechat_pay', 'mini_program', 'CNY', 'CN', 'active', 310, 310, '{"bootstrap":true}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-payment-channel-wechat-h5', '100001', '0', 'wechat-h5', 'WeChat Pay H5', 'bootstrap-payment-provider-wechat-pay', 'bootstrap-payment-method-wechat-h5', 'wechat_pay', 'web', 'CNY', 'CN', 'active', 320, 320, '{"bootstrap":true}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-payment-channel-wechat-app', '100001', '0', 'wechat-app', 'WeChat Pay App', 'bootstrap-payment-provider-wechat-pay', 'bootstrap-payment-method-wechat-app', 'wechat_pay', 'app', 'CNY', 'CN', 'active', 330, 330, '{"bootstrap":true}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
ON CONFLICT DO NOTHING;
