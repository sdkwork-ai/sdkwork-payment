import type { RouteRule } from './route-rule';
export interface RouteRuleResponse {
    code: 0;
    data: unknown & {
        item: RouteRule;
    };
    /** Server-owned request correlation id. */
    traceId: string;
}
//# sourceMappingURL=route-rule-response.d.ts.map