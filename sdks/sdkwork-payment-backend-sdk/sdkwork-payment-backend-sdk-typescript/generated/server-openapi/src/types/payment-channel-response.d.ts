import type { PaymentChannel } from './payment-channel';
export interface PaymentChannelResponse {
    code: 0;
    data: unknown & {
        item: PaymentChannel;
    };
    /** Server-owned request correlation id. */
    traceId: string;
}
//# sourceMappingURL=payment-channel-response.d.ts.map