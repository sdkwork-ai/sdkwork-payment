export interface UpdatePaymentDomainSettingRequest {
  h5Protocol: 'https' | 'http';
  /** Bare hostname without scheme, port, or path (e.g. h5.example.com). */
  h5Hostname: string;
  h5Port?: number | null;
  h5Status?: 'active' | 'inactive';
}
