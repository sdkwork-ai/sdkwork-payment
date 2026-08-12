export interface NotifyDomainCreateRequest {
  protocol: 'https' | 'http';
  hostname: string;
  port?: number | null;
  isDefault?: boolean;
  status?: 'active' | 'inactive';
  sortOrder?: number;
}
