/**
 * Provider admin type definitions.
 *
 * Mirrors backend OpenAPI schemas in
 * apis/backend-api/payment/sdkwork-payment-backend-api.openapi.yaml:
 *   - ProviderAccount (direct + partner/ISV mode)
 *   - CreateProviderAccountCommand / UpdateProviderAccountCommand
 *   - ProviderAccountTestResult
 *   - CredentialRotateCommand
 *   - SubMerchant (Alipay sub_appid / WeChat sub_mch_id / Stripe Connected Account)
 *   - Certificate metadata
 *
 * All remote payloads are typed as `unknown` at the SDK boundary; these types are
 * controller-side projections consumed by React. Field names mirror the wire contract.
 */

import type { SdkWorkPageInfo } from "@sdkwork/payment-contracts";
import type { SdkworkPaymentBackendService } from "@sdkwork/payment-service";

export type PaymentProviderCode = "stripe" | "alipay" | "wechat_pay" | "sandbox";

export type PaymentProviderAccountMode = "direct" | "partner";

export type PaymentProviderEnvironment = "development" | "sandbox" | "production";

export type PaymentProviderAccountStatus =
  | "active"
  | "inactive"
  | "suspended"
  | "deprecated";

export type PaymentLastTestStatus = "success" | "failure" | "unknown";

export type PaymentSubMerchantStatus =
  | "active"
  | "inactive"
  | "suspended"
  | "deprecated";

export type PaymentCertificateKind =
  | "merchant_private_key"
  | "provider_public_key"
  | "platform_certificate"
  | "webhook_secret";

export type PaymentCertificateStatus =
  | "active"
  | "expired"
  | "revoked"
  | "pending_rotation";

export interface PaymentProviderCapabilities {
  readonly pay?: boolean;
  readonly refund?: boolean;
  readonly close?: boolean;
  readonly query?: boolean;
  readonly reconcile?: boolean;
  readonly download?: boolean;
  readonly [key: string]: boolean | undefined;
}

export interface PaymentProviderAccountView {
  readonly id: string;
  readonly accountNo: string;
  readonly providerCode: PaymentProviderCode;
  readonly merchantId?: string;
  readonly accountName?: string;
  readonly accountNameI18n?: Record<string, string>;
  readonly accountMode: PaymentProviderAccountMode;
  readonly partnerProviderAccountId?: string;
  readonly environment: PaymentProviderEnvironment;
  readonly countryCode?: string;
  readonly settlementCurrency: string;
  readonly hasPrimarySecret: boolean;
  readonly hasWebhookSecret: boolean;
  readonly hasCertificate: boolean;
  readonly credentialStorage: "database_encrypted" | "legacy_reference" | "none";
  readonly capabilities: PaymentProviderCapabilities;
  readonly status: PaymentProviderAccountStatus;
  readonly metadata: Record<string, unknown>;
  readonly certificateExpiresAt?: string;
  readonly lastTestedAt?: string;
  readonly lastTestStatus?: PaymentLastTestStatus;
  readonly createdAt: string;
  readonly updatedAt: string;
}

/**
 * Resolve the operator-facing account name for display: the accountName field
 * is shown verbatim (it is the operator-edited business name), falling back to
 * the machine account no. Localized name overrides are intentionally NOT
 * applied — an i18n entry would otherwise shadow names edited in the admin
 * workspace and make the edit appear ineffective. Seeded and operator-edited
 * names both flow through this path so every surface shows the same label.
 */
export function resolveProviderAccountName(
  account: Pick<PaymentProviderAccountView, "accountNo" | "accountName" | "accountNameI18n">,
  _localeTag?: string | null,
): string {
  return account.accountName || account.accountNo;
}

export interface PaymentSubMerchantView {
  readonly id: string;
  readonly providerAccountId: string;
  readonly subMerchantNo: string;
  readonly subMerchantName?: string;
  readonly subAppId?: string;
  readonly subMchId?: string;
  readonly stripeConnectedAccountId?: string;
  readonly providerCode: PaymentProviderCode;
  readonly status: PaymentSubMerchantStatus;
  readonly metadata: Record<string, unknown>;
  readonly createdAt: string;
  readonly updatedAt: string;
}

export interface ProviderAccountCredentialsView {
  readonly providerAccountId: string;
  readonly primarySecret?: string;
  readonly webhookSecret?: string;
  readonly certificate?: string;
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
 * Base-data select option (countries/currencies served by the
 * sdkwork-appbase base-data capability). Shared with the other payment admin
 * capability packages via admin-core; the host app resolves the records and
 * passes the options down, and forms degrade to free-text fields when no
 * options are available.
 */
export type { PaymentBaseDataOption } from "@sdkwork/payment-pc-admin-core";

export interface PaymentProviderAccountDraft {
  readonly accountNo: string;
  readonly providerCode: PaymentProviderCode;
  readonly merchantId: string;
  readonly accountName?: string;
  readonly accountMode: PaymentProviderAccountMode;
  readonly partnerProviderAccountId?: string;
  readonly environment: PaymentProviderEnvironment;
  readonly countryCode: string;
  readonly settlementCurrency: string;
  readonly primarySecret: string;
  readonly webhookSecret?: string;
  readonly certificate?: string;
  readonly capabilities?: PaymentProviderCapabilities;
  readonly status?: PaymentProviderAccountStatus;
  readonly metadata?: Record<string, unknown>;
}

export interface PaymentProviderAccountUpdateDraft {
  readonly merchantId?: string;
  readonly accountName?: string;
  readonly accountMode?: PaymentProviderAccountMode;
  readonly partnerProviderAccountId?: string;
  readonly environment?: PaymentProviderEnvironment;
  readonly countryCode?: string;
  readonly settlementCurrency?: string;
  readonly primarySecret?: string;
  readonly webhookSecret?: string;
  readonly certificate?: string;
  readonly capabilities?: PaymentProviderCapabilities;
  readonly status?: PaymentProviderAccountStatus;
  readonly metadata?: Record<string, unknown>;
}

export interface PaymentSubMerchantDraft {
  readonly providerAccountId: string;
  readonly subMerchantNo: string;
  readonly providerCode: PaymentProviderCode;
  readonly subMerchantName?: string;
  readonly subAppId?: string;
  readonly subMchId?: string;
  readonly stripeConnectedAccountId?: string;
  readonly status?: PaymentSubMerchantStatus;
  readonly metadata?: Record<string, unknown>;
}

export interface PaymentSubMerchantUpdateDraft {
  readonly subMerchantName?: string;
  readonly subAppId?: string;
  readonly subMchId?: string;
  readonly stripeConnectedAccountId?: string;
  readonly status?: PaymentSubMerchantStatus;
  readonly metadata?: Record<string, unknown>;
}

export interface PaymentCredentialRotateDraft {
  readonly primarySecret: string;
  readonly webhookSecret?: string;
  readonly certificate?: string;
  readonly invalidatePrevious?: boolean;
}

export interface PaymentProviderAccountTestOptions {
  readonly environment?: PaymentProviderEnvironment;
  readonly dryRun?: boolean;
}

export interface PaymentProviderAdminResourceSnapshot {
  readonly providerAccounts: readonly PaymentProviderAccountView[];
  readonly subMerchants: readonly PaymentSubMerchantView[];
}

export type PaymentProviderAdminStatus =
  | "idle"
  | "loading"
  | "ready"
  | "saving"
  | "testing"
  | "error";

export interface PaymentProviderAdminState extends PaymentProviderAdminResourceSnapshot {
  readonly listPageInfo?: Partial<Record<keyof PaymentProviderAdminResourceSnapshot, SdkWorkPageInfo>>;
  readonly status: PaymentProviderAdminStatus;
  readonly lastError?: string;
  readonly lastTestResult?: PaymentProviderAccountTestResult;
  readonly lastRotatedAccountId?: string;
  readonly selectedProviderAccount?: PaymentProviderAccountView;
  readonly selectedSubMerchant?: PaymentSubMerchantView;
}

export interface PaymentProviderAdminController {
  getState(): PaymentProviderAdminState;
  subscribe(listener: () => void): () => void;
  load(): Promise<PaymentProviderAdminState>;
  loadMoreProviderAccounts(): Promise<readonly PaymentProviderAccountView[]>;
  loadMoreSubMerchants(providerAccountId?: string): Promise<readonly PaymentSubMerchantView[]>;
  selectProviderAccount(id?: string): PaymentProviderAccountView | undefined;
  selectSubMerchant(id?: string): PaymentSubMerchantView | undefined;
  createProviderAccount(draft: PaymentProviderAccountDraft): Promise<PaymentProviderAccountView>;
  updateProviderAccount(id: string, draft: PaymentProviderAccountUpdateDraft): Promise<PaymentProviderAccountView>;
  deleteProviderAccount(id: string): Promise<void>;
  testProviderAccount(id: string, options?: PaymentProviderAccountTestOptions): Promise<PaymentProviderAccountTestResult>;
  rotateProviderAccountCredentials(id: string, draft: PaymentCredentialRotateDraft): Promise<PaymentProviderAccountView>;
  readProviderAccountCredentials(id: string): Promise<ProviderAccountCredentialsView>;
  createSubMerchant(draft: PaymentSubMerchantDraft): Promise<PaymentSubMerchantView>;
  updateSubMerchant(id: string, draft: PaymentSubMerchantUpdateDraft): Promise<PaymentSubMerchantView>;
  deleteSubMerchant(id: string): Promise<void>;
}

export interface CreatePaymentProviderAdminControllerInput {
  readonly service: SdkworkPaymentBackendService;
}
