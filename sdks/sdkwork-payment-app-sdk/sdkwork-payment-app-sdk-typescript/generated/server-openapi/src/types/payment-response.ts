import type { Payment } from './payment';

export interface PaymentResponse {
  code: 0;
  data: { item: Payment; };
  traceId: string;
}
