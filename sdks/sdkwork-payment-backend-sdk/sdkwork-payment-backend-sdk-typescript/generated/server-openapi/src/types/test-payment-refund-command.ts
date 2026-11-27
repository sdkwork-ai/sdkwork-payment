export interface TestPaymentRefundCommand {
  /** Payment intent id of the settled test payment to refund */
  paymentIntentId: string;
  /** Refund amount; defaults to the full captured test amount */
  amount?: string;
  /** Refund reason recorded with the refund */
  reasonCode?: 'customer_request' | 'duplicate' | 'fraud' | 'service_failure' | 'other';
}
