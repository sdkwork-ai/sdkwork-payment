-- One-time-compatible repair for databases initialized with the legacy PSP
-- templates. Only untouched bootstrap rows are changed; administrator-owned
-- provider accounts no longer carry the bootstrap marker and are skipped.
--
-- Organization id 0 is the default tenant-user organization scope. Keep
-- untouched payment bootstrap templates in that scope so app users can resolve
-- the default methods and channels without selecting an admin organization.
--
-- Bootstrap provider accounts are active with immediately usable data: the
-- payment service host fills real-format test credentials on first boot, and
-- operators replace them with real merchant credentials at any time. Methods
-- and channels are active unconditionally; no fail-closed activation gate
-- remains.
UPDATE commerce_payment_method
SET organization_id = '0', updated_at = CURRENT_TIMESTAMP
WHERE tenant_id = '100001'
  AND organization_id = '100002'
  AND id LIKE 'bootstrap-payment-method-%'
  AND CAST(metadata AS TEXT) LIKE '%bootstrap%';

UPDATE commerce_payment_provider_account
SET organization_id = '0', updated_at = CURRENT_TIMESTAMP
WHERE tenant_id = '100001'
  AND organization_id = '100002'
  AND id IN (
      'bootstrap-payment-provider-stripe',
      'bootstrap-payment-provider-alipay',
      'bootstrap-payment-provider-wechat-pay',
      'bootstrap-payment-provider-sandbox'
  )
  AND CAST(metadata AS TEXT) LIKE '%bootstrap%';

UPDATE commerce_payment_provider_credential
SET organization_id = '0', updated_at = CURRENT_TIMESTAMP
WHERE tenant_id = '100001'
  AND organization_id = '100002'
  AND provider_account_id IN (
      'bootstrap-payment-provider-stripe',
      'bootstrap-payment-provider-alipay',
      'bootstrap-payment-provider-wechat-pay',
      'bootstrap-payment-provider-sandbox'
  );

UPDATE commerce_payment_channel
SET organization_id = '0', updated_at = CURRENT_TIMESTAMP
WHERE tenant_id = '100001'
  AND organization_id = '100002'
  AND id LIKE 'bootstrap-payment-channel-%'
  AND CAST(metadata AS TEXT) LIKE '%bootstrap%';

UPDATE commerce_payment_provider_account
SET merchant_id = 'acct_y8yk2pWrAxGf27',
    account_name = 'Stripe Global Production Account',
    status = 'active',
    metadata = '{"bootstrap":true}',
    updated_at = CURRENT_TIMESTAMP
WHERE id = 'bootstrap-payment-provider-stripe'
  AND status <> 'active'
  AND CAST(metadata AS TEXT) LIKE '%bootstrap%';

UPDATE commerce_payment_provider_account
SET merchant_id = '2088138651154610',
    account_name = 'Alipay Global Production Account',
    status = 'active',
    metadata = '{"bootstrap":true,"appId":"8826820674017219"}',
    updated_at = CURRENT_TIMESTAMP
WHERE id = 'bootstrap-payment-provider-alipay'
  AND status <> 'active'
  AND CAST(metadata AS TEXT) LIKE '%bootstrap%';

UPDATE commerce_payment_provider_account
SET merchant_id = '1900977762',
    account_name = 'WeChat Pay Global Production Account',
    status = 'active',
    secret_ref = 'database:primary_secret',
    webhook_secret_ref = 'database:webhook_secret',
    certificate_ref = 'database:certificate',
    metadata = '{"bootstrap":true,"appId":"wxf82c8051283ea5cf","merchantSerialNo":"2BBB5DA90616A3B93D9AA0EBCF2D1EF1BBCEAC70","notifyUrl":"https://api.sdkwork.com/app/v3/api/orders/payments/webhooks/wechat_pay"}',
    updated_at = CURRENT_TIMESTAMP
WHERE id = 'bootstrap-payment-provider-wechat-pay'
  AND status <> 'active'
  AND CAST(metadata AS TEXT) LIKE '%bootstrap%';

-- Catalog methods and channels are active unconditionally: the bootstrap
-- provider accounts are active too, so the whole checkout path runs end to end
-- with whatever credentials are configured.
UPDATE commerce_payment_method
SET status = 'active', updated_at = CURRENT_TIMESTAMP
WHERE id IN (
    'bootstrap-payment-method-stripe-card',
    'bootstrap-payment-method-stripe-apple-pay',
    'bootstrap-payment-method-stripe-google-pay',
    'bootstrap-payment-method-stripe-alipay',
    'bootstrap-payment-method-stripe-wechat-pay',
    'bootstrap-payment-method-alipay-qr',
    'bootstrap-payment-method-alipay-pc',
    'bootstrap-payment-method-alipay-wap',
    'bootstrap-payment-method-alipay-app',
    'bootstrap-payment-method-alipay-jsapi',
    'bootstrap-payment-method-wechat-native',
    'bootstrap-payment-method-wechat-jsapi',
    'bootstrap-payment-method-wechat-h5',
    'bootstrap-payment-method-wechat-app'
);

UPDATE commerce_payment_channel
SET status = 'active', updated_at = CURRENT_TIMESTAMP
WHERE id IN (
    'bootstrap-payment-channel-stripe-card',
    'bootstrap-payment-channel-stripe-apple-pay',
    'bootstrap-payment-channel-stripe-google-pay',
    'bootstrap-payment-channel-stripe-alipay',
    'bootstrap-payment-channel-stripe-wechat-pay',
    'bootstrap-payment-channel-alipay-qr',
    'bootstrap-payment-channel-alipay-pc',
    'bootstrap-payment-channel-alipay-wap',
    'bootstrap-payment-channel-alipay-app',
    'bootstrap-payment-channel-alipay-jsapi',
    'bootstrap-payment-channel-wechat-native',
    'bootstrap-payment-channel-wechat-jsapi',
    'bootstrap-payment-channel-wechat-h5',
    'bootstrap-payment-channel-wechat-app'
);
