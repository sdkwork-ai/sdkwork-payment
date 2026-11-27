import type { TestPaymentProducts } from './test-payment-products';

export interface TestPaymentProductsResponse {
  code: 0;
  data: unknown & { item: TestPaymentProducts; };
  /** Server-owned request correlation id. */
  traceId: string;
}
