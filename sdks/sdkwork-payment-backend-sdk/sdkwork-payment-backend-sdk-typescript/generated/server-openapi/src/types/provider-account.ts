export interface ProviderAccount {
  id: string;
  accountNo: string;
  providerCode: 'stripe' | 'alipay' | 'wechat_pay' | 'sandbox';
  merchantId?: string;
  /** Operator-facing account name. Seeded bootstrap accounts carry readable names; operators may edit it through update. */
  accountName?: string;
  /** Locale display map for accountName, e.g. { "zh-CN": "Stripe 生产账户" }. Filled idempotently by locale seeds. */
  accountNameI18n?: Record<string, string>;
  /** direct = merchant self-connection; partner = ISV/service provider mode with sub-merchants */
  accountMode: 'direct' | 'partner';
  /** Parent partner account id when accountMode=partner; null when direct */
  partnerProviderAccountId?: string;
  environment: 'development' | 'sandbox' | 'production';
  countryCode?: string;
  settlementCurrency: string;
  /** Whether a primary provider credential is configured. Secret values are never returned. */
  hasPrimarySecret: boolean;
  /** Whether a webhook signing secret or WeChat API v3 key is configured. */
  hasWebhookSecret: boolean;
  /** Whether a provider public key or platform certificate is configured. */
  hasCertificate: boolean;
  credentialStorage: 'database_encrypted' | 'legacy_reference' | 'none';
  /** Supported payment capabilities: { pay: true, refund: true, close: true, query: true } */
  capabilities: Record<string, unknown>;
  status: 'active' | 'inactive' | 'suspended' | 'deprecated';
  /** Provider-specific extras: appId, merchantSerialNo, notifyUrl, returnUrl, sub_appid mappings. WeChat Pay also uses signVerifyMode ("wechatpay_public_key" default/recommended or "platform_certificate"), wechatpayPublicKeyId (PUB_KEY_ID_ prefix) and platformCertificateSerialNo for API v3 signature verification. */
  metadata: Record<string, unknown>;
  /** Parsed PEM expiry for certificate_ref */
  certificateExpiresAt?: string;
  lastTestedAt?: string;
  lastTestStatus?: 'success' | 'failure' | 'unknown';
  createdAt: string;
  updatedAt: string;
}
