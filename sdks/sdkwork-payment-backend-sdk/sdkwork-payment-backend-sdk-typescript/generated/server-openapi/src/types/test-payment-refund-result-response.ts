import type { TestPaymentRefundResult } from './test-payment-refund-result';

export interface TestPaymentRefundResultResponse {
  code: 0;
  data: unknown & { item: TestPaymentRefundResult; };
  /** Server-owned request correlation id. */
  traceId: string;
}
