import type { PaymentIntent } from './payment-intent';

export interface PaymentIntentResponse {
  code: 0;
  data: { item: PaymentIntent; };
  traceId: string;
}
