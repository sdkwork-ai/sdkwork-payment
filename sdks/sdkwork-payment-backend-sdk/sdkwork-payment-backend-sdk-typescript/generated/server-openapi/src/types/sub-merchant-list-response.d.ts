import type { PageInfo } from './page-info';
import type { SubMerchant } from './sub-merchant';
export interface SubMerchantListResponse {
    code: 0;
    data: unknown & {
        items: SubMerchant[];
        pageInfo: PageInfo;
    };
    /** Server-owned request correlation id. */
    traceId: string;
}
//# sourceMappingURL=sub-merchant-list-response.d.ts.map