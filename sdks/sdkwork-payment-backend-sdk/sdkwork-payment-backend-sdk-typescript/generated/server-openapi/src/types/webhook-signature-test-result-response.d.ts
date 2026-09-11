import type { WebhookSignatureTestResult } from './webhook-signature-test-result';
export interface WebhookSignatureTestResultResponse {
    code: 0;
    data: unknown & {
        item: WebhookSignatureTestResult;
    };
    /** Server-owned request correlation id. */
    traceId: string;
}
//# sourceMappingURL=webhook-signature-test-result-response.d.ts.map