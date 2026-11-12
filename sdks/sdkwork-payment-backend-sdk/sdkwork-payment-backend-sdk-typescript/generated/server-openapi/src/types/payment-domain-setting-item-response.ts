import type { PaymentDomainSetting } from './payment-domain-setting';

export interface PaymentDomainSettingItemResponse {
  code: 0;
  data: unknown & { item: PaymentDomainSetting; };
  /** Server-owned request correlation id. */
  traceId: string;
}
