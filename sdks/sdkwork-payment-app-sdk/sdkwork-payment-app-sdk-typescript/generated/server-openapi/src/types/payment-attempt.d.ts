export interface PaymentAttempt {
    attemptId: string;
    paymentIntentId?: string;
    orderId: string;
    outTradeNo?: string;
    paymentMethod: string;
    providerCode?: string;
    amount: string;
    status: string;
    paymentParams?: Record<string, string>;
}
//# sourceMappingURL=payment-attempt.d.ts.map