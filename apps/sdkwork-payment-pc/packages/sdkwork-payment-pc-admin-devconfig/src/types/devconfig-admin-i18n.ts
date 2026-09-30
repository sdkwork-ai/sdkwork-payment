/**
 * Typed message contract for the payment dev-config admin package
 * (environment switcher, webhook debugger, certificate manager, integration
 * logs, and the workspace shell).
 */

export interface DevConfigAdminMessages {
  workspace: {
    defaultTitle: string;
    sectionsAriaLabel: string;
    tabEnvironment: string;
    tabWebhook: string;
    tabCertificates: string;
    tabLogs: string;
  };
  common: {
    cancel: string;
    delete: string;
    provider: string;
    providerAccount: string;
    accountPlaceholder: string;
    noEligibleAccounts: string;
    status: string;
    errorAccountRequired: string;
  };
  environment: {
    emptyState: string;
    envDevelopment: string;
    envSandbox: string;
    envProduction: string;
    /** `{env} environment` badge title. */
    envBadgeTitle: string;
    modePartner: string;
    modeDirect: string;
    testHealthy: string;
    testFailed: string;
    testUntested: string;
    merchantIdLabel: string;
    certExpiryLabel: string;
    secretRefLabel: string;
    secretConfigured: string;
    secretMissing: string;
    secretHelper: string;
    /** `Environment for {accountNo}` sr-only label. */
    environmentFor: string;
    testButton: string;
    testBusyTitle: string;
    testTitle: string;
    resultVerified: string;
    resultFailed: string;
    providerLabel: string;
    environmentLabel: string;
    latencyLabel: string;
    pspCodeLabel: string;
    confirmSwitchTitle: string;
    confirmSwitchToProductionTitle: string;
    /** `Switch {accountNo} from {from} to {to}?` */
    confirmSwitchDescription: string;
    confirmSwitchToProductionWarning: string;
    confirmSwitchLabel: string;
  };
  certificates: {
    header: string;
    headerHint: string;
    registerButton: string;
    registerBusyTitle: string;
    registerTitle: string;
    emptyState: string;
    typeMerchantPrivateKey: string;
    typeProviderPublicKey: string;
    typePlatformCertificate: string;
    typeWebhookSecret: string;
    statusActive: string;
    statusPendingRotation: string;
    statusExpired: string;
    statusRevoked: string;
    /** `Expired {days}d ago` badge. */
    expiredDaysAgo: string;
    /** `Expires in {days}d` badge. */
    expiresInDays: string;
    subjectLabel: string;
    issuerLabel: string;
    expiresLabel: string;
    contentLabel: string;
    contentEncrypted: string;
    contentMissing: string;
    fingerprintLabel: string;
    deleteButtonTitle: string;
    dialogTitle: string;
    certificateNo: string;
    certificateNoPlaceholder: string;
    type: string;
    providerOptional: string;
    anyProvider: string;
    pemContent: string;
    pemPlaceholder: string;
    cancelTitle: string;
    registering: string;
    submitTitle: string;
    errorRequired: string;
    errorCreateFailed: string;
    errorRegisterFailed: string;
    confirmDeleteTitle: string;
    /** `Delete certificate {certificateNo}? …` */
    confirmDeleteDescription: string;
  };
  webhook: {
    sandboxHeader: string;
    sandboxIntroLead: string;
    sandboxIntroTail: string;
    eventType: string;
    eventTypePlaceholder: string;
    amountOptional: string;
    amountPlaceholder: string;
    currencyOptional: string;
    currencyPlaceholder: string;
    outTradeNoOptional: string;
    outTradeNoPlaceholder: string;
    errorEventTypeRequired: string;
    errorTriggerFailed: string;
    triggerSubmit: string;
    triggering: string;
    triggerSuccess: string;
    triggerFailed: string;
    operationLabel: string;
    statusLabel: string;
    signatureHeader: string;
    signatureIntroLead: string;
    signatureIntroTail: string;
    rawPayload: string;
    rawPayloadPlaceholder: string;
    signature: string;
    signaturePlaceholder: string;
    timestampOptional: string;
    timestampPlaceholder: string;
    headerNameOverride: string;
    headerNamePlaceholder: string;
    errorPayloadRequired: string;
    errorSignatureRequired: string;
    errorTestFailed: string;
    verifySubmit: string;
    verifying: string;
    verifySuccess: string;
    verifyFailed: string;
    algorithmLabel: string;
    testedAtLabel: string;
    recentHeader: string;
    recentIntro: string;
  };
  logs: {
    filterHeader: string;
    filterIntro: string;
    providerPlaceholder: string;
    statusAll: string;
    statusQueued: string;
    statusProcessing: string;
    statusProcessed: string;
    statusFailed: string;
    statusDead: string;
    receivedFrom: string;
    receivedTo: string;
    reset: string;
    applyFilter: string;
    replaySuccess: string;
    replayFailed: string;
    eventIdLabel: string;
    replayedAtLabel: string;
    emptyState: string;
    clearFilters: string;
    receivedLabel: string;
    processedLabel: string;
    lastErrorLabel: string;
    /** `Retries: {retries}` badge. */
    retriesBadge: string;
    retriesMaxSuffix: string;
    replayButton: string;
    replayDeadTitle: string;
    /** `Retry cap ({max}) reached` */
    replayCapTitle: string;
    replayTitle: string;
  };
}
