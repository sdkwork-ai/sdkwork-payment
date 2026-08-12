import type { NotifyDomain } from './notify-domain';

export interface NotifyDomainItemResponse {
  code: 0;
  data: unknown & { item: NotifyDomain; };
  /** Server-owned request correlation id. */
  traceId: string;
}
