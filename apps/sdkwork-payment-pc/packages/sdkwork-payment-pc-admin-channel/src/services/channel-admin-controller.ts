/**
 * Channel admin controller.
 *
 * Stateful controller that consumes `SdkworkPaymentBackendService` (via the
 * port-adapter-service pattern from APP_SDK_INTEGRATION_SPEC.md §9). It owns:
 *   - Four paged list sessions (methods, channels, routeRules, providerAccounts)
 *   - Method CRUD (create + update by `methodKey`; API has no delete)
 *   - Channel create only (API has no update/delete — UI surfaces this honestly)
 *   - RouteRule CRUD (create + update + delete; no retrieve)
 *   - React-friendly external store contract (subscribe/getState)
 *
 * The controller NEVER imports `@sdkwork/payment-backend-sdk` directly; the
 * backend SDK client is injected via `service.backend` on the parent app service.
 */

import {
  createSdkWorkPagedListSession,
  extractSdkWorkResourceItem,
  type SdkWorkPagedListSession,
} from "@sdkwork/payment-contracts";
import type { SdkworkPaymentBackendService } from "@sdkwork/payment-service";
import {
  asNumber,
  asRecord,
  asRequiredString,
  asStatus,
  asString,
} from "@sdkwork/payment-pc-admin-core";
import type {
  CreatePaymentChannelAdminControllerInput,
  PaymentChannelAdminController,
  PaymentChannelAdminState,
  PaymentChannelAdminStatus,
  PaymentChannelDraft,
  PaymentChannelView,
  PaymentMethodDraft,
  PaymentMethodUpdateDraft,
  PaymentMethodView,
  PaymentProviderAccountView,
  PaymentRouteRuleDraft,
  PaymentRouteRuleUpdateDraft,
  PaymentRouteRuleView,
} from "../types/channel-admin-types";

type Snapshot = Pick<
  PaymentChannelAdminState,
  "methods" | "channels" | "routeRules" | "providerAccounts"
>;

const EMPTY_SNAPSHOT: Snapshot = {
  methods: [],
  channels: [],
  routeRules: [],
  providerAccounts: [],
};

function cloneSnapshot(snapshot: Snapshot): Snapshot {
  return {
    methods: [...snapshot.methods],
    channels: [...snapshot.channels],
    routeRules: [...snapshot.routeRules],
    providerAccounts: [...snapshot.providerAccounts],
  };
}

const PROVIDER_CODES = ["stripe", "alipay", "wechat_pay", "paypal", "sandbox"] as const;
const ENTITY_STATUSES = ["active", "inactive", "deprecated"] as const;
const PROVIDER_ACCOUNT_STATUSES = ["active", "inactive", "suspended", "deprecated"] as const;
const SCENES = ["app", "web", "mini_program", "api"] as const;
const SCOPES = ["global", "tenant", "organization"] as const;
const ENVIRONMENTS = ["development", "sandbox", "production"] as const;
const ACCOUNT_MODES = ["direct", "partner"] as const;

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
    accountMode: asStatus(record.accountMode, ACCOUNT_MODES, "direct"),
    environment: asStatus(record.environment, ENVIRONMENTS, "production"),
    countryCode: asString(record.countryCode),
    settlementCurrency: asString(record.settlementCurrency) ?? "CNY",
    status: asStatus(record.status, PROVIDER_ACCOUNT_STATUSES, "active"),
  };
}

function mapMethod(value: unknown): PaymentMethodView | undefined {
  if (!value || typeof value !== "object") {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  const id = asString(record.id);
  const methodKey = asString(record.methodKey ?? record.method_key);
  if (!id || !methodKey) {
    return undefined;
  }
  return {
    id,
    methodKey,
    displayName: asRequiredString(record.displayName ?? record.display_name, methodKey),
    providerCode: asStatus(record.providerCode ?? record.provider_code, PROVIDER_CODES, "sandbox"),
    status: asStatus(record.status, ENTITY_STATUSES, "active"),
    scope: asStatus(record.scope, SCOPES, "tenant"),
    currencyCode: asString(record.currencyCode ?? record.currency_code) ?? "CNY",
    countryCode: asString(record.countryCode ?? record.country_code),
    sortOrder: asNumber(record.sortOrder ?? record.sort_order) ?? 0,
    metadata: asRecord(record.metadata),
    createdAt: asString(record.createdAt ?? record.created_at) ?? new Date(0).toISOString(),
    updatedAt: asString(record.updatedAt ?? record.updated_at) ?? new Date(0).toISOString(),
  };
}

function mapChannel(value: unknown): PaymentChannelView | undefined {
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
    channelNo: asRequiredString(record.channelNo ?? record.channel_no, id),
    channelName: asString(record.channelName ?? record.channel_name),
    providerAccountId: asRequiredString(record.providerAccountId ?? record.provider_account_id, ""),
    methodId: asRequiredString(record.methodId ?? record.method_id, ""),
    providerCode: asString(record.providerCode ?? record.provider_code) as PaymentChannelView["providerCode"],
    sceneCode: asStatus(record.sceneCode ?? record.scene_code, SCENES, "api"),
    currencyCode: asRequiredString(record.currencyCode ?? record.currency_code, "CNY"),
    countryCode: asRequiredString(record.countryCode ?? record.country_code, ""),
    status: asStatus(record.status, ENTITY_STATUSES, "active"),
    priority: asNumber(record.priority) ?? 0,
    sortOrder: asNumber(record.sortOrder ?? record.sort_order) ?? 0,
    metadata: asRecord(record.metadata),
    createdAt: asString(record.createdAt ?? record.created_at) ?? new Date(0).toISOString(),
    updatedAt: asString(record.updatedAt ?? record.updated_at) ?? new Date(0).toISOString(),
  };
}

function mapRouteRule(value: unknown): PaymentRouteRuleView | undefined {
  if (!value || typeof value !== "object") {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  const id = asString(record.id);
  const ruleNo = asString(record.ruleNo ?? record.rule_no);
  if (!id || !ruleNo) {
    return undefined;
  }
  return {
    id,
    ruleNo,
    priority: asNumber(record.priority) ?? 0,
    purchaseType: asString(record.purchaseType ?? record.purchase_type),
    countryCode: asString(record.countryCode ?? record.country_code),
    currencyCode: asString(record.currencyCode ?? record.currency_code),
    clientPlatform: asString(record.clientPlatform ?? record.client_platform),
    amountMin: asString(record.amountMin ?? record.amount_min),
    amountMax: asString(record.amountMax ?? record.amount_max),
    userSegment: asString(record.userSegment ?? record.user_segment),
    riskLevel: asString(record.riskLevel ?? record.risk_level),
    channelId: asRequiredString(record.channelId ?? record.channel_id, ""),
    status: asStatus(record.status, ENTITY_STATUSES, "active"),
    startsAt: asString(record.startsAt ?? record.starts_at),
    endsAt: asString(record.endsAt ?? record.ends_at),
    createdAt: asString(record.createdAt ?? record.created_at) ?? new Date(0).toISOString(),
    updatedAt: asString(record.updatedAt ?? record.updated_at) ?? new Date(0).toISOString(),
  };
}

interface ChannelAdminSessions {
  methods: SdkWorkPagedListSession<PaymentMethodView>;
  channels: SdkWorkPagedListSession<PaymentChannelView>;
  routeRules: SdkWorkPagedListSession<PaymentRouteRuleView>;
  providerAccounts: SdkWorkPagedListSession<PaymentProviderAccountView>;
}

function createSessions(service: SdkworkPaymentBackendService): ChannelAdminSessions {
  return {
    methods: createSdkWorkPagedListSession<PaymentMethodView>({
      fetchPage: (query) => service.methods.list(query),
      mapItem: mapMethod,
    }),
    channels: createSdkWorkPagedListSession<PaymentChannelView>({
      fetchPage: (query) => service.channels.list(query),
      mapItem: mapChannel,
    }),
    routeRules: createSdkWorkPagedListSession<PaymentRouteRuleView>({
      fetchPage: (query) => service.routeRules.list(query),
      mapItem: mapRouteRule,
    }),
    providerAccounts: createSdkWorkPagedListSession<PaymentProviderAccountView>({
      fetchPage: (query) => service.providerAccounts.list(query),
      mapItem: mapProviderAccount,
    }),
  };
}

type ReloadTarget = "methods" | "channels" | "routeRules" | "providerAccounts" | "none";

export function createPaymentChannelAdminController(
  input: CreatePaymentChannelAdminControllerInput,
): PaymentChannelAdminController {
  const service = input.service;
  const listeners = new Set<() => void>();
  const sessions = createSessions(service);

  let state: PaymentChannelAdminState = {
    ...EMPTY_SNAPSHOT,
    status: "idle",
  };

  function emit(): void {
    listeners.forEach((listener) => listener());
  }

  function setState(patch: Partial<PaymentChannelAdminState>): void {
    const nextSnapshot: Snapshot = {
      methods: [...sessions.methods.getItems()],
      channels: [...sessions.channels.getItems()],
      routeRules: [...sessions.routeRules.getItems()],
      providerAccounts: [...sessions.providerAccounts.getItems()],
    };
    state = {
      ...state,
      ...patch,
      ...cloneSnapshot(nextSnapshot),
    };
    emit();
  }

  function setStatus(status: PaymentChannelAdminStatus, lastError?: string): void {
    state = { ...state, status, ...(lastError === undefined ? {} : { lastError }) };
    emit();
  }

  async function reload(target: ReloadTarget): Promise<void> {
    if (target === "methods") {
      await sessions.methods.list();
    } else if (target === "channels") {
      await sessions.channels.list();
    } else if (target === "routeRules") {
      await sessions.routeRules.list();
    } else if (target === "providerAccounts") {
      await sessions.providerAccounts.list();
    }
  }

  async function wrapMutation<T>(
    action: () => Promise<T>,
    errorMessage: string,
    options: { reload: ReloadTarget } = { reload: "none" },
  ): Promise<T> {
    setStatus("saving", undefined);
    try {
      const result = await action();
      if (options.reload !== "none") {
        await reload(options.reload);
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
      setStatus("loading", undefined);
      sessions.methods.reset();
      sessions.channels.reset();
      sessions.routeRules.reset();
      sessions.providerAccounts.reset();
      try {
        await Promise.all([
          sessions.methods.list(),
          sessions.channels.list(),
          sessions.routeRules.list(),
          sessions.providerAccounts.list(),
        ]);
        setState({
          status: "ready",
          lastError: undefined,
          selectedMethodId: undefined,
          selectedChannelId: undefined,
          selectedRouteRuleId: undefined,
        });
        return state;
      } catch (error) {
        setStatus("error", error instanceof Error ? error.message : "Failed to load channel admin data.");
        throw error;
      }
    },

    async loadMoreMethods() {
      try {
        return await sessions.methods.loadMore();
      } finally {
        setState({});
      }
    },

    async loadMoreChannels() {
      try {
        return await sessions.channels.loadMore();
      } finally {
        setState({});
      }
    },

    async loadMoreRouteRules() {
      try {
        return await sessions.routeRules.loadMore();
      } finally {
        setState({});
      }
    },

    async loadMoreProviderAccounts() {
      try {
        return await sessions.providerAccounts.loadMore();
      } finally {
        setState({});
      }
    },

    selectMethod(id) {
      const next = id ? state.methods.find((method) => method.id === id) : undefined;
      state = { ...state, selectedMethodId: next?.id };
      emit();
      return next;
    },

    selectChannel(id) {
      const next = id ? state.channels.find((channel) => channel.id === id) : undefined;
      state = { ...state, selectedChannelId: next?.id };
      emit();
      return next;
    },

    selectRouteRule(id) {
      const next = id ? state.routeRules.find((rule) => rule.id === id) : undefined;
      state = { ...state, selectedRouteRuleId: next?.id };
      emit();
      return next;
    },

    async createMethod(draft) {
      return wrapMutation(
        async () => {
          const response = await service.methods.create(draft);
          const item = extractSdkWorkResourceItem<unknown>(response);
          const mapped = mapMethod(item);
          if (!mapped) {
            throw new Error("Failed to parse created payment method.");
          }
          return mapped;
        },
        "Failed to create payment method.",
        { reload: "methods" },
      );
    },

    async updateMethod(methodKey, draft) {
      return wrapMutation(
        async () => {
          const response = await service.methods.update(methodKey, draft);
          const item = extractSdkWorkResourceItem<unknown>(response);
          const mapped = mapMethod(item);
          if (!mapped) {
            throw new Error("Failed to parse updated payment method.");
          }
          return mapped;
        },
        "Failed to update payment method.",
        { reload: "methods" },
      );
    },

    async createChannel(draft) {
      return wrapMutation(
        async () => {
          const response = await service.channels.create(draft);
          const item = extractSdkWorkResourceItem<unknown>(response);
          const mapped = mapChannel(item);
          if (!mapped) {
            throw new Error("Failed to create payment channel.");
          }
          return mapped;
        },
        "Failed to create payment channel.",
        { reload: "channels" },
      );
    },

    async createRouteRule(draft) {
      return wrapMutation(
        async () => {
          const response = await service.routeRules.create(draft);
          const item = extractSdkWorkResourceItem<unknown>(response);
          const mapped = mapRouteRule(item);
          if (!mapped) {
            throw new Error("Failed to create route rule.");
          }
          return mapped;
        },
        "Failed to create route rule.",
        { reload: "routeRules" },
      );
    },

    async updateRouteRule(id, draft) {
      return wrapMutation(
        async () => {
          const response = await service.routeRules.update(id, draft);
          const item = extractSdkWorkResourceItem<unknown>(response);
          const mapped = mapRouteRule(item);
          if (!mapped) {
            throw new Error("Failed to update route rule.");
          }
          return mapped;
        },
        "Failed to update route rule.",
        { reload: "routeRules" },
      );
    },

    async deleteRouteRule(id) {
      return wrapMutation(
        async () => {
          await service.routeRules.delete(id);
        },
        "Failed to delete route rule.",
        { reload: "routeRules" },
      );
    },
  };
}
