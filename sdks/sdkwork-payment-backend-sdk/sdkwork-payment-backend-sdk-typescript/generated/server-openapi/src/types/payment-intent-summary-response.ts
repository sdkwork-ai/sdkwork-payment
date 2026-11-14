import type { PaymentIntentSummary } from './payment-intent-summary';

export interface PaymentIntentSummaryResponse {
  code: 0;
  data: unknown & { item: PaymentIntentSummary; };
  /** Server-owned request correlation id. */
  traceId: string;
}
