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
  /** Normalized launch surface for the cashier, projected from the channel's nextAction plus provider semantics: qr_code shows the QR, redirect opens payUrl, form_redirect auto-submits payForm, card mounts the Stripe.js element, jsapi/app render the client invocation payload, local_sandbox has no PSP surface, unsupported means the channel returned nothing launchable (a configuration finding) */
  launchKind?: 'qr_code' | 'redirect' | 'form_redirect' | 'card' | 'jsapi' | 'app' | 'local_sandbox' | 'unsupported';
  /** Client invocation payload for jsapi/app launches (wx.requestPayment params, WeChat PayReq params, or the Alipay App SDK orderStr) */
  launchPayload?: string;
  /** nextAction recorded by the checkout normalizer (diagnosis only) */
  nextAction?: string;
  /** True when the operator can settle this test payment from the dialog through dev.settleTestPayment (sandbox and client-payload products) instead of paying for real */
  settleSupported?: boolean;
  expiresAt?: string;
  createdAt: string;
}
