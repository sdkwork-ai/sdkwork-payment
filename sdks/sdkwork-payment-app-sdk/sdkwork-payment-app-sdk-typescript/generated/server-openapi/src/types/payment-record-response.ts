import type { PaymentRecord } from './payment-record';

export interface PaymentRecordResponse {
  code: 0;
  data: { item: PaymentRecord; };
  traceId: string;
}
