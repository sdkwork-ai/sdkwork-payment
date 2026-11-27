/**
 * Dev config admin type definitions.
 *
 * Mirrors backend OpenAPI schemas in
 * apis/backend-api/payment/sdkwork-payment-backend-api.openapi.yaml:
 *   - Certificate (PEM certificate reference + expiry metadata)
 *   - WebhookEvent (stored PSP webhook events for replay + integration logs)
 *   - DevSandboxTrigger command (simulate a PSP webhook event)
 *   - DevWebhookSignatureTest command (verify a webhook signature)
 *   - ProviderAccountTestResult (credential test result)
 *
 * Dev config functionality is spread across multiple backend namespaces:
 *   - service.backend.providerAccounts.test / .credentials.rotate / .update
 *   - service.backend.certificates.list / .create / .retrieve / .delete
 *   - service.backend.webhookEvents.list / .replay
 *   - service.backend.dev.sandboxTrigger / .webhookSignatureTest
 *
 * All remote payloads are typed as `unknown` at the SDK boundary; these types are
 * controller-side projections consumed by React. Field names mirror the wire contract.
 *
 * The provider account + test result types are duplicated from
 * `@sdkwork/payment-pc-admin-provider` intentionally to keep this package
 * self-contained (no cross-admin-package dependency); both copies mirror the
 * same OpenAPI `ProviderAccount` / `ProviderAccountTestResult` schemas.
 */

import type { SdkWorkPageInfo } from "@sdkwork/payment-contracts";
import type { SdkworkPaymentBackendService } from "@sdkwork/payment-service";

export type PaymentProviderCode = "stripe" | "alipay" | "wechat_pay" | "paypal" | "sandbox";

export type PaymentProviderEnvironment = "development" | "sandbox" | "production";

export type PaymentProviderAccountStatus =
  | "active"
  | "inactive"
  | "suspended"
  | "deprecated";

export type PaymentLastTestStatus = "success" | "failure" | "unknown";

export interface PaymentProviderAccountView {
  readonly id: string;
  readonly accountNo: string;
  readonly providerCode: PaymentProviderCode;
  readonly merchantId?: string;
  readonly accountMode: "direct" | "partner";
  readonly partnerProviderAccountId?: string;
  readonly environment: PaymentProviderEnvironment;
  readonly countryCode?: string;
  readonly settlementCurrency: string;
  readonly hasPrimarySecret: boolean;
  readonly hasWebhookSecret: boolean;
  readonly hasCertificate: boolean;
  readonly credentialStorage: "database_encrypted" | "legacy_reference" | "none";
  readonly status: PaymentProviderAccountStatus;
  readonly metadata: Record<string, unknown>;
  readonly certificateExpiresAt?: string;
  readonly lastTestedAt?: string;
  readonly lastTestStatus?: PaymentLastTestStatus;
  readonly createdAt: string;
  readonly updatedAt: string;
}

export interface PaymentProviderAccountTestResult {
  readonly ok: boolean;
  readonly providerCode: PaymentProviderCode;
  readonly environment: PaymentProviderEnvironment;
  readonly pspResponseCode?: string;
  readonly pspResponseTimeMs?: number;
  readonly diagnostic?: string;
  readonly testedAt: string;
}

/**
 * Certificate kind, mirroring `certificateType` query param on
 * `GET /backend/v3/api/payments/certificates`.
 */
export type PaymentCertificateKind =
  | "merchant_private_key"
  | "provider_public_key"
  | "platform_certificate"
  | "webhook_secret";

export type PaymentCertificateStatus = "active" | "expired" | "revoked" | "pending_rotation";

/**
 * Read-only certificate view (mirrors OpenAPI `Certificate` schema).
 *
 * PEM content is write-only and encrypted before database persistence.
 */
export interface PaymentCertificateView {
  readonly id: string;
  readonly certificateNo: string;
  readonly providerCode?: PaymentProviderCode;
  readonly certificateType: PaymentCertificateKind;
  readonly hasContent: boolean;
  readonly credentialStorage: "database_encrypted" | "legacy_reference";
  readonly subject?: string;
  readonly issuer?: string;
  readonly fingerprint?: string;
  readonly expiresAt?: string;
  readonly status: PaymentCertificateStatus;
  readonly metadata: Record<string, unknown>;
  readonly createdAt: string;
  readonly updatedAt: string;
}

/**
 * Create certificate command (mirrors OpenAPI `CreateCertificateCommand`).
 *
 * Certificate content is never returned by the backend.
 */
export interface PaymentCertificateDraft {
  readonly certificateNo: string;
  readonly providerCode?: PaymentProviderCode;
  readonly certificateType: PaymentCertificateKind;
  readonly certificate: string;
  readonly metadata?: Record<string, unknown>;
}

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
export type PaymentWebhookSignatureStatus =
  | "valid"
  | "invalid"
  | "unverified"
  | "unknown";

/**
 * Stored webhook event (mirrors OpenAPI `WebhookEvent` schema).
 *
 * The `Integration Logs` tab renders these in chronological order with replay
 * capability. `Webhook Debugger` tab focuses on sandbox trigger + signature
 * test but also lists recent events for context.
 *
 * Status lifecycle: `queued` → `processing` → `processed` (success) or
 * `failed` (transient) → `dead` (exhausted retries).
 *
 * `signatureStatus` / `payload` / `headers` are extension fields allowed by
 * the OpenAPI `WebhookEvent` schema (`additionalProperties: true`). The backend
 * may return them for detail views; absent values default to `unverified` /
 * `undefined`.
 */
export interface PaymentWebhookEventView {
  readonly id: string;
  readonly providerCode: PaymentProviderCode;
  readonly eventId?: string;
  readonly eventType: string;
  readonly receivedAt: string;
  readonly processedAt?: string;
  readonly status: "queued" | "processing" | "processed" | "failed" | "dead";
  readonly retries: number;
  readonly lastError?: string;
  readonly signatureStatus?: PaymentWebhookSignatureStatus;
  readonly payload?: Readonly<Record<string, unknown>>;
  readonly headers?: Readonly<Record<string, string>>;
}

export interface PaymentWebhookEventListFilter {
  readonly providerCode?: PaymentProviderCode;
  readonly status?: PaymentWebhookEventView["status"];
  readonly receivedFrom?: string;
  readonly receivedTo?: string;
}

/**
 * Sandbox trigger command (mirrors OpenAPI `SandboxTriggerCommand`).
 *
 * Only allowed when the target provider account's environment is
 * `development` or `sandbox`. Returns 202 async with an `operationId`.
 *
 * `amount` / `currencyCode` / `outTradeNo` override the default sandbox payload
 * template; when omitted the backend uses a provider-specific default.
 */
export interface PaymentDevSandboxTriggerDraft {
  readonly providerAccountId: string;
  readonly eventType: string;
  readonly amount?: string;
  readonly currencyCode?: string;
  readonly outTradeNo?: string;
}

export interface PaymentDevSandboxTriggerResult {
  readonly ok: boolean;
  readonly operationId?: string;
  readonly status?: string;
  readonly pollUrl?: string;
  readonly providerCode: PaymentProviderCode;
  readonly environment: PaymentProviderEnvironment;
  readonly diagnostic?: string;
  readonly triggeredAt: string;
}

/**
 * Webhook signature test command (mirrors OpenAPI `WebhookSignatureTestCommand`).
 *
 * Verifies a raw payload + signature against the configured
 * `webhook_secret_ref` of the target provider account.
 *
 * - `payload`: raw webhook request body (the content that was signed)
 * - `signature`: signature header value (e.g., stripe-signature value)
 * - `timestamp`: timestamp header for replay protection (e.g., stripe `t=`)
 * - `signatureHeader`: override for non-standard signature header names
 */
export interface PaymentDevWebhookSignatureTestDraft {
  readonly providerAccountId: string;
  readonly payload: string;
  readonly signature: string;
  readonly timestamp?: string;
  readonly signatureHeader?: string;
}

export interface PaymentDevWebhookSignatureTestResult {
  readonly ok: boolean;
  readonly providerCode: PaymentProviderCode;
  readonly algorithm?: string;
  readonly diagnostic?: string;
  readonly testedAt: string;
}

/**
 * Webhook event replay result.
 */
export interface PaymentWebhookReplayResult {
  readonly ok: boolean;
  readonly eventId: string;
  readonly replayedAt: string;
  readonly diagnostic?: string;
}

export type PaymentDevConfigStatus =
  | "idle"
  | "loading"
  | "ready"
  | "saving"
  | "testing"
  | "error";

export type PaymentDevConfigAdminSection =
  | "testpayments"
  | "environment"
  | "webhook"
  | "certificates"
  | "logs";

// ---------------------------------------------------------------------------
// One-cent test console
// ---------------------------------------------------------------------------

/** Payer identity a product addresses instead of "whoever scans this". */
export interface PaymentTestPayerField {
  readonly methodKey: string;
  readonly field: "openid" | "client_ip" | "buyer_id" | string;
  readonly label: string;
}

export type PaymentTestLaunchKind =
  | "qr_code"
  | "redirect"
  | "form_redirect"
  | "card"
  | "jsapi"
  | "app"
  | "local_sandbox"
  | "unsupported";

/** One row of the service-provider × product coverage matrix. */
export interface PaymentTestProductView {
  readonly providerCode: PaymentProviderCode;
  readonly providerLabel: string;
  readonly methodKey: string;
  readonly methodStatus: "active" | "inactive" | "missing" | string;
  readonly launchKind: PaymentTestLaunchKind;
  readonly settleSupported: boolean;
  readonly refundSupported: boolean;
  readonly testEnabled: boolean;
  readonly requiredPayerFields: readonly PaymentTestPayerField[];
  readonly activeProviderAccounts: number;
  readonly activeChannelCount: number;
  readonly readiness: readonly string[];
  readonly ready: boolean;
}

export interface PaymentTestProductsView {
  readonly products: readonly PaymentTestProductView[];
  readonly unconfiguredProviders: readonly string[];
  readonly readyCount: number;
  readonly totalCount: number;
  readonly checkedAt: string;
}

/** Result of creating one one-cent test payment. */
export interface PaymentTestPaymentView {
  readonly paymentId: string;
  readonly paymentIntentId: string;
  readonly paymentIntentNo?: string;
  readonly attemptId: string;
  readonly outTradeNo: string;
  readonly methodKey: string;
  readonly providerCode: string;
  readonly amount: string;
  readonly currencyCode: string;
  readonly status: string;
  readonly qrCodeUrl?: string;
  readonly cashierUrl?: string;
  readonly payUrl?: string;
  readonly payForm?: string;
  readonly clientSecret?: string;
  readonly publishableKey?: string;
  readonly launchKind: PaymentTestLaunchKind;
  readonly launchPayload?: string;
  readonly nextAction?: string;
  readonly settleSupported: boolean;
  readonly providerParamKeys: readonly string[];
  readonly requiredPayerFields: readonly PaymentTestPayerField[];
  readonly expiresAt?: string;
  readonly createdAt: string;
}

export interface PaymentTestPaymentDraft {
  readonly methodKey: string;
  readonly amount?: string;
  readonly currencyCode?: string;
  readonly payerIdentity?: {
    readonly openid?: string;
    readonly clientIp?: string;
    readonly buyerId?: string;
    readonly buyerOpenId?: string;
  };
}

/** Outcome of querying / settling / cancelling a test payment. */
export interface PaymentTestPaymentActionView {
  readonly paymentIntentId: string;
  readonly attemptId: string;
  readonly outTradeNo?: string;
  readonly previousStatus?: string;
  readonly localStatus: string;
  readonly providerCode: string;
  readonly appliedStatus?: string;
  readonly webhookEventId?: string;
  readonly eventId?: string;
  /** Present on `checkAttemptStatus`. */
  readonly providerStatus?: string;
  readonly paid?: boolean;
}

/** Outcome of the refund entry for a settled test payment. */
export interface PaymentTestRefundView {
  readonly paymentIntentId: string;
  readonly attemptId: string;
  readonly refundId: string;
  readonly refundNo: string;
  readonly amount: string;
  readonly currencyCode: string;
  readonly status: string;
  readonly providerCode: string;
  readonly refunded: boolean;
  readonly nextStep: string;
}

export type PaymentTestSigner = "hmac_sha256" | "wechat_pay_rsa_sha256" | "alipay_rsa_sha256" | "none";

export interface PaymentTestNotificationDraft {
  readonly providerAccountId: string;
  readonly payload: string;
  readonly headers?: Readonly<Record<string, string>>;
  readonly signer?: PaymentTestSigner;
}

/** Outcome of the self-signed notification delivery test. */
export interface PaymentTestNotificationView {
  readonly providerAccountId: string;
  readonly providerCode: string;
  readonly verification: "verified" | "rejected";
  readonly accepted: boolean;
  readonly nativeSignature: string;
  readonly documentSignature: string;
  readonly signatureHeader?: string;
  readonly providerEventId?: string;
  readonly outTradeNo?: string;
  readonly webhookEventId?: string;
  readonly appliedStatus?: string;
  readonly paymentAttemptId?: string;
  readonly diagnostic?: string;
  readonly deliveredAt: string;
}

/** Last outcome of each console action, kept side by side for one screen. */
export interface PaymentTestConsoleResults {
  readonly products?: PaymentTestProductsView;
  readonly payment?: PaymentTestPaymentView;
  readonly status?: PaymentTestPaymentActionView;
  readonly settled?: PaymentTestPaymentActionView;
  readonly cancelled?: PaymentTestPaymentActionView;
  readonly refund?: PaymentTestRefundView;
  readonly notification?: PaymentTestNotificationView;
}

export interface PaymentDevConfigAdminState {
  /** Lightweight provider account list for environment switcher + credential test. */
  readonly providerAccounts: readonly PaymentProviderAccountView[];
  readonly certificates: readonly PaymentCertificateView[];
  readonly webhookEvents: readonly PaymentWebhookEventView[];
  readonly listPageInfo?: Partial<{
    certificates: SdkWorkPageInfo;
    webhookEvents: SdkWorkPageInfo;
    providerAccounts: SdkWorkPageInfo;
  }>;
  readonly status: PaymentDevConfigStatus;
  readonly lastError?: string;
  readonly lastTestResult?: PaymentProviderAccountTestResult;
  readonly lastSandboxTriggerResult?: PaymentDevSandboxTriggerResult;
  readonly lastSignatureTestResult?: PaymentDevWebhookSignatureTestResult;
  readonly lastReplayResult?: PaymentWebhookReplayResult;
  /** One-cent test console: service-provider × product coverage and outcomes. */
  readonly testConsole: PaymentTestConsoleResults;
  readonly selectedProviderAccountId?: string;
  readonly selectedCertificateId?: string;
}

export interface PaymentDevConfigAdminController {
  getState(): PaymentDevConfigAdminState;
  subscribe(listener: () => void): () => void;
  load(section?: PaymentDevConfigAdminSection): Promise<PaymentDevConfigAdminState>;
  loadMoreCertificates(): Promise<readonly PaymentCertificateView[]>;
  loadMoreWebhookEvents(filter?: PaymentWebhookEventListFilter): Promise<readonly PaymentWebhookEventView[]>;
  loadMoreProviderAccounts(): Promise<readonly PaymentProviderAccountView[]>;
  selectProviderAccount(id?: string): PaymentProviderAccountView | undefined;
  selectCertificate(id?: string): PaymentCertificateView | undefined;
  /**
   * Switches the environment of a provider account. Internally calls
   * `service.backend.providerAccounts.update` with the new environment value.
   * Sandbox/production transitions are gated by the backend per OpenAPI spec.
   */
  switchProviderAccountEnvironment(
    id: string,
    environment: PaymentProviderEnvironment,
  ): Promise<PaymentProviderAccountView>;
  /**
   * Invokes `service.backend.providerAccounts.test` to verify credentials via
   * the lowest-cost PSP API. Updates `last_tested_at` / `last_test_status`
   * on the provider account.
   */
  testProviderAccount(
    id: string,
    options?: { environment?: PaymentProviderEnvironment; dryRun?: boolean },
  ): Promise<PaymentProviderAccountTestResult>;
  createCertificate(draft: PaymentCertificateDraft): Promise<PaymentCertificateView>;
  deleteCertificate(id: string): Promise<void>;
  /**
   * Triggers a simulated PSP webhook event for local/sandbox integration.
   * Only allowed when target provider account environment is development/sandbox.
   */
  triggerSandboxEvent(
    draft: PaymentDevSandboxTriggerDraft,
  ): Promise<PaymentDevSandboxTriggerResult>;
  /**
   * Verifies a webhook signature against the configured webhook_secret_ref.
   */
  testWebhookSignature(
    draft: PaymentDevWebhookSignatureTestDraft,
  ): Promise<PaymentDevWebhookSignatureTestResult>;
  /**
   * Replays a stored webhook event. Capped at
   * `WEBHOOK_STORED_REPLAY_MAX_RETRIES` (5); returns 409 when exceeded.
   */
  replayWebhookEvent(eventId: string): Promise<PaymentWebhookReplayResult>;
  /**
   * Loads the service-provider × product coverage matrix: every product the
   * shipped adapters declare plus the configuration still missing for it.
   */
  loadTestProducts(): Promise<PaymentTestProductsView>;
  /** Creates one one-cent test payment for `methodKey`. */
  createTestPayment(draft: PaymentTestPaymentDraft): Promise<PaymentTestPaymentView>;
  /** Queries the provider for the current state of the test payment. */
  checkTestPaymentStatus(paymentIntentId: string): Promise<PaymentTestPaymentActionView>;
  /** Simulates settlement for sandbox / client-payload products. */
  settleTestPayment(
    paymentIntentId: string,
    outcome?: "success" | "failed",
  ): Promise<PaymentTestPaymentActionView>;
  /** Closes the provider trade and the local attempt. */
  cancelTestPayment(paymentIntentId: string): Promise<PaymentTestPaymentActionView>;
  /** Refunds a settled test payment through the production refund path. */
  refundTestPayment(
    paymentIntentId: string,
    options?: { amount?: string; reasonCode?: string },
  ): Promise<PaymentTestRefundView>;
  /** Delivers a self-signed notification through the real pipeline. */
  testNotification(draft: PaymentTestNotificationDraft): Promise<PaymentTestNotificationView>;
}

export interface CreatePaymentDevConfigAdminControllerInput {
  readonly service: SdkworkPaymentBackendService;
}
