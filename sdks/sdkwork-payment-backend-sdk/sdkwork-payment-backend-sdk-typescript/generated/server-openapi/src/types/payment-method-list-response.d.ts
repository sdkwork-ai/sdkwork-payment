import type { PageInfo } from './page-info';
import type { PaymentMethod } from './payment-method';
export interface PaymentMethodListResponse {
    code: 0;
    data: unknown & {
        items: PaymentMethod[];
        pageInfo: PageInfo;
    };
    /** Server-owned request correlation id. */
    traceId: string;
}
//# sourceMappingURL=payment-method-list-response.d.ts.map