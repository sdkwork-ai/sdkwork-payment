import type { PageInfo } from './page-info';
import type { RouteRule } from './route-rule';
export interface RouteRuleListResponse {
    code: 0;
    data: unknown & {
        items: RouteRule[];
        pageInfo: PageInfo;
    };
    /** Server-owned request correlation id. */
    traceId: string;
}
//# sourceMappingURL=route-rule-list-response.d.ts.map