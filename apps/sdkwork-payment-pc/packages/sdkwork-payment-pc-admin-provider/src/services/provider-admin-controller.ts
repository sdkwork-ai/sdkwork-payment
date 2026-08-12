/**
 * Provider admin controller.
 *
 * Stateful controller that consumes `SdkworkPaymentBackendService` (via the
 * port-adapter-service pattern from APP_SDK_INTEGRATION_SPEC.md §9). It owns:
 *   - Two paged list sessions (providerAccounts, subMerchants)
 *   - CRUD mutations for provider accounts and sub-merchants
 *   - Dev-config operations: credential test + credential replace (rotate)
 *   - React-friendly external store contract (subscribe/getState)
 *
 * The controller NEVER imports `@sdkwork/payment-backend-sdk` directly; the
 * backend SDK client is injected via `service.backend` on the parent app service.
 */

import {
  createSdkWorkPagedListSession,
  extractSdkWorkResourceItem,
  type SdkWorkPageInfo,
  type SdkWorkPagedListSession,
} from "@sdkwork/payment-contracts";
import type { SdkworkPaymentBackendService } from "@sdkwork/payment-service";
import { uuid } from "@sdkwork/utils";
import {
  asRecord,
  asRequiredString,
  asStatus,
  asString,
} from "@sdkwork/payment-pc-admin-core";
import type {
  CreatePaymentProviderAdminControllerInput,
  PaymentProviderAccountTestResult,
  PaymentProviderAccountView,
  PaymentProviderAdminController,
  PaymentProviderAdminState,
  PaymentProviderAdminStatus,
  PaymentSubMerchantView,
} from "../types/provider-admin-types";

type Snapshot = Pick<PaymentProviderAdminState, "providerAccounts" | "subMerchants">;

const EMPTY_SNAPSHOT: Snapshot = {
  providerAccounts: [],
  subMerchants: [],
};

function cloneSnapshot(snapshot: Snapshot): Snapshot {
  return {
    providerAccounts: [...snapshot.providerAccounts],
    subMerchants: [...snapshot.subMerchants],
  };
}

const PROVIDER_CODES = ["stripe", "alipay", "wechat_pay", "sandbox"] as const;
const ACCOUNT_MODES = ["direct", "partner"] as const;
const ENVIRONMENTS = ["development", "sandbox", "production"] as const;
const PROVIDER_ACCOUNT_STATUSES = ["active", "inactive", "suspended", "deprecated"] as const;
const LAST_TEST_STATUSES = ["success", "failure", "unknown"] as const;
const SUB_MERCHANT_STATUSES = ["active", "inactive", "suspended", "deprecated"] as const;

function asCapabilities(value: unknown): PaymentProviderAccountView["capabilities"] {
  const record = asRecord(value);
  const capabilities: Record<string, boolean> = {};
  for (const [key, val] of Object.entries(record)) {
    if (typeof val === "boolean") {
      capabilities[key] = val;
    }
  }
  return capabilities;
}

function mapProviderAccount(value: unknown): PaymentProviderAccountView | undefined {
  if (!value || typeof value !== "object") {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  const id = asString(record.id);
  if (!id) {
    return undefined;
  }
  return {
    id,
    accountNo: asRequiredString(record.accountNo, id),
    providerCode: asStatus(record.providerCode, PROVIDER_CODES, "sandbox"),
    merchantId: asString(record.merchantId),
    accountName: asString(record.accountName ?? record.account_name),
    accountNameI18n: asRecord(record.accountNameI18n ?? record.account_name_i18n) as Record<
      string,
      string
    >,
    accountMode: asStatus(record.accountMode, ACCOUNT_MODES, "direct"),
    partnerProviderAccountId: asString(record.partnerProviderAccountId),
    environment: asStatus(record.environment, ENVIRONMENTS, "production"),
    countryCode: asString(record.countryCode),
    settlementCurrency: asString(record.settlementCurrency) ?? "CNY",
    hasPrimarySecret: record.hasPrimarySecret === true,
    hasWebhookSecret: record.hasWebhookSecret === true,
    hasCertificate: record.hasCertificate === true,
    credentialStorage: asStatus(
      record.credentialStorage,
      ["database_encrypted", "legacy_reference", "none"] as const,
      "none",
    ),
    capabilities: asCapabilities(record.capabilities),
    status: asStatus(record.status, PROVIDER_ACCOUNT_STATUSES, "active"),
    metadata: asRecord(record.metadata),
    certificateExpiresAt: asString(record.certificateExpiresAt),
    lastTestedAt: asString(record.lastTestedAt),
    lastTestStatus: asStatus(record.lastTestStatus, LAST_TEST_STATUSES, "unknown"),
    createdAt: asString(record.createdAt) ?? new Date(0).toISOString(),
    updatedAt: asString(record.updatedAt) ?? new Date(0).toISOString(),
  };
}

function mapSubMerchant(value: unknown): PaymentSubMerchantView | undefined {
  if (!value || typeof value !== "object") {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  const id = asString(record.id);
  if (!id) {
    return undefined;
  }
  return {
    id,
    providerAccountId: asRequiredString(record.providerAccountId, id),
    subMerchantNo: asRequiredString(record.subMerchantNo ?? record.sub_merchant_no ?? record.externalSubMerchantId ?? record.external_sub_merchant_id, id),
    subMerchantName: asString(record.subMerchantName ?? record.sub_merchant_name ?? record.displayName ?? record.display_name),
    subAppId: asString(record.subAppId ?? record.sub_app_id ?? record.subAppid ?? record.sub_appid),
    subMchId: asString(record.subMchId ?? record.sub_mch_id),
    stripeConnectedAccountId: asString(record.stripeConnectedAccountId ?? record.stripe_connected_account_id),
    providerCode: asStatus(record.providerCode ?? record.provider_code, PROVIDER_CODES, "sandbox"),
    status: asStatus(record.status, SUB_MERCHANT_STATUSES, "active"),
    metadata: asRecord(record.metadata),
    createdAt: asString(record.createdAt) ?? new Date(0).toISOString(),
    updatedAt: asString(record.updatedAt) ?? new Date(0).toISOString(),
  };
}

function mapTestResult(value: unknown): PaymentProviderAccountTestResult | undefined {
  if (!value || typeof value !== "object") {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  if (typeof record.ok !== "boolean") {
    return undefined;
  }
  return {
    ok: record.ok,
    providerCode: asStatus(record.providerCode, PROVIDER_CODES, "sandbox"),
    environment: asStatus(record.environment, ENVIRONMENTS, "production"),
    pspResponseCode: asString(record.pspResponseCode ?? record.psp_response_code),
    pspResponseTimeMs: typeof record.pspResponseTimeMs === "number" ? record.pspResponseTimeMs : typeof record.psp_response_time_ms === "number" ? record.psp_response_time_ms : undefined,
    diagnostic: asString(record.diagnostic),
    testedAt: asString(record.testedAt ?? record.tested_at) ?? new Date().toISOString(),
  };
}

interface ProviderAdminSessions {
  providerAccounts: SdkWorkPagedListSession<PaymentProviderAccountView>;
  subMerchants: SdkWorkPagedListSession<PaymentSubMerchantView>;
  subMerchantsProviderAccountId?: string;
}

function createSessions(service: SdkworkPaymentBackendService): ProviderAdminSessions {
  return {
    providerAccounts: createSdkWorkPagedListSession<PaymentProviderAccountView>({
      fetchPage: (query) => service.providerAccounts.list(query),
      mapItem: mapProviderAccount,
    }),
    subMerchants: createSdkWorkPagedListSession<PaymentSubMerchantView>({
      fetchPage: (query) => service.subMerchants.list(query),
      mapItem: mapSubMerchant,
    }),
  };
}

function pageInfoFromSessions(
  sessions: ProviderAdminSessions,
): Partial<Record<keyof Snapshot, SdkWorkPageInfo>> {
  const pageInfo: Partial<Record<keyof Snapshot, SdkWorkPageInfo>> = {};
  const providerAccounts = sessions.providerAccounts.getPageInfo();
  const subMerchants = sessions.subMerchants.getPageInfo();
  if (providerAccounts) {
    pageInfo.providerAccounts = providerAccounts;
  }
  if (subMerchants) {
    pageInfo.subMerchants = subMerchants;
  }
  return pageInfo;
}

export function createPaymentProviderAdminController(
  input: CreatePaymentProviderAdminControllerInput,
): PaymentProviderAdminController {
  const service = input.service;
  const listeners = new Set<() => void>();
  const sessions = createSessions(service);

  let state: PaymentProviderAdminState = {
    ...EMPTY_SNAPSHOT,
    status: "idle",
  };

  function emit(): void {
    listeners.forEach((listener) => listener());
  }

  function setState(patch: Partial<PaymentProviderAdminState>): void {
    const nextSnapshot: Snapshot = {
      providerAccounts: [...sessions.providerAccounts.getItems()],
      subMerchants: [...sessions.subMerchants.getItems()],
    };
    const selectedProviderAccount = Object.prototype.hasOwnProperty.call(patch, "selectedProviderAccount")
      ? patch.selectedProviderAccount
      : state.selectedProviderAccount
        ? nextSnapshot.providerAccounts.find((account) => account.id === state.selectedProviderAccount?.id)
        : undefined;
    const selectedSubMerchant = Object.prototype.hasOwnProperty.call(patch, "selectedSubMerchant")
      ? patch.selectedSubMerchant
      : state.selectedSubMerchant
        ? nextSnapshot.subMerchants.find((merchant) => merchant.id === state.selectedSubMerchant?.id)
        : undefined;
    state = {
      ...state,
      ...patch,
      ...cloneSnapshot(nextSnapshot),
      listPageInfo: pageInfoFromSessions(sessions),
      selectedProviderAccount,
      selectedSubMerchant,
    };
    emit();
  }

  function setStatus(status: PaymentProviderAdminStatus, lastError?: string): void {
    state = { ...state, status, lastError };
    emit();
  }

  async function wrapMutation<T>(
    action: () => Promise<T>,
    errorMessage: string,
    options: { reload: "providerAccounts" | "subMerchants" | "both" | "none" } = { reload: "providerAccounts" },
  ): Promise<T> {
    setStatus("saving", undefined);
    try {
      const result = await action();
      if (options.reload === "providerAccounts" || options.reload === "both") {
        await sessions.providerAccounts.list();
      }
      if (options.reload === "subMerchants" || options.reload === "both") {
        await sessions.subMerchants.list(
          sessions.subMerchantsProviderAccountId
            ? { providerAccountId: sessions.subMerchantsProviderAccountId }
            : undefined,
        );
      }
      setState({ status: "ready", lastError: undefined });
      return result;
    } catch (error) {
      setStatus("error", error instanceof Error ? error.message : errorMessage);
      throw error;
    }
  }

  return {
    getState() {
      return state;
    },

    subscribe(listener) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },

    async load() {
      sessions.providerAccounts.reset();
      sessions.subMerchants.reset();
      sessions.subMerchantsProviderAccountId = undefined;
      setState({
        status: "loading",
        lastError: undefined,
        selectedProviderAccount: undefined,
        selectedSubMerchant: undefined,
      });
      try {
        await sessions.providerAccounts.list();
        const firstPartnerAccount = sessions.providerAccounts
          .getItems()
          .find((account) => account.accountMode === "partner");
        if (firstPartnerAccount) {
          sessions.subMerchantsProviderAccountId = firstPartnerAccount.id;
          await sessions.subMerchants.list({ providerAccountId: firstPartnerAccount.id });
        }
        setState({
          status: "ready",
          lastError: undefined,
          selectedProviderAccount: firstPartnerAccount,
          selectedSubMerchant: undefined,
        });
        return state;
      } catch (error) {
        setState({
          status: "error",
          lastError: error instanceof Error ? error.message : "Failed to load provider admin data.",
        });
        throw error;
      }
    },

    async loadMoreProviderAccounts() {
      setStatus("loading", undefined);
      try {
        const items = await sessions.providerAccounts.loadMore();
        setState({ status: "ready", lastError: undefined });
        return items;
      } catch (error) {
        setState({
          status: "error",
          lastError: error instanceof Error ? error.message : "Failed to load more provider accounts.",
        });
        throw error;
      }
    },

    async loadMoreSubMerchants(providerAccountId) {
      setStatus("loading", undefined);
      try {
        if (providerAccountId && sessions.subMerchantsProviderAccountId !== providerAccountId) {
          sessions.subMerchants.reset();
          sessions.subMerchantsProviderAccountId = providerAccountId;
          setState({ selectedSubMerchant: undefined });
          const items = await sessions.subMerchants.list({ providerAccountId });
          setState({ status: "ready", lastError: undefined });
          return items;
        }
        const items = await sessions.subMerchants.loadMore(
          providerAccountId ? { providerAccountId } : undefined,
        );
        setState({ status: "ready", lastError: undefined });
        return items;
      } catch (error) {
        setState({
          status: "error",
          lastError: error instanceof Error ? error.message : "Failed to load more sub-merchants.",
        });
        throw error;
      }
    },

    selectProviderAccount(id) {
      const next = id
        ? state.providerAccounts.find((account) => account.id === id)
        : undefined;
      state = { ...state, selectedProviderAccount: next };
      emit();
      return next;
    },

    selectSubMerchant(id) {
      const next = id
        ? state.subMerchants.find((merchant) => merchant.id === id)
        : undefined;
      state = { ...state, selectedSubMerchant: next };
      emit();
      return next;
    },

    async createProviderAccount(draft) {
      return wrapMutation(
        async () => {
          const response = await service.providerAccounts.create(draft, {
            idempotencyKey: paymentCommandIdempotencyKey("provider-account-create"),
          });
          const item = extractSdkWorkResourceItem<unknown>(response);
          const mapped = mapProviderAccount(item);
          if (!mapped) {
            throw new Error("Failed to parse created provider account.");
          }
          return mapped;
        },
        "Failed to create provider account.",
        { reload: "providerAccounts" },
      );
    },

    async updateProviderAccount(id, draft) {
      return wrapMutation(
        async () => {
          const response = await service.providerAccounts.update(id, draft, {
            idempotencyKey: paymentCommandIdempotencyKey("provider-account-update"),
          });
          const item = extractSdkWorkResourceItem<unknown>(response);
          const mapped = mapProviderAccount(item);
          if (!mapped) {
            throw new Error("Failed to parse updated provider account.");
          }
          return mapped;
        },
        "Failed to update provider account.",
        { reload: "providerAccounts" },
      );
    },

    async deleteProviderAccount(id) {
      return wrapMutation(
        async () => {
          await service.providerAccounts.delete(id);
        },
        "Failed to delete provider account.",
        { reload: "providerAccounts" },
      ).then(() => {
        const wasSelected = state.selectedProviderAccount?.id === id;
        if (wasSelected) {
          state = { ...state, selectedProviderAccount: undefined };
          emit();
        }
      });
    },

    async testProviderAccount(id, options) {
      setStatus("testing", undefined);
      try {
        // SDK 签名: test(id, params, body?, requestOptions?) — idempotencyKey
        // 走 params（生成 Idempotency-Key 请求头），测试选项走 body。
        // 此前把 options 误传给 params 导致 dryRun 丢失，后端走了非 dry-run
        // 分支并报出 "does not expose a non-mutating remote connectivity
        // probe" 的诊断。
        const response = await service.providerAccounts.test(
          id,
          { idempotencyKey: paymentCommandIdempotencyKey("provider-account-test") },
          options ?? {},
        );
        const item = extractSdkWorkResourceItem<unknown>(response);
        const mapped = mapTestResult(item);
        if (!mapped) {
          throw new Error("Failed to parse provider account test result.");
        }
        // Reload provider account to refresh lastTestedAt/lastTestStatus.
        await sessions.providerAccounts.list();
        setState({
          status: "ready",
          lastError: undefined,
          lastTestResult: mapped,
        });
        return mapped;
      } catch (error) {
        setStatus("error", error instanceof Error ? error.message : "Failed to test provider account credentials.");
        throw error;
      }
    },

    async rotateProviderAccountCredentials(id, draft) {
      return wrapMutation(
        async () => {
          const response = await service.providerAccounts.credentials.rotate(id, draft, {
            idempotencyKey: paymentCommandIdempotencyKey("provider-account-rotate"),
          });
          const item = extractSdkWorkResourceItem<unknown>(response);
          const mapped = mapProviderAccount(item);
          if (!mapped) {
            throw new Error("Failed to parse provider account after credential replacement.");
          }
          return mapped;
        },
        "Failed to replace provider account credentials.",
        { reload: "providerAccounts" },
      ).then((account) => {
        state = { ...state, lastRotatedAccountId: id };
        emit();
        return account;
      });
    },

    /** Decrypts and returns the account's active credentials for display,
     *  copy, and download in the admin workspace. */
    async readProviderAccountCredentials(id) {
      const response = await service.providerAccounts.credentials.read(id);
      return {
        providerAccountId: String(response.providerAccountId ?? id),
        primarySecret: response.primarySecret ?? "",
        webhookSecret: response.webhookSecret ?? "",
        certificate: response.certificate ?? "",
      };
    },

    async createSubMerchant(draft) {
      return wrapMutation(
        async () => {
          const response = await service.subMerchants.create(draft, {
            idempotencyKey: paymentCommandIdempotencyKey("sub-merchant-create"),
          });
          const item = extractSdkWorkResourceItem<unknown>(response);
          const mapped = mapSubMerchant(item);
          if (!mapped) {
            throw new Error("Failed to parse created sub-merchant.");
          }
          return mapped;
        },
        "Failed to create sub-merchant.",
        { reload: "subMerchants" },
      );
    },

    async updateSubMerchant(id, draft) {
      return wrapMutation(
        async () => {
          const response = await service.subMerchants.update(id, draft, {
            idempotencyKey: paymentCommandIdempotencyKey("sub-merchant-update"),
          });
          const item = extractSdkWorkResourceItem<unknown>(response);
          const mapped = mapSubMerchant(item);
          if (!mapped) {
            throw new Error("Failed to parse updated sub-merchant.");
          }
          return mapped;
        },
        "Failed to update sub-merchant.",
        { reload: "subMerchants" },
      );
    },

    async deleteSubMerchant(id) {
      return wrapMutation(
        async () => {
          await service.subMerchants.delete(id);
        },
        "Failed to delete sub-merchant.",
        { reload: "subMerchants" },
      );
    },
  };
}

function paymentCommandIdempotencyKey(prefix: string): string {
  return `${prefix}-${uuid()}`;
}
