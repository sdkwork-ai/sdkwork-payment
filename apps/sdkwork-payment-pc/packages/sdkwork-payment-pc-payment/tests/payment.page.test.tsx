import {
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it, vi } from "vitest";
import { SdkworkThemeProvider } from "@sdkwork/ui-pc-react/theme";
import {
  createSdkworkPaymentBackdropStyle,
  SdkworkPaymentPage,
  createSdkworkPaymentHeroStyle,
  createSdkworkPaymentPanelStyle,
  createSdkworkPaymentController,
} from "../src";

describe("sdkwork-payment-pc-payment page", () => {
  it("renders the reusable payment center with create dialog and detail drawer flows", async () => {
    const controller = createSdkworkPaymentController({
      service: {
        closePayment: vi.fn().mockResolvedValue({
          closed: true,
          paymentId: "1001",
        }),
        createPayment: vi.fn().mockResolvedValue({
          amountCny: 699,
          createdAt: "2026-04-03T10:05:00.000Z",
          id: "1001",
          needQuery: true,
          orderId: "ORDER-9",
          outTradeNo: "OUT-ORDER-9",
          paymentMethod: "WECHAT_PAY" as const,
          paymentProvider: "WECHAT_PAY" as const,
          paymentSn: "PAY-1001",
          paymentUrl: "https://pay.sdkwork.ai/wechat/1001",
          productType: "native" as const,
          qrContent: "weixin://wxpay/bizpayurl?pr=PAY1001",
          queryIntervalSeconds: 3,
          status: "pending" as const,
          statusLabel: "Pending",
          subject: "Pro Annual",
        }),
        getDashboard: vi.fn().mockResolvedValue({
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
          ],
          statistics: {
            closedCount: 0,
            failedCount: 0,
            pendingCount: 1,
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
        getPaymentDetail: vi.fn().mockResolvedValue({
          amountCny: 699,
          createdAt: "2026-04-03T10:05:00.000Z",
          id: "1001",
          needQuery: true,
          orderId: "ORDER-9",
          outTradeNo: "OUT-ORDER-9",
          paymentMethod: "WECHAT_PAY" as const,
          paymentProvider: "WECHAT_PAY" as const,
          paymentSn: "PAY-1001",
          paymentUrl: "https://pay.sdkwork.ai/wechat/1001",
          productType: "native" as const,
          qrContent: "weixin://wxpay/bizpayurl?pr=PAY1001",
          queryIntervalSeconds: 3,
          status: "pending" as const,
          statusLabel: "Pending",
          subject: "Pro Annual",
        }),
        getPaymentStatus: vi.fn().mockResolvedValue({
          amountCny: 699,
          createdAt: "2026-04-03T10:05:00.000Z",
          id: "1001",
          orderId: "ORDER-9",
          outTradeNo: "OUT-ORDER-9",
          paymentMethod: "WECHAT_PAY" as const,
          paymentProvider: "WECHAT_PAY" as const,
          paymentSn: "PAY-1001",
          status: "success" as const,
          statusLabel: "Success",
          successTime: "2026-04-03T10:08:00.000Z",
          transactionId: "TXN-1001",
        }),
        listOrderPayments: vi.fn().mockResolvedValue([
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
        ]),
        reconcilePayment: vi.fn().mockResolvedValue({
          amountCny: 699,
          createdAt: "2026-04-03T10:05:00.000Z",
          id: "1001",
          orderId: "ORDER-9",
          outTradeNo: "OUT-ORDER-9",
          paymentMethod: "WECHAT_PAY" as const,
          paymentProvider: "WECHAT_PAY" as const,
          paymentSn: "PAY-1001",
          status: "success" as const,
          statusLabel: "Success",
        }),
      },
    });

    render(
      <SdkworkThemeProvider defaultTheme="light">
        <SdkworkPaymentPage controller={controller} />
      </SdkworkThemeProvider>,
    );

    expect(
      await screen.findByRole("heading", {
        name: /payment center/i,
      }),
    ).toBeInTheDocument();
    expect(screen.getByText(/wechat pay/i)).toBeInTheDocument();

    fireEvent.click(
      screen.getByRole("button", {
        name: /new payment/i,
      }),
    );
    const createDialog = await screen.findByRole("dialog", {
      name: /create payment/i,
    });
    fireEvent.change(
      within(createDialog).getByLabelText(/order id/i),
      {
        target: {
          value: "ORDER-9",
        },
      },
    );
    fireEvent.click(
      within(createDialog).getByRole("button", {
        name: /create payment/i,
      }),
    );

    expect(
      await screen.findByRole("heading", {
        name: /payment detail/i,
      }),
    ).toBeInTheDocument();
    expect(
      await screen.findByText(/pro annual/i),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("link", {
        name: /https:\/\/pay\.sdkwork\.ai\/wechat\/1001/i,
      }),
    ).toBeInTheDocument();
  });

  it("keeps the sdkwork-aligned payment halo and hero gradients as valid inline styles", async () => {
    const service = {
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
      getDashboard: vi.fn().mockResolvedValue({
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
      }),
    };

    const controller = createSdkworkPaymentController({
      service,
    });

    const { container } = render(
      <SdkworkThemeProvider defaultTheme="light">
        <SdkworkPaymentPage controller={controller} />
      </SdkworkThemeProvider>,
    );

    await screen.findByRole("heading", {
      name: /payment center/i,
    });

    const halo = container.querySelector(".pointer-events-none") as HTMLDivElement | null;
    const hero = container.querySelector(".mx-auto section > div") as HTMLDivElement | null;
    const methodButton = screen.getByRole("button", {
      name: /wechat pay/i,
    });
    const heroStatCard = screen.getAllByText(/total attempts/i)[0]?.closest("div.rounded-\\[1\\.5rem\\]") as HTMLDivElement | null;

    expect(halo?.style.backgroundImage).toBe(
      createSdkworkPaymentBackdropStyle().backgroundImage,
    );
    expect(hero?.style.backgroundImage).toBe(
      createSdkworkPaymentHeroStyle().backgroundImage,
    );
    expect(hero?.className).toContain("shadow-[var(--sdk-shadow-lg)]");
    expect(hero?.className).not.toContain("border-white/10");
    expect(heroStatCard?.className).not.toContain("bg-white/8");
    expect(methodButton.style.backgroundImage).toBe(
      createSdkworkPaymentPanelStyle("brand", {
        backgroundWeight: 12,
        borderWeight: 36,
        surfaceColor: "var(--sdk-color-surface-panel-muted)",
      }).backgroundImage,
    );
  });

  it("keeps the payment hero free of raw white utility text treatments", () => {
    const pageSource = readFileSync(
      resolve(import.meta.dirname, "../src/pages/PaymentPage.tsx"),
      "utf8",
    );

    expect(pageSource).not.toContain("text-white/72");
    expect(pageSource).not.toContain("text-white/65");
  });
});
