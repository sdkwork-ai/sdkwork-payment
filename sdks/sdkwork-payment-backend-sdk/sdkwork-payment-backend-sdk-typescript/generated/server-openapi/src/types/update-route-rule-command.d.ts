export interface UpdateRouteRuleCommand {
    priority?: number;
    purchaseType?: string;
    countryCode?: string;
    currencyCode?: string;
    clientPlatform?: string;
    amountMin?: string;
    amountMax?: string;
    userSegment?: string;
    riskLevel?: string;
    channelId?: string;
    status?: 'active' | 'inactive' | 'deprecated';
    startsAt?: string;
    endsAt?: string;
}
//# sourceMappingURL=update-route-rule-command.d.ts.map