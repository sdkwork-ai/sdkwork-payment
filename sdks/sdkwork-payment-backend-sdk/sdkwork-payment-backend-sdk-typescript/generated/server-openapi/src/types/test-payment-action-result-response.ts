import type { TestPaymentActionResult } from './test-payment-action-result';

export interface TestPaymentActionResultResponse {
  code: 0;
  data: unknown & { item: TestPaymentActionResult; };
  /** Server-owned request correlation id. */
  traceId: string;
}
