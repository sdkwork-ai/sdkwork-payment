-- Default checkout scene policy: which provider product serves a buyer context.
--
-- Without these rows every deployment has to configure the mapping before the
-- first payment. The defaults cover the supported app architectures and systems:
-- an iOS app uses the native WeChat/Alipay SDKs, browsers use the H5/PC products,
-- and a mini-program uses JSAPI. `any` is the wildcard, so an operator only adds a
-- row when a platform needs something different (Pad, Harmony, a new app shell).
--
-- Resolution order (see checkout_scene_policy.rs):
--   scene + architecture + os  ->  scene + architecture  ->  scene
INSERT INTO commerce_payment_checkout_scene_policy (
    id, tenant_id, organization_id, scene, app_architecture, os_family,
    payment_method, product_code, status, priority, metadata, created_at, updated_at
)
VALUES
    -- WeChat
    ('bootstrap-checkout-scene-wechat-web',         '100001', '0', 'checkout', 'web',           'any',     'wechat', 'wechat_h5',    'active', 100, '{"bootstrap":true}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-checkout-scene-wechat-mp',          '100001', '0', 'checkout', 'mini_program',  'any',     'wechat', 'wechat_jsapi', 'active', 100, '{"bootstrap":true}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-checkout-scene-wechat-app-ios',     '100001', '0', 'checkout', 'app',           'ios',     'wechat', 'wechat_app',   'active', 100, '{"bootstrap":true}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-checkout-scene-wechat-app-android', '100001', '0', 'checkout', 'app',           'android', 'wechat', 'wechat_app',   'active', 100, '{"bootstrap":true}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-checkout-scene-wechat-app-any',     '100001', '0', 'checkout', 'app',           'any',     'wechat', 'wechat_h5',    'active', 200, '{"bootstrap":true}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    -- Alipay
    ('bootstrap-checkout-scene-alipay-web',         '100001', '0', 'checkout', 'web',           'any',     'alipay', 'alipay_pc',    'active', 100, '{"bootstrap":true}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-checkout-scene-alipay-wap',         '100001', '0', 'checkout', 'h5',            'any',     'alipay', 'alipay_wap',   'active', 100, '{"bootstrap":true}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-checkout-scene-alipay-app-ios',     '100001', '0', 'checkout', 'app',           'ios',     'alipay', 'alipay_app',   'active', 100, '{"bootstrap":true}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-checkout-scene-alipay-app-android', '100001', '0', 'checkout', 'app',           'android', 'alipay', 'alipay_app',   'active', 100, '{"bootstrap":true}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-checkout-scene-alipay-app-any',     '100001', '0', 'checkout', 'app',           'any',     'alipay', 'alipay_wap',   'active', 200, '{"bootstrap":true}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    -- Stripe (card + wallet products)
    ('bootstrap-checkout-scene-stripe-web',         '100001', '0', 'checkout', 'web',           'any',     'stripe', 'stripe_card',  'active', 100, '{"bootstrap":true}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-checkout-scene-stripe-app-ios',     '100001', '0', 'checkout', 'app',           'ios',     'stripe', 'stripe_apple_pay',  'active', 100, '{"bootstrap":true}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-checkout-scene-stripe-app-android', '100001', '0', 'checkout', 'app',           'android', 'stripe', 'stripe_google_pay', 'active', 100, '{"bootstrap":true}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('bootstrap-checkout-scene-stripe-app-any',     '100001', '0', 'checkout', 'app',           'any',     'stripe', 'stripe_card',  'active', 200, '{"bootstrap":true}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
ON CONFLICT DO NOTHING;
