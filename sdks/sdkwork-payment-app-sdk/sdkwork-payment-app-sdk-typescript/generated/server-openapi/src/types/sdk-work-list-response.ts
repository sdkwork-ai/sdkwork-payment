import type { PageInfo } from './page-info';

export interface SdkWorkListResponse {
  code: 0;
  data: { items: Record<string, unknown>[]; pageInfo: PageInfo; };
  traceId: string;
}
