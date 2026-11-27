import type { TestPaymentNotificationResult } from './test-payment-notification-result';

export interface TestPaymentNotificationResultResponse {
  code: 0;
  data: unknown & { item: TestPaymentNotificationResult; };
  /** Server-owned request correlation id. */
  traceId: string;
}
