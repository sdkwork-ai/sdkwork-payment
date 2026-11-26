export interface CheckAttemptStatusCommand {
  /** Payment intent id of the one-cent test payment to check */
  paymentIntentId: string;
  /** Settle a sandbox or client-payload product the payer has not paid yet, in the same request (same gate as dev.settleTestPayment) */
  settleIfUnpaid?: boolean;
}
