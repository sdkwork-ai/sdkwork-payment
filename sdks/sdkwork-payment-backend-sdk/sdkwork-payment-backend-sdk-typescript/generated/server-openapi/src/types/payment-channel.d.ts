export interface PaymentChannel {
    id?: string;
    channelNo?: string;
    channelName?: string;
    providerAccountId?: string;
    methodId?: string;
    providerCode?: string;
    sceneCode?: 'app' | 'web' | 'mini_program' | 'api';
    currencyCode?: string;
    countryCode?: string;
    status?: 'active' | 'inactive' | 'deprecated';
    priority?: number;
    sortOrder?: number;
    metadata?: Record<string, unknown>;
    createdAt?: string;
    updatedAt?: string;
}
//# sourceMappingURL=payment-channel.d.ts.map