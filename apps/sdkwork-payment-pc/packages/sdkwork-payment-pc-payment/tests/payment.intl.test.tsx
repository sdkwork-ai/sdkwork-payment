import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { SdkworkThemeProvider } from "@sdkwork/ui-pc-react/theme";
import {
  SdkworkPaymentIntlProvider,
  SdkworkPaymentPage,
  SdkworkPaymentStatGrid,
  createSdkworkPaymentController,
} from "../src";

function createPaymentDashboard() {
  return {
    clientType: "WEB" as const,
    digest: {
      actionablePayments: 1,
      closedCount: 0,
      failedCount: 0,
      successfulPayments: 1,
      timedOutPayments: 0,
      totalAmountCny: 699,
      totalCount: 2,
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
            code: "native" as const,
            label: "Native",
          },
        ],
        recommendedProductType: "native" as const,
        sort: 100,
      },
    ],
    records: [
      {
        amountCny: 699,
        canClose: false,
        canReconcile: true,
        canRefreshStatus: true,
        createdAt: "2026-04-03T10:05:00.000Z",
        id: "1001",
        orderId: "ORDER-9",
        outTradeNo: "OUT-ORDER-9",
        paymentMethod: "WECHAT_PAY",
        paymentProvider: "WECHAT_PAY",
        paymentSn: "PAY-1001",
        status: "pending" as const,
        statusLabel: "Pending",
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
}

function createEmptyDashboard() {
  return {
    clientType: "WEB" as const,
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
  };
}

describe("sdkwork-payment-pc-payment intl", () => {
  it("renders Chinese copy across the payment page when a Chinese locale is provided", async () => {
    const controller = createSdkworkPaymentController({
      service: {
        closePayment: vi.fn(),
        createPayment: vi.fn(),
        getDashboard: vi.fn().mockResolvedValue(createPaymentDashboard()),
        getEmptyDashboard: vi.fn().mockReturnValue(createEmptyDashboard()),
        getPaymentDetail: vi.fn(),
        getPaymentStatus: vi.fn(),
        listOrderPayments: vi.fn().mockResolvedValue([]),
        reconcilePayment: vi.fn(),
      },
    });

    render(
      <SdkworkThemeProvider defaultTheme="light">
        <SdkworkPaymentPage controller={controller} locale="zh-CN" />
      </SdkworkThemeProvider>,
    );

    expect(
      await screen.findByRole("heading", {
        name: "支付中心",
      }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "新建支付" })).toBeInTheDocument();
    expect(screen.getAllByRole("heading", { name: "支付记录" }).length).toBeGreaterThan(0);
  });

  it("applies host message overrides on top of the localized payment copy seam", async () => {
    const controller = createSdkworkPaymentController({
      service: {
        closePayment: vi.fn(),
        createPayment: vi.fn(),
        getDashboard: vi.fn().mockResolvedValue(createPaymentDashboard()),
        getEmptyDashboard: vi.fn().mockReturnValue(createEmptyDashboard()),
        getPaymentDetail: vi.fn(),
        getPaymentStatus: vi.fn(),
        listOrderPayments: vi.fn().mockResolvedValue([]),
        reconcilePayment: vi.fn(),
      },
    });

    render(
      <SdkworkThemeProvider defaultTheme="light">
        <SdkworkPaymentPage
          controller={controller}
          locale="zh-CN"
          messages={{
            actions: {
              newPayment: "Launch settlement",
            },
            page: {
              title: "Host payment cockpit",
            },
          }}
        />
      </SdkworkThemeProvider>,
    );

    expect(
      await screen.findByRole("heading", {
        name: "Host payment cockpit",
      }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Launch settlement" })).toBeInTheDocument();
    expect(screen.getAllByRole("heading", { name: "支付记录" }).length).toBeGreaterThan(0);
  });

  it("falls back to built-in English copy for standalone components without a host intl provider", () => {
    render(
      <SdkworkThemeProvider defaultTheme="light">
        <SdkworkPaymentStatGrid
          digest={{
            actionablePayments: 2,
            closedCount: 1,
            failedCount: 1,
            successfulPayments: 3,
            timedOutPayments: 0,
            totalAmountCny: 999,
            totalCount: 7,
          }}
          statistics={{
            closedCount: 1,
            failedCount: 1,
            pendingCount: 2,
            succeededCount: 3,
            timeoutCount: 0,
            totalCount: 7,
          }}
        />
      </SdkworkThemeProvider>,
    );

    expect(screen.getByText("Action required")).toBeInTheDocument();
    expect(screen.getByText("Successful")).toBeInTheDocument();
  });

  it("lets standalone payment components consume Chinese copy through the intl provider", () => {
    render(
      <SdkworkThemeProvider defaultTheme="light">
        <SdkworkPaymentIntlProvider locale="zh-CN">
          <SdkworkPaymentStatGrid
            digest={{
              actionablePayments: 2,
              closedCount: 1,
              failedCount: 1,
              successfulPayments: 3,
              timedOutPayments: 0,
              totalAmountCny: 999,
              totalCount: 7,
            }}
            statistics={{
              closedCount: 1,
              failedCount: 1,
              pendingCount: 2,
              succeededCount: 3,
              timeoutCount: 0,
              totalCount: 7,
            }}
          />
        </SdkworkPaymentIntlProvider>
      </SdkworkThemeProvider>,
    );

    expect(screen.getAllByText("待处理支付").length).toBeGreaterThan(0);
    expect(screen.getAllByText("支付成功").length).toBeGreaterThan(0);
  });
});
