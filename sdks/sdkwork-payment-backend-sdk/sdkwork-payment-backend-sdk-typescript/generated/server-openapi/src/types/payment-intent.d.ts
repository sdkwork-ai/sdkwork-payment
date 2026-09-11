export interface PaymentIntent {
    id?: string;
    paymentIntentNo?: string;
    orderId?: string;
    ownerUserId?: string;
    paymentMethod?: string;
    providerCode?: string;
    amount?: string;
    currencyCode?: string;
    status?: 'created' | 'pending' | 'processing' | 'succeeded' | 'failed' | 'canceled' | 'closed';
    createdAt?: string;
    updatedAt?: string;
}
//# sourceMappingURL=payment-intent.d.ts.map