import { describe, expect, it, vi } from "vitest";
import {
  createSdkworkPaymentController,
  type CreateSdkworkPaymentControllerOptions,
  type SdkworkPaymentMessagesOverrides,
} from "../src";

describe("sdkwork-payment-pc-payment controller", () => {
  it("bootstraps payment state, filters records, opens detail, and refreshes after create and status actions", async () => {
    const firstDashboard = {
      clientType: "WEB" as const,
      digest: {
        actionablePayments: 1,
        closedCount: 0,
        failedCount: 0,
        successfulPayments: 1,
        timedOutPayments: 0,
        totalAmountCny: 998,
        totalCount: 2,
      },
      methods: [
        {
          available: true,
          code: "WECHAT_PAY" as const,
          id: "wechat-pay",
          label: "WeChat Pay",
          productTypes: [
            {
              available: true,
              code: "native" as const,
              label: "Native",
            },
          ],
          recommendedProductType: "native" as const,
          sort: 100,
        },
        {
          available: true,
          code: "ALIPAY" as const,
          id: "alipay",
          label: "Alipay",
          productTypes: [
            {
              available: true,
              code: "pc" as const,
              label: "PC Web",
            },
          ],
          recommendedProductType: "pc" as const,
          sort: 90,
        },
      ],
      records: [
        {
          amountCny: 699,
          createdAt: "2026-04-03T10:05:00.000Z",
          id: "1001",
          orderId: "ORDER-9",
          outTradeNo: "OUT-ORDER-9",
          paymentMethod: "WECHAT_PAY" as const,
          paymentProvider: "WECHAT_PAY" as const,
          paymentSn: "PAY-1001",
          status: "pending" as const,
          statusLabel: "Pending",
        },
        {
          amountCny: 299,
          createdAt: "2026-04-02T09:00:00.000Z",
          id: "1000",
          orderId: "ORDER-8",
          outTradeNo: "OUT-ORDER-8",
          paymentMethod: "ALIPAY" as const,
          paymentProvider: "ALIPAY" as const,
          paymentSn: "PAY-1000",
          status: "success" as const,
          statusLabel: "Success",
        },
      ],
      statistics: {
        closedCount: 0,
        failedCount: 0,
        pendingCount: 1,
        succeededCount: 1,
        timeoutCount: 0,
        totalCount: 2,
      },
    };
    const secondDashboard = {
      ...firstDashboard,
      digest: {
        actionablePayments: 1,
        closedCount: 0,
        failedCount: 0,
        successfulPayments: 2,
        timedOutPayments: 0,
        totalAmountCny: 1697,
        totalCount: 3,
      },
      records: [
        {
          amountCny: 699,
          createdAt: "2026-04-03T10:10:00.000Z",
          id: "1002",
          orderId: "ORDER-11",
          outTradeNo: "OUT-ORDER-11",
          paymentMethod: "WECHAT_PAY" as const,
          paymentProvider: "WECHAT_PAY" as const,
          paymentSn: "PAY-1002",
          status: "success" as const,
          statusLabel: "Success",
        },
        ...firstDashboard.records,
      ],
      statistics: {
        closedCount: 0,
        failedCount: 0,
        pendingCount: 0,
        succeededCount: 2,
        timeoutCount: 0,
        totalCount: 3,
      },
    };
    const createdPayment = {
      amountCny: 699,
      createdAt: "2026-04-03T10:10:00.000Z",
      id: "1002",
      needQuery: true,
      orderId: "ORDER-11",
      outTradeNo: "OUT-ORDER-11",
      paymentMethod: "WECHAT_PAY" as const,
      paymentProvider: "WECHAT_PAY" as const,
      paymentSn: "PAY-1002",
      paymentUrl: "https://pay.sdkwork.ai/wechat/1002",
      productType: "native" as const,
      qrContent: "weixin://wxpay/bizpayurl?pr=PAY1002",
      queryIntervalSeconds: 3,
      status: "pending" as const,
      statusLabel: "Pending",
      subject: "Pro Annual",
    };
    const paymentDetail = {
      ...createdPayment,
      relatedPayments: undefined,
      remark: "Scan to pay",
    };
    const statusAfterRefresh = {
      amountCny: 699,
      createdAt: "2026-04-03T10:10:00.000Z",
      id: "1002",
      orderId: "ORDER-11",
      outTradeNo: "OUT-ORDER-11",
      paymentMethod: "WECHAT_PAY" as const,
      paymentProvider: "WECHAT_PAY" as const,
      paymentSn: "PAY-1002",
      status: "success" as const,
      statusLabel: "Success",
      successTime: "2026-04-03T10:12:00.000Z",
      transactionId: "TXN-1002",
    };
    const relatedPayments = [
      statusAfterRefresh,
      {
        amountCny: 699,
        createdAt: "2026-04-03T10:00:00.000Z",
        id: "1001",
        orderId: "ORDER-11",
        outTradeNo: "OUT-ORDER-11-RETRY",
        paymentMethod: "ALIPAY" as const,
        paymentProvider: "ALIPAY" as const,
        paymentSn: "PAY-1001",
        status: "failed" as const,
        statusLabel: "Failed",
      },
    ];
    const service = {
      closePayment: vi.fn().mockResolvedValue({
        closed: true,
        paymentId: "1002",
      }),
      createPayment: vi.fn().mockResolvedValue(createdPayment),
      getDashboard: vi
        .fn()
        .mockResolvedValueOnce(firstDashboard)
        .mockResolvedValueOnce(secondDashboard)
        .mockResolvedValueOnce(secondDashboard),
      getEmptyDashboard: vi.fn().mockReturnValue({
        clientType: "WEB",
        digest: {
          actionablePayments: 0,
          closedCount: 0,
          failedCount: 0,
          successfulPayments: 0,
          timedOutPayments: 0,
          totalAmountCny: 0,
          totalCount: 0,
        },
        methods: [],
        records: [],
        statistics: {
          closedCount: 0,
          failedCount: 0,
          pendingCount: 0,
          succeededCount: 0,
          timeoutCount: 0,
          totalCount: 0,
        },
      }),
      getPaymentDetail: vi.fn().mockResolvedValue(paymentDetail),
      getPaymentStatus: vi.fn().mockResolvedValue(statusAfterRefresh),
      listOrderPayments: vi.fn().mockResolvedValue(relatedPayments),
      reconcilePayment: vi.fn().mockResolvedValue(statusAfterRefresh),
    };

    const controller = createSdkworkPaymentController({
      service,
    });

    await controller.bootstrap();
    expect(controller.getState()).toMatchObject({
      activeFilter: "all",
      isBootstrapped: true,
      selectedMethodCode: "WECHAT_PAY",
      visibleRecords: firstDashboard.records,
    });

    controller.setFilter("pending");
    expect(controller.getState().visibleRecords).toHaveLength(1);

    controller.selectMethod("ALIPAY");
    expect(controller.getState().selectedMethodCode).toBe("ALIPAY");

    await controller.openDetail("1002");
    expect(controller.getState()).toMatchObject({
      detail: paymentDetail,
      isDetailOpen: true,
      relatedPayments,
      selectedPaymentId: "1002",
    });

    controller.selectMethod("WECHAT_PAY");
    controller.openCreateDialog();
    expect(controller.getState().isCreateOpen).toBe(true);

    await controller.createPayment({
      orderId: "ORDER-11",
    });
    expect(service.createPayment).toHaveBeenCalledWith({
      orderId: "ORDER-11",
      paymentMethod: "WECHAT_PAY",
      productType: "native",
    });
    expect(controller.getState()).toMatchObject({
      detail: createdPayment,
      isCreateOpen: false,
      isDetailOpen: true,
      selectedPaymentId: "1002",
    });

    await controller.refreshPaymentStatus("1002");
    expect(service.getPaymentStatus).toHaveBeenCalledWith("1002");
    expect(controller.getState().detail).toMatchObject({
      id: "1002",
      status: "success",
      transactionId: "TXN-1002",
    });

    await controller.closePayment("1002");
    expect(service.closePayment).toHaveBeenCalledWith("1002");
    expect(controller.getState().dashboard.statistics.totalCount).toBe(3);
  });

  it("uses controller copy overrides for validation fallbacks and localized closed detail state", async () => {
    const controller = createSdkworkPaymentController({
      initialState: {
        dashboard: {
          clientType: "WEB",
          digest: {
            actionablePayments: 0,
            closedCount: 0,
            failedCount: 0,
            successfulPayments: 0,
            timedOutPayments: 0,
            totalAmountCny: 0,
            totalCount: 0,
          },
          methods: [],
          records: [],
          statistics: {
            closedCount: 0,
            failedCount: 0,
            pendingCount: 0,
            succeededCount: 0,
            timeoutCount: 0,
            totalCount: 0,
          },
        },
      },
      messages: {
        controller: {
          reconcileContextRequired: "Need payment context from overrides",
          selectPaymentMethodRequired: "Choose a method from overrides",
        },
        status: {
          closed: "Closed from overrides",
        },
      } satisfies SdkworkPaymentMessagesOverrides,
      service: {
        closePayment: vi.fn().mockResolvedValue({
          closed: true,
          paymentId: "1001",
        }),
        createPayment: vi.fn(),
        getDashboard: vi.fn().mockResolvedValue({
          clientType: "WEB",
          digest: {
            actionablePayments: 0,
            closedCount: 1,
            failedCount: 0,
            successfulPayments: 0,
            timedOutPayments: 0,
            totalAmountCny: 0,
            totalCount: 1,
          },
          methods: [],
          records: [],
          statistics: {
            closedCount: 1,
            failedCount: 0,
            pendingCount: 0,
            succeededCount: 0,
            timeoutCount: 0,
            totalCount: 1,
          },
        }),
        getEmptyDashboard: vi.fn().mockReturnValue({
          clientType: "WEB",
          digest: {
            actionablePayments: 0,
            closedCount: 0,
            failedCount: 0,
            successfulPayments: 0,
            timedOutPayments: 0,
            totalAmountCny: 0,
            totalCount: 0,
          },
          methods: [],
          records: [],
          statistics: {
            closedCount: 0,
            failedCount: 0,
            pendingCount: 0,
            succeededCount: 0,
            timeoutCount: 0,
            totalCount: 0,
          },
        }),
        getPaymentDetail: vi.fn(),
        getPaymentStatus: vi.fn(),
        listOrderPayments: vi.fn().mockResolvedValue([]),
        reconcilePayment: vi.fn(),
      },
    } satisfies CreateSdkworkPaymentControllerOptions);

    await expect(controller.createPayment({ orderId: "ORDER-9" })).rejects.toThrow(
      "Choose a method from overrides",
    );
    await expect(controller.reconcilePayment()).rejects.toThrow(
      "Need payment context from overrides",
    );

    const closeController = createSdkworkPaymentController({
      initialState: {
        dashboard: {
          clientType: "WEB",
          digest: {
            actionablePayments: 1,
            closedCount: 0,
            failedCount: 0,
            successfulPayments: 0,
            timedOutPayments: 0,
            totalAmountCny: 699,
            totalCount: 1,
          },
          methods: [
            {
              available: true,
              code: "WECHAT_PAY",
              id: "wechat-pay",
              label: "WeChat Pay",
              productTypes: [
                {
                  available: true,
                  code: "native",
                  label: "Native",
                },
              ],
              recommendedProductType: "native",
              sort: 100,
            },
          ],
          records: [],
          statistics: {
            closedCount: 0,
            failedCount: 0,
            pendingCount: 1,
            succeededCount: 0,
            timeoutCount: 0,
            totalCount: 1,
          },
        },
        detail: {
          amountCny: 699,
          canClose: true,
          canReconcile: true,
          canRefreshStatus: true,
          createdAt: "2026-04-03T10:05:00.000Z",
          id: "1001",
          needQuery: true,
          orderId: "ORDER-9",
          outTradeNo: "OUT-ORDER-9",
          paymentMethod: "WECHAT_PAY",
          paymentParams: {},
          paymentProvider: "WECHAT_PAY",
          paymentSn: "PAY-1001",
          status: "pending",
          statusLabel: "Pending",
        },
        isBootstrapped: true,
        isDetailOpen: true,
        selectedMethodCode: "WECHAT_PAY",
        selectedPaymentId: "1001",
      },
      messages: {
        status: {
          closed: "Closed from overrides",
        },
      } satisfies SdkworkPaymentMessagesOverrides,
      service: {
        closePayment: vi.fn().mockResolvedValue({
          closed: true,
          paymentId: "1001",
        }),
        createPayment: vi.fn(),
        getDashboard: vi.fn().mockResolvedValue({
          clientType: "WEB",
          digest: {
            actionablePayments: 0,
            closedCount: 1,
            failedCount: 0,
            successfulPayments: 0,
            timedOutPayments: 0,
            totalAmountCny: 699,
            totalCount: 1,
          },
          methods: [
            {
              available: true,
              code: "WECHAT_PAY",
              id: "wechat-pay",
              label: "WeChat Pay",
              productTypes: [
                {
                  available: true,
                  code: "native",
                  label: "Native",
                },
              ],
              recommendedProductType: "native",
              sort: 100,
            },
          ],
          records: [],
          statistics: {
            closedCount: 1,
            failedCount: 0,
            pendingCount: 0,
            succeededCount: 0,
            timeoutCount: 0,
            totalCount: 1,
          },
        }),
        getEmptyDashboard: vi.fn().mockReturnValue({
          clientType: "WEB",
          digest: {
            actionablePayments: 0,
            closedCount: 0,
            failedCount: 0,
            successfulPayments: 0,
            timedOutPayments: 0,
            totalAmountCny: 0,
            totalCount: 0,
          },
          methods: [],
          records: [],
          statistics: {
            closedCount: 0,
            failedCount: 0,
            pendingCount: 0,
            succeededCount: 0,
            timeoutCount: 0,
            totalCount: 0,
          },
        }),
        getPaymentDetail: vi.fn(),
        getPaymentStatus: vi.fn(),
        listOrderPayments: vi.fn().mockResolvedValue([]),
        reconcilePayment: vi.fn(),
      },
    } satisfies CreateSdkworkPaymentControllerOptions);

    await closeController.closePayment("1001");

    expect(closeController.getState().detail?.statusLabel).toBe("Closed from overrides");
  });
});
