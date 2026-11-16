export interface CashierTokenSecretReveal {
  /** Scope that supplied the signing key (a tenant id or the platform 0). */
  tenantId: string;
  /** Epoch the stored key signs under, as a decimal string; null when no key is stored. */
  keyEpoch?: string | null;
  /** Stored signing key material; null when no signing key is stored for the tenant. */
  key?: string | null;
  /** Where the effective signing key comes from. */
  source: 'tenant' | 'platform' | 'default' | 'deployment';
}
