import type { Refund } from './refund';
export interface RefundResponse {
    code: 0;
    data: unknown & {
        item: Refund;
    };
    /** Server-owned request correlation id. */
    traceId: string;
}
//# sourceMappingURL=refund-response.d.ts.map