import type { TestPayment } from './test-payment';
export interface TestPaymentResponse {
    code: 0;
    data: unknown & {
        item: TestPayment;
    };
    /** Server-owned request correlation id. */
    traceId: string;
}
//# sourceMappingURL=test-payment-response.d.ts.map