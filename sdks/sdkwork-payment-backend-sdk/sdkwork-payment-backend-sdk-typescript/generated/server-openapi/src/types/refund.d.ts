export interface Refund {
    id: string;
    refundNo: string;
    orderId: string;
    paymentIntentId: string;
    paymentAttemptId: string;
    providerCode: 'stripe' | 'alipay' | 'wechat_pay' | 'sandbox';
    providerAccountId?: string;
    /** Refund amount as an integer in the currency's smallest unit (minor units, e.g. cents). */
    amount: string;
    currencyCode: string;
    status: 'submitted' | 'processing' | 'succeeded' | 'failed' | 'closed';
    reasonCode?: 'customer_request' | 'duplicate' | 'fraud' | 'service_failure' | 'other';
    requestedByType: 'buyer' | 'operator' | 'system';
    requestedBy?: string;
    createdAt: string;
    updatedAt: string;
}
//# sourceMappingURL=refund.d.ts.map