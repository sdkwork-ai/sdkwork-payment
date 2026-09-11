import type { Certificate } from './certificate';
import type { PageInfo } from './page-info';
export interface CertificateListResponse {
    code: 0;
    data: unknown & {
        items: Certificate[];
        pageInfo: PageInfo;
    };
    /** Server-owned request correlation id. */
    traceId: string;
}
//# sourceMappingURL=certificate-list-response.d.ts.map