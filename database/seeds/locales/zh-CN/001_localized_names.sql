-- sdkwork:seed-locale zh-CN
-- Localized zh-CN display names for the payment method catalog, provider
-- catalog, and channel inventory seeded by database/seeds/common.
-- Each locale file only manages its own keys through jsonb_set, so repeated
-- seeds are idempotent and locales never overwrite each other.

UPDATE commerce_payment_method
SET display_name_i18n = jsonb_set(display_name_i18n, '{zh-CN}', '"信用卡 / 借记卡"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND method_key = 'stripe_card'
  AND deleted_at IS NULL;

UPDATE commerce_payment_method
SET display_name_i18n = jsonb_set(display_name_i18n, '{zh-CN}', '"Apple Pay"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND method_key = 'stripe_apple_pay'
  AND deleted_at IS NULL;

UPDATE commerce_payment_method
SET display_name_i18n = jsonb_set(display_name_i18n, '{zh-CN}', '"Google Pay"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND method_key = 'stripe_google_pay'
  AND deleted_at IS NULL;

UPDATE commerce_payment_method
SET display_name_i18n = jsonb_set(display_name_i18n, '{zh-CN}', '"支付宝（跨境）"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND method_key = 'stripe_alipay'
  AND deleted_at IS NULL;

UPDATE commerce_payment_method
SET display_name_i18n = jsonb_set(display_name_i18n, '{zh-CN}', '"微信支付（跨境）"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND method_key = 'stripe_wechat_pay'
  AND deleted_at IS NULL;

UPDATE commerce_payment_method
SET display_name_i18n = jsonb_set(display_name_i18n, '{zh-CN}', '"支付宝当面付"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND method_key = 'alipay_qr'
  AND deleted_at IS NULL;

UPDATE commerce_payment_method
SET display_name_i18n = jsonb_set(display_name_i18n, '{zh-CN}', '"支付宝电脑网站支付"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND method_key = 'alipay_pc'
  AND deleted_at IS NULL;

UPDATE commerce_payment_method
SET display_name_i18n = jsonb_set(display_name_i18n, '{zh-CN}', '"支付宝手机网站支付"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND method_key = 'alipay_wap'
  AND deleted_at IS NULL;

UPDATE commerce_payment_method
SET display_name_i18n = jsonb_set(display_name_i18n, '{zh-CN}', '"支付宝 App 支付"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND method_key = 'alipay_app'
  AND deleted_at IS NULL;

UPDATE commerce_payment_method
SET display_name_i18n = jsonb_set(display_name_i18n, '{zh-CN}', '"支付宝 JSAPI 支付"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND method_key = 'alipay_jsapi'
  AND deleted_at IS NULL;

UPDATE commerce_payment_method
SET display_name_i18n = jsonb_set(display_name_i18n, '{zh-CN}', '"微信支付 Native"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND method_key = 'wechat_native'
  AND deleted_at IS NULL;

UPDATE commerce_payment_method
SET display_name_i18n = jsonb_set(display_name_i18n, '{zh-CN}', '"微信支付 JSAPI"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND method_key = 'wechat_jsapi'
  AND deleted_at IS NULL;

UPDATE commerce_payment_method
SET display_name_i18n = jsonb_set(display_name_i18n, '{zh-CN}', '"微信支付 H5"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND method_key = 'wechat_h5'
  AND deleted_at IS NULL;

UPDATE commerce_payment_method
SET display_name_i18n = jsonb_set(display_name_i18n, '{zh-CN}', '"微信支付 App"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND method_key = 'wechat_app'
  AND deleted_at IS NULL;

UPDATE commerce_payment_method
SET display_name_i18n = jsonb_set(display_name_i18n, '{zh-CN}', '"微信支付充值"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND method_key = 'wechat_pay'
  AND deleted_at IS NULL;

UPDATE commerce_payment_method
SET display_name_i18n = jsonb_set(display_name_i18n, '{zh-CN}', '"沙箱"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND method_key = 'sandbox_test'
  AND deleted_at IS NULL;

UPDATE commerce_payment_channel
SET channel_name_i18n = jsonb_set(channel_name_i18n, '{zh-CN}', '"Stripe 卡支付"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND channel_no = 'stripe-card'
  AND deleted_at IS NULL;

UPDATE commerce_payment_channel
SET channel_name_i18n = jsonb_set(channel_name_i18n, '{zh-CN}', '"Apple Pay（Stripe）"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND channel_no = 'stripe-apple-pay'
  AND deleted_at IS NULL;

UPDATE commerce_payment_channel
SET channel_name_i18n = jsonb_set(channel_name_i18n, '{zh-CN}', '"Google Pay（Stripe）"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND channel_no = 'stripe-google-pay'
  AND deleted_at IS NULL;

UPDATE commerce_payment_channel
SET channel_name_i18n = jsonb_set(channel_name_i18n, '{zh-CN}', '"支付宝（Stripe 跨境）"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND channel_no = 'stripe-alipay'
  AND deleted_at IS NULL;

UPDATE commerce_payment_channel
SET channel_name_i18n = jsonb_set(channel_name_i18n, '{zh-CN}', '"微信支付（Stripe 跨境）"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND channel_no = 'stripe-wechat-pay'
  AND deleted_at IS NULL;

UPDATE commerce_payment_channel
SET channel_name_i18n = jsonb_set(channel_name_i18n, '{zh-CN}', '"支付宝扫码"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND channel_no = 'alipay-qr'
  AND deleted_at IS NULL;

UPDATE commerce_payment_channel
SET channel_name_i18n = jsonb_set(channel_name_i18n, '{zh-CN}', '"支付宝电脑网站"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND channel_no = 'alipay-pc'
  AND deleted_at IS NULL;

UPDATE commerce_payment_channel
SET channel_name_i18n = jsonb_set(channel_name_i18n, '{zh-CN}', '"支付宝手机网站"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND channel_no = 'alipay-wap'
  AND deleted_at IS NULL;

UPDATE commerce_payment_channel
SET channel_name_i18n = jsonb_set(channel_name_i18n, '{zh-CN}', '"支付宝 App"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND channel_no = 'alipay-app'
  AND deleted_at IS NULL;

UPDATE commerce_payment_channel
SET channel_name_i18n = jsonb_set(channel_name_i18n, '{zh-CN}', '"支付宝 JSAPI"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND channel_no = 'alipay-jsapi'
  AND deleted_at IS NULL;

UPDATE commerce_payment_channel
SET channel_name_i18n = jsonb_set(channel_name_i18n, '{zh-CN}', '"微信支付 Native"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND channel_no = 'wechat-native'
  AND deleted_at IS NULL;

UPDATE commerce_payment_channel
SET channel_name_i18n = jsonb_set(channel_name_i18n, '{zh-CN}', '"微信支付 JSAPI"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND channel_no = 'wechat-jsapi'
  AND deleted_at IS NULL;

UPDATE commerce_payment_channel
SET channel_name_i18n = jsonb_set(channel_name_i18n, '{zh-CN}', '"微信支付 H5"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND channel_no = 'wechat-h5'
  AND deleted_at IS NULL;

UPDATE commerce_payment_channel
SET channel_name_i18n = jsonb_set(channel_name_i18n, '{zh-CN}', '"微信支付 App"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND channel_no = 'wechat-app'
  AND deleted_at IS NULL;

UPDATE commerce_payment_channel
SET channel_name_i18n = jsonb_set(channel_name_i18n, '{zh-CN}', '"微信支付充值"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND channel_no = 'recharge-wechat-pay'
  AND deleted_at IS NULL;

UPDATE commerce_payment_channel
SET channel_name_i18n = jsonb_set(channel_name_i18n, '{zh-CN}', '"沙箱"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND channel_no = 'sandbox-channel'
  AND deleted_at IS NULL;

UPDATE commerce_payment_provider
SET display_name_i18n = jsonb_set(display_name_i18n, '{zh-CN}', '"支付宝"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND provider_code = 'alipay'
  AND deleted_at IS NULL;

UPDATE commerce_payment_provider
SET display_name_i18n = jsonb_set(display_name_i18n, '{zh-CN}', '"微信支付"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND provider_code = 'wechat_pay'
  AND deleted_at IS NULL;

UPDATE commerce_payment_provider
SET display_name_i18n = jsonb_set(display_name_i18n, '{zh-CN}', '"Stripe"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND provider_code = 'stripe'
  AND deleted_at IS NULL;

UPDATE commerce_payment_provider
SET display_name_i18n = jsonb_set(display_name_i18n, '{zh-CN}', '"PayPal"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND provider_code = 'paypal'
  AND deleted_at IS NULL;

UPDATE commerce_payment_provider
SET display_name_i18n = jsonb_set(display_name_i18n, '{zh-CN}', '"Apple Pay"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND provider_code = 'apple_pay'
  AND deleted_at IS NULL;

UPDATE commerce_payment_provider
SET display_name_i18n = jsonb_set(display_name_i18n, '{zh-CN}', '"Google Pay"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND provider_code = 'google_pay'
  AND deleted_at IS NULL;

UPDATE commerce_payment_provider
SET display_name_i18n = jsonb_set(display_name_i18n, '{zh-CN}', '"沙箱"'::jsonb, true)
WHERE tenant_id = '100001'
  AND organization_id = '0'
  AND provider_code = 'sandbox'
  AND deleted_at IS NULL;

-- Provider account names. The sandbox bootstrap row is shared by the
-- development and test profiles, so the localized name follows the row's
-- environment.
UPDATE commerce_payment_provider_account
SET account_name_i18n = jsonb_set(account_name_i18n, '{zh-CN}', '"Stripe 生产账户"'::jsonb, true)
WHERE tenant_id = '100001'
  AND id = 'bootstrap-payment-provider-stripe'
  AND deleted_at IS NULL;

UPDATE commerce_payment_provider_account
SET account_name_i18n = jsonb_set(account_name_i18n, '{zh-CN}', '"支付宝生产账户"'::jsonb, true)
WHERE tenant_id = '100001'
  AND id = 'bootstrap-payment-provider-alipay'
  AND deleted_at IS NULL;

UPDATE commerce_payment_provider_account
SET account_name_i18n = jsonb_set(account_name_i18n, '{zh-CN}', '"微信支付生产账户"'::jsonb, true)
WHERE tenant_id = '100001'
  AND id = 'bootstrap-payment-provider-wechat-pay'
  AND deleted_at IS NULL;

UPDATE commerce_payment_provider_account
SET account_name_i18n = jsonb_set(account_name_i18n, '{zh-CN}', '"沙箱账户"'::jsonb, true)
WHERE tenant_id = '100001'
  AND id = 'bootstrap-payment-provider-sandbox'
  AND deleted_at IS NULL;

-- Development profile PSP template accounts.
UPDATE commerce_payment_provider_account
SET account_name_i18n = jsonb_set(account_name_i18n, '{zh-CN}', '"Stripe 开发账户"'::jsonb, true)
WHERE tenant_id = '100001'
  AND id = 'bootstrap-payment-provider-stripe-dev'
  AND deleted_at IS NULL;

UPDATE commerce_payment_provider_account
SET account_name_i18n = jsonb_set(account_name_i18n, '{zh-CN}', '"支付宝开发账户"'::jsonb, true)
WHERE tenant_id = '100001'
  AND id = 'bootstrap-payment-provider-alipay-dev'
  AND deleted_at IS NULL;

UPDATE commerce_payment_provider_account
SET account_name_i18n = jsonb_set(account_name_i18n, '{zh-CN}', '"微信支付开发账户"'::jsonb, true)
WHERE tenant_id = '100001'
  AND id = 'bootstrap-payment-provider-wechat-pay-dev'
  AND deleted_at IS NULL;

-- Test profile PSP template accounts (sandbox environment).
UPDATE commerce_payment_provider_account
SET account_name_i18n = jsonb_set(account_name_i18n, '{zh-CN}', '"Stripe 沙箱账户"'::jsonb, true)
WHERE tenant_id = '100001'
  AND id = 'bootstrap-payment-provider-stripe-sandbox'
  AND deleted_at IS NULL;

UPDATE commerce_payment_provider_account
SET account_name_i18n = jsonb_set(account_name_i18n, '{zh-CN}', '"支付宝沙箱账户"'::jsonb, true)
WHERE tenant_id = '100001'
  AND id = 'bootstrap-payment-provider-alipay-sandbox'
  AND deleted_at IS NULL;

UPDATE commerce_payment_provider_account
SET account_name_i18n = jsonb_set(account_name_i18n, '{zh-CN}', '"微信支付沙箱账户"'::jsonb, true)
WHERE tenant_id = '100001'
  AND id = 'bootstrap-payment-provider-wechat-pay-sandbox'
  AND deleted_at IS NULL;
