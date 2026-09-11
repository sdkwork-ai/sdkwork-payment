export interface ProviderAccountTestCommand {
    /** Override target environment for this test (defaults to account.environment) */
    environment?: 'development' | 'sandbox' | 'production';
    /** Decrypt database credentials and initialize the provider adapter without creating a payment */
    dryRun?: boolean;
}
//# sourceMappingURL=provider-account-test-command.d.ts.map