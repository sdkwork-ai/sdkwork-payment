export interface RouteRule {
    id?: string;
    ruleNo?: string;
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
    createdAt?: string;
    updatedAt?: string;
}
//# sourceMappingURL=route-rule.d.ts.map