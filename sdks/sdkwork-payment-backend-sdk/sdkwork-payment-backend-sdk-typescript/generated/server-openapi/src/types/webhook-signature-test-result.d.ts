export interface WebhookSignatureTestResult {
    ok: boolean;
    providerCode: string;
    /** Detected algorithm: HMAC-SHA256, RSA-SHA256, AES-GCM */
    algorithm?: string;
    /** Safe diagnostic message */
    diagnostic?: string;
    testedAt?: string;
}
//# sourceMappingURL=webhook-signature-test-result.d.ts.map