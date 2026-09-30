/**
 * Typed message contract for the payment channel admin package.
 *
 * The catalog in `../i18n` implements this interface for every supported
 * locale, so a missing translation is a type error rather than a runtime
 * fallback to English inside a zh-CN workspace.
 */

export interface ChannelAdminMessages {
  /** Cross-component vocabulary shared by all three managers. */
  common: {
    cancel: string;
    create: string;
    saveChanges: string;
    edit: string;
    delete: string;
    select: string;
    status: string;
    currency: string;
    country: string;
    sortOrder: string;
    priority: string;
    statusActive: string;
    statusInactive: string;
    statusDeprecated: string;
    sceneApp: string;
    sceneWeb: string;
    sceneMiniProgram: string;
    sceneApi: string;
    scopeGlobal: string;
    scopeTenant: string;
    scopeOrganization: string;
    /** Fallback for a rule/channel that has no country restriction. */
    global: string;
  };
  channel: {
    intro: string;
    createTitle: string;
    createButton: string;
    createDisabledTitle: string;
    createTitle2: string;
    emptyState: string;
    routeAriaLabel: string;
    methodFallback: string;
    providerFallback: string;
    /** `Priority {priority}` badge. */
    priorityBadge: string;
    dialogTitle: string;
    channelNumber: string;
    channelNumberPlaceholder: string;
    channelName: string;
    channelNamePlaceholder: string;
    paymentMethod: string;
    methodPlaceholder: string;
    providerAccount: string;
    providerPlaceholder: string;
    scene: string;
    errorChannelNumberRequired: string;
    errorMethodRequired: string;
    errorProviderRequired: string;
    errorPriorityInteger: string;
    errorCreateFailed: string;
  };
  method: {
    createButton: string;
    createBusyTitle: string;
    createTitle: string;
    emptyState: string;
    currencyLabel: string;
    countryLabel: string;
    sortOrderLabel: string;
    selectTitle: string;
    editTitle: string;
    editDialogTitle: string;
    createDialogTitle: string;
    provider: string;
    paymentMethod: string;
    methodPlaceholder: string;
    displayName: string;
    displayNamePlaceholder: string;
    scope: string;
    scopeImmutable: string;
    errorMethodKeyRequired: string;
    errorDisplayNameRequired: string;
    errorSortOrderInteger: string;
    errorSaveFailed: string;
  };
  routeRule: {
    createButton: string;
    createDisabledTitle: string;
    createTitle: string;
    emptyState: string;
    priorityHint: string;
    missingChannel: string;
    purchaseTypeLabel: string;
    countryLabel: string;
    currencyLabel: string;
    clientPlatformLabel: string;
    amountLabel: string;
    userSegmentLabel: string;
    riskLevelLabel: string;
    /** `Valid window: {starts} → {ends}` row. */
    validWindow: string;
    validWindowNow: string;
    validWindowForever: string;
    editTitle: string;
    deleteTitle: string;
    editBusyTitle: string;
    deleteBusyTitle: string;
    createDialogTitle: string;
    editDialogTitle: string;
    confirmDeleteTitle: string;
    /** `Delete route rule {ruleNo}? …` confirm body. */
    confirmDeleteDescription: string;
    confirmDeleteLabel: string;
    errorDeleteFailed: string;
    ruleNumber: string;
    ruleNumberPlaceholder: string;
    ruleNumberImmutable: string;
    targetChannel: string;
    channelPlaceholder: string;
    priorityPlaceholder: string;
    matchConditionsLegend: string;
    purchaseType: string;
    purchaseTypePlaceholder: string;
    clientPlatform: string;
    clientPlatformPlaceholder: string;
    amountMin: string;
    amountMinPlaceholder: string;
    amountMax: string;
    amountMaxPlaceholder: string;
    userSegment: string;
    userSegmentPlaceholder: string;
    riskLevel: string;
    riskLevelPlaceholder: string;
    validityWindowLegend: string;
    startsAt: string;
    endsAt: string;
    errorRuleNumberRequired: string;
    errorChannelRequired: string;
    errorPriorityInteger: string;
    errorAmountPattern: string;
    errorSaveFailed: string;
    createSubmit: string;
  };
}
