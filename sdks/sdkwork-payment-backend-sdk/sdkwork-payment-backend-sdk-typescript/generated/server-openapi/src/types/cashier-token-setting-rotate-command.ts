export interface CashierTokenSettingRotateCommand {
  /** Epoch the operator saw. A stale value is rejected with a conflict instead of rotating twice and overwriting the rotation grace secret. */
  expectedEpoch?: string;
}
