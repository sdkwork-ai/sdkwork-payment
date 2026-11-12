export interface PaymentDomainSetting {
  id?: string;
  organizationId?: string | null;
  h5Protocol: 'https' | 'http';
  h5Hostname: string;
  h5Port?: number | null;
  h5Status: 'active' | 'inactive';
  /** False when no settings row exists for the scope; the form starts empty and checkout keeps the deployment default origin. */
  configured: boolean;
  h5BaseUrl: string;
  /** Cashier deep-link template built from the configured H5 domain. */
  cashierUrlTemplate: string;
}
