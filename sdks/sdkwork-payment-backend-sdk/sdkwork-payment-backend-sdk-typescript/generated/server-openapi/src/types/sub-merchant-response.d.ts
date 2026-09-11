import type { SubMerchant } from './sub-merchant';
export interface SubMerchantResponse {
    code: 0;
    data: unknown & {
        item: SubMerchant;
    };
    /** Server-owned request correlation id. */
    traceId: string;
}
//# sourceMappingURL=sub-merchant-response.d.ts.map