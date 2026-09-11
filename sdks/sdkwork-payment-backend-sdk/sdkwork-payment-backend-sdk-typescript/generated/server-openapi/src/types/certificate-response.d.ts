import type { Certificate } from './certificate';
export interface CertificateResponse {
    code: 0;
    data: unknown & {
        item: Certificate;
    };
    /** Server-owned request correlation id. */
    traceId: string;
}
//# sourceMappingURL=certificate-response.d.ts.map