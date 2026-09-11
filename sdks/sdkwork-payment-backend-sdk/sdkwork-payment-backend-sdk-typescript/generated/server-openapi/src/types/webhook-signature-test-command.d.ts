export interface WebhookSignatureTestCommand {
    providerAccountId: string;
    /** Raw webhook request body (the content that was signed) */
    payload: string;
    /** Signature header value (e.g., stripe-signature, WeChat Wechatpay-Signature) */
    signature: string;
    /** Timestamp header for replay protection (e.g., stripe t=, WeChat Wechatpay-Timestamp) */
    timestamp?: string;
    /** Full signature header name override when non-standard */
    signatureHeader?: string;
}
//# sourceMappingURL=webhook-signature-test-command.d.ts.map