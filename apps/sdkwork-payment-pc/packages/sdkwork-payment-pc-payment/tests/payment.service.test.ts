import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  configurePaymentServiceMockSession,
  createPaymentAppServiceMock,
  resetPaymentServiceMockSession,
} from "../../../tests/test-utils/payment-service-mock";
import {
  createSdkworkPaymentService,
  type CreateSdkworkPaymentServiceOptions,
  type SdkworkPaymentMessagesOverrides,
} from "../src";

const wechatIcon = {
  kind: "image",
  publicUrl: "https://cdn.sdkwork.ai/icons/wechat.png",
  source: "external_url",
  url: "https://cdn.sdkwork.ai/icons/wechat.png",
} as const;

const alipayIcon = {
  kind: "image",
  publicUrl: "https://cdn.sdkwork.ai/icons/alipay.png",
  source: "external_url",
  url: "https://cdn.sdkwork.ai/icons/alipay.png",
} as const;

describe("sdkwork-payment-pc-payment service", () => {
  beforeEach(() => {
    configurePaymentServiceMockSession({ authToken: "payment-auth-token" });
  });

  afterEach(() => {
    resetPaymentServiceMockSession();
  });

  it("maps payment methods, records, statistics, and digests into a reusable payment center snapshot", async () => {
    const listPaymentMethods = vi.fn().mockResolvedValue({
      code: 0,
      data: [
        {
          available: true,
          code: "WECHAT_PAY",
          icon: wechatIcon,
          methodName: "WeChat Pay",
          productTypes: [
            {
              available: true,
              code: "jsapi",
              name: "JSAPI",
            },
            {
              available: true,
              code: "native",
              name: "Native",
            },
          ],
          sort: 100,
        },
        {
          available: true,
          code: "ALIPAY",
          icon: alipayIcon,
          methodName: "Alipay",
          productTypes: [
            {
              available: true,
              code: "pc",
              name: "PC Web",
            },
            {
              available: true,
              code: "native",
              name: "Native",
            },
          ],
          sort: 90,
        },
      ],
    });
    const paymentAppService = createPaymentAppServiceMock({
      payments: {
        statistics: {
          summary: {
            retrieve: vi.fn().mockResolvedValue({
          code: 0,
          data: {
            closedCount: 1,
            failedCount: 1,
            pendingCount: 1,
            succeededCount: 1,
            timeoutCount: 0,
            totalCount: 4,
          },
            }),
          },
        },
        methods: { list: listPaymentMethods },
        records: {
          list: vi.fn().mockResolvedValue({
            code: 0,
            data: {
              items: [
                {
                  amount: "699",
                  createdAt: "2026-04-03T10:05:00.000Z",
                  orderId: "ORDER-9",
                  outTradeNo: "OUT-ORDER-9",
                  paymentId: 1001,
                  paymentMethod: "WECHAT_PAY",
                  paymentProvider: "WECHAT_PAY",
                  paymentSn: "PAY-1001",
                  status: "PENDING",
                  statusName: "Pending",
                  successTime: undefined,
                  transactionId: undefined,
                },
                {
                  amount: "299",
                  createdAt: "2026-04-02T09:00:00.000Z",
                  orderId: "ORDER-8",
                  outTradeNo: "OUT-ORDER-8",
                  paymentId: 1000,
                  paymentMethod: "ALIPAY",
                  paymentProvider: "ALIPAY",
                  paymentSn: "PAY-1000",
                  status: "SUCCESS",
                  statusName: "Success",
                  successTime: "2026-04-02T09:02:00.000Z",
                  transactionId: "TXN-1000",
                },
              ],
              pageInfo: {
                mode: "offset",
                page: 1,
                pageSize: 20,
                totalItems: 2,
                totalPages: 1,
                hasNextPage: false,
              },
            },
          }),
        },
      },
    });
    const service = createSdkworkPaymentService({
      clientType: "WEB",
      paymentAppService,
    });

    const dashboard = await service.getDashboard();

    expect(listPaymentMethods).toHaveBeenCalledWith({
      clientType: "WEB",
    });
    expect(dashboard.statistics).toEqual({
      closedCount: 1,
      failedCount: 1,
      pendingCount: 1,
      succeededCount: 1,
      timeoutCount: 0,
      totalCount: 4,
    });
    expect(dashboard.digest).toEqual({
      actionablePayments: 1,
      closedCount: 0,
      failedCount: 0,
      successfulPayments: 1,
      timedOutPayments: 0,
      totalAmountCny: 998,
      totalCount: 2,
    });
    expect(dashboard.methods[0]).toMatchObject({
      code: "WECHAT_PAY",
      icon: wechatIcon,
      label: "WeChat Pay",
      recommendedProductType: "native",
    });
    expect(dashboard.records[0]).toMatchObject({
      amountCny: 699,
      id: "1001",
      orderId: "ORDER-9",
      paymentMethod: "WECHAT_PAY",
      status: "pending",
    });
  });

  it("maps payment creation, detail, status, reconcile, close, and order-payment history through the generated payment SDK boundary", async () => {
    const close = vi.fn().mockResolvedValue({
      code: 0,
    });
    const createPayment = vi.fn().mockResolvedValue({
      code: 0,
      data: {
        amount: "699",
        createdAt: "2026-04-03T10:05:00.000Z",
        expireTime: "2026-04-03T10:20:00.000Z",
        needQuery: true,
        orderId: "ORDER-9",
        outTradeNo: "OUT-ORDER-9",
        paymentId: 1001,
        paymentMethod: "WECHAT_PAY",
        paymentOrderId: "MCH-9001",
        paymentParams: {
          codeUrl: "weixin://wxpay/bizpayurl?pr=PAY1001",
          mwebUrl: "https://pay.sdkwork.ai/wechat/1001",
          prepayId: "prepay-1001",
        },
        paymentProvider: "WECHAT_PAY",
        paymentProviderName: "WeChat Pay",
        paymentSn: "PAY-1001",
        productType: "native",
        productTypeName: "Native",
        qrCode: undefined,
        queryInterval: 3,
        status: "PENDING",
        statusName: "Pending",
        subject: "Pro Annual",
      },
    });
    const reconcile = vi.fn().mockResolvedValue({
      code: 0,
      data: {
        amount: "699",
        orderId: "ORDER-9",
        outTradeNo: "OUT-ORDER-9",
        paymentId: 1001,
        paymentMethod: "WECHAT_PAY",
        paymentProvider: "WECHAT_PAY",
        paymentSn: "PAY-1001",
        status: "SUCCESS",
        statusName: "Success",
        successTime: "2026-04-03T10:08:00.000Z",
        transactionId: "TXN-1001",
      },
    });
    const paymentAppService = createPaymentAppServiceMock({
      payments: {
        close,
        create: createPayment,
        status: {
          retrieve: vi.fn().mockResolvedValue({
            code: 0,
            data: {
              amount: "699",
              orderId: "ORDER-9",
              outTradeNo: "OUT-ORDER-9",
              paymentId: 1001,
              paymentMethod: "WECHAT_PAY",
              paymentProvider: "WECHAT_PAY",
              paymentSn: "PAY-1001",
              status: "SUCCESS",
              statusName: "Success",
              successTime: "2026-04-03T10:08:00.000Z",
              transactionId: "TXN-1001",
            },
          }),
          outTradeNo: {
            retrieve: vi.fn().mockResolvedValue({
              code: 0,
              data: {
                amount: "699",
                orderId: "ORDER-9",
                outTradeNo: "OUT-ORDER-9",
                paymentId: 1001,
                paymentMethod: "WECHAT_PAY",
                paymentProvider: "WECHAT_PAY",
                paymentSn: "PAY-1001",
                status: "SUCCESS",
                statusName: "Success",
              },
            }),
          },
        },
        records: {
          retrieve: vi.fn().mockResolvedValue({
            code: 0,
            data: {
              amount: "699",
              createdAt: "2026-04-03T10:05:00.000Z",
              orderId: "ORDER-9",
              outTradeNo: "OUT-ORDER-9",
              paymentId: 1001,
              paymentMethod: "WECHAT_PAY",
              paymentProvider: "WECHAT_PAY",
              paymentSn: "PAY-1001",
              status: "PENDING",
              statusName: "Pending",
            },
          }),
          list: vi.fn().mockResolvedValue({
            code: 0,
            data: { items: [
              {
                amount: "699",
                createdAt: "2026-04-03T10:05:00.000Z",
                orderId: "ORDER-9",
                outTradeNo: "OUT-ORDER-9",
                paymentId: 1001,
                paymentMethod: "WECHAT_PAY",
                paymentProvider: "WECHAT_PAY",
                paymentSn: "PAY-1001",
                status: "SUCCESS",
                statusName: "Success",
                successTime: "2026-04-03T10:08:00.000Z",
                transactionId: "TXN-1001",
              },
              {
                amount: "699",
                createdAt: "2026-04-03T09:55:00.000Z",
                orderId: "ORDER-9",
                outTradeNo: "OUT-ORDER-9-RETRY",
                paymentId: 1000,
                paymentMethod: "ALIPAY",
                paymentProvider: "ALIPAY",
                paymentSn: "PAY-1000",
                status: "FAILED",
                statusName: "Failed",
              },
            ], pageInfo: { mode: "offset", page: 1, pageSize: 200, totalItems: 2, totalPages: 1 } },
          }),
        },
        reconcile,
      },
    });
    const service = createSdkworkPaymentService({
      clientType: "WEB",
      paymentAppService,
    });

    await expect(
      service.createPayment({
        orderId: "ORDER-9",
        paymentMethod: "WECHAT_PAY",
        productType: "native",
      }),
    ).resolves.toMatchObject({
      amountCny: 699,
      id: "1001",
      orderId: "ORDER-9",
      paymentMethod: "WECHAT_PAY",
      paymentUrl: "https://pay.sdkwork.ai/wechat/1001",
      qrContent: "weixin://wxpay/bizpayurl?pr=PAY1001",
      status: "pending",
      subject: "Pro Annual",
    });

    await expect(service.getPaymentDetail("1001")).resolves.toMatchObject({
      amountCny: 699,
      id: "1001",
      orderId: "ORDER-9",
      status: "pending",
    });

    await expect(service.getPaymentStatus("1001")).resolves.toMatchObject({
      id: "1001",
      status: "success",
      successTime: "2026-04-03T10:08:00.000Z",
      transactionId: "TXN-1001",
    });

    await expect(service.getPaymentStatusByOutTradeNo("OUT-ORDER-9")).resolves.toMatchObject({
      id: "1001",
      status: "success",
    });

    await expect(
      service.reconcilePayment({
        orderId: "ORDER-9",
      }),
    ).resolves.toMatchObject({
      id: "1001",
      status: "success",
    });

    await expect(service.closePayment("1001")).resolves.toEqual({
      closed: true,
      paymentId: "1001",
    });

    await expect(service.listOrderPayments("ORDER-9")).resolves.toHaveLength(2);

    expect(createPayment).toHaveBeenCalledWith({
      amount: undefined,
      businessOrderId: undefined,
      businessType: undefined,
      clientIp: undefined,
      orderId: "ORDER-9",
      paymentMethod: "WECHAT_PAY",
      paymentProvider: undefined,
      paymentScene: undefined,
      productType: "native",
    });
    expect(reconcile).toHaveBeenCalledWith({
      orderId: "ORDER-9",
      outTradeNo: undefined,
      reconcileType: "ORDER_ID",
    });
    expect(close).toHaveBeenCalledWith("1001");
  });

  it("returns a guest-safe empty payment dashboard without creating a client", async () => {
    resetPaymentServiceMockSession();
    const service = createSdkworkPaymentService();

    const dashboard = await service.getDashboard();

    expect(dashboard.records).toEqual([]);
    expect(dashboard.statistics.totalCount).toBe(0);
    expect(dashboard.methods).toEqual([]);
  });

  it("uses copy overrides for payment fallback labels, auth errors, and service failure messages", async () => {
    const paymentAppService = createPaymentAppServiceMock({
      payments: {
        statistics: {
          summary: {
            retrieve: vi.fn().mockResolvedValue({
              code: 0,
              data: {
                closedCount: 1,
                failedCount: 0,
                pendingCount: 0,
                succeededCount: 0,
                timeoutCount: 0,
                totalCount: 1,
              },
            }),
          },
        },
        methods: {
          list: vi.fn().mockResolvedValue({
            code: 0,
            data: [
              {
                available: true,
                code: "BANK_PAY",
                productTypes: [
                  {
                    available: true,
                    code: "online_bank",
                  },
                ],
                sort: 1,
              },
            ],
          }),
        },
        records: {
          list: vi.fn().mockResolvedValue({
            code: 0,
            data: {
              items: [
                {
                  amount: "0",
                  createdAt: "2026-04-03T10:05:00.000Z",
                  paymentId: 1001,
                  status: "CLOSED",
                },
              ],
              pageInfo: {
                mode: "offset",
                page: 1,
                pageSize: 20,
                totalItems: 1,
                totalPages: 1,
                hasNextPage: false,
              },
            },
          }),
        },
      },
    });

    const service = createSdkworkPaymentService({
      paymentAppService,
      messages: {
        common: {
          payment: "Fallback method",
        },
        productType: {
          onlineBank: "Bank rail",
        },
        service: {
          closeFailed: "Unable to close payment from override",
          signInRequired: "Override auth required",
        },
        status: {
          closed: "Settled shut",
        },
      } satisfies SdkworkPaymentMessagesOverrides,
    } satisfies CreateSdkworkPaymentServiceOptions);

    const dashboard = await service.getDashboard();

    expect(dashboard.methods[0]).toMatchObject({
      label: "Fallback method",
      productTypes: [
        expect.objectContaining({
          label: "Bank rail",
        }),
      ],
    });
    expect(dashboard.records[0]?.statusLabel).toBe("Settled shut");

    resetPaymentServiceMockSession();
    const guestService = createSdkworkPaymentService({
      messages: {
        service: {
          signInRequired: "Override auth required",
        },
      } satisfies SdkworkPaymentMessagesOverrides,
    } satisfies CreateSdkworkPaymentServiceOptions);

    await expect(guestService.getPaymentDetail("1001")).rejects.toThrow("Override auth required");

    configurePaymentServiceMockSession({ authToken: "payment-auth-token" });
    const failingService = createSdkworkPaymentService({
      paymentAppService: createPaymentAppServiceMock({
        payments: {
          close: vi.fn().mockResolvedValue({
            code: 5000,
          }),
        },
      }),
      messages: {
        service: {
          closeFailed: "Unable to close payment from override",
        },
      } satisfies SdkworkPaymentMessagesOverrides,
    } satisfies CreateSdkworkPaymentServiceOptions);

    await expect(failingService.closePayment("1001")).rejects.toThrow("Unable to close payment from override");
  });
});
