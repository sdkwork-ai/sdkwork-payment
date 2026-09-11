import type { ProviderAccountTestResult } from './provider-account-test-result';
export interface ProviderAccountTestResultResponse {
    code: 0;
    data: unknown & {
        item: ProviderAccountTestResult;
    };
    /** Server-owned request correlation id. */
    traceId: string;
}
//# sourceMappingURL=provider-account-test-result-response.d.ts.map