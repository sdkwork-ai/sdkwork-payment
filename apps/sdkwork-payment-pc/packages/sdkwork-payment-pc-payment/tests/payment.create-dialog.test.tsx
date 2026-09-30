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
  SdkworkPaymentCreateDialog,
  SdkworkPaymentIntlProvider,
  createSdkworkPaymentController,
} from "../src";

function createPaymentDashboard() {
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
    methods: [
      {
        available: true,
        code: "WECHAT_PAY",
        id: "wechat-pay",
        label: "微信支付",
        productTypes: [
          {
            available: true,
            code: "native" as const,
            label: "原生扫码",
          },
        ],
        recommendedProductType: "native" as const,
        sort: 100,
      },
    ],
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

describe("sdkwork-payment-pc-payment create dialog", () => {
  it("renders Chinese create-payment copy and submits a trimmed order id", async () => {
    const createPayment = vi.fn().mockResolvedValue({
      amountCny: 699,
      canClose: false,
      canReconcile: true,
      canRefreshStatus: true,
      createdAt: "2026-04-03T10:05:00.000Z",
      id: "1001",
      needQuery: true,
      orderId: "ORDER-9",
      outTradeNo: "OUT-ORDER-9",
      paymentMethod: "WECHAT_PAY",
      paymentProvider: "WECHAT_PAY",
      paymentSn: "PAY-1001",
      paymentUrl: "https://pay.sdkwork.ai/wechat/1001",
      productType: "native" as const,
      qrContent: "weixin://wxpay/bizpayurl?pr=PAY1001",
      queryIntervalSeconds: 3,
      status: "pending" as const,
      statusLabel: "待支付",
      subject: "年度会员",
    });
    const controller = createSdkworkPaymentController({
      initialState: {
        dashboard: createPaymentDashboard(),
        isBootstrapped: true,
        isCreateOpen: true,
        selectedMethodCode: "WECHAT_PAY",
      },
      service: {
        closePayment: vi.fn(),
        createPayment,
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
        <SdkworkPaymentIntlProvider locale="zh-CN">
          <SdkworkPaymentCreateDialog controller={controller} />
        </SdkworkPaymentIntlProvider>
      </SdkworkThemeProvider>,
    );

    const dialog = await screen.findByRole("dialog", { name: "新建支付" });
    const submitButton = within(dialog).getByRole("button", { name: "创建支付" });

    expect(within(dialog).getByText("为现有商业订单发起新的支付尝试，并立即展示二维码与支付材料。")).toBeInTheDocument();
    expect(within(dialog).getByLabelText("订单编号")).toBeInTheDocument();
    expect(within(dialog).getByText("支付方式")).toBeInTheDocument();
    expect(within(dialog).getByText("支付产品")).toBeInTheDocument();

    fireEvent.change(within(dialog).getByLabelText("订单编号"), {
      target: {
        value: " ORDER-9 ",
      },
    });
    fireEvent.click(submitButton);

    await waitFor(() => {
      expect(createPayment).toHaveBeenCalledWith({
        orderId: "ORDER-9",
        paymentMethod: "WECHAT_PAY",
        productType: "native",
      });
    });
  });

  it("renders the localized create-payment error surface when the controller exposes a create failure", async () => {
    const controller = createSdkworkPaymentController({
      initialState: {
        dashboard: createPaymentDashboard(),
        isBootstrapped: true,
        isCreateOpen: true,
        lastError: "支付渠道暂不可用",
        selectedMethodCode: "WECHAT_PAY",
      },
      service: {
        getDashboard: vi.fn().mockResolvedValue(createPaymentDashboard()),
        getEmptyDashboard: vi.fn().mockReturnValue(createPaymentDashboard()),
      },
    });

    render(
      <SdkworkThemeProvider defaultTheme="light">
        <SdkworkPaymentIntlProvider locale="zh-CN">
          <SdkworkPaymentCreateDialog controller={controller} />
        </SdkworkPaymentIntlProvider>
      </SdkworkThemeProvider>,
    );

    const dialog = await screen.findByRole("dialog", { name: "新建支付" });
    expect(within(dialog).getByText("创建支付异常")).toBeInTheDocument();
    expect(within(dialog).getByText("支付渠道暂不可用")).toBeInTheDocument();
  });
});
