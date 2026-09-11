import type { ApiRequestOptions, HttpClient } from '../http/client';
import type { CreatePaymentCommand, CreatePaymentIntentCommand, CreateRefundCommand, PageInfo, Payment, PaymentAttempt, PaymentIntent, PaymentMethod, PaymentRecord, PaymentStatistics, ReconcilePaymentCommand, Refund, SdkWorkCommandData } from '../types';
export interface CommerceRefundsListParams {
    page?: number;
    pageSize?: number;
    status?: string;
}
export declare class CommerceRefundsApi {
    private client;
    constructor(client: HttpClient);
    /** List refunds. */
    list(params?: CommerceRefundsListParams, requestOptions?: ApiRequestOptions): Promise<{
        items: Refund[];
        pageInfo: PageInfo;
    }>;
    /** Create a refund. */
    create(body: CreateRefundCommand, requestOptions?: ApiRequestOptions): Promise<Refund>;
    /** Retrieve a refund. */
    retrieve(refundId: string, requestOptions?: ApiRequestOptions): Promise<Refund>;
}
export declare class CommercePaymentsStatusOutTradeNoApi {
    private client;
    constructor(client: HttpClient);
    /** Retrieve payment status by provider trade number. */
    retrieve(outTradeNo: string, requestOptions?: ApiRequestOptions): Promise<PaymentRecord>;
}
export declare class CommercePaymentsStatusApi {
    private client;
    readonly outTradeNo: CommercePaymentsStatusOutTradeNoApi;
    constructor(client: HttpClient);
    /** Retrieve payment status. */
    retrieve(paymentId: string, requestOptions?: ApiRequestOptions): Promise<PaymentRecord>;
}
export declare class CommercePaymentsCheckoutApi {
    private client;
    constructor(client: HttpClient);
    /** Retrieve payment checkout data. */
    retrieve(paymentId: string, requestOptions?: ApiRequestOptions): Promise<Payment>;
}
export declare class CommercePaymentsStatisticsSummaryApi {
    private client;
    constructor(client: HttpClient);
    /** Retrieve the payment statistics summary. */
    retrieve(requestOptions?: ApiRequestOptions): Promise<PaymentStatistics>;
}
export declare class CommercePaymentsStatisticsApi {
    readonly summary: CommercePaymentsStatisticsSummaryApi;
    constructor(client: HttpClient);
}
export declare class CommercePaymentsAttemptsApi {
    private client;
    constructor(client: HttpClient);
    /** Retrieve a payment attempt. */
    retrieve(paymentAttemptId: string, requestOptions?: ApiRequestOptions): Promise<PaymentAttempt>;
}
export interface CommercePaymentsRecordsListParams {
    page?: number;
    pageSize?: number;
    orderId?: string;
}
export declare class CommercePaymentsRecordsApi {
    private client;
    constructor(client: HttpClient);
    /** List payment records. */
    list(params?: CommercePaymentsRecordsListParams, requestOptions?: ApiRequestOptions): Promise<{
        items: PaymentRecord[];
        pageInfo: PageInfo;
    }>;
    /** Retrieve a payment record. */
    retrieve(paymentId: string, requestOptions?: ApiRequestOptions): Promise<PaymentRecord>;
}
export interface CommercePaymentsMethodsListParams {
    page?: number;
    pageSize?: number;
    clientType?: string;
}
export declare class CommercePaymentsMethodsApi {
    private client;
    constructor(client: HttpClient);
    /** List available payment methods. */
    list(params?: CommercePaymentsMethodsListParams, requestOptions?: ApiRequestOptions): Promise<{
        items: PaymentMethod[];
        pageInfo: PageInfo;
    }>;
}
export declare class CommercePaymentsIntentsAttemptsApi {
    private client;
    constructor(client: HttpClient);
    /** Create a payment attempt. */
    create(paymentIntentId: string, requestOptions?: ApiRequestOptions): Promise<PaymentAttempt>;
}
export declare class CommercePaymentsIntentsApi {
    private client;
    readonly attempts: CommercePaymentsIntentsAttemptsApi;
    constructor(client: HttpClient);
    /** Create a payment intent. */
    create(body: CreatePaymentIntentCommand, requestOptions?: ApiRequestOptions): Promise<PaymentIntent>;
    /** Retrieve a payment intent. */
    retrieve(paymentIntentId: string, requestOptions?: ApiRequestOptions): Promise<PaymentIntent>;
    /** Cancel a payment intent. */
    cancel(paymentIntentId: string, requestOptions?: ApiRequestOptions): Promise<SdkWorkCommandData>;
}
export declare class CommercePaymentsApi {
    private client;
    readonly intents: CommercePaymentsIntentsApi;
    readonly methods: CommercePaymentsMethodsApi;
    readonly records: CommercePaymentsRecordsApi;
    readonly attempts: CommercePaymentsAttemptsApi;
    readonly statistics: CommercePaymentsStatisticsApi;
    readonly checkout: CommercePaymentsCheckoutApi;
    readonly status: CommercePaymentsStatusApi;
    constructor(client: HttpClient);
    /** Create a payment. */
    create(body: CreatePaymentCommand, requestOptions?: ApiRequestOptions): Promise<Payment>;
    /** Resolve the latest local payment record. */
    reconcile(body: ReconcilePaymentCommand, requestOptions?: ApiRequestOptions): Promise<PaymentRecord>;
    /** Close a payment. */
    close(paymentId: string, requestOptions?: ApiRequestOptions): Promise<SdkWorkCommandData>;
}
export declare class CommerceApi {
    readonly payments: CommercePaymentsApi;
    readonly refunds: CommerceRefundsApi;
    constructor(client: HttpClient);
}
export declare function createCommerceApi(client: HttpClient): CommerceApi;
//# sourceMappingURL=commerce.d.ts.map