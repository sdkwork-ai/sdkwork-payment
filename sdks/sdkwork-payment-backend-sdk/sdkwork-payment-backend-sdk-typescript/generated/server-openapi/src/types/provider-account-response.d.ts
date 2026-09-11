import type { ProviderAccount } from './provider-account';
export interface ProviderAccountResponse {
    code: 0;
    data: unknown & {
        item: ProviderAccount;
    };
    /** Server-owned request correlation id. */
    traceId: string;
}
//# sourceMappingURL=provider-account-response.d.ts.map