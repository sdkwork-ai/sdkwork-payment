-- One-time-compatible repair for databases initialized with the legacy PSP
-- templates. Only untouched bootstrap rows are changed; administrator-owned
-- provider accounts no longer carry the bootstrap/configure marker and are
-- skipped.
--
-- Organization id 0 is the default tenant-user organization scope. Keep
-- untouched payment bootstrap templates in that scope so app users can resolve
-- the default methods and channels without selecting an admin organization.
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
SET merchant_id = 'acct_1FjKpLmNqRsT2uVwXyZ',
    account_name = 'Stripe Global Production Account',
    metadata = '{"bootstrap":true,"configureBeforeActivation":true}',
    updated_at = CURRENT_TIMESTAMP
WHERE id = 'bootstrap-payment-provider-stripe'
  AND status = 'inactive'
  AND (merchant_id IS NULL OR TRIM(merchant_id) = '')
  AND CAST(metadata AS TEXT) LIKE '%configureBeforeActivation%';

UPDATE commerce_payment_provider_account
SET merchant_id = '2088123456789012',
    account_name = 'Alipay Global Production Account',
    metadata = '{"bootstrap":true,"appId":"2021001122668845","configureBeforeActivation":true}',
    updated_at = CURRENT_TIMESTAMP
WHERE id = 'bootstrap-payment-provider-alipay'
  AND status = 'inactive'
  AND (merchant_id IS NULL OR TRIM(merchant_id) = '')
  AND CAST(metadata AS TEXT) LIKE '%configureBeforeActivation%';

UPDATE commerce_payment_provider_account
SET merchant_id = '1900000109',
    account_name = 'WeChat Pay Global Production Account',
    secret_ref = 'database:primary_secret',
    webhook_secret_ref = 'database:webhook_secret',
    certificate_ref = 'database:certificate',
    metadata = '{"bootstrap":true,"appId":"wx9a2b3c4d5e6f7081","merchantSerialNo":"4A5B6C7D8E9F0A1B2C3D4E5F6A7B8C9D0E1F2A3B","notifyUrl":"https://api.sdkwork.com/app/v3/api/orders/payments/webhooks/wechat_pay","configureBeforeActivation":true}',
    updated_at = CURRENT_TIMESTAMP
WHERE id = 'bootstrap-payment-provider-wechat-pay'
  AND status = 'inactive'
  AND (
        merchant_id IS NULL
        OR TRIM(merchant_id) = ''
        OR secret_ref = 'SDKWORK_PAYMENT_WECHAT_PAY_API_V3_KEY'
        OR certificate_ref = 'SDKWORK_PAYMENT_WECHAT_PAY_CERTIFICATE'
      )
  AND CAST(metadata AS TEXT) LIKE '%configureBeforeActivation%';

-- Catalog and channels are pre-enabled, while the inactive provider account is
-- the fail-closed routing gate. Once real credentials replace the template
-- references and the account is activated, no second method/channel activation
-- pass is required.
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
)
AND EXISTS (
    SELECT 1
    FROM commerce_payment_provider_account a
    WHERE a.tenant_id = commerce_payment_method.tenant_id
      AND a.provider_code = commerce_payment_method.provider_code
      AND a.status = 'inactive'
      AND a.deleted_at IS NULL
      AND CAST(a.metadata AS TEXT) LIKE '%bootstrap%'
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
)
AND EXISTS (
    SELECT 1
    FROM commerce_payment_provider_account a
    WHERE a.id = commerce_payment_channel.provider_account_id
      AND a.tenant_id = commerce_payment_channel.tenant_id
      AND a.status = 'inactive'
      AND a.deleted_at IS NULL
      AND CAST(a.metadata AS TEXT) LIKE '%bootstrap%'
);
