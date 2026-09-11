export interface ReconciliationRun {
    id?: string;
    runNo?: string;
    providerCode?: string;
    providerAccountId?: string;
    reconciliationType?: 'daily' | 'weekly' | 'monthly' | 'manual' | 'settlement';
    periodStart?: string;
    periodEnd?: string;
    status?: 'pending' | 'queued' | 'running' | 'succeeded' | 'failed' | 'canceled';
    matchedCount?: number;
    mismatchedCount?: number;
    unmatchedCount?: number;
    totalDifferenceAmount?: string;
    currencyCode?: string;
    createdAt?: string;
}
//# sourceMappingURL=reconciliation-run.d.ts.map