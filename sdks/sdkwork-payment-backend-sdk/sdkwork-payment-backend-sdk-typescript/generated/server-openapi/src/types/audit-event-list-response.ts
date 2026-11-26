import type { AuditEvent } from './audit-event';
import type { PageInfo } from './page-info';

export interface AuditEventListResponse {
  code: 0;
  data: unknown & { items: AuditEvent[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
