export interface TestPayment {
  paymentId: string;
  paymentIntentId: string;
  paymentIntentNo?: string;
  attemptId: string;
  outTradeNo: string;
  methodKey: string;
  providerCode: string;
  amount: string;
  currencyCode: string;
  /** Payment attempt status (pending, succeeded, failed, ...) */
  status: string;
  /** Scan-to-pay QR code payload (e.g., WeChat native code_url, Alipay precreate qr_code) when the provider returned one; falls back to the configured H5 cashier deep-link generated from the payment domain settings */
  qrCodeUrl?: string;
  /** Order-bound H5 cashier deep-link built from the configured H5 payment domain (支付设置 → 域名设置); scan it to open the mobile cashier */
  cashierUrl?: string;
  /** Web cashier redirect URL (Alipay WAP/PC cashier link) when the provider returned one; open it in a browser to pay */
  payUrl?: string;
  /** Full Alipay cashier form HTML for browser render and auto-submit (Alipay PC website pay) */
  payForm?: string;
  /** Stripe PaymentIntent client secret for Stripe.js card collection (stripe methods) */
  clientSecret?: string;
  /** Stripe publishable key for Stripe.js (stripe methods) */
  publishableKey?: string;
  expiresAt?: string;
  createdAt: string;
}
