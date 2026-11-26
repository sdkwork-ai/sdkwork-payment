import type { CheckoutScenePolicyItem } from './checkout-scene-policy-item';
import type { PageInfo } from './page-info';

export interface CheckoutScenePolicyListResponse {
  code: 0;
  data: unknown & { items: CheckoutScenePolicyItem[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
