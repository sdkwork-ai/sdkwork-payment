export interface NotifyDomainCreateRequest {
    protocol: 'https' | 'http';
    hostname: string;
    port?: number | null;
    isDefault?: boolean;
    status?: 'active' | 'inactive';
    sortOrder?: number;
}
//# sourceMappingURL=notify-domain-create-request.d.ts.map