export interface CreateCertificateCommand {
    certificateNo: string;
    providerCode: 'stripe' | 'alipay' | 'wechat_pay';
    certificateType: 'merchant_private_key' | 'provider_public_key' | 'platform_certificate' | 'webhook_secret';
    /** PEM content encrypted before database persistence */
    certificate: string;
    metadata?: Record<string, unknown>;
}
//# sourceMappingURL=create-certificate-command.d.ts.map