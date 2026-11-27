import { describe, expect, it, vi } from "vitest";
import type { SdkworkPaymentBackendService } from "@sdkwork/payment-service";

import { createPaymentDevConfigAdminController } from "../src/services/devconfig-admin-controller";
import { PAYMENT_PC_ADMIN_DEVCONFIG_ROUTES } from "../src/routes/devconfig-admin-routes";

function listResult() {
  return {
    items: [],
    pageInfo: { hasMore: false, mode: "offset", page: 1, pageSize: 20, totalItems: "0" },
  };
}

function createBackendService() {
  const providerAccounts = vi.fn().mockResolvedValue(listResult());
  const certificates = vi.fn().mockResolvedValue(listResult());
  const webhookEvents = vi.fn().mockResolvedValue(listResult());
  const service = {
    certificates: { list: certificates },
    providerAccounts: { list: providerAccounts },
    webhookEvents: { list: webhookEvents },
  } as unknown as SdkworkPaymentBackendService;
  return { certificates, providerAccounts, service, webhookEvents };
}

describe("createPaymentDevConfigAdminController", () => {
  it("loads only provider resources for the independent environment page", async () => {
    const backend = createBackendService();
    const controller = createPaymentDevConfigAdminController({ service: backend.service });

    await controller.load("environment");

    expect(backend.providerAccounts).toHaveBeenCalledOnce();
    expect(backend.certificates).not.toHaveBeenCalled();
    expect(backend.webhookEvents).not.toHaveBeenCalled();
  });

  it("loads provider context and recent events for the webhook debugger", async () => {
    const backend = createBackendService();
    const controller = createPaymentDevConfigAdminController({ service: backend.service });

    await controller.load("webhook");

    expect(backend.providerAccounts).toHaveBeenCalledOnce();
    expect(backend.webhookEvents).toHaveBeenCalledOnce();
    expect(backend.certificates).not.toHaveBeenCalled();
  });

  it("loads the provider account context for the one-cent test console", async () => {
    const backend = createBackendService();
    const controller = createPaymentDevConfigAdminController({ service: backend.service });

    await controller.load("testpayments");

    // The console renders products for configured accounts, so the account
    // list is part of its section load; certificates/logs are not.
    expect(backend.providerAccounts).toHaveBeenCalledOnce();
    expect(backend.certificates).not.toHaveBeenCalled();
    expect(backend.webhookEvents).not.toHaveBeenCalled();
  });
});

function createTestConsoleBackendService() {
  const list = vi.fn().mockResolvedValue({
    products: [
      {
        providerCode: "wechat_pay",
        providerLabel: "WeChat Pay",
        methodKey: "wechat_native",
        methodStatus: "active",
        launchKind: "qr_code",
        settleSupported: false,
        refundSupported: true,
        testEnabled: true,
        requiredPayerFields: [],
        activeProviderAccounts: 1,
        activeChannelCount: 1,
        readiness: ["ready"],
        ready: true,
      },
      {
        providerCode: "wechat_pay",
        providerLabel: "WeChat Pay",
        methodKey: "wechat_jsapi",
        methodStatus: "active",
        launchKind: "jsapi",
        settleSupported: true,
        refundSupported: true,
        testEnabled: true,
        requiredPayerFields: [
          { methodKey: "wechat_jsapi", field: "openid", label: "payer openid" },
        ],
        activeProviderAccounts: 1,
        activeChannelCount: 1,
        readiness: ["ready"],
        ready: true,
      },
    ],
    unconfiguredProviders: [],
    readyCount: 2,
    totalCount: 2,
    checkedAt: "2026-07-17T00:00:00.000Z",
  });
  const testPayments = vi.fn().mockResolvedValue({
    paymentId: "attempt-1",
    paymentIntentId: "intent-1",
    attemptId: "attempt-1",
    outTradeNo: "TP-1",
    methodKey: "wechat_native",
    providerCode: "wechat_pay",
    amount: "0.01",
    currencyCode: "CNY",
    status: "pending",
    launchKind: "qr_code",
    qrCodeUrl: "weixin://wxpay/bizpayurl?pr=1",
    settleSupported: false,
    providerParamKeys: ["qrCodeUrl", "nextAction"],
    requiredPayerFields: [],
    createdAt: "2026-07-17T00:00:00.000Z",
  });
  const checkAttemptStatus = vi.fn().mockResolvedValue({
    paymentIntentId: "intent-1",
    attemptId: "attempt-1",
    localStatus: "pending",
    paid: false,
  });
  const settleTestPayment = vi.fn().mockResolvedValue({
    paymentIntentId: "intent-1",
    attemptId: "attempt-1",
    outTradeNo: "TP-1",
    previousStatus: "pending",
    localStatus: "succeeded",
    providerCode: "wechat_pay",
    webhookEventId: "event-1",
  });
  const cancelTestPayment = vi.fn().mockResolvedValue({
    paymentIntentId: "intent-1",
    attemptId: "attempt-1",
    previousStatus: "pending",
    localStatus: "closed",
    providerCode: "wechat_pay",
  });
  const refundTestPayment = vi.fn().mockResolvedValue({
    paymentIntentId: "intent-1",
    attemptId: "attempt-1",
    refundId: "refund-1",
    refundNo: "RF-1",
    amount: "0.01",
    currencyCode: "CNY",
    status: "succeeded",
    providerCode: "wechat_pay",
    refunded: true,
    nextStep: "verify in the refund records",
  });
  const notificationTest = vi.fn().mockResolvedValue({
    providerAccountId: "account-1",
    providerCode: "wechat_pay",
    verification: "verified",
    accepted: true,
    nativeSignature: "verified",
    documentSignature: "verified",
    webhookEventId: "event-2",
    deliveredAt: "2026-07-17T00:00:00.000Z",
  });
  const service = {
    certificates: { list: vi.fn().mockResolvedValue(listResult()) },
    providerAccounts: { list: vi.fn().mockResolvedValue(listResult()) },
    webhookEvents: { list: vi.fn().mockResolvedValue(listResult()) },
    dev: {
      testPaymentProducts: { list },
      testPayments,
      checkAttemptStatus,
      settleTestPayment,
      cancelTestPayment,
      refundTestPayment,
      notificationTest,
    },
  } as unknown as SdkworkPaymentBackendService;
  return {
    service,
    list,
    testPayments,
    checkAttemptStatus,
    settleTestPayment,
    cancelTestPayment,
    refundTestPayment,
    notificationTest,
  };
}

describe("one-cent test console", () => {
  it("maps the coverage matrix and keeps findings verbatim", async () => {
    const backend = createTestConsoleBackendService();
    const controller = createPaymentDevConfigAdminController({ service: backend.service });

    const products = await controller.loadTestProducts();

    expect(backend.list).toHaveBeenCalledOnce();
    expect(products.totalCount).toBe(2);
    expect(products.products[0].launchKind).toBe("qr_code");
    expect(products.products[1].requiredPayerFields[0].field).toBe("openid");
    expect(controller.getState().testConsole.products).toEqual(products);
  });

  it("creates a one-cent test payment and refreshes the coverage", async () => {
    const backend = createTestConsoleBackendService();
    const controller = createPaymentDevConfigAdminController({ service: backend.service });

    const payment = await controller.createTestPayment({ methodKey: "wechat_native" });

    expect(backend.testPayments).toHaveBeenCalledWith({ methodKey: "wechat_native" });
    expect(payment.paymentIntentId).toBe("intent-1");
    expect(payment.launchKind).toBe("qr_code");
    expect(payment.settleSupported).toBe(false);
    // A checkout must not leave the console on a stale coverage board.
    expect(backend.list).toHaveBeenCalled();
    expect(controller.getState().testConsole.products?.readyCount).toBe(2);
  });

  it("keeps each console action's outcome side by side", async () => {
    const backend = createTestConsoleBackendService();
    const controller = createPaymentDevConfigAdminController({ service: backend.service });

    await controller.createTestPayment({ methodKey: "wechat_native" });
    await controller.checkTestPaymentStatus("intent-1");
    await controller.settleTestPayment("intent-1", "success");
    await controller.refundTestPayment("intent-1");
    await controller.testNotification({
      providerAccountId: "account-1",
      payload: "{\"paymentStatus\":\"succeeded\"}",
    });

    const console = controller.getState().testConsole;
    expect(backend.settleTestPayment).toHaveBeenCalledWith({
      paymentIntentId: "intent-1",
      outcome: "success",
    });
    expect(console.status?.paid).toBe(false);
    expect(console.settled?.localStatus).toBe("succeeded");
    expect(console.refund?.refunded).toBe(true);
    expect(console.notification?.accepted).toBe(true);
    // The payment itself is never overwritten by a later action's result.
    expect(console.payment?.paymentIntentId).toBe("intent-1");
  });

  it("surfaces a failing action without dropping the previous outcomes", async () => {
    const backend = createTestConsoleBackendService();
    backend.settleTestPayment.mockRejectedValueOnce(new Error("attempt is closed"));
    const controller = createPaymentDevConfigAdminController({ service: backend.service });

    await controller.createTestPayment({ methodKey: "wechat_native" });
    await expect(controller.settleTestPayment("intent-1", "success")).rejects.toThrow(
      "attempt is closed",
    );

    expect(controller.getState().status).toBe("error");
    expect(controller.getState().lastError).toBe("attempt is closed");
    expect(controller.getState().testConsole.payment?.paymentIntentId).toBe("intent-1");
  });
});

describe("PAYMENT_PC_ADMIN_DEVCONFIG_ROUTES", () => {
  it("publishes independently permissioned integration sections", () => {
    expect(
      Object.values(PAYMENT_PC_ADMIN_DEVCONFIG_ROUTES.sections).map(
        (section) => section.path,
      ),
    ).toEqual([
      "/admin/payments/devconfig/one-cent-test",
      "/admin/payments/devconfig/environments",
      "/admin/payments/devconfig/webhook-debugger",
      "/admin/payments/devconfig/certificates",
      "/admin/payments/devconfig/logs",
    ]);
    expect(
      PAYMENT_PC_ADMIN_DEVCONFIG_ROUTES.sections.testPayments.requiredPermissions,
    ).toContain("commerce.payments.dev.test_payments");
    expect(
      PAYMENT_PC_ADMIN_DEVCONFIG_ROUTES.sections.certificates
        .requiredPermissions,
    ).toEqual([
      "commerce.payments.certificates.read",
      "commerce.payments.certificates.create",
      "commerce.payments.certificates.delete",
    ]);
  });
});
