/** ISV/partner mode sub-merchant. Maps to Alipay sub_appid, WeChat Pay sub_mch_id, or Stripe connected account id. */
export interface SubMerchant {
    id?: string;
    /** Parent partner (service provider) provider account id */
    providerAccountId?: string;
    subMerchantNo?: string;
    subMerchantName?: string;
    /** Alipay sub_appid under ISV appid */
    subAppId?: string;
    /** WeChat Pay sub_mch_id under partner mch_id */
    subMchId?: string;
    /** Stripe Connect connected account id (acct_...) */
    stripeConnectedAccountId?: string;
    providerCode?: 'stripe' | 'alipay' | 'wechat_pay';
    status?: 'active' | 'inactive' | 'suspended' | 'deprecated';
    /** Provider-specific: settlement config, fee split, rate */
    metadata?: Record<string, unknown>;
    createdAt?: string;
    updatedAt?: string;
}
//# sourceMappingURL=sub-merchant.d.ts.map