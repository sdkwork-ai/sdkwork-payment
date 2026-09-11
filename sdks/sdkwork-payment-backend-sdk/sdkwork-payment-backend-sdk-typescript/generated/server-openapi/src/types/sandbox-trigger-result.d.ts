export interface SandboxTriggerResult {
    operationId: string;
    eventId: string;
    /** Stored webhook event id */
    webhookEventId: string;
    paymentAttemptId?: string;
    /** Payment attempt status after ingesting the simulated callback (e.g. succeeded) */
    appliedStatus?: string;
}
//# sourceMappingURL=sandbox-trigger-result.d.ts.map