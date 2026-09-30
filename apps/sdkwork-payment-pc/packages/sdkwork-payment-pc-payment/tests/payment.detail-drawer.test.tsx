import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { SdkworkThemeProvider } from "@sdkwork/ui-pc-react/theme";
import {
  SdkworkPaymentDetailDrawer,
  SdkworkPaymentIntlProvider,
  createSdkworkPaymentController,
  type SdkworkPaymentDetail,
} from "../src";

function createPaymentDashboard() {
  return {
    clientType: "WEB" as const,
    digest: {
      actionablePayments: 1,
      closedCount: 0,
      failedCount: 0,
      successfulPayments: 0,
      timedOutPayments: 0,
      totalAmountCny: 699,
      totalCount: 1,
    },
    methods: [],
    records: [],
    statistics: {
      closedCount: 0,
      failedCount: 0,
      pendingCount: 1,
      succeededCount: 0,
      timeoutCount: 0,
      totalCount: 1,
    },
  };
}

describe("sdkwork-payment-pc-payment detail drawer", () => {
  it("renders localized payment detail copy, timestamps, QR copy, and action labels", async () => {
    const refreshPaymentStatus = vi.fn().mockResolvedValue({
      amountCny: 699,
      canClose: false,
      canReconcile: false,
      canRefreshStatus: false,
      createdAt: "2026-04-03T10:05:00.000Z",
      id: "1001",
      orderId: "ORDER-9",
      outTradeNo: "OUT-ORDER-9",
      paymentMethod: "WECHAT_PAY",
      paymentProvider: "WECHAT_PAY",
      paymentSn: "PAY-1001",
      status: "success" as const,
      statusLabel: "支付成功",
      successTime: "2026-04-03T10:08:00.000Z",
      transactionId: "TXN-1001",
    });
    const reconcilePayment = vi.fn().mockResolvedValue({
      amountCny: 699,
      canClose: false,
      canReconcile: false,
      canRefreshStatus: false,
      createdAt: "2026-04-03T10:05:00.000Z",
      id: "1001",
      orderId: "ORDER-9",
      outTradeNo: "OUT-ORDER-9",
      paymentMethod: "WECHAT_PAY",
      paymentProvider: "WECHAT_PAY",
      paymentSn: "PAY-1001",
      status: "success" as const,
      statusLabel: "支付成功",
    });
    const closePayment = vi.fn().mockResolvedValue({
      closed: true,
      paymentId: "1001",
    });
    const detail: SdkworkPaymentDetail = {
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
      paymentOrderId: "PO-1001",
      paymentParams: {},
      paymentProvider: "WECHAT_PAY",
      paymentSn: "PAY-1001",
      paymentUrl: "https://pay.sdkwork.ai/wechat/1001",
      productType: "native" as const,
      qrImage: {
        kind: "image",
        publicUrl: "data:image/png;base64,AAAA",
        source: "data_url",
        url: "data:image/png;base64,AAAA",
      },
      queryIntervalSeconds: 3,
      remark: "Scan to pay",
      status: "pending" as const,
      statusLabel: "待支付",
      subject: "年度会员",
    };
    const controller = createSdkworkPaymentController({
      initialState: {
        dashboard: createPaymentDashboard(),
        detail,
        isBootstrapped: true,
        isDetailOpen: true,
        relatedPayments: [
          {
            amountCny: 699,
            canClose: false,
            canReconcile: false,
            canRefreshStatus: false,
            createdAt: "2026-04-03T09:55:00.000Z",
            id: "0999",
            orderId: "ORDER-9",
            outTradeNo: "OUT-ORDER-9-RETRY",
            paymentMethod: "ALIPAY",
            paymentProvider: "ALIPAY",
            paymentSn: "PAY-0999",
            status: "failed" as const,
            statusLabel: "支付失败",
          },
        ],
        selectedPaymentId: "1001",
      },
      service: {
        closePayment,
        createPayment: vi.fn(),
        getDashboard: vi.fn().mockResolvedValue(createPaymentDashboard()),
        getEmptyDashboard: vi.fn().mockReturnValue(createPaymentDashboard()),
        getPaymentDetail: vi.fn(),
        getPaymentStatus: refreshPaymentStatus,
        listOrderPayments: vi.fn().mockResolvedValue([]),
        reconcilePayment,
      },
    });

    render(
      <SdkworkThemeProvider defaultTheme="light">
        <SdkworkPaymentIntlProvider locale="zh-CN">
          <SdkworkPaymentDetailDrawer controller={controller} />
        </SdkworkPaymentIntlProvider>
      </SdkworkThemeProvider>,
    );

    const drawer = await screen.findByRole("dialog", { name: "支付详情" });
    const expectedCreatedAt = new Intl.DateTimeFormat("zh-CN", {
      dateStyle: "medium",
      timeStyle: "short",
    }).format(new Date("2026-04-03T10:05:00.000Z"));

    expect(within(drawer).getByText("扫码支付")).toBeInTheDocument();
    expect(within(drawer).getByText(`创建时间: ${expectedCreatedAt}`)).toBeInTheDocument();
    expect(within(drawer).getByText("每 3 秒查询一次支付状态。")).toBeInTheDocument();
    const qrImage = within(drawer).getByAltText("支付二维码");
    expect(qrImage).toBeInTheDocument();
    expect(qrImage.className).not.toContain("bg-white");
    expect(qrImage.getAttribute("style")).toContain("var(--sdk-color-surface-canvas)");
    expect(within(drawer).getByRole("button", { name: "刷新状态" })).toBeInTheDocument();
    expect(within(drawer).getByRole("button", { name: "对账" })).toBeInTheDocument();
    expect(within(drawer).getByRole("button", { name: "关闭支付" })).toBeInTheDocument();
    expect(within(drawer).getByText("状态").closest("[data-sdk-pattern='detail-drawer-metric']")).toHaveAttribute("data-tone", "warning");

    fireEvent.click(within(drawer).getByRole("button", { name: "刷新状态" }));
    fireEvent.click(within(drawer).getByRole("button", { name: "对账" }));
    fireEvent.click(within(drawer).getByRole("button", { name: "关闭支付" }));

    await waitFor(() => {
      expect(refreshPaymentStatus).toHaveBeenCalledWith("1001");
    });
    await waitFor(() => {
      expect(reconcilePayment).toHaveBeenCalledWith({
        orderId: "ORDER-9",
      });
    });
    await waitFor(() => {
      expect(closePayment).toHaveBeenCalledWith("1001");
    });
  });

  it("uses the shared empty-value copy when payment links are missing", async () => {
    const controller = createSdkworkPaymentController({
      initialState: {
        dashboard: createPaymentDashboard(),
        detail: {
          amountCny: 699,
          canClose: false,
          canReconcile: false,
          canRefreshStatus: false,
          createdAt: "2026-04-03T10:05:00.000Z",
          id: "1002",
          needQuery: false,
          orderId: "ORDER-10",
          outTradeNo: "OUT-ORDER-10",
          paymentMethod: "WECHAT_PAY",
          paymentParams: {},
          paymentProvider: "WECHAT_PAY",
          paymentSn: "PAY-1002",
          productType: "native",
          qrContent: "weixin://wxpay/bizpayurl?pr=PAY1002",
          status: "pending",
          statusLabel: "Pending",
          subject: "Workspace billing",
        },
        isBootstrapped: true,
        isDetailOpen: true,
        relatedPayments: [],
        selectedPaymentId: "1002",
      },
      service: {
        closePayment: vi.fn(),
        createPayment: vi.fn(),
        getDashboard: vi.fn().mockResolvedValue(createPaymentDashboard()),
        getEmptyDashboard: vi.fn().mockReturnValue(createPaymentDashboard()),
        getPaymentDetail: vi.fn(),
        getPaymentStatus: vi.fn(),
        listOrderPayments: vi.fn().mockResolvedValue([]),
        reconcilePayment: vi.fn(),
      },
    });

    render(
      <SdkworkThemeProvider defaultTheme="light">
        <SdkworkPaymentIntlProvider
          messages={{
            common: {
              emptyValue: "EMPTY-VALUE",
            },
          }}
        >
          <SdkworkPaymentDetailDrawer controller={controller} />
        </SdkworkPaymentIntlProvider>
      </SdkworkThemeProvider>,
    );

    const drawer = await screen.findByRole("dialog", { name: /payment detail/i });
    expect(within(drawer).getByText("EMPTY-VALUE")).toBeInTheDocument();
  });
});
