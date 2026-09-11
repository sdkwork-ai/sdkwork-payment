import type { PageInfo } from './page-info';
import type { Refund } from './refund';
export interface RefundListResponse {
    code: 0;
    data: {
        items: Refund[];
        pageInfo: PageInfo;
    };
    traceId: string;
}
//# sourceMappingURL=refund-list-response.d.ts.map