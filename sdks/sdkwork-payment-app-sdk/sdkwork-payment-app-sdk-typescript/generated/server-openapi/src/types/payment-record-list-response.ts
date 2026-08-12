import type { PageInfo } from './page-info';
import type { PaymentRecord } from './payment-record';

export interface PaymentRecordListResponse {
  code: 0;
  data: { items: PaymentRecord[]; pageInfo: PageInfo; };
  traceId: string;
}
