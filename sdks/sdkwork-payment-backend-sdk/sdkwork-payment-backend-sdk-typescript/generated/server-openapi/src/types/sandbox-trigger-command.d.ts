export interface SandboxTriggerCommand {
    /** Must reference a development/sandbox environment account */
    providerAccountId: string;
    /** PSP event type: stripe payment_intent.succeeded, alipay TRADE_SUCCESS, wechat_pay TRANSACTION_SUCCESS */
    eventType: string;
    amount?: string;
    currencyCode?: string;
    /** Optional existing attempt out_trade_no to attach */
    outTradeNo?: string;
}
//# sourceMappingURL=sandbox-trigger-command.d.ts.map