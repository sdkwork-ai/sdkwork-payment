export interface CreateProviderAccountCommand {
  accountNo: string;
  providerCode: 'stripe' | 'alipay' | 'wechat_pay' | 'sandbox';
  merchantId: string;
  /** Operator-facing account name shown in admin surfaces. Optional; defaults to the account no when absent. */
  accountName?: string;
  accountMode?: 'direct' | 'partner';
  partnerProviderAccountId?: string;
  environment: 'development' | 'sandbox' | 'production';
  countryCode: string;
  settlementCurrency: string;
  /** Primary PSP secret material. Encrypted before database persistence and never returned. */
  primarySecret: string;
  /** Stripe webhook secret or WeChat API v3 key. */
  webhookSecret?: string;
  /** Alipay public key, WeChat platform certificate PEM (platform certificate mode), or WeChat Pay public key PEM (pub_key.pem, public key mode). */
  certificate?: string;
  capabilities?: Record<string, unknown>;
  /** Account state. Bootstrap accounts start active; saving credentials takes effect immediately with no activation gate. */
  status?: 'active' | 'inactive' | 'suspended' | 'deprecated';
  /** Provider-specific extras: appId, merchantSerialNo, notifyUrl, returnUrl, sub_appid mappings. WeChat Pay also uses signVerifyMode ("wechatpay_public_key" default/recommended or "platform_certificate"), wechatpayPublicKeyId (PUB_KEY_ID_ prefix) and platformCertificateSerialNo for API v3 signature verification. */
  metadata?: Record<string, unknown>;
}
