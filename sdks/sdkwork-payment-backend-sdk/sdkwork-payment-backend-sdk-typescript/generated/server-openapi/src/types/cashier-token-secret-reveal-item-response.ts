import type { CashierTokenSecretReveal } from './cashier-token-secret-reveal';

export interface CashierTokenSecretRevealItemResponse {
  code: 0;
  data: unknown & { item: CashierTokenSecretReveal; };
  /** Server-owned request correlation id. */
  traceId: string;
}
