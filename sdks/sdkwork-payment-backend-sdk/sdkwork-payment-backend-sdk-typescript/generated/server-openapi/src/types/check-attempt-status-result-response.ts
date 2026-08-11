import type { CheckAttemptStatusResult } from './check-attempt-status-result';

export interface CheckAttemptStatusResultResponse {
  code: 0;
  data: unknown & { item: CheckAttemptStatusResult; };
  /** Server-owned request correlation id. */
  traceId: string;
}
