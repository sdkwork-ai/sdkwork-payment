export interface CredentialRotateCommand {
    primarySecret: string;
    webhookSecret?: string;
    certificate?: string;
    /** Supersede previous active credential versions */
    invalidatePrevious?: boolean;
}
//# sourceMappingURL=credential-rotate-command.d.ts.map