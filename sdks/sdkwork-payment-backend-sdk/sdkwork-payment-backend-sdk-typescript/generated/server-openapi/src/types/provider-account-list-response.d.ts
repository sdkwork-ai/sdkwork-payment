import type { PageInfo } from './page-info';
import type { ProviderAccount } from './provider-account';
export interface ProviderAccountListResponse {
    code: 0;
    data: unknown & {
        items: ProviderAccount[];
        pageInfo: PageInfo;
    };
    /** Server-owned request correlation id. */
    traceId: string;
}
//# sourceMappingURL=provider-account-list-response.d.ts.map