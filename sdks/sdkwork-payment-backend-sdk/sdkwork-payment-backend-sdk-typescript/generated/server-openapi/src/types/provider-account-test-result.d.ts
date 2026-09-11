export interface ProviderAccountTestResult {
    ok: boolean;
    providerCode: string;
    environment: string;
    /** Raw PSP response code (e.g., 200 for Stripe balance) */
    pspResponseCode?: string;
    /** PSP round-trip latency in ms */
    pspResponseTimeMs?: number;
    /** Safe diagnostic message (no secrets) */
    diagnostic?: string;
    testedAt?: string;
}
//# sourceMappingURL=provider-account-test-result.d.ts.map