import type { NotifyDomain } from './notify-domain';
import type { PageInfo } from './page-info';

export interface NotifyDomainListResponse {
  code: 0;
  data: unknown & { items: NotifyDomain[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
