import type { SdkWorkCommandData } from './sdk-work-command-data';
export interface WebhookReplayCommandResponse {
    code: 0;
    data: unknown & SdkWorkCommandData;
    /** Server-owned request correlation id. */
    traceId: string;
}
//# sourceMappingURL=webhook-replay-command-response.d.ts.map