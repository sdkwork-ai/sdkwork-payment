import type { PageInfo } from './page-info';
import type { PaymentAttempt } from './payment-attempt';
export interface PaymentAttemptListResponse {
    code: 0;
    data: unknown & {
        items: PaymentAttempt[];
        pageInfo: PageInfo;
    };
    /** Server-owned request correlation id. */
    traceId: string;
}
//# sourceMappingURL=payment-attempt-list-response.d.ts.map