export interface NotifyDomainUpdateRequest {
    protocol?: 'https' | 'http';
    hostname?: string;
    port?: number | null;
    isDefault?: boolean;
    status?: 'active' | 'inactive';
    sortOrder?: number;
}
//# sourceMappingURL=notify-domain-update-request.d.ts.map