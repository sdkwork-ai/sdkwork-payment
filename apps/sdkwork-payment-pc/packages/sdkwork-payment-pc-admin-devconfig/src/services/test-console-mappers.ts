/**
 * Wire → view mappers for the one-cent test console.
 *
 * The backend SDK returns typed payloads, but the console must stay resilient
 * to a deployment that runs an older payment build (missing fields must degrade
 * to an explicit `unknown`/`missing` state instead of throwing). Every mapper
 * therefore reads optional fields defensively and keeps the provider/product
 * vocabulary exactly as the backend sends it.
 */

import { asNumber, asString } from "@sdkwork/payment-pc-admin-core";
import type {
  PaymentTestLaunchKind,
  PaymentTestNotificationView,
  PaymentTestPaymentActionView,
  PaymentTestPaymentView,
  PaymentTestPayerField,
  PaymentTestProductView,
  PaymentTestProductsView,
  PaymentTestRefundView,
} from "../types/devconfig-admin-types";

const LAUNCH_KINDS: readonly PaymentTestLaunchKind[] = [
  "qr_code",
  "redirect",
  "form_redirect",
  "card",
  "jsapi",
  "app",
  "local_sandbox",
  "unsupported",
];

function asLaunchKind(value: unknown): PaymentTestLaunchKind {
  const text = asString(value);
  return LAUNCH_KINDS.includes(text as PaymentTestLaunchKind)
    ? (text as PaymentTestLaunchKind)
    : "unsupported";
}

function asStringArray(value: unknown): string[] {
  return Array.isArray(value)
    ? value.filter((item): item is string => typeof item === "string")
    : [];
}

function mapPayerFields(value: unknown): PaymentTestPayerField[] {
  if (!Array.isArray(value)) {
    return [];
  }
  return value.flatMap((item) => {
    const record = asRecord(item);
    const field = asString(record.field);
    if (!field) {
      return [];
    }
    return [
      {
        methodKey: asString(record.methodKey) ?? "",
        field,
        label: asString(record.label) ?? field,
      },
    ];
  });
}

function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}

function mapTestProduct(value: unknown): PaymentTestProductView | undefined {
  const record = asRecord(value);
  const methodKey = asString(record.methodKey);
  const providerCode = asString(record.providerCode);
  if (!methodKey || !providerCode) {
    return undefined;
  }
  const readiness = asStringArray(record.readiness);
  return {
    providerCode: providerCode as PaymentTestProductView["providerCode"],
    providerLabel: asString(record.providerLabel) ?? providerCode,
    methodKey,
    methodStatus: asString(record.methodStatus) ?? "missing",
    launchKind: asLaunchKind(record.launchKind),
    settleSupported: record.settleSupported === true,
    refundSupported: record.refundSupported === true,
    testEnabled: record.testEnabled === true,
    requiredPayerFields: mapPayerFields(record.requiredPayerFields),
    activeProviderAccounts: asNumber(record.activeProviderAccounts) ?? 0,
    activeChannelCount: asNumber(record.activeChannelCount) ?? 0,
    readiness: readiness.length > 0 ? readiness : ["unknown"],
    ready: record.ready === true,
  };
}

export function mapTestPaymentProducts(value: unknown): PaymentTestProductsView | undefined {
  const record = asRecord(value);
  if (!Array.isArray(record.products)) {
    return undefined;
  }
  const products = record.products
    .map(mapTestProduct)
    .filter((product): product is PaymentTestProductView => product !== undefined);
  return {
    products,
    unconfiguredProviders: asStringArray(record.unconfiguredProviders),
    readyCount: asNumber(record.readyCount) ?? products.filter((product) => product.ready).length,
    totalCount: asNumber(record.totalCount) ?? products.length,
    checkedAt: asString(record.checkedAt) ?? new Date().toISOString(),
  };
}

export function mapTestPayment(value: unknown): PaymentTestPaymentView | undefined {
  const record = asRecord(value);
  const paymentIntentId = asString(record.paymentIntentId);
  if (!paymentIntentId) {
    return undefined;
  }
  return {
    paymentId: asString(record.paymentId) ?? "",
    paymentIntentId,
    paymentIntentNo: asString(record.paymentIntentNo),
    attemptId: asString(record.attemptId) ?? "",
    outTradeNo: asString(record.outTradeNo) ?? "",
    methodKey: asString(record.methodKey) ?? "",
    providerCode: asString(record.providerCode) ?? "",
    amount: asString(record.amount) ?? "",
    currencyCode: asString(record.currencyCode) ?? "CNY",
    status: asString(record.status) ?? "unknown",
    qrCodeUrl: asString(record.qrCodeUrl),
    cashierUrl: asString(record.cashierUrl),
    payUrl: asString(record.payUrl),
    payForm: asString(record.payForm),
    clientSecret: asString(record.clientSecret),
    publishableKey: asString(record.publishableKey),
    launchKind: asLaunchKind(record.launchKind),
    launchPayload: asString(record.launchPayload),
    nextAction: asString(record.nextAction),
    settleSupported: record.settleSupported === true,
    providerParamKeys: asStringArray(record.providerParamKeys),
    requiredPayerFields: mapPayerFields(record.requiredPayerFields),
    expiresAt: asString(record.expiresAt),
    createdAt: asString(record.createdAt) ?? new Date().toISOString(),
  };
}

export function mapTestPaymentAction(value: unknown): PaymentTestPaymentActionView | undefined {
  const record = asRecord(value);
  const paymentIntentId = asString(record.paymentIntentId);
  if (!paymentIntentId) {
    return undefined;
  }
  return {
    paymentIntentId,
    attemptId: asString(record.attemptId) ?? "",
    outTradeNo: asString(record.outTradeNo),
    previousStatus: asString(record.previousStatus),
    localStatus: asString(record.localStatus) ?? "unknown",
    providerCode: asString(record.providerCode) ?? "",
    appliedStatus: asString(record.appliedStatus),
    webhookEventId: asString(record.webhookEventId),
    eventId: asString(record.eventId),
    providerStatus: asString(record.providerStatus),
    paid: typeof record.paid === "boolean" ? record.paid : undefined,
  };
}

export function mapTestRefund(value: unknown): PaymentTestRefundView | undefined {
  const record = asRecord(value);
  const refundId = asString(record.refundId);
  if (!refundId) {
    return undefined;
  }
  return {
    paymentIntentId: asString(record.paymentIntentId) ?? "",
    attemptId: asString(record.attemptId) ?? "",
    refundId,
    refundNo: asString(record.refundNo) ?? "",
    amount: asString(record.amount) ?? "",
    currencyCode: asString(record.currencyCode) ?? "CNY",
    status: asString(record.status) ?? "unknown",
    providerCode: asString(record.providerCode) ?? "",
    refunded: record.refunded === true,
    nextStep: asString(record.nextStep) ?? "",
  };
}

export function mapTestNotification(value: unknown): PaymentTestNotificationView | undefined {
  const record = asRecord(value);
  const providerAccountId = asString(record.providerAccountId);
  if (!providerAccountId) {
    return undefined;
  }
  const verification = asString(record.verification) === "verified" ? "verified" : "rejected";
  return {
    providerAccountId,
    providerCode: asString(record.providerCode) ?? "",
    verification,
    accepted: record.accepted === true,
    nativeSignature: asString(record.nativeSignature) ?? "unknown",
    documentSignature: asString(record.documentSignature) ?? "unknown",
    signatureHeader: asString(record.signatureHeader),
    providerEventId: asString(record.providerEventId),
    outTradeNo: asString(record.outTradeNo),
    webhookEventId: asString(record.webhookEventId),
    appliedStatus: asString(record.appliedStatus),
    paymentAttemptId: asString(record.paymentAttemptId),
    diagnostic: asString(record.diagnostic),
    deliveredAt: asString(record.deliveredAt) ?? new Date().toISOString(),
  };
}
