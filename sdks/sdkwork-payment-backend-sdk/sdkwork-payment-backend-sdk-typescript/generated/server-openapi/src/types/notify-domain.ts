export interface NotifyDomain {
  id: string;
  organizationId?: string | null;
  protocol: 'https' | 'http';
  hostname: string;
  port?: number | null;
  isDefault: boolean;
  status: 'active' | 'inactive';
  sortOrder?: number;
  paymentNotifyUrl: string;
  refundNotifyUrl: string;
}
