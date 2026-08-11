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
  /** Scan-to-pay QR code payload (e.g., WeChat native code_url, Alipay precreate qr_code) when the provider returned one */
  qrCodeUrl?: string;
  /** Web cashier redirect URL (Alipay WAP/PC cashier link) when the provider returned one; open it in a browser to pay */
  payUrl?: string;
  /** Full Alipay cashier form HTML for browser render and auto-submit (Alipay PC website pay) */
  payForm?: string;
  expiresAt?: string;
  createdAt: string;
}
