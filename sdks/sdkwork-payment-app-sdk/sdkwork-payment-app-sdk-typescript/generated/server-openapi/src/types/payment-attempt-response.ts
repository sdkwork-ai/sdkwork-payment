import type { PaymentAttempt } from './payment-attempt';

export interface PaymentAttemptResponse {
  code: 0;
  data: { item: PaymentAttempt; };
  traceId: string;
}
