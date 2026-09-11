import type { PageInfo } from './page-info';
import type { WebhookEvent } from './webhook-event';
export interface WebhookEventListResponse {
    code: 0;
    data: unknown & {
        items: WebhookEvent[];
        pageInfo: PageInfo;
    };
    /** Server-owned request correlation id. */
    traceId: string;
}
//# sourceMappingURL=webhook-event-list-response.d.ts.map