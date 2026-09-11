import type { PageInfo } from './page-info';
import type { ReconciliationRun } from './reconciliation-run';
export interface ReconciliationRunListResponse {
    code: 0;
    data: unknown & {
        items: ReconciliationRun[];
        pageInfo: PageInfo;
    };
    /** Server-owned request correlation id. */
    traceId: string;
}
//# sourceMappingURL=reconciliation-run-list-response.d.ts.map