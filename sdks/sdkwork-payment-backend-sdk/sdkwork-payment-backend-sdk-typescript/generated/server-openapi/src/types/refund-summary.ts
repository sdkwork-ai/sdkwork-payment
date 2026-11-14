import type { PaymentAmountBucket } from './payment-amount-bucket';
import type { PaymentReasonBucket } from './payment-reason-bucket';
import type { PaymentStatusBucket } from './payment-status-bucket';

export interface RefundSummary {
  totalCount: string;
  totalAmounts: PaymentAmountBucket[];
  statusCounts: PaymentStatusBucket[];
  reasonCounts: PaymentReasonBucket[];
}
