export interface CreateReconciliationRunCommand {
    providerCode: 'stripe' | 'alipay' | 'wechat_pay' | 'sandbox';
    providerAccountId: string;
    reconciliationType: 'daily' | 'weekly' | 'monthly' | 'manual' | 'settlement';
    periodStart: string;
    periodEnd: string;
    currencyCode: string;
}
//# sourceMappingURL=create-reconciliation-run-command.d.ts.map