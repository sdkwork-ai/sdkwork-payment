import type { RefundSummary } from './refund-summary';

export interface RefundSummaryResponse {
  code: 0;
  data: unknown & { item: RefundSummary; };
  /** Server-owned request correlation id. */
  traceId: string;
}
