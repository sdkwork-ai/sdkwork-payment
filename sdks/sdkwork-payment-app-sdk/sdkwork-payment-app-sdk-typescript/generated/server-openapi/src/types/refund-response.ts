import type { Refund } from './refund';

export interface RefundResponse {
  code: 0;
  data: { item: Refund; };
  traceId: string;
}
