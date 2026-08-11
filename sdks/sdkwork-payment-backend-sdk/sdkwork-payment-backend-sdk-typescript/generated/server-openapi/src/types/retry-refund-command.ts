export interface RetryRefundCommand {
  /** Exact refund number typed by the operator. */
  confirmRefundNo: string;
  /** Current refund status the retry is anchored to: failed re-submits, processing reconciles against the provider first */
  expectedStatus: 'failed' | 'processing';
}
