import type { ReconciliationRun } from './reconciliation-run';
export interface ReconciliationRunResponse {
    code: 0;
    data: unknown & {
        item: ReconciliationRun;
    };
    /** Server-owned request correlation id. */
    traceId: string;
}
//# sourceMappingURL=reconciliation-run-response.d.ts.map