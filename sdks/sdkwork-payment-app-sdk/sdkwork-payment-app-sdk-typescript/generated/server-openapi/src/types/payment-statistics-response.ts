import type { PaymentStatistics } from './payment-statistics';

export interface PaymentStatisticsResponse {
  code: 0;
  data: { item: PaymentStatistics; };
  traceId: string;
}
