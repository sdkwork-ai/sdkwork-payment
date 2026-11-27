export interface TestPaymentNotificationCommand {
  /** Provider account whose stored credentials sign and verify the notification */
  providerAccountId: string;
  /** Raw notification body (JSON for Stripe/WeChat Pay, JSON or form for Alipay) */
  payload: string;
  /** Extra headers added to the delivery after the signed ones (e.g. Wechatpay-Serial) */
  headers?: Record<string, string>;
  /** Signing scheme; defaults to the provider's contract. `none` is only allowed for the sandbox provider */
  signer?: 'hmac_sha256' | 'wechat_pay_rsa_sha256' | 'alipay_rsa_sha256' | 'none';
}
