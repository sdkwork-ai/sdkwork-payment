export interface CreatePaymentMethodCommand {
    methodKey: string;
    displayName: string;
    providerCode: 'stripe' | 'alipay' | 'wechat_pay' | 'sandbox';
    status?: 'active' | 'inactive' | 'deprecated';
    scope?: 'global' | 'tenant' | 'organization';
    currencyCode?: string;
    countryCode?: string;
    sortOrder?: number;
    metadata?: Record<string, unknown>;
}
//# sourceMappingURL=create-payment-method-command.d.ts.map