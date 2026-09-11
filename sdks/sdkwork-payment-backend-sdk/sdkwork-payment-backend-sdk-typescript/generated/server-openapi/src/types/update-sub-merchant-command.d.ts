export interface UpdateSubMerchantCommand {
    subMerchantName?: string;
    subAppId?: string;
    subMchId?: string;
    stripeConnectedAccountId?: string;
    status?: 'active' | 'inactive' | 'suspended' | 'deprecated';
    metadata?: Record<string, unknown>;
}
//# sourceMappingURL=update-sub-merchant-command.d.ts.map