import type { PaymentAmountBucket } from './payment-amount-bucket';
import type { PaymentStatusBucket } from './payment-status-bucket';

export interface PaymentIntentSummary {
  totalCount: string;
  totalAmounts: PaymentAmountBucket[];
  statusCounts: PaymentStatusBucket[];
}
