/**
 * Payment admin monitor types.
 *
 * Mirrors the backend API OpenAPI schemas under
 * `/backend/v3/api/payments/{intents,attempts,webhook_events,reconciliation_runs}`.
 * Field names follow camelCase wire form (the backend SDK unwraps `data` and
 * keeps payload shape as-is). Every list resource has its own readonly filter
 * interface; mutations (webhook replay, reconciliation run creation) have
 * their own draft and result types.
 *
 * NOTE: These view types are intentionally scoped to this package. They are
 * projections of `unknown` SDK payloads through defensive mappers in
 * `monitor-admin-controller.ts` — consumers must NOT import the raw SDK
 * transport types directly.
 */

// ---------------------------------------------------------------------------
// Shared enums (mirrors backend OpenAPI schemas)
// ---------------------------------------------------------------------------

export type PaymentProviderCode = "stripe" | "alipay" | "wechat_pay" | "paypal" | "sandbox";

/**
 * Payment status per backend OpenAPI `PaymentIntent.status` /
 * `PaymentAttempt.status`. The OpenAPI enum lists 7 values; `refunding` and
 * `refunded` are domain-layer wire states that may appear in payloads but are
 * not part of the list filter schema. They are accepted here for display only.
 */
export type PaymentStatus =
  | "created"
  | "pending"
  | "processing"
  | "succeeded"
  | "failed"
  | "canceled"
  | "closed"
  | "refunding"
  | "refunded";

export type WebhookEventStatus = "queued" | "processing" | "processed" | "failed" | "dead";

/**
 * Webhook signature verification status.
 *
 * Mirrors how industry PSPs surface signature verification outcomes:
 *   - Stripe Dashboard: "Signature verified" / "Signature verification failed"
 *   - Alipay/WeChat merchant platforms: signature check pass/fail
 *
 * The backend may return this in the webhook event payload; when absent the
 * UI treats it as "unverified" (not yet checked).
 */
export type WebhookSignatureStatus = "valid" | "invalid" | "unverified" | "unknown";

export type ReconciliationRunStatus =
  | "pending"
  | "queued"
  | "running"
  | "succeeded"
  | "failed"
  | "canceled";

export type RefundStatus = "submitted" | "processing" | "succeeded" | "failed" | "closed";

export type RefundReasonCode =
  | "customer_request"
  | "duplicate"
  | "fraud"
  | "service_failure"
  | "other";

export type ReconciliationType = "daily" | "weekly" | "monthly" | "manual" | "settlement";

export const PAYMENT_STATUS_VALUES: readonly PaymentStatus[] = [
  "created",
  "pending",
  "processing",
  "succeeded",
  "failed",
  "canceled",
  "closed",
  "refunding",
  "refunded",
] as const;

export const WEBHOOK_EVENT_STATUS_VALUES: readonly WebhookEventStatus[] = [
  "queued",
  "processing",
  "processed",
  "failed",
  "dead",
] as const;

export const WEBHOOK_SIGNATURE_STATUS_VALUES: readonly WebhookSignatureStatus[] = [
  "valid",
  "invalid",
  "unverified",
  "unknown",
] as const;

export const RECONCILIATION_RUN_STATUS_VALUES: readonly ReconciliationRunStatus[] = [
  "pending",
  "queued",
  "running",
  "succeeded",
  "failed",
  "canceled",
] as const;

export const RECONCILIATION_TYPE_VALUES: readonly ReconciliationType[] = [
  "daily",
  "weekly",
  "monthly",
  "manual",
  "settlement",
] as const;

export const REFUND_STATUS_VALUES: readonly RefundStatus[] = [
  "submitted",
  "processing",
  "succeeded",
  "failed",
  "closed",
] as const;

export const REFUND_REASON_VALUES: readonly RefundReasonCode[] = [
  "customer_request",
  "duplicate",
  "fraud",
  "service_failure",
  "other",
] as const;

export const PROVIDER_CODES: readonly PaymentProviderCode[] = [
  "stripe",
  "alipay",
  "wechat_pay",
  "sandbox",
] as const;

// ---------------------------------------------------------------------------
// List filters (readonly — passed to `sessions.<resource>.list({...filter})`)
// ---------------------------------------------------------------------------

export interface PaymentIntentListFilter {
  readonly status?: PaymentStatus;
  readonly ownerUserId?: string;
  readonly orderId?: string;
  readonly providerCode?: PaymentProviderCode;
  readonly currencyCode?: string;
  readonly createdAtFrom?: string;
  readonly createdAtTo?: string;
  readonly q?: string;
}

export interface PaymentAttemptListFilter {
  readonly status?: PaymentStatus;
  readonly providerCode?: PaymentProviderCode;
  readonly paymentIntentId?: string;
  readonly q?: string;
}

export interface PaymentWebhookEventListFilter {
  readonly status?: WebhookEventStatus;
  readonly providerCode?: PaymentProviderCode;
  readonly eventType?: string;
  readonly q?: string;
}

export interface ReconciliationRunListFilter {
  readonly status?: ReconciliationRunStatus;
  readonly providerCode?: PaymentProviderCode;
  readonly providerAccountId?: string;
  readonly q?: string;
}

export interface RefundListFilter {
  readonly status?: RefundStatus;
  readonly orderId?: string;
  readonly paymentIntentId?: string;
  readonly q?: string;
}

// ---------------------------------------------------------------------------
// Views (mirror backend OpenAPI component schemas)
// ---------------------------------------------------------------------------

export interface PaymentIntentView {
  readonly id: string;
  readonly paymentIntentNo: string;
  readonly orderId: string;
  readonly ownerUserId: string;
  readonly paymentMethod: string;
  readonly providerCode: string;
  readonly amount: string;
  readonly currencyCode: string;
  readonly status: PaymentStatus;
  readonly createdAt: string;
  readonly updatedAt: string;
  /** Attempt summaries. List endpoints typically do not return attempts, but if the backend includes them in the list payload, the count is displayed. */
  readonly attempts?: readonly PaymentAttemptView[];
}

export interface PaymentAttemptView {
  readonly id: string;
  readonly paymentIntentId: string;
  readonly attemptNo: string;
  readonly providerCode: PaymentProviderCode;
  readonly channelId: string;
  readonly amount: string;
  readonly currencyCode: string;
  readonly status: PaymentStatus;
  readonly providerTransactionId?: string;
  readonly outTradeNo?: string;
  readonly paidAt?: string;
  readonly createdAt: string;
}

export interface PaymentWebhookEventView {
  readonly id: string;
  readonly eventId?: string;
  readonly providerCode: PaymentProviderCode;
  readonly eventType: string;
  readonly status: WebhookEventStatus;
  readonly retries: number;
  readonly lastError?: string;
  readonly receivedAt: string;
  readonly processedAt?: string;
  /**
   * Signature verification status. The backend OpenAPI `WebhookEvent` uses
   * `additionalProperties: true` and may return a `signatureStatus` field;
   * when absent, the UI treats it as "unverified".
   * Mirrors Stripe Dashboard webhook event "Signature" indicator.
   */
  readonly signatureStatus?: WebhookSignatureStatus;
  /**
   * Raw webhook request body (JSON-parsed object). Used by the detail Drawer's
   * payload viewer. The backend may return `payload` or `requestBody` (camelCase
   * / snake_case compatible).
   */
  readonly payload?: Readonly<Record<string, unknown>>;
  /**
   * Webhook request headers (key-value pairs). Used by the detail Drawer to
   * display transport-layer metadata.
   */
  readonly headers?: Readonly<Record<string, string>>;
}

export interface ReconciliationRunView {
  readonly id: string;
  readonly runNo: string;
  readonly providerCode: PaymentProviderCode;
  readonly providerAccountId: string;
  readonly reconciliationType: ReconciliationType;
  readonly periodStart: string;
  readonly periodEnd: string;
  readonly status: ReconciliationRunStatus;
  readonly matchedCount: number;
  readonly mismatchedCount: number;
  readonly unmatchedCount: number;
  readonly totalDifferenceAmount: string;
  readonly currencyCode: string;
  readonly createdAt: string;
}

export interface RefundView {
  readonly id: string;
  readonly refundNo: string;
  readonly orderId: string;
  readonly paymentIntentId: string;
  readonly paymentAttemptId: string;
  readonly providerCode: PaymentProviderCode;
  readonly providerAccountId?: string;
  readonly amount: string;
  readonly currencyCode: string;
  readonly status: RefundStatus;
  readonly reasonCode?: RefundReasonCode;
  readonly requestedByType: "buyer" | "operator" | "system";
  readonly requestedBy?: string;
  readonly createdAt: string;
  readonly updatedAt: string;
}

/**
 * Provider account dropdown option structure.
 *
 * Used as the data source for the providerAccountId dropdown in the
 * reconciliation run creation form. Injected by the caller (e.g., from
 * controller state) to avoid coupling the form directly to the backend API.
 */
export interface ReconciliationProviderAccountOption {
  readonly id: string;
  readonly accountNo: string;
  readonly providerCode: PaymentProviderCode;
}

// ---------------------------------------------------------------------------
// Detail retrieval (intents.retrieve)
// ---------------------------------------------------------------------------

/**
 * Detailed intent payload — extends the list view with attempt summaries and
 * raw metadata. The backend `intents.retrieve` operation may return a richer
 * shape than `intents.list`; we model the superset conservatively.
 */
export interface PaymentIntentDetail extends PaymentIntentView {
  readonly attempts?: readonly PaymentAttemptView[];
  readonly metadata?: Record<string, unknown>;
}

// ---------------------------------------------------------------------------
// Mutation drafts & results
// ---------------------------------------------------------------------------

export interface CreateReconciliationRunDraft {
  readonly providerCode: PaymentProviderCode;
  readonly providerAccountId: string;
  readonly reconciliationType: ReconciliationType;
  readonly periodStart: string;
  readonly periodEnd: string;
  readonly currencyCode: string;
}

export interface CreateRefundDraft {
  readonly paymentIntentId: string;
  readonly amount?: string;
  readonly reasonCode: RefundReasonCode;
  readonly confirmPaymentIntentNo: string;
}

export interface WebhookReplayResult {
  readonly ok: boolean;
  readonly eventId: string;
  readonly replayedAt: string;
  readonly diagnostic?: string;
}

// ---------------------------------------------------------------------------
// Admin controller state & interface
// ---------------------------------------------------------------------------

export type PaymentMonitorAdminStatus = "idle" | "loading" | "ready" | "saving" | "error";

export interface PaymentMonitorAdminState {
  readonly status: PaymentMonitorAdminStatus;
  readonly lastError?: string;
  readonly intents: readonly PaymentIntentView[];
  readonly attempts: readonly PaymentAttemptView[];
  readonly webhookEvents: readonly PaymentWebhookEventView[];
  readonly reconciliationRuns: readonly ReconciliationRunView[];
  readonly refunds: readonly RefundView[];
  readonly listPageInfo?: Partial<{
    intents: import("@sdkwork/payment-contracts").SdkWorkPageInfo;
    attempts: import("@sdkwork/payment-contracts").SdkWorkPageInfo;
    webhookEvents: import("@sdkwork/payment-contracts").SdkWorkPageInfo;
    reconciliationRuns: import("@sdkwork/payment-contracts").SdkWorkPageInfo;
    refunds: import("@sdkwork/payment-contracts").SdkWorkPageInfo;
  }>;
  readonly selectedIntentId?: string;
  readonly selectedIntentDetail?: PaymentIntentDetail;
  readonly lastReplayResult?: WebhookReplayResult;
  readonly lastReconciliationRunId?: string;
  readonly lastRefundId?: string;
  // Provider account dropdown data source for reconciliation run creation form; optional, injected by caller as needed
  readonly providerAccounts?: readonly ReconciliationProviderAccountOption[];
}

export interface CreatePaymentMonitorAdminControllerInput {
  readonly service: import("@sdkwork/payment-service").SdkworkPaymentBackendService;
}

export interface PaymentMonitorAdminController {
  getState(): PaymentMonitorAdminState;
  subscribe(listener: () => void): () => void;
  load(): Promise<PaymentMonitorAdminState>;
  refreshIntents(): Promise<readonly PaymentIntentView[]>;
  loadMoreIntents(): Promise<readonly PaymentIntentView[]>;
  loadMoreAttempts(): Promise<readonly PaymentAttemptView[]>;
  loadMoreWebhookEvents(): Promise<readonly PaymentWebhookEventView[]>;
  loadMoreReconciliationRuns(): Promise<readonly ReconciliationRunView[]>;
  loadMoreRefunds(): Promise<readonly RefundView[]>;
  applyIntentFilter(filter: PaymentIntentListFilter): Promise<readonly PaymentIntentView[]>;
  applyAttemptFilter(filter: PaymentAttemptListFilter): Promise<readonly PaymentAttemptView[]>;
  applyWebhookEventFilter(
    filter: PaymentWebhookEventListFilter,
  ): Promise<readonly PaymentWebhookEventView[]>;
  applyReconciliationRunFilter(
    filter: ReconciliationRunListFilter,
  ): Promise<readonly ReconciliationRunView[]>;
  applyRefundFilter(filter: RefundListFilter): Promise<readonly RefundView[]>;
  selectIntent(id?: string): Promise<PaymentIntentDetail | undefined>;
  replayWebhookEvent(eventId: string): Promise<WebhookReplayResult>;
  createReconciliationRun(draft: CreateReconciliationRunDraft): Promise<ReconciliationRunView>;
  createRefund(draft: CreateRefundDraft): Promise<RefundView>;
  retryRefund(refundId: string, confirmRefundNo: string): Promise<void>;
}
