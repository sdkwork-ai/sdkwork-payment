import type { CashierTokenSetting } from './cashier-token-setting';

export interface CashierTokenSettingItemResponse {
  code: 0;
  data: unknown & { item: CashierTokenSetting; };
  /** Server-owned request correlation id. */
  traceId: string;
}
