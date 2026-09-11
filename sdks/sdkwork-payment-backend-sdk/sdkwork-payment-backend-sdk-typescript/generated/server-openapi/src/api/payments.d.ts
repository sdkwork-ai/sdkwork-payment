import type { ApiRequestOptions, HttpClient } from '../http/client';
import type { Certificate, CheckAttemptStatusCommand, CheckAttemptStatusResult, CreateCertificateCommand, CreatePaymentChannelCommand, CreatePaymentMethodCommand, CreateProviderAccountCommand, CreateReconciliationRunCommand, CreateRefundCommand, CreateRouteRuleCommand, CreateSubMerchantCommand, CreateTestPaymentCommand, CredentialRotateCommand, NotifyDomain, NotifyDomainCreateRequest, NotifyDomainUpdateRequest, PageInfo, PaymentAttempt, PaymentChannel, PaymentIntent, PaymentMethod, ProviderAccount, ProviderAccountTestCommand, ProviderAccountTestResult, ReconciliationRun, Refund, RetryRefundCommand, RouteRule, SandboxTriggerCommand, SandboxTriggerResult, SdkWorkCommandData, SubMerchant, TestPayment, UpdatePaymentChannelCommand, UpdatePaymentMethodCommand, UpdateProviderAccountCommand, UpdateRouteRuleCommand, UpdateSubMerchantCommand, WebhookEvent, WebhookEventsReplayRequest, WebhookSignatureTestCommand, WebhookSignatureTestResult } from '../types';
export interface PaymentsDevSandboxTriggerParams {
    idempotencyKey: string;
}
export interface PaymentsDevTestPaymentsParams {
    idempotencyKey: string;
}
export interface PaymentsDevCheckAttemptStatusParams {
    idempotencyKey: string;
}
export interface PaymentsDevWebhookSignatureTestParams {
    idempotencyKey: string;
}
export declare class PaymentsDevApi {
    private client;
    constructor(client: HttpClient);
    /** Sandbox event trigger (dev config). */
    sandboxTrigger(body: SandboxTriggerCommand, params: PaymentsDevSandboxTriggerParams, requestOptions?: ApiRequestOptions): Promise<SandboxTriggerResult>;
    /** Create a test payment (dev config). */
    testPayments(body: CreateTestPaymentCommand, params: PaymentsDevTestPaymentsParams, requestOptions?: ApiRequestOptions): Promise<TestPayment>;
    /** Check PSP payment status for a dev test payment (dev config). */
    checkAttemptStatus(body: CheckAttemptStatusCommand, params: PaymentsDevCheckAttemptStatusParams, requestOptions?: ApiRequestOptions): Promise<CheckAttemptStatusResult>;
    /** Webhook signature verification test (dev config). */
    webhookSignatureTest(body: WebhookSignatureTestCommand, params: PaymentsDevWebhookSignatureTestParams, requestOptions?: ApiRequestOptions): Promise<WebhookSignatureTestResult>;
}
export interface PaymentsReconciliationRunsListParams {
    page?: number;
    pageSize?: number;
    sort?: string;
    q?: string;
    status?: 'pending' | 'queued' | 'running' | 'succeeded' | 'failed' | 'canceled';
    providerCode?: string;
    providerAccountId?: string;
}
export interface PaymentsReconciliationRunsCreateParams {
    idempotencyKey: string;
}
export declare class PaymentsReconciliationRunsApi {
    private client;
    constructor(client: HttpClient);
    /** Reconciliation runs list. */
    list(params?: PaymentsReconciliationRunsListParams, requestOptions?: ApiRequestOptions): Promise<{
        items: ReconciliationRun[];
        pageInfo: PageInfo;
    }>;
    /** Reconciliation run create. */
    create(body: CreateReconciliationRunCommand, params: PaymentsReconciliationRunsCreateParams, requestOptions?: ApiRequestOptions): Promise<ReconciliationRun>;
}
export interface PaymentsWebhookEventsListParams {
    page?: number;
    pageSize?: number;
    sort?: string;
    q?: string;
    status?: 'queued' | 'processing' | 'processed' | 'failed' | 'dead';
    providerCode?: string;
    eventType?: string;
}
export declare class PaymentsWebhookEventsApi {
    private client;
    constructor(client: HttpClient);
    /** Webhook events list. */
    list(params?: PaymentsWebhookEventsListParams, requestOptions?: ApiRequestOptions): Promise<{
        items: WebhookEvent[];
        pageInfo: PageInfo;
    }>;
    /** Webhook event replay. */
    replay(eventId: string, body?: WebhookEventsReplayRequest, requestOptions?: ApiRequestOptions): Promise<SdkWorkCommandData>;
}
export interface PaymentsAttemptsListParams {
    page?: number;
    pageSize?: number;
    sort?: string;
    q?: string;
    status?: 'created' | 'pending' | 'processing' | 'succeeded' | 'failed' | 'canceled' | 'closed';
    providerCode?: string;
    paymentIntentId?: string;
}
export declare class PaymentsAttemptsApi {
    private client;
    constructor(client: HttpClient);
    /** Payment attempts list. */
    list(params?: PaymentsAttemptsListParams, requestOptions?: ApiRequestOptions): Promise<{
        items: PaymentAttempt[];
        pageInfo: PageInfo;
    }>;
}
export interface PaymentsCertificatesListParams {
    page?: number;
    pageSize?: number;
    sort?: string;
    q?: string;
    providerCode?: string;
    certificateType?: 'merchant_private_key' | 'provider_public_key' | 'platform_certificate' | 'webhook_secret';
    expiringWithinDays?: number;
}
export interface PaymentsCertificatesCreateParams {
    idempotencyKey: string;
}
export declare class PaymentsCertificatesApi {
    private client;
    constructor(client: HttpClient);
    /** Certificates list. */
    list(params?: PaymentsCertificatesListParams, requestOptions?: ApiRequestOptions): Promise<{
        items: Certificate[];
        pageInfo: PageInfo;
    }>;
    /** Certificate create (upload/register PEM). */
    create(body: CreateCertificateCommand, params: PaymentsCertificatesCreateParams, requestOptions?: ApiRequestOptions): Promise<Certificate>;
    /** Certificate retrieve. */
    retrieve(certificateId: string, requestOptions?: ApiRequestOptions): Promise<Certificate>;
    /** Certificate delete. */
    delete(certificateId: string, requestOptions?: ApiRequestOptions): Promise<void>;
}
export interface PaymentsSubMerchantsListParams {
    page?: number;
    pageSize?: number;
    sort?: string;
    q?: string;
    providerAccountId?: string;
    providerCode?: string;
    status?: 'active' | 'inactive' | 'suspended' | 'deprecated';
}
export interface PaymentsSubMerchantsCreateParams {
    idempotencyKey: string;
}
export interface PaymentsSubMerchantsUpdateParams {
    idempotencyKey: string;
}
export declare class PaymentsSubMerchantsApi {
    private client;
    constructor(client: HttpClient);
    /** Sub-merchants list (ISV/partner mode only). */
    list(params?: PaymentsSubMerchantsListParams, requestOptions?: ApiRequestOptions): Promise<{
        items: SubMerchant[];
        pageInfo: PageInfo;
    }>;
    /** Sub-merchant create (ISV/partner mode only). */
    create(body: CreateSubMerchantCommand, params: PaymentsSubMerchantsCreateParams, requestOptions?: ApiRequestOptions): Promise<SubMerchant>;
    /** Sub-merchant retrieve. */
    retrieve(subMerchantId: string, requestOptions?: ApiRequestOptions): Promise<SubMerchant>;
    /** Sub-merchant update. */
    update(subMerchantId: string, body: UpdateSubMerchantCommand, params: PaymentsSubMerchantsUpdateParams, requestOptions?: ApiRequestOptions): Promise<SubMerchant>;
    /** Sub-merchant delete. */
    delete(subMerchantId: string, requestOptions?: ApiRequestOptions): Promise<void>;
}
export interface PaymentsRouteRulesListParams {
    page?: number;
    pageSize?: number;
    sort?: string;
    q?: string;
    status?: 'active' | 'inactive' | 'deprecated';
    channelId?: string;
}
export interface PaymentsRouteRulesCreateParams {
    idempotencyKey: string;
}
export interface PaymentsRouteRulesUpdateParams {
    idempotencyKey: string;
}
export declare class PaymentsRouteRulesApi {
    private client;
    constructor(client: HttpClient);
    /** Route rules list. */
    list(params?: PaymentsRouteRulesListParams, requestOptions?: ApiRequestOptions): Promise<{
        items: RouteRule[];
        pageInfo: PageInfo;
    }>;
    /** Route rule create. */
    create(body: CreateRouteRuleCommand, params: PaymentsRouteRulesCreateParams, requestOptions?: ApiRequestOptions): Promise<RouteRule>;
    /** Route rule update. */
    update(routeRuleId: string, body: UpdateRouteRuleCommand, params: PaymentsRouteRulesUpdateParams, requestOptions?: ApiRequestOptions): Promise<RouteRule>;
    /** Route rule delete. */
    delete(routeRuleId: string, requestOptions?: ApiRequestOptions): Promise<void>;
}
export interface PaymentsChannelsListParams {
    page?: number;
    pageSize?: number;
    sort?: string;
    q?: string;
    providerCode?: string;
    methodId?: string;
    sceneCode?: 'app' | 'web' | 'mini_program' | 'api';
    status?: 'active' | 'inactive' | 'deprecated';
}
export interface PaymentsChannelsCreateParams {
    idempotencyKey: string;
}
export interface PaymentsChannelsUpdateParams {
    idempotencyKey: string;
}
export declare class PaymentsChannelsApi {
    private client;
    constructor(client: HttpClient);
    /** Payment channels list. */
    list(params?: PaymentsChannelsListParams, requestOptions?: ApiRequestOptions): Promise<{
        items: PaymentChannel[];
        pageInfo: PageInfo;
    }>;
    /** Payment channel create. */
    create(body: CreatePaymentChannelCommand, params: PaymentsChannelsCreateParams, requestOptions?: ApiRequestOptions): Promise<PaymentChannel>;
    /** Payment channel update. */
    update(channelId: string, body: UpdatePaymentChannelCommand, params: PaymentsChannelsUpdateParams, requestOptions?: ApiRequestOptions): Promise<PaymentChannel>;
    /** Payment channel delete. */
    delete(channelId: string, requestOptions?: ApiRequestOptions): Promise<void>;
}
export interface PaymentsProviderAccountsCredentialsRotateParams {
    idempotencyKey: string;
}
export declare class PaymentsProviderAccountsCredentialsApi {
    private client;
    constructor(client: HttpClient);
    /** Provider account credentials read (decrypted). */
    read(providerAccountId: string, requestOptions?: ApiRequestOptions): Promise<{
        providerAccountId?: string;
        primarySecret?: string;
        webhookSecret?: string;
        certificate?: string;
    }>;
    /** Provider account credential rotation. */
    rotate(providerAccountId: string, body: CredentialRotateCommand, params: PaymentsProviderAccountsCredentialsRotateParams, requestOptions?: ApiRequestOptions): Promise<ProviderAccount>;
}
export interface PaymentsProviderAccountsListParams {
    page?: number;
    pageSize?: number;
    sort?: string;
    q?: string;
    providerCode?: 'stripe' | 'alipay' | 'wechat_pay' | 'sandbox';
    environment?: 'development' | 'sandbox' | 'production';
    accountMode?: 'direct' | 'partner';
    status?: 'active' | 'inactive' | 'suspended' | 'deprecated';
}
export interface PaymentsProviderAccountsCreateParams {
    idempotencyKey: string;
}
export interface PaymentsProviderAccountsUpdateParams {
    idempotencyKey: string;
}
export interface PaymentsProviderAccountsTestParams {
    idempotencyKey: string;
}
export declare class PaymentsProviderAccountsApi {
    private client;
    readonly credentials: PaymentsProviderAccountsCredentialsApi;
    constructor(client: HttpClient);
    /** Provider accounts list. */
    list(params?: PaymentsProviderAccountsListParams, requestOptions?: ApiRequestOptions): Promise<{
        items: ProviderAccount[];
        pageInfo: PageInfo;
    }>;
    /** Provider account create. */
    create(body: CreateProviderAccountCommand, params: PaymentsProviderAccountsCreateParams, requestOptions?: ApiRequestOptions): Promise<ProviderAccount>;
    /** Provider account update. */
    update(providerAccountId: string, body: UpdateProviderAccountCommand, params: PaymentsProviderAccountsUpdateParams, requestOptions?: ApiRequestOptions): Promise<ProviderAccount>;
    /** Provider account delete. */
    delete(providerAccountId: string, requestOptions?: ApiRequestOptions): Promise<void>;
    /** Provider account credential connectivity test. */
    test(providerAccountId: string, params: PaymentsProviderAccountsTestParams, body?: ProviderAccountTestCommand, requestOptions?: ApiRequestOptions): Promise<ProviderAccountTestResult>;
}
export interface PaymentsNotifyDomainsListParams {
    page?: number;
    pageSize?: number;
}
export interface PaymentsNotifyDomainsCreateParams {
    idempotencyKey: string;
}
export interface PaymentsNotifyDomainsUpdateParams {
    idempotencyKey: string;
}
export declare class PaymentsNotifyDomainsApi {
    private client;
    constructor(client: HttpClient);
    /** List payment notify domains */
    list(params?: PaymentsNotifyDomainsListParams, requestOptions?: ApiRequestOptions): Promise<{
        items: NotifyDomain[];
        pageInfo: PageInfo;
    }>;
    /** Create payment notify domain */
    create(body: NotifyDomainCreateRequest, params: PaymentsNotifyDomainsCreateParams, requestOptions?: ApiRequestOptions): Promise<NotifyDomain>;
    /** Update payment notify domain */
    update(domainId: string, body: NotifyDomainUpdateRequest, params: PaymentsNotifyDomainsUpdateParams, requestOptions?: ApiRequestOptions): Promise<NotifyDomain>;
    /** Delete payment notify domain */
    delete(domainId: string, requestOptions?: ApiRequestOptions): Promise<void>;
}
export interface PaymentsMethodsListParams {
    page?: number;
    pageSize?: number;
    sort?: string;
    q?: string;
    status?: 'active' | 'inactive' | 'deprecated';
}
export interface PaymentsMethodsCreateParams {
    idempotencyKey: string;
}
export interface PaymentsMethodsUpdateParams {
    idempotencyKey: string;
}
export declare class PaymentsMethodsApi {
    private client;
    constructor(client: HttpClient);
    /** Payment methods list. */
    list(params?: PaymentsMethodsListParams, requestOptions?: ApiRequestOptions): Promise<{
        items: PaymentMethod[];
        pageInfo: PageInfo;
    }>;
    /** Payment method create. */
    create(body: CreatePaymentMethodCommand, params: PaymentsMethodsCreateParams, requestOptions?: ApiRequestOptions): Promise<PaymentMethod>;
    /** Payment method update. */
    update(methodKey: string, body: UpdatePaymentMethodCommand, params: PaymentsMethodsUpdateParams, requestOptions?: ApiRequestOptions): Promise<PaymentMethod>;
}
export interface PaymentsRefundsListParams {
    page?: number;
    pageSize?: number;
    q?: string;
    status?: 'submitted' | 'processing' | 'succeeded' | 'failed' | 'closed';
    orderId?: string;
    paymentIntentId?: string;
}
export interface PaymentsRefundsCreateParams {
    idempotencyKey: string;
}
export interface PaymentsRefundsRetryParams {
    idempotencyKey: string;
}
export declare class PaymentsRefundsApi {
    private client;
    constructor(client: HttpClient);
    /** Refunds list. */
    list(params?: PaymentsRefundsListParams, requestOptions?: ApiRequestOptions): Promise<{
        items: Refund[];
        pageInfo: PageInfo;
    }>;
    /** Refund create. */
    create(body: CreateRefundCommand, params: PaymentsRefundsCreateParams, requestOptions?: ApiRequestOptions): Promise<Refund>;
    /** Refund retrieve. */
    retrieve(refundId: string, requestOptions?: ApiRequestOptions): Promise<Refund>;
    /** Refund provider submission retry. */
    retry(refundId: string, body: RetryRefundCommand, params: PaymentsRefundsRetryParams, requestOptions?: ApiRequestOptions): Promise<SdkWorkCommandData>;
}
export interface PaymentsIntentsListParams {
    page?: number;
    pageSize?: number;
    sort?: string;
    q?: string;
    status?: string;
    ownerUserId?: string;
    orderId?: string;
}
export declare class PaymentsIntentsApi {
    private client;
    constructor(client: HttpClient);
    /** Payment intents list. */
    list(params?: PaymentsIntentsListParams, requestOptions?: ApiRequestOptions): Promise<{
        items: PaymentIntent[];
        pageInfo: PageInfo;
    }>;
    /** Payment intent retrieve. */
    retrieve(paymentIntentId: string, requestOptions?: ApiRequestOptions): Promise<PaymentIntent>;
}
export declare class PaymentsApi {
    readonly intents: PaymentsIntentsApi;
    readonly refunds: PaymentsRefundsApi;
    readonly methods: PaymentsMethodsApi;
    readonly notifyDomains: PaymentsNotifyDomainsApi;
    readonly providerAccounts: PaymentsProviderAccountsApi;
    readonly channels: PaymentsChannelsApi;
    readonly routeRules: PaymentsRouteRulesApi;
    readonly subMerchants: PaymentsSubMerchantsApi;
    readonly certificates: PaymentsCertificatesApi;
    readonly attempts: PaymentsAttemptsApi;
    readonly webhookEvents: PaymentsWebhookEventsApi;
    readonly reconciliationRuns: PaymentsReconciliationRunsApi;
    readonly dev: PaymentsDevApi;
    constructor(client: HttpClient);
}
export declare function createPaymentsApi(client: HttpClient): PaymentsApi;
//# sourceMappingURL=payments.d.ts.map