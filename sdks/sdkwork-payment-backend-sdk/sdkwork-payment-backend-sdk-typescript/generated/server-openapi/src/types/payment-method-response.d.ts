import type { PaymentMethod } from './payment-method';
export interface PaymentMethodResponse {
    code: 0;
    data: unknown & {
        item: PaymentMethod;
    };
    /** Server-owned request correlation id. */
    traceId: string;
}
//# sourceMappingURL=payment-method-response.d.ts.map