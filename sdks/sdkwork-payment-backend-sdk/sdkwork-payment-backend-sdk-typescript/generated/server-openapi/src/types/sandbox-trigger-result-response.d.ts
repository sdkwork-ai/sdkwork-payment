import type { SandboxTriggerResult } from './sandbox-trigger-result';
export interface SandboxTriggerResultResponse {
    code: 0;
    data: unknown & {
        item: SandboxTriggerResult;
    };
    /** Server-owned request correlation id. */
    traceId: string;
}
//# sourceMappingURL=sandbox-trigger-result-response.d.ts.map