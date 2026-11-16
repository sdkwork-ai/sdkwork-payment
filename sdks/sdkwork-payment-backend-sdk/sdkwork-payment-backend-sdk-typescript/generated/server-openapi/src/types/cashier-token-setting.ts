export interface CashierTokenSetting {
  /** Tenant scope the effective setting belongs to. */
  tenantId?: string;
  /** Active key epoch, serialized as a decimal string per API_SPEC §13.6. Epoch 1 is the legacy derivation. */
  keyEpoch: string;
  /** Last rotation time, RFC 3339 UTC; null when the tenant never rotated. */
  rotatedAt?: string | null;
  /** Operator id that performed the last rotation; empty when unavailable. */
  rotatedBy: string;
  updatedAt?: string | null;
  /** Which scope supplied the effective epoch: the tenant row, the platform '0' fallback row, or the legacy default. */
  source: 'tenant' | 'platform' | 'default';
}
