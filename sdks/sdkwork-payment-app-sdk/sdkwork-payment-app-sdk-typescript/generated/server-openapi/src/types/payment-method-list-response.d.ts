import type { PageInfo } from './page-info';
import type { PaymentMethod } from './payment-method';
export interface PaymentMethodListResponse {
    code: 0;
    data: {
        items: PaymentMethod[];
        pageInfo: PageInfo;
    };
    traceId: string;
}
//# sourceMappingURL=payment-method-list-response.d.ts.map