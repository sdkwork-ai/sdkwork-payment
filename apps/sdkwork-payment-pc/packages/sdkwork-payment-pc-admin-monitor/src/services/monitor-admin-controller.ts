/**
 * Payment admin monitor controller.
 *
 * Stateful controller that consumes `SdkworkPaymentBackendService` (via the
 * port-adapter-service pattern from APP_SDK_INTEGRATION_SPEC.md §9). It owns:
 *   - Four paged list sessions (intents, attempts, webhookEvents, reconciliationRuns)
 *   - Intent detail retrieval (intents.retrieve by id)
 *   - Webhook event replay (webhookEvents.replay)
 *   - Reconciliation run creation (reconciliationRuns.create)
 *   - Per-resource filters (status/providerCode/etc.) applied via reset+list
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
import { uuid } from "@sdkwork/utils/id";
import type {
  CreatePaymentMonitorAdminControllerInput,
  CreateRefundDraft,
  CreateReconciliationRunDraft,
  PaymentAttemptListFilter,
  PaymentAttemptView,
  PaymentIntentDetail,
  PaymentIntentListFilter,
  PaymentIntentView,
  PaymentMonitorAdminController,
  PaymentMonitorAdminState,
  PaymentMonitorAdminStatus,
  PaymentProviderCode,
  PaymentStatus,
  PaymentWebhookEventListFilter,
  PaymentWebhookEventView,
  ReconciliationRunListFilter,
  ReconciliationRunView,
  ReconciliationRunStatus,
  ReconciliationType,
  RefundListFilter,
  RefundView,
  WebhookEventStatus,
  WebhookReplayResult,
  WebhookSignatureStatus,
} from "../types/monitor-admin-types";
import {
  PAYMENT_STATUS_VALUES,
  PROVIDER_CODES,
  RECONCILIATION_RUN_STATUS_VALUES,
  RECONCILIATION_TYPE_VALUES,
  REFUND_REASON_VALUES,
  REFUND_STATUS_VALUES,
  WEBHOOK_EVENT_STATUS_VALUES,
  WEBHOOK_SIGNATURE_STATUS_VALUES,
} from "../types/monitor-admin-types";

// ---------------------------------------------------------------------------
// Wire → View mappers
// ---------------------------------------------------------------------------

function mapIntent(value: unknown): PaymentIntentView | undefined {
  if (!value || typeof value !== "object") {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  const id = asString(record.id ?? record.paymentIntentId ?? record.payment_intent_id);
  if (!id) {
    return undefined;
  }
  // Parse attempts summary if included in the list payload for row display
  const rawAttempts = record.attempts;
  const attempts: readonly PaymentAttemptView[] | undefined = Array.isArray(rawAttempts)
    ? rawAttempts
        .map(mapAttempt)
        .filter((item): item is PaymentAttemptView => item !== undefined)
    : undefined;
  return {
    id,
    paymentIntentNo: asRequiredString(record.paymentIntentNo ?? record.payment_intent_no, id),
    orderId: asRequiredString(record.orderId ?? record.order_id, ""),
    ownerUserId: asRequiredString(record.ownerUserId ?? record.owner_user_id, ""),
    paymentMethod: asRequiredString(record.paymentMethod ?? record.payment_method, ""),
    providerCode: asRequiredString(record.providerCode ?? record.provider_code, ""),
    amount: asRequiredString(record.amount, "0"),
    currencyCode: asRequiredString(record.currencyCode ?? record.currency_code, "CNY"),
    status: asStatus(record.status, PAYMENT_STATUS_VALUES, "created"),
    createdAt: asString(record.createdAt ?? record.created_at) ?? new Date(0).toISOString(),
    updatedAt: asString(record.updatedAt ?? record.updated_at) ?? new Date(0).toISOString(),
    attempts,
  };
}

function mapAttempt(value: unknown): PaymentAttemptView | undefined {
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
    paymentIntentId: asRequiredString(record.paymentIntentId ?? record.payment_intent_id, ""),
    attemptNo: asRequiredString(record.attemptNo ?? record.attempt_no, id),
    providerCode: asStatus(record.providerCode ?? record.provider_code, PROVIDER_CODES, "sandbox"),
    channelId: asRequiredString(record.channelId ?? record.channel_id, ""),
    amount: asRequiredString(record.amount, "0"),
    currencyCode: asRequiredString(record.currencyCode ?? record.currency_code, "CNY"),
    status: asStatus(record.status, PAYMENT_STATUS_VALUES, "created"),
    providerTransactionId: asString(record.providerTransactionId ?? record.provider_transaction_id),
    outTradeNo: asString(record.outTradeNo ?? record.out_trade_no),
    paidAt: asString(record.paidAt ?? record.paid_at),
    createdAt: asString(record.createdAt ?? record.created_at) ?? new Date(0).toISOString(),
  };
}

function mapWebhookEvent(value: unknown): PaymentWebhookEventView | undefined {
  if (!value || typeof value !== "object") {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  const id = asString(record.id);
  if (!id) {
    return undefined;
  }
  // payload: backend may return `payload` or `requestBody` (camelCase/snake_case compatible)
  const rawPayload = record.payload ?? record.request_body ?? record.requestBody;
  const payload =
    rawPayload && typeof rawPayload === "object" && !Array.isArray(rawPayload)
      ? (rawPayload as Record<string, unknown>)
      : undefined;
  // headers: backend may return `headers` (key-value pairs)
  const rawHeaders = record.headers;
  const headers: Record<string, string> | undefined = (() => {
    if (!rawHeaders || typeof rawHeaders !== "object" || Array.isArray(rawHeaders)) {
      return undefined;
    }
    const result: Record<string, string> = {};
    for (const [key, val] of Object.entries(rawHeaders as Record<string, unknown>)) {
      if (typeof val === "string") {
        result[key] = val;
      } else if (val !== null && val !== undefined) {
        result[key] = String(val);
      }
    }
    return Object.keys(result).length > 0 ? result : undefined;
  })();
  return {
    id,
    eventId: asString(record.eventId ?? record.event_id),
    providerCode: asStatus(record.providerCode ?? record.provider_code, PROVIDER_CODES, "sandbox"),
    eventType: asRequiredString(record.eventType ?? record.event_type, id),
    status: asStatus(record.status, WEBHOOK_EVENT_STATUS_VALUES, "queued"),
    retries: asNumber(record.retries) ?? 0,
    lastError: asString(record.lastError ?? record.last_error),
    receivedAt: asString(record.receivedAt ?? record.received_at) ?? new Date(0).toISOString(),
    processedAt: asString(record.processedAt ?? record.processed_at),
    signatureStatus: asStatus(
      record.signatureStatus ?? record.signature_status,
      WEBHOOK_SIGNATURE_STATUS_VALUES,
      "unverified",
    ),
    payload,
    headers,
  };
}

function mapReconciliationRun(value: unknown): ReconciliationRunView | undefined {
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
    runNo: asRequiredString(record.runNo ?? record.run_no, id),
    providerCode: asStatus(record.providerCode ?? record.provider_code, PROVIDER_CODES, "sandbox"),
    providerAccountId: asRequiredString(
      record.providerAccountId ?? record.provider_account_id,
      "",
    ),
    reconciliationType: asStatus(
      record.reconciliationType ?? record.reconciliation_type,
      RECONCILIATION_TYPE_VALUES,
      "manual",
    ),
    periodStart: asString(record.periodStart ?? record.period_start) ?? new Date(0).toISOString(),
    periodEnd: asString(record.periodEnd ?? record.period_end) ?? new Date(0).toISOString(),
    status: asStatus(record.status, RECONCILIATION_RUN_STATUS_VALUES, "pending"),
    matchedCount: asNumber(record.matchedCount ?? record.matched_count) ?? 0,
    mismatchedCount: asNumber(record.mismatchedCount ?? record.mismatched_count) ?? 0,
    unmatchedCount: asNumber(record.unmatchedCount ?? record.unmatched_count) ?? 0,
    totalDifferenceAmount: asRequiredString(
      record.totalDifferenceAmount ?? record.total_difference_amount,
      "0",
    ),
    currencyCode: asRequiredString(record.currencyCode ?? record.currency_code, "CNY"),
    createdAt: asString(record.createdAt ?? record.created_at) ?? new Date(0).toISOString(),
  };
}

function mapRefund(value: unknown): RefundView | undefined {
  if (!value || typeof value !== "object") {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  const id = asString(record.id ?? record.refundId ?? record.refund_id);
  if (!id) {
    return undefined;
  }
  return {
    id,
    refundNo: asRequiredString(record.refundNo ?? record.refund_no, id),
    orderId: asRequiredString(record.orderId ?? record.order_id, ""),
    paymentIntentId: asRequiredString(
      record.paymentIntentId ?? record.payment_intent_id,
      "",
    ),
    paymentAttemptId: asRequiredString(
      record.paymentAttemptId ?? record.payment_attempt_id,
      "",
    ),
    providerCode: asStatus(
      record.providerCode ?? record.provider_code,
      PROVIDER_CODES,
      "sandbox",
    ),
    providerAccountId: asString(record.providerAccountId ?? record.provider_account_id),
    amount: asRequiredString(record.amount, "0"),
    currencyCode: asRequiredString(record.currencyCode ?? record.currency_code, "CNY"),
    status: asStatus(record.status, REFUND_STATUS_VALUES, "submitted"),
    reasonCode: asStatus(
      record.reasonCode ?? record.reason_code,
      REFUND_REASON_VALUES,
      "other",
    ),
    requestedByType: asStatus(
      record.requestedByType ?? record.requested_by_type,
      ["buyer", "operator", "system"] as const,
      "system",
    ),
    requestedBy: asString(record.requestedBy ?? record.requested_by),
    createdAt: asString(record.createdAt ?? record.created_at) ?? new Date(0).toISOString(),
    updatedAt: asString(record.updatedAt ?? record.updated_at) ?? new Date(0).toISOString(),
  };
}

function mapIntentDetail(value: unknown, fallback: PaymentIntentView): PaymentIntentDetail {
  const base = mapIntent(value) ?? fallback;
  const record = asRecord(value);
  const rawAttempts = record.attempts;
  const attempts: readonly PaymentAttemptView[] = Array.isArray(rawAttempts)
    ? rawAttempts
        .map(mapAttempt)
        .filter((item): item is PaymentAttemptView => item !== undefined)
    : [];
  return {
    ...base,
    attempts,
    metadata: asRecord(record.metadata),
  };
}

function mapReplayResult(value: unknown, eventId: string): WebhookReplayResult | undefined {
  if (!value || typeof value !== "object") {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  // The replay endpoint returns SdkWorkCommandData { accepted, resourceId?, status? }.
  // We synthesize ok from accepted and inject eventId/diagnostic for the UI.
  const accepted = typeof record.accepted === "boolean" ? record.accepted : true;
  return {
    ok: accepted,
    eventId: asString(record.eventId ?? record.event_id) ?? eventId,
    replayedAt: asString(record.replayedAt ?? record.replayed_at) ?? new Date().toISOString(),
    diagnostic: asString(record.diagnostic ?? record.status),
  };
}

// ---------------------------------------------------------------------------
// Sessions & filters
// ---------------------------------------------------------------------------

interface MonitorAdminSessions {
  intents: SdkWorkPagedListSession<PaymentIntentView>;
  attempts: SdkWorkPagedListSession<PaymentAttemptView>;
  webhookEvents: SdkWorkPagedListSession<PaymentWebhookEventView>;
  reconciliationRuns: SdkWorkPagedListSession<ReconciliationRunView>;
  refunds: SdkWorkPagedListSession<RefundView>;
  intentFilter?: PaymentIntentListFilter;
  attemptFilter?: PaymentAttemptListFilter;
  webhookEventFilter?: PaymentWebhookEventListFilter;
  reconciliationRunFilter?: ReconciliationRunListFilter;
  refundFilter?: RefundListFilter;
}

function createSessions(service: SdkworkPaymentBackendService): MonitorAdminSessions {
  return {
    intents: createSdkWorkPagedListSession<PaymentIntentView>({
      fetchPage: (query) => service.intents.list(query),
      mapItem: mapIntent,
    }),
    attempts: createSdkWorkPagedListSession<PaymentAttemptView>({
      fetchPage: (query) => service.attempts.list(query),
      mapItem: mapAttempt,
    }),
    webhookEvents: createSdkWorkPagedListSession<PaymentWebhookEventView>({
      fetchPage: (query) => service.webhookEvents.list(query),
      mapItem: mapWebhookEvent,
    }),
    reconciliationRuns: createSdkWorkPagedListSession<ReconciliationRunView>({
      fetchPage: (query) => service.reconciliationRuns.list(query),
      mapItem: mapReconciliationRun,
    }),
    refunds: createSdkWorkPagedListSession<RefundView>({
      fetchPage: (query) => service.refunds.list(query),
      mapItem: mapRefund,
    }),
  };
}

type Snapshot = Pick<
  PaymentMonitorAdminState,
  "intents" | "attempts" | "webhookEvents" | "reconciliationRuns" | "refunds"
>;

const EMPTY_SNAPSHOT: Snapshot = {
  intents: [],
  attempts: [],
  webhookEvents: [],
  reconciliationRuns: [],
  refunds: [],
};

function cloneSnapshot(snapshot: Snapshot): Snapshot {
  return {
    intents: [...snapshot.intents],
    attempts: [...snapshot.attempts],
    webhookEvents: [...snapshot.webhookEvents],
    reconciliationRuns: [...snapshot.reconciliationRuns],
    refunds: [...snapshot.refunds],
  };
}

function snapshotFromSessions(sessions: MonitorAdminSessions): Snapshot {
  return {
    intents: [...sessions.intents.getItems()],
    attempts: [...sessions.attempts.getItems()],
    webhookEvents: [...sessions.webhookEvents.getItems()],
    reconciliationRuns: [...sessions.reconciliationRuns.getItems()],
    refunds: [...sessions.refunds.getItems()],
  };
}

/**
 * Convert a readonly filter interface into a plain `Record<string, unknown>`
 * suitable for the paged session's `list()` call. Only truthy values are
 * included so the backend treats them as active filters.
 */
function filterToRecord(filter?: Record<string, unknown>): Record<string, unknown> {
  if (!filter) {
    return {};
  }
  const result: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(filter)) {
    if (value !== undefined && value !== null && value !== "") {
      result[key] = value;
    }
  }
  return result;
}

type ReloadTarget =
  | "intents"
  | "attempts"
  | "webhookEvents"
  | "reconciliationRuns"
  | "refunds"
  | "none";

// ---------------------------------------------------------------------------
// Controller factory
// ---------------------------------------------------------------------------

export function createPaymentMonitorAdminController(
  input: CreatePaymentMonitorAdminControllerInput,
): PaymentMonitorAdminController {
  const service = input.service;
  const listeners = new Set<() => void>();
  const sessions = createSessions(service);

  let state: PaymentMonitorAdminState = {
    ...EMPTY_SNAPSHOT,
    status: "idle",
  };

  function emit(): void {
    listeners.forEach((listener) => listener());
  }

  function setState(patch: Partial<PaymentMonitorAdminState>): void {
    const nextSnapshot: Snapshot = snapshotFromSessions(sessions);
    state = {
      ...state,
      ...patch,
      ...cloneSnapshot(nextSnapshot),
      listPageInfo: {
        intents: sessions.intents.getPageInfo(),
        attempts: sessions.attempts.getPageInfo(),
        webhookEvents: sessions.webhookEvents.getPageInfo(),
        reconciliationRuns: sessions.reconciliationRuns.getPageInfo(),
        refunds: sessions.refunds.getPageInfo(),
      },
    };
    emit();
  }

  function setStatus(status: PaymentMonitorAdminStatus, lastError?: string): void {
    state = { ...state, status, ...(lastError === undefined ? {} : { lastError }) };
    emit();
  }

  async function reload(target: ReloadTarget): Promise<void> {
    if (target === "intents") {
      await sessions.intents.list(filterToRecord({ ...sessions.intentFilter }));
    } else if (target === "attempts") {
      await sessions.attempts.list(filterToRecord({ ...sessions.attemptFilter }));
    } else if (target === "webhookEvents") {
      await sessions.webhookEvents.list(filterToRecord({ ...sessions.webhookEventFilter }));
    } else if (target === "reconciliationRuns") {
      await sessions.reconciliationRuns.list(
        filterToRecord({ ...sessions.reconciliationRunFilter }),
      );
    } else if (target === "refunds") {
      await sessions.refunds.list(filterToRecord({ ...sessions.refundFilter }));
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
      sessions.intents.reset();
      sessions.attempts.reset();
      sessions.webhookEvents.reset();
      sessions.reconciliationRuns.reset();
      sessions.refunds.reset();
      sessions.intentFilter = undefined;
      sessions.attemptFilter = undefined;
      sessions.webhookEventFilter = undefined;
      sessions.reconciliationRunFilter = undefined;
      sessions.refundFilter = undefined;
      try {
        await Promise.all([
          sessions.intents.list(),
          sessions.attempts.list(),
          sessions.webhookEvents.list(),
          sessions.reconciliationRuns.list(),
          sessions.refunds.list(),
        ]);
        setState({
          status: "ready",
          lastError: undefined,
          selectedIntentId: undefined,
          selectedIntentDetail: undefined,
          lastReplayResult: undefined,
          lastReconciliationRunId: undefined,
          lastRefundId: undefined,
        });
        return state;
      } catch (error) {
        setStatus(
          "error",
          error instanceof Error ? error.message : "Failed to load payment monitor data.",
        );
        throw error;
      }
    },

    async loadMoreIntents() {
      try {
        return await sessions.intents.loadMore();
      } finally {
        setState({});
      }
    },

    async refreshIntents() {
      setStatus("loading", undefined);
      try {
        await reload("intents");
        setState({ status: "ready", lastError: undefined });
        return sessions.intents.getItems();
      } catch (error) {
        setStatus(
          "error",
          error instanceof Error ? error.message : "Failed to refresh payment records.",
        );
        throw error;
      }
    },

    async loadMoreAttempts() {
      try {
        return await sessions.attempts.loadMore();
      } finally {
        setState({});
      }
    },

    async loadMoreWebhookEvents() {
      try {
        return await sessions.webhookEvents.loadMore();
      } finally {
        setState({});
      }
    },

    async loadMoreReconciliationRuns() {
      try {
        return await sessions.reconciliationRuns.loadMore();
      } finally {
        setState({});
      }
    },

    async loadMoreRefunds() {
      try {
        return await sessions.refunds.loadMore();
      } finally {
        setState({});
      }
    },

    async applyIntentFilter(filter) {
      sessions.intentFilter = filter;
      sessions.intents.reset();
      setStatus("loading", undefined);
      try {
        const items = await sessions.intents.list(filterToRecord({ ...filter }));
        setState({
          status: "ready",
          lastError: undefined,
          selectedIntentId: undefined,
          selectedIntentDetail: undefined,
        });
        return items;
      } catch (error) {
        setStatus(
          "error",
          error instanceof Error ? error.message : "Failed to apply intent filter.",
        );
        throw error;
      }
    },

    async applyAttemptFilter(filter) {
      sessions.attemptFilter = filter;
      sessions.attempts.reset();
      setStatus("loading", undefined);
      try {
        const items = await sessions.attempts.list(filterToRecord({ ...filter }));
        setState({ status: "ready", lastError: undefined });
        return items;
      } catch (error) {
        setStatus(
          "error",
          error instanceof Error ? error.message : "Failed to apply attempt filter.",
        );
        throw error;
      }
    },

    async applyWebhookEventFilter(filter) {
      sessions.webhookEventFilter = filter;
      sessions.webhookEvents.reset();
      setStatus("loading", undefined);
      try {
        const items = await sessions.webhookEvents.list(filterToRecord({ ...filter }));
        setState({ status: "ready", lastError: undefined });
        return items;
      } catch (error) {
        setStatus(
          "error",
          error instanceof Error ? error.message : "Failed to apply webhook event filter.",
        );
        throw error;
      }
    },

    async applyReconciliationRunFilter(filter) {
      sessions.reconciliationRunFilter = filter;
      sessions.reconciliationRuns.reset();
      setStatus("loading", undefined);
      try {
        const items = await sessions.reconciliationRuns.list(filterToRecord({ ...filter }));
        setState({ status: "ready", lastError: undefined });
        return items;
      } catch (error) {
        setStatus(
          "error",
          error instanceof Error ? error.message : "Failed to apply reconciliation run filter.",
        );
        throw error;
      }
    },

    async applyRefundFilter(filter) {
      sessions.refundFilter = filter;
      sessions.refunds.reset();
      setStatus("loading", undefined);
      try {
        const items = await sessions.refunds.list(filterToRecord({ ...filter }));
        setState({ status: "ready", lastError: undefined });
        return items;
      } catch (error) {
        setStatus(
          "error",
          error instanceof Error ? error.message : "Failed to apply refund filter.",
        );
        throw error;
      }
    },

    async selectIntent(id) {
      if (!id) {
        state = { ...state, selectedIntentId: undefined, selectedIntentDetail: undefined };
        emit();
        return undefined;
      }
      // Optimistically mark selected; the list view already holds the summary.
      const summary = state.intents.find((intent) => intent.id === id);
      state = {
        ...state,
        selectedIntentId: id,
        selectedIntentDetail: summary
          ? { ...summary, attempts: [], metadata: {} }
          : undefined,
      };
      emit();
      try {
        const response = await service.intents.retrieve(id);
        const item = extractSdkWorkResourceItem<unknown>(response);
        if (!summary) {
          // If we had no summary, try to map the retrieved payload as a base intent.
          const mapped = mapIntent(item);
          if (!mapped) {
            throw new Error("Failed to parse retrieved payment intent.");
          }
          const detail = mapIntentDetail(item, mapped);
          state = { ...state, selectedIntentId: id, selectedIntentDetail: detail };
          emit();
          return detail;
        }
        const detail = mapIntentDetail(item, summary);
        state = { ...state, selectedIntentId: id, selectedIntentDetail: detail };
        emit();
        return detail;
      } catch (error) {
        setStatus(
          "error",
          error instanceof Error ? error.message : "Failed to retrieve payment intent detail.",
        );
        throw error;
      }
    },

    async replayWebhookEvent(eventId) {
      return wrapMutation(
        async () => {
          const response = await service.webhookEvents.replay(eventId);
          // Replay returns SdkWorkCommandData envelope; extract and synthesize result.
          const record = asRecord(response);
          const data = asRecord(record.data ?? record);
          const result = mapReplayResult(data, eventId);
          if (!result) {
            throw new Error("Failed to parse webhook replay response.");
          }
          return result;
        },
        "Failed to replay webhook event.",
        { reload: "webhookEvents" },
      ).then((result) => {
        state = { ...state, lastReplayResult: result };
        emit();
        return result;
      });
    },

    async createReconciliationRun(draft) {
      return wrapMutation(
        async () => {
          const response = await service.reconciliationRuns.create(draft);
          const item = extractSdkWorkResourceItem<unknown>(response);
          const mapped = mapReconciliationRun(item);
          if (!mapped) {
            throw new Error("Failed to parse created reconciliation run.");
          }
          return mapped;
        },
        "Failed to create reconciliation run.",
        { reload: "reconciliationRuns" },
      ).then((mapped) => {
        state = { ...state, lastReconciliationRunId: mapped.id };
        emit();
        return mapped;
      });
    },

    async createRefund(draft: CreateRefundDraft) {
      return wrapMutation(
        async () => {
          const response = await service.refunds.create(draft, {
            idempotencyKey: paymentCommandIdempotencyKey("refund"),
          });
          const item = extractSdkWorkResourceItem<unknown>(response);
          const mapped = mapRefund(item);
          if (!mapped) {
            throw new Error("Failed to parse created refund.");
          }
          return mapped;
        },
        "Failed to create refund.",
        { reload: "refunds" },
      ).then((mapped) => {
        state = { ...state, lastRefundId: mapped.id };
        emit();
        return mapped;
      });
    },

    async retryRefund(refundId, confirmRefundNo) {
      await wrapMutation(
        async () => {
          await service.refunds.retry(
            refundId,
            { confirmRefundNo, expectedStatus: "failed" },
            { idempotencyKey: paymentCommandIdempotencyKey("refund-retry") },
          );
        },
        "Failed to retry refund.",
        { reload: "refunds" },
      );
    },
  };
}

function paymentCommandIdempotencyKey(prefix: string): string {
  return `${prefix}-${uuid()}`;
}
