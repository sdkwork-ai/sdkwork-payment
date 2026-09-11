export interface CreateSubMerchantCommand {
    /** Must reference a partner-mode provider account */
    providerAccountId: string;
    subMerchantNo: string;
    subMerchantName?: string;
    subAppId?: string;
    subMchId?: string;
    stripeConnectedAccountId?: string;
    providerCode: 'stripe' | 'alipay' | 'wechat_pay';
    status?: 'active' | 'inactive' | 'suspended' | 'deprecated';
    metadata?: Record<string, unknown>;
}
//# sourceMappingURL=create-sub-merchant-command.d.ts.map