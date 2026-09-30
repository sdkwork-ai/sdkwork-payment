import { describe, expect, it, vi } from "vitest";
import type { SdkworkPaymentBackendService } from "@sdkwork/payment-service";

import { createPaymentMonitorAdminController } from "../src/services/monitor-admin-controller";

function createBackendService(intentsList: ReturnType<typeof vi.fn>): SdkworkPaymentBackendService {
  const emptyList = vi.fn().mockResolvedValue({
    items: [],
    pageInfo: { hasMore: false, mode: "cursor", totalItems: "0" },
  });
  return {
    attempts: { list: emptyList },
    intents: {
      list: intentsList,
      retrieve: vi.fn(),
    },
    reconciliationRuns: {
      create: vi.fn(),
      list: emptyList,
    },
    refunds: {
      create: vi.fn(),
      list: emptyList,
      retrieve: vi.fn(),
      retry: vi.fn(),
    },
    webhookEvents: {
      list: emptyList,
      replay: vi.fn(),
    },
  } as unknown as SdkworkPaymentBackendService;
}

describe("createPaymentMonitorAdminController", () => {
  it("refreshes payment records through the current server-side filter", async () => {
    const intentsList = vi.fn().mockResolvedValue({
      items: [{
        amount: "9.99",
        createdAt: "2026-07-19T00:00:00.000Z",
        currencyCode: "CNY",
        id: "intent-failed",
        orderId: "order-1",
        ownerUserId: "user-1",
        paymentIntentNo: "pay-failed",
        paymentMethod: "sandbox_test",
        providerCode: "sandbox",
        status: "failed",
        updatedAt: "2026-07-19T00:01:00.000Z",
      }],
      pageInfo: { hasMore: false, mode: "cursor", totalItems: "1" },
    });
    const controller = createPaymentMonitorAdminController({
      service: createBackendService(intentsList),
    });

    await controller.applyIntentFilter({ providerCode: "sandbox", status: "failed" });
    await controller.refreshIntents();

    expect(intentsList).toHaveBeenCalledTimes(2);
    expect(intentsList).toHaveBeenLastCalledWith(expect.objectContaining({
      pageSize: 20,
      providerCode: "sandbox",
      status: "failed",
    }));
    expect(controller.getState().intents).toHaveLength(1);
    expect(controller.getState().status).toBe("ready");
  });

  it("keeps payment records returned by a rolling-upgrade backend id alias", async () => {
    const intentsList = vi.fn().mockResolvedValue({
      items: [{
        amount: "19.90",
        createdAt: "2026-07-20T00:00:00.000Z",
        currencyCode: "CNY",
        orderId: "order-legacy-1",
        ownerUserId: "user-1",
        paymentIntentId: "intent-legacy-1",
        paymentIntentNo: "PI-LEGACY-1",
        paymentMethod: "sandbox_test",
        providerCode: "sandbox",
        status: "succeeded",
        updatedAt: "2026-07-20T00:01:00.000Z",
      }],
      pageInfo: { hasMore: false, mode: "offset", totalItems: "1" },
    });
    const controller = createPaymentMonitorAdminController({
      service: createBackendService(intentsList),
    });

    await controller.load();

    expect(controller.getState().intents).toEqual([
      expect.objectContaining({ id: "intent-legacy-1" }),
    ]);
  });

  it("creates and retries refunds through the typed backend service with idempotency", async () => {
    const intentsList = vi.fn().mockResolvedValue({
      items: [],
      pageInfo: { hasMore: false, mode: "offset", totalItems: "0" },
    });
    const service = createBackendService(intentsList);
    const refunds = service.refunds as unknown as {
      create: ReturnType<typeof vi.fn>;
      list: ReturnType<typeof vi.fn>;
      retry: ReturnType<typeof vi.fn>;
    };
    refunds.create.mockResolvedValue({
      amount: "9.99",
      createdAt: "2026-07-20T00:00:00.000Z",
      currencyCode: "CNY",
      id: "refund-1",
      orderId: "order-1",
      paymentAttemptId: "attempt-1",
      paymentIntentId: "intent-1",
      providerAccountId: "account-1",
      providerCode: "sandbox",
      reasonCode: "customer_request",
      refundNo: "RF-1",
      requestedBy: "operator-1",
      requestedByType: "operator",
      status: "processing",
      updatedAt: "2026-07-20T00:00:00.000Z",
    });
    refunds.retry.mockResolvedValue({ accepted: true, resourceId: "refund-1" });
    const controller = createPaymentMonitorAdminController({ service });

    await controller.createRefund({
      confirmPaymentIntentNo: "PI-1",
      paymentIntentId: "intent-1",
      reasonCode: "customer_request",
    });
    await controller.retryRefund("refund-1", "RF-1");

    expect(refunds.create).toHaveBeenCalledWith(
      expect.objectContaining({ paymentIntentId: "intent-1", confirmPaymentIntentNo: "PI-1" }),
      expect.objectContaining({ idempotencyKey: expect.stringMatching(/^refund-/) }),
    );
    expect(refunds.retry).toHaveBeenCalledWith(
      "refund-1",
      { confirmRefundNo: "RF-1", expectedStatus: "failed" },
      expect.objectContaining({ idempotencyKey: expect.stringMatching(/^refund-retry-/) }),
    );
  });
});
