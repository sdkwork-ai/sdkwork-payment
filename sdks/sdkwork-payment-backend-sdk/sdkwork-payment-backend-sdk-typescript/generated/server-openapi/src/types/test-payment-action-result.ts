export interface TestPaymentActionResult {
  paymentIntentId: string;
  attemptId: string;
  outTradeNo: string;
  /** Attempt status before the action */
  previousStatus: string;
  /** Attempt status after the action was applied */
  localStatus: string;
  providerCode: string;
  /** Status the simulated provider event mapped to (null when nothing was applied) */
  appliedStatus?: string;
  /** Webhook event record id created by the action (empty when no event was recorded) */
  webhookEventId?: string;
  /** Provider event id used for de-duplication */
  eventId?: string;
}
