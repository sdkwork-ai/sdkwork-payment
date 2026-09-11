import type { PageInfo } from './page-info';
import type { Refund } from './refund';
export interface RefundListResponse {
    code: 0;
    data: unknown & {
        items: Refund[];
        pageInfo: PageInfo;
    };
    /** Server-owned request correlation id. */
    traceId: string;
}
//# sourceMappingURL=refund-list-response.d.ts.map