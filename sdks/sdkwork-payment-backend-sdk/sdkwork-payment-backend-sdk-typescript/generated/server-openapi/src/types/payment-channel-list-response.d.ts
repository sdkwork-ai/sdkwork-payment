import type { PageInfo } from './page-info';
import type { PaymentChannel } from './payment-channel';
export interface PaymentChannelListResponse {
    code: 0;
    data: unknown & {
        items: PaymentChannel[];
        pageInfo: PageInfo;
    };
    /** Server-owned request correlation id. */
    traceId: string;
}
//# sourceMappingURL=payment-channel-list-response.d.ts.map