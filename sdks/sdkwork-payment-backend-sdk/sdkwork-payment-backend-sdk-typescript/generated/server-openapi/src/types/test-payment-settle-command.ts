export interface TestPaymentSettleCommand {
  /** Payment intent id of the one-cent test payment to settle */
  paymentIntentId: string;
  /** Simulated provider outcome: success settles the payment, failed exercises the failure branch */
  outcome?: 'success' | 'failed';
}
