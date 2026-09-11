export interface PaymentAttempt {
    id?: string;
    paymentIntentId?: string;
    attemptNo?: string;
    providerCode?: string;
    channelId?: string;
    amount?: string;
    currencyCode?: string;
    status?: 'created' | 'pending' | 'processing' | 'succeeded' | 'failed' | 'canceled' | 'closed';
    providerTransactionId?: string;
    outTradeNo?: string;
    paidAt?: string;
    createdAt?: string;
}
//# sourceMappingURL=payment-attempt.d.ts.map