export interface CheckoutScenePolicyResolution {
  code: 0;
  data: unknown & { configured: boolean; productCode?: string | null; matchedScope?: string | null; };
  /** Server-owned request correlation id. */
  traceId: string;
}
