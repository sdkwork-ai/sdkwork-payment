export interface UpdateProviderAccountCommand {
    merchantId?: string;
    /** Operator-facing account name. Omit to preserve the current value. */
    accountName?: string;
    accountMode?: 'direct' | 'partner';
    partnerProviderAccountId?: string;
    environment?: 'development' | 'sandbox' | 'production';
    countryCode?: string;
    settlementCurrency?: string;
    /** Replacement primary secret. Omit to preserve the current value. */
    primarySecret?: string;
    /** Replacement webhook/API v3 secret. Omit to preserve the current value. */
    webhookSecret?: string;
    /** Replacement certificate/public key. Omit to preserve the current value. */
    certificate?: string;
    capabilities?: Record<string, unknown>;
    status?: 'active' | 'inactive' | 'suspended' | 'deprecated';
    metadata?: Record<string, unknown>;
}
//# sourceMappingURL=update-provider-account-command.d.ts.map