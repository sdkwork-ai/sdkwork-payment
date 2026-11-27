/**
 * Dev config admin controller.
 *
 * Stateful controller that consumes `SdkworkPaymentBackendService` (via the
 * port-adapter-service pattern from APP_SDK_INTEGRATION_SPEC.md §9). It owns:
 *   - Three paged list sessions (providerAccounts, certificates, webhookEvents)
 *   - Environment switching (providerAccounts.update with new environment)
 *   - Credential testing (providerAccounts.test)
 *   - Certificate CRUD (certificates.create + certificates.delete)
 *   - Webhook debugging (dev.sandboxTrigger + dev.webhookSignatureTest)
 *   - Webhook event replay (webhookEvents.replay)
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
  CreatePaymentDevConfigAdminControllerInput,
  PaymentCertificateDraft,
  PaymentCertificateView,
  PaymentDevConfigAdminController,
  PaymentDevConfigAdminSection,
  PaymentDevConfigAdminState,
  PaymentDevConfigStatus,
  PaymentDevSandboxTriggerDraft,
  PaymentDevSandboxTriggerResult,
  PaymentDevWebhookSignatureTestDraft,
  PaymentDevWebhookSignatureTestResult,
  PaymentProviderAccountTestResult,
  PaymentProviderAccountView,
  PaymentTestNotificationDraft,
  PaymentTestNotificationView,
  PaymentTestPaymentActionView,
  PaymentTestPaymentDraft,
  PaymentTestPaymentView,
  PaymentTestPayerField,
  PaymentTestProductView,
  PaymentTestProductsView,
  PaymentTestRefundView,
  PaymentWebhookEventListFilter,
  PaymentWebhookEventView,
  PaymentWebhookReplayResult,
} from "../types/devconfig-admin-types";
import {
  mapTestNotification,
  mapTestPayment,
  mapTestPaymentAction,
  mapTestPaymentProducts,
  mapTestRefund,
} from "./test-console-mappers";

type Snapshot = Pick<
  PaymentDevConfigAdminState,
  "providerAccounts" | "certificates" | "webhookEvents"
>;

const EMPTY_SNAPSHOT: Snapshot = {
  providerAccounts: [],
  certificates: [],
  webhookEvents: [],
};

function cloneSnapshot(snapshot: Snapshot): Snapshot {
  return {
    providerAccounts: [...snapshot.providerAccounts],
    certificates: [...snapshot.certificates],
    webhookEvents: [...snapshot.webhookEvents],
  };
}

const PROVIDER_CODES = ["stripe", "alipay", "wechat_pay", "paypal", "sandbox"] as const;
const ACCOUNT_MODES = ["direct", "partner"] as const;
const ENVIRONMENTS = ["development", "sandbox", "production"] as const;
const PROVIDER_ACCOUNT_STATUSES = ["active", "inactive", "suspended", "deprecated"] as const;
const LAST_TEST_STATUSES = ["success", "failure", "unknown"] as const;
const CERTIFICATE_KINDS = ["merchant_private_key", "provider_public_key", "platform_certificate", "webhook_secret"] as const;
const CERTIFICATE_STATUSES = ["active", "expired", "revoked", "pending_rotation"] as const;
const WEBHOOK_EVENT_STATUSES = ["queued", "processing", "processed", "failed", "dead"] as const;
const WEBHOOK_SIGNATURE_STATUSES = ["valid", "invalid", "unverified", "unknown"] as const;

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
    partnerProviderAccountId: asString(record.partnerProviderAccountId),
    environment: asStatus(record.environment, ENVIRONMENTS, "production"),
    countryCode: asString(record.countryCode),
    settlementCurrency: asString(record.settlementCurrency) ?? "CNY",
    hasPrimarySecret: record.hasPrimarySecret === true,
    hasWebhookSecret: record.hasWebhookSecret === true,
    hasCertificate: record.hasCertificate === true,
    credentialStorage: asStatus(record.credentialStorage, ["database_encrypted", "legacy_reference", "none"] as const, "none"),
    status: asStatus(record.status, PROVIDER_ACCOUNT_STATUSES, "active"),
    metadata: asRecord(record.metadata),
    certificateExpiresAt: asString(record.certificateExpiresAt),
    lastTestedAt: asString(record.lastTestedAt),
    lastTestStatus: asStatus(record.lastTestStatus, LAST_TEST_STATUSES, "unknown"),
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
    pspResponseTimeMs: asNumber(record.pspResponseTimeMs ?? record.psp_response_time_ms),
    diagnostic: asString(record.diagnostic),
    testedAt: asString(record.testedAt ?? record.tested_at) ?? new Date().toISOString(),
  };
}

function mapCertificate(value: unknown): PaymentCertificateView | undefined {
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
    certificateNo: asRequiredString(record.certificateNo, id),
    providerCode: asString(record.providerCode ?? record.provider_code) as PaymentCertificateView["providerCode"],
    certificateType: asStatus(
      record.certificateType ?? record.certificate_type ?? record.kind,
      CERTIFICATE_KINDS,
      "merchant_private_key",
    ),
    hasContent: record.hasContent === true,
    credentialStorage: asStatus(record.credentialStorage, ["database_encrypted", "legacy_reference"] as const, "legacy_reference"),
    subject: asString(record.subject ?? record.subjectCn ?? record.subject_cn),
    issuer: asString(record.issuer ?? record.issuerCn ?? record.issuer_cn),
    fingerprint: asString(record.fingerprint ?? record.fingerprintSha256 ?? record.fingerprint_sha256),
    expiresAt: asString(record.expiresAt ?? record.expires_at ?? record.validUntil ?? record.valid_until),
    status: asStatus(record.status, CERTIFICATE_STATUSES, "active"),
    metadata: asRecord(record.metadata),
    createdAt: asString(record.createdAt) ?? new Date(0).toISOString(),
    updatedAt: asString(record.updatedAt) ?? new Date(0).toISOString(),
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
  // payload: backend may return `payload` or `requestBody` (camelCase/snake_case)
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
    providerCode: asStatus(record.providerCode ?? record.provider_code, PROVIDER_CODES, "sandbox"),
    eventId: asString(record.eventId ?? record.event_id),
    eventType: asRequiredString(record.eventType ?? record.event_type, id),
    receivedAt: asString(record.receivedAt ?? record.received_at) ?? new Date(0).toISOString(),
    processedAt: asString(record.processedAt ?? record.processed_at),
    status: asStatus(record.status, WEBHOOK_EVENT_STATUSES, "queued"),
    retries: asNumber(record.retries ?? record.replayCount ?? record.replay_count) ?? 0,
    lastError: asString(record.lastError ?? record.last_error ?? record.lastReplayError ?? record.last_replay_error),
    signatureStatus: asStatus(
      record.signatureStatus ?? record.signature_status,
      WEBHOOK_SIGNATURE_STATUSES,
      "unverified",
    ),
    payload,
    headers,
  };
}

function mapSandboxTriggerResult(value: unknown): PaymentDevSandboxTriggerResult | undefined {
  if (!value || typeof value !== "object") {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  if (typeof record.ok !== "boolean") {
    return undefined;
  }
  return {
    ok: record.ok,
    operationId: asString(record.operationId ?? record.operation_id),
    status: asString(record.status),
    pollUrl: asString(record.pollUrl ?? record.poll_url),
    providerCode: asStatus(record.providerCode ?? record.provider_code, PROVIDER_CODES, "sandbox"),
    environment: asStatus(record.environment, ENVIRONMENTS, "production"),
    diagnostic: asString(record.diagnostic),
    triggeredAt: asString(record.triggeredAt ?? record.triggered_at) ?? new Date().toISOString(),
  };
}

function mapSignatureTestResult(value: unknown): PaymentDevWebhookSignatureTestResult | undefined {
  if (!value || typeof value !== "object") {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  if (typeof record.ok !== "boolean") {
    return undefined;
  }
  return {
    ok: record.ok,
    providerCode: asStatus(record.providerCode ?? record.provider_code, PROVIDER_CODES, "sandbox"),
    algorithm: asString(record.algorithm),
    diagnostic: asString(record.diagnostic),
    testedAt: asString(record.testedAt ?? record.tested_at) ?? new Date().toISOString(),
  };
}

function mapReplayResult(value: unknown, eventId: string): PaymentWebhookReplayResult | undefined {
  if (!value || typeof value !== "object") {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  if (typeof record.ok !== "boolean") {
    return undefined;
  }
  return {
    ok: record.ok,
    eventId: asString(record.eventId ?? record.event_id) ?? eventId,
    replayedAt: asString(record.replayedAt ?? record.replayed_at) ?? new Date().toISOString(),
    diagnostic: asString(record.diagnostic),
  };
}

interface DevConfigAdminSessions {
  providerAccounts: SdkWorkPagedListSession<PaymentProviderAccountView>;
  certificates: SdkWorkPagedListSession<PaymentCertificateView>;
  webhookEvents: SdkWorkPagedListSession<PaymentWebhookEventView>;
  webhookEventsFilter?: PaymentWebhookEventListFilter;
}

function createSessions(service: SdkworkPaymentBackendService): DevConfigAdminSessions {
  return {
    providerAccounts: createSdkWorkPagedListSession<PaymentProviderAccountView>({
      fetchPage: (query) => service.providerAccounts.list(query),
      mapItem: mapProviderAccount,
    }),
    certificates: createSdkWorkPagedListSession<PaymentCertificateView>({
      fetchPage: (query) => service.certificates.list(query),
      mapItem: mapCertificate,
    }),
    webhookEvents: createSdkWorkPagedListSession<PaymentWebhookEventView>({
      fetchPage: (query) => service.webhookEvents.list(query),
      mapItem: mapWebhookEvent,
    }),
  };
}

function snapshotFromSessions(sessions: DevConfigAdminSessions): Snapshot {
  return {
    providerAccounts: [...sessions.providerAccounts.getItems()],
    certificates: [...sessions.certificates.getItems()],
    webhookEvents: [...sessions.webhookEvents.getItems()],
  };
}

export function createPaymentDevConfigAdminController(
  input: CreatePaymentDevConfigAdminControllerInput,
): PaymentDevConfigAdminController {
  const service = input.service;
  const listeners = new Set<() => void>();
  const sessions = createSessions(service);

  let state: PaymentDevConfigAdminState = {
    ...EMPTY_SNAPSHOT,
    status: "idle",
    testConsole: {},
  };

  function emit(): void {
    listeners.forEach((listener) => listener());
  }

  function setState(patch: Partial<PaymentDevConfigAdminState>): void {
    const nextSnapshot: Snapshot = snapshotFromSessions(sessions);
    state = {
      ...state,
      ...patch,
      ...cloneSnapshot(nextSnapshot),
    };
    emit();
  }

  function setStatus(status: PaymentDevConfigStatus, lastError?: string): void {
    state = { ...state, status, ...(lastError === undefined ? {} : { lastError }) };
    emit();
  }

  async function wrapMutation<T>(
    action: () => Promise<T>,
    errorMessage: string,
    options: { reload: "providerAccounts" | "certificates" | "webhookEvents" | "none" } = { reload: "none" },
  ): Promise<T> {
    setStatus("saving", undefined);
    try {
      const result = await action();
      if (options.reload === "providerAccounts") {
        await sessions.providerAccounts.list();
      } else if (options.reload === "certificates") {
        await sessions.certificates.list();
      } else if (options.reload === "webhookEvents") {
        await sessions.webhookEvents.list();
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

    async load(section?: PaymentDevConfigAdminSection) {
      setStatus("loading", undefined);
      const loadProviderAccounts =
        !section ||
        section === "environment" ||
        section === "webhook" ||
        section === "testpayments";
      const loadCertificates = !section || section === "certificates";
      const loadWebhookEvents = !section || section === "webhook" || section === "logs";
      if (loadProviderAccounts) {
        sessions.providerAccounts.reset();
      }
      if (loadCertificates) {
        sessions.certificates.reset();
      }
      if (loadWebhookEvents) {
        sessions.webhookEvents.reset();
        sessions.webhookEventsFilter = undefined;
      }
      try {
        await Promise.all([
          ...(loadProviderAccounts ? [sessions.providerAccounts.list()] : []),
          ...(loadCertificates ? [sessions.certificates.list()] : []),
          ...(loadWebhookEvents ? [sessions.webhookEvents.list()] : []),
        ]);
        setState({
          status: "ready",
          lastError: undefined,
          selectedProviderAccountId: undefined,
          selectedCertificateId: undefined,
        });
        return state;
      } catch (error) {
        setStatus("error", error instanceof Error ? error.message : "Failed to load dev config data.");
        throw error;
      }
    },

    async loadMoreCertificates() {
      try {
        return await sessions.certificates.loadMore();
      } finally {
        setState({});
      }
    },

    async loadMoreWebhookEvents(filter) {
      const filterParams: Record<string, unknown> = filter ? { ...filter } : {};
      const hasFilter = Object.keys(filterParams).length > 0;
      const changed =
        hasFilter &&
        (!sessions.webhookEventsFilter ||
          JSON.stringify(filter) !== JSON.stringify(sessions.webhookEventsFilter));
      if (hasFilter && changed) {
        sessions.webhookEvents.reset();
        sessions.webhookEventsFilter = filter;
        const items = await sessions.webhookEvents.list(filterParams);
        setState({});
        return items;
      }
      const items = await sessions.webhookEvents.loadMore();
      setState({});
      return items;
    },

    async loadMoreProviderAccounts() {
      try {
        return await sessions.providerAccounts.loadMore();
      } finally {
        setState({});
      }
    },

    selectProviderAccount(id) {
      const next = id
        ? state.providerAccounts.find((account) => account.id === id)
        : undefined;
      state = { ...state, selectedProviderAccountId: next?.id };
      emit();
      return next;
    },

    selectCertificate(id) {
      const next = id
        ? state.certificates.find((certificate) => certificate.id === id)
        : undefined;
      state = { ...state, selectedCertificateId: next?.id };
      emit();
      return next;
    },

    async switchProviderAccountEnvironment(id, environment) {
      return wrapMutation(
        async () => {
          const response = await service.providerAccounts.update(id, { environment });
          const item = extractSdkWorkResourceItem<unknown>(response);
          const mapped = mapProviderAccount(item);
          if (!mapped) {
            throw new Error("Failed to parse provider account after environment switch.");
          }
          return mapped;
        },
        "Failed to switch provider account environment.",
        { reload: "providerAccounts" },
      );
    },

    async testProviderAccount(id, options) {
      setStatus("testing", undefined);
      try {
        const response = await service.providerAccounts.test(id, options ?? {});
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

    async createCertificate(draft) {
      return wrapMutation(
        async () => {
          const response = await service.certificates.create(draft);
          const item = extractSdkWorkResourceItem<unknown>(response);
          const mapped = mapCertificate(item);
          if (!mapped) {
            throw new Error("Failed to parse created certificate.");
          }
          return mapped;
        },
        "Failed to create certificate.",
        { reload: "certificates" },
      );
    },

    async deleteCertificate(id) {
      return wrapMutation(
        async () => {
          await service.certificates.delete(id);
        },
        "Failed to delete certificate.",
        { reload: "certificates" },
      );
    },

    async triggerSandboxEvent(draft: PaymentDevSandboxTriggerDraft) {
      setStatus("testing", undefined);
      try {
        const response = await service.dev.sandboxTrigger(draft);
        const item = extractSdkWorkResourceItem<unknown>(response);
        const mapped = mapSandboxTriggerResult(item);
        if (!mapped) {
          throw new Error("Failed to parse sandbox trigger result.");
        }
        // Reload webhook events to surface the newly simulated event.
        await sessions.webhookEvents.list();
        setState({
          status: "ready",
          lastError: undefined,
          lastSandboxTriggerResult: mapped,
        });
        return mapped;
      } catch (error) {
        setStatus("error", error instanceof Error ? error.message : "Failed to trigger sandbox event.");
        throw error;
      }
    },

    async testWebhookSignature(draft: PaymentDevWebhookSignatureTestDraft) {
      setStatus("testing", undefined);
      try {
        const response = await service.dev.webhookSignatureTest(draft);
        const item = extractSdkWorkResourceItem<unknown>(response);
        const mapped = mapSignatureTestResult(item);
        if (!mapped) {
          throw new Error("Failed to parse webhook signature test result.");
        }
        setState({
          status: "ready",
          lastError: undefined,
          lastSignatureTestResult: mapped,
        });
        return mapped;
      } catch (error) {
        setStatus("error", error instanceof Error ? error.message : "Failed to test webhook signature.");
        throw error;
      }
    },

    async replayWebhookEvent(eventId) {
      setStatus("testing", undefined);
      try {
        const response = await service.webhookEvents.replay(eventId);
        const item = extractSdkWorkResourceItem<unknown>(response);
        const mapped = mapReplayResult(item, eventId);
        if (!mapped) {
          throw new Error("Failed to parse webhook replay result.");
        }
        // Reload webhook events to reflect replay count + last replay time.
        await sessions.webhookEvents.list();
        setState({
          status: "ready",
          lastError: undefined,
          lastReplayResult: mapped,
        });
        return mapped;
      } catch (error) {
        setStatus("error", error instanceof Error ? error.message : "Failed to replay webhook event.");
        throw error;
      }
    },

    // -----------------------------------------------------------------
    // One-cent test console
    // -----------------------------------------------------------------

    async loadTestProducts() {
      setStatus("loading", undefined);
      try {
        const response = await service.dev.testPaymentProducts.list();
        const mapped = mapTestPaymentProducts(
          extractSdkWorkResourceItem<unknown>(response),
        );
        if (!mapped) {
          throw new Error("Failed to parse one-cent test coverage.");
        }
        setState({
          status: "ready",
          lastError: undefined,
          testConsole: { ...state.testConsole, products: mapped },
        });
        return mapped;
      } catch (error) {
        setStatus(
          "error",
          error instanceof Error ? error.message : "Failed to load one-cent test coverage.",
        );
        throw error;
      }
    },

    async createTestPayment(draft: PaymentTestPaymentDraft) {
      setStatus("testing", undefined);
      try {
        const response = await service.dev.testPayments(draft);
        const mapped = mapTestPayment(extractSdkWorkResourceItem<unknown>(response));
        if (!mapped) {
          throw new Error("Failed to parse the one-cent test payment.");
        }
        // A checkout either resolves a launch surface or reports
        // `unsupported`; refresh the coverage matrix so a channel that was
        // just configured shows up as ready without another click.
        const products = await service.dev.testPaymentProducts
          .list()
          .then((value) => mapTestPaymentProducts(extractSdkWorkResourceItem<unknown>(value)))
          .catch(() => undefined);
        setState({
          status: "ready",
          lastError: undefined,
          testConsole: {
            ...state.testConsole,
            payment: mapped,
            ...(products ? { products } : {}),
          },
        });
        return mapped;
      } catch (error) {
        setStatus(
          "error",
          error instanceof Error ? error.message : "Failed to create the one-cent test payment.",
        );
        throw error;
      }
    },

    async checkTestPaymentStatus(paymentIntentId: string) {
      setStatus("testing", undefined);
      try {
        const response = await service.dev.checkAttemptStatus({
          paymentIntentId,
          settleIfUnpaid: false,
        });
        const mapped = mapTestPaymentAction(extractSdkWorkResourceItem<unknown>(response));
        if (!mapped) {
          throw new Error("Failed to parse the test payment status.");
        }
        setState({
          status: "ready",
          lastError: undefined,
          testConsole: { ...state.testConsole, status: mapped },
        });
        return mapped;
      } catch (error) {
        setStatus(
          "error",
          error instanceof Error ? error.message : "Failed to check the test payment status.",
        );
        throw error;
      }
    },

    async settleTestPayment(paymentIntentId: string, outcome: "success" | "failed" = "success") {
      setStatus("testing", undefined);
      try {
        const response = await service.dev.settleTestPayment({
          paymentIntentId,
          outcome,
        });
        const mapped = mapTestPaymentAction(extractSdkWorkResourceItem<unknown>(response));
        if (!mapped) {
          throw new Error("Failed to parse the simulated settlement.");
        }
        setState({
          status: "ready",
          lastError: undefined,
          testConsole: { ...state.testConsole, settled: mapped },
        });
        return mapped;
      } catch (error) {
        setStatus(
          "error",
          error instanceof Error ? error.message : "Failed to settle the test payment.",
        );
        throw error;
      }
    },

    async cancelTestPayment(paymentIntentId: string) {
      setStatus("testing", undefined);
      try {
        const response = await service.dev.cancelTestPayment({ paymentIntentId });
        const mapped = mapTestPaymentAction(extractSdkWorkResourceItem<unknown>(response));
        if (!mapped) {
          throw new Error("Failed to parse the cancelled test payment.");
        }
        setState({
          status: "ready",
          lastError: undefined,
          testConsole: { ...state.testConsole, cancelled: mapped },
        });
        return mapped;
      } catch (error) {
        setStatus(
          "error",
          error instanceof Error ? error.message : "Failed to cancel the test payment.",
        );
        throw error;
      }
    },

    async refundTestPayment(
      paymentIntentId: string,
      options?: { amount?: string; reasonCode?: string },
    ) {
      setStatus("testing", undefined);
      try {
        const response = await service.dev.refundTestPayment({
          paymentIntentId,
          ...(options?.amount ? { amount: options.amount } : {}),
          ...(options?.reasonCode ? { reasonCode: options.reasonCode } : {}),
        });
        const mapped = mapTestRefund(extractSdkWorkResourceItem<unknown>(response));
        if (!mapped) {
          throw new Error("Failed to parse the test refund.");
        }
        setState({
          status: "ready",
          lastError: undefined,
          testConsole: { ...state.testConsole, refund: mapped },
        });
        return mapped;
      } catch (error) {
        setStatus(
          "error",
          error instanceof Error ? error.message : "Failed to refund the test payment.",
        );
        throw error;
      }
    },

    async testNotification(draft: PaymentTestNotificationDraft) {
      setStatus("testing", undefined);
      try {
        const response = await service.dev.notificationTest(draft);
        const mapped = mapTestNotification(extractSdkWorkResourceItem<unknown>(response));
        if (!mapped) {
          throw new Error("Failed to parse the notification delivery result.");
        }
        // A delivered notification is a webhook event: reload the log so the
        // verification row and the integration log never disagree.
        await sessions.webhookEvents.list().catch(() => undefined);
        setState({
          status: "ready",
          lastError: undefined,
          testConsole: { ...state.testConsole, notification: mapped },
        });
        return mapped;
      } catch (error) {
        setStatus(
          "error",
          error instanceof Error ? error.message : "Failed to deliver the test notification.",
        );
        throw error;
      }
    },
  };
}
