export interface TestPaymentRefundResult {
  paymentIntentId: string;
  attemptId: string;
  refundId: string;
  refundNo: string;
  amount: string;
  currencyCode: string;
  /** Refund status after the provider submission (succeeded, processing, failed, submitted) */
  status: string;
  providerCode: string;
  /** True when the provider submission reached a terminal success */
  refunded: boolean;
  /** Operator guidance derived from the resulting status */
  nextStep: string;
}
