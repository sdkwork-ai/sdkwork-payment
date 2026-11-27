/**
 * Channel admin type definitions.
 *
 * Mirrors backend OpenAPI schemas in
 * apis/backend-api/payment/sdkwork-payment-backend-api.openapi.yaml:
 *   - PaymentMethod (1) ──< PaymentChannel >── (1) ProviderAccount
 *   - RouteRule (N) → (1) PaymentChannel
 *
 * API capability matrix (per OpenAPI):
 *   - methods:    list + create + update (PATCH by `methodKey`, no delete)
 *   - channels:   list + create ONLY (no update/delete/retrieve)
 *   - routeRules: list + create + update + delete (no retrieve)
 *
 * All remote payloads are typed as `unknown` at the SDK boundary; these types
 * are controller-side projections consumed by React. Field names mirror the
 * wire contract.
 *
 * The provider account types are duplicated from
 * `@sdkwork/payment-pc-admin-provider` intentionally to keep this package
 * self-contained (no cross-admin-package dependency); both copies mirror the
 * same OpenAPI `ProviderAccount` schema.
 */

import type { SdkWorkPageInfo } from "@sdkwork/payment-contracts";
import type { SdkworkPaymentBackendService } from "@sdkwork/payment-service";

export type PaymentProviderCode = "stripe" | "alipay" | "wechat_pay" | "paypal" | "sandbox";

export type PaymentEntityStatus = "active" | "inactive" | "deprecated";

export type PaymentProviderAccountStatus =
  | "active"
  | "inactive"
  | "suspended"
  | "deprecated";

export type PaymentMethodScope = "global" | "tenant" | "organization";

export type PaymentSceneCode = "app" | "web" | "mini_program" | "api";

/**
 * Lightweight provider account projection for channel linkage.
 * Full provider account management lives in `@sdkwork/payment-pc-admin-provider`.
 */
export interface PaymentProviderAccountView {
  readonly id: string;
  readonly accountNo: string;
  readonly providerCode: PaymentProviderCode;
  readonly merchantId?: string;
  readonly accountMode: "direct" | "partner";
  readonly environment: "development" | "sandbox" | "production";
  readonly countryCode?: string;
  readonly settlementCurrency: string;
  readonly status: PaymentProviderAccountStatus;
}

export interface PaymentMethodView {
  readonly id: string;
  /** Business key — used as path param for PATCH /methods/{methodKey}. */
  readonly methodKey: string;
  readonly displayName: string;
  readonly providerCode: PaymentProviderCode;
  readonly status: PaymentEntityStatus;
  readonly scope: PaymentMethodScope;
  readonly currencyCode: string;
  readonly countryCode?: string;
  readonly sortOrder: number;
  readonly metadata: Record<string, unknown>;
  readonly createdAt: string;
  readonly updatedAt: string;
}

export interface PaymentMethodDraft {
  readonly methodKey: string;
  readonly displayName: string;
  readonly providerCode: PaymentProviderCode;
  readonly status?: PaymentEntityStatus;
  readonly scope?: PaymentMethodScope;
  readonly currencyCode?: string;
  readonly countryCode?: string;
  readonly sortOrder?: number;
  readonly metadata?: Record<string, unknown>;
}

/**
 * Update draft for PATCH /methods/{methodKey}. `methodKey` and `scope` are
 * immutable after creation per OpenAPI `additionalProperties: false`.
 */
export interface PaymentMethodUpdateDraft {
  readonly displayName?: string;
  readonly providerCode?: PaymentProviderCode;
  readonly status?: PaymentEntityStatus;
  readonly currencyCode?: string;
  readonly countryCode?: string;
  readonly sortOrder?: number;
  readonly metadata?: Record<string, unknown>;
}

export interface PaymentChannelView {
  readonly id: string;
  readonly channelNo: string;
  readonly channelName?: string;
  readonly providerAccountId: string;
  readonly methodId: string;
  readonly providerCode?: PaymentProviderCode;
  readonly sceneCode: PaymentSceneCode;
  readonly currencyCode: string;
  readonly countryCode: string;
  readonly status: PaymentEntityStatus;
  /** Routing priority — lower number = higher priority. */
  readonly priority: number;
  /** Display sort order. */
  readonly sortOrder: number;
  readonly metadata: Record<string, unknown>;
  readonly createdAt: string;
  readonly updatedAt: string;
}

export interface PaymentChannelDraft {
  readonly channelNo: string;
  readonly channelName?: string;
  readonly providerAccountId: string;
  readonly methodId: string;
  readonly providerCode?: PaymentProviderCode;
  readonly sceneCode: PaymentSceneCode;
  readonly currencyCode: string;
  readonly countryCode: string;
  readonly status?: PaymentEntityStatus;
  readonly priority?: number;
  readonly sortOrder?: number;
  readonly metadata?: Record<string, unknown>;
}

/**
 * Route rule matching conditions are flat fields on the rule itself
 * (not a separate schema). All condition fields are optional — an empty
 * condition set means "match all".
 */
export interface PaymentRouteRuleView {
  readonly id: string;
  readonly ruleNo: string;
  readonly priority: number;
  // === Match conditions (flat) ===
  readonly purchaseType?: string;
  readonly countryCode?: string;
  readonly currencyCode?: string;
  readonly clientPlatform?: string;
  readonly amountMin?: string;
  readonly amountMax?: string;
  readonly userSegment?: string;
  readonly riskLevel?: string;
  // === Action ===
  readonly channelId: string;
  readonly status: PaymentEntityStatus;
  readonly startsAt?: string;
  readonly endsAt?: string;
  readonly createdAt: string;
  readonly updatedAt: string;
}

export interface PaymentRouteRuleDraft {
  readonly ruleNo: string;
  readonly priority?: number;
  readonly purchaseType?: string;
  readonly countryCode?: string;
  readonly currencyCode?: string;
  readonly clientPlatform?: string;
  readonly amountMin?: string;
  readonly amountMax?: string;
  readonly userSegment?: string;
  readonly riskLevel?: string;
  readonly channelId: string;
  readonly status?: PaymentEntityStatus;
  readonly startsAt?: string;
  readonly endsAt?: string;
}

/**
 * Update draft for PATCH /route_rules/{routeRuleId}. `ruleNo` is immutable
 * after creation per OpenAPI `additionalProperties: false`.
 */
export interface PaymentRouteRuleUpdateDraft {
  readonly priority?: number;
  readonly purchaseType?: string;
  readonly countryCode?: string;
  readonly currencyCode?: string;
  readonly clientPlatform?: string;
  readonly amountMin?: string;
  readonly amountMax?: string;
  readonly userSegment?: string;
  readonly riskLevel?: string;
  readonly channelId?: string;
  readonly status?: PaymentEntityStatus;
  readonly startsAt?: string;
  readonly endsAt?: string;
}

export interface PaymentChannelListFilter {
  readonly providerCode?: PaymentProviderCode;
  readonly sceneCode?: PaymentSceneCode;
  readonly status?: PaymentEntityStatus;
}

export interface PaymentMethodListFilter {
  readonly status?: PaymentEntityStatus;
}

export interface PaymentRouteRuleListFilter {
  readonly status?: PaymentEntityStatus;
  readonly channelId?: string;
}

export type PaymentChannelAdminStatus =
  | "idle"
  | "loading"
  | "ready"
  | "saving"
  | "error";

export interface PaymentChannelAdminState {
  readonly methods: readonly PaymentMethodView[];
  readonly channels: readonly PaymentChannelView[];
  readonly routeRules: readonly PaymentRouteRuleView[];
  readonly providerAccounts: readonly PaymentProviderAccountView[];
  readonly listPageInfo?: Partial<{
    methods: SdkWorkPageInfo;
    channels: SdkWorkPageInfo;
    routeRules: SdkWorkPageInfo;
    providerAccounts: SdkWorkPageInfo;
  }>;
  readonly status: PaymentChannelAdminStatus;
  readonly lastError?: string;
  readonly selectedMethodId?: string;
  readonly selectedChannelId?: string;
  readonly selectedRouteRuleId?: string;
}

export interface PaymentChannelAdminController {
  getState(): PaymentChannelAdminState;
  subscribe(listener: () => void): () => void;
  load(): Promise<PaymentChannelAdminState>;
  loadMoreMethods(): Promise<readonly PaymentMethodView[]>;
  loadMoreChannels(): Promise<readonly PaymentChannelView[]>;
  loadMoreRouteRules(): Promise<readonly PaymentRouteRuleView[]>;
  loadMoreProviderAccounts(): Promise<readonly PaymentProviderAccountView[]>;
  selectMethod(id?: string): PaymentMethodView | undefined;
  selectChannel(id?: string): PaymentChannelView | undefined;
  selectRouteRule(id?: string): PaymentRouteRuleView | undefined;
  createMethod(draft: PaymentMethodDraft): Promise<PaymentMethodView>;
  updateMethod(methodKey: string, draft: PaymentMethodUpdateDraft): Promise<PaymentMethodView>;
  createChannel(draft: PaymentChannelDraft): Promise<PaymentChannelView>;
  createRouteRule(draft: PaymentRouteRuleDraft): Promise<PaymentRouteRuleView>;
  updateRouteRule(id: string, draft: PaymentRouteRuleUpdateDraft): Promise<PaymentRouteRuleView>;
  deleteRouteRule(id: string): Promise<void>;
}

export interface CreatePaymentChannelAdminControllerInput {
  readonly service: SdkworkPaymentBackendService;
}
