import type { PageInfo } from './page-info';
import type { PaymentIntent } from './payment-intent';
export interface PaymentIntentListResponse {
    code: 0;
    data: unknown & {
        items: PaymentIntent[];
        pageInfo: PageInfo;
    };
    /** Server-owned request correlation id. */
    traceId: string;
}
//# sourceMappingURL=payment-intent-list-response.d.ts.map