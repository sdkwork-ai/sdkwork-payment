export interface WebhookEvent {
    id?: string;
    eventId?: string;
    providerCode?: string;
    eventType?: string;
    status?: 'queued' | 'processing' | 'processed' | 'failed' | 'dead';
    retries?: number;
    lastError?: string;
    receivedAt?: string;
    processedAt?: string;
}
//# sourceMappingURL=webhook-event.d.ts.map