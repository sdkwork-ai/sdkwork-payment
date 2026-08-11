export interface CreateTestPaymentCommand {
  /** Payment method key (e.g., wechat_native, alipay_qr, alipay_wap, alipay_pc, stripe_card); must reference an active test-payment-capable method */
  methodKey: string;
  /** Test amount (defaults to 0.01) */
  amount?: string;
  /** Test currency (defaults to CNY) */
  currencyCode?: string;
}
