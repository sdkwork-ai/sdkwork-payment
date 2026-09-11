export interface CreateRefundCommand {
    paymentIntentId: string;
    /** Refund amount as an integer in the currency's smallest unit (minor units, e.g. cents). Omitted for a full refund. */
    amount?: string;
    reasonCode: 'customer_request' | 'duplicate' | 'fraud' | 'service_failure' | 'other';
    /** Exact payment intent number typed by the operator as a high-risk action confirmation. */
    confirmPaymentIntentNo: string;
}
//# sourceMappingURL=create-refund-command.d.ts.map