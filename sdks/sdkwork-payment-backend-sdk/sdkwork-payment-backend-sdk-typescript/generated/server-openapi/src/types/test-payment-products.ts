import type { TestPaymentProduct } from './test-payment-product';

export interface TestPaymentProducts {
  products: TestPaymentProduct[];
  /** Provider codes whose adapter declares no product in this build */
  unconfiguredProviders: string[];
  readyCount: number;
  totalCount: number;
  checkedAt: string;
}
