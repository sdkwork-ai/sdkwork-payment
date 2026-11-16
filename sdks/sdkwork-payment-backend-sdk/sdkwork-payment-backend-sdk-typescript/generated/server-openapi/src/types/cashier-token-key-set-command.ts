export interface CashierTokenKeySetCommand {
  /** New signing key material. Letters, digits and - _ + / = only; at least 32 characters. */
  key: string;
  /** Epoch the operator saw; a stale value is rejected instead of overwriting a concurrent change. */
  expectedEpoch?: string;
}
