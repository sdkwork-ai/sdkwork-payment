export interface PaymentMethod {
    id?: string;
    methodKey?: string;
    displayName?: string;
    providerCode?: string;
    status?: 'active' | 'inactive' | 'deprecated';
    scope?: 'global' | 'tenant' | 'organization';
    currencyCode?: string;
    countryCode?: string;
    sortOrder?: number;
    metadata?: Record<string, unknown>;
    createdAt?: string;
    updatedAt?: string;
}
//# sourceMappingURL=payment-method.d.ts.map