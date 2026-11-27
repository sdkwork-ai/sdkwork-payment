import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { PaymentAdminI18nBoundary } from "@sdkwork/payment-pc-admin-core";

import { TestPaymentConsole } from "../src/components/TestPaymentConsole";
import type {
  PaymentProviderAccountView,
  PaymentTestConsoleResults,
  PaymentTestProductsView,
} from "../src/types/devconfig-admin-types";

afterEach(cleanup);

const products: PaymentTestProductsView = {
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
      providerCode: "alipay",
      providerLabel: "Alipay",
      methodKey: "alipay_qr",
      methodStatus: "inactive",
      launchKind: "qr_code",
      settleSupported: false,
      refundSupported: true,
      testEnabled: true,
      requiredPayerFields: [],
      activeProviderAccounts: 0,
      activeChannelCount: 0,
      readiness: [
        "the method alipay_qr is inactive; enable it in the payment-methods workspace",
        "no active payment channel routes alipay_qr to a provider account; create a channel in the channel workspace",
      ],
      ready: false,
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
        { methodKey: "wechat_jsapi", field: "openid", label: "payer openid (the WeChat user paying)" },
      ],
      activeProviderAccounts: 1,
      activeChannelCount: 1,
      readiness: ["ready"],
      ready: true,
    },
  ],
  unconfiguredProviders: [],
  readyCount: 2,
  totalCount: 3,
  checkedAt: "2026-07-17T00:00:00.000Z",
};

const accounts = [
  {
    id: "account-1",
    accountNo: "wechat-sandbox",
    providerCode: "wechat_pay",
    accountMode: "direct",
    environment: "sandbox",
    settlementCurrency: "CNY",
    hasPrimarySecret: true,
    hasWebhookSecret: true,
    hasCertificate: true,
    credentialStorage: "database_encrypted",
    status: "active",
    metadata: {},
    createdAt: "2026-07-17T00:00:00.000Z",
    updatedAt: "2026-07-17T00:00:00.000Z",
  },
] as unknown as readonly PaymentProviderAccountView[];

function renderConsole(results: PaymentTestConsoleResults = {}) {
  const handlers = {
    onReloadProducts: vi.fn(),
    onCreateTestPayment: vi.fn().mockResolvedValue(undefined),
    onCheckStatus: vi.fn().mockResolvedValue(undefined),
    onSettle: vi.fn().mockResolvedValue(undefined),
    onCancel: vi.fn().mockResolvedValue(undefined),
    onRefund: vi.fn().mockResolvedValue(undefined),
    onTestNotification: vi.fn().mockResolvedValue(undefined),
  };
  render(
    <PaymentAdminI18nBoundary>
      <TestPaymentConsole
        products={products}
        accounts={accounts}
        results={results}
        {...handlers}
      />
    </PaymentAdminI18nBoundary>,
  );
  return handlers;
}

describe("one-cent test console", () => {
  it("renders every product with its readiness findings", () => {
    renderConsole();

    const board = screen.getByText("One-cent test coverage").closest("section");
    expect(board).not.toBeNull();
    const rows = within(board as HTMLElement).getAllByRole("listitem", {
      name: "",
    });
    expect(rows.length).toBeGreaterThanOrEqual(3);
    expect(screen.getByText("wechat_native")).toBeInTheDocument();
    expect(screen.getByText("alipay_qr")).toBeInTheDocument();
    // A blocked product states exactly what is missing instead of a generic
    // "unavailable" state.
    expect(
      screen.getByText(/the method alipay_qr is inactive/),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/no active payment channel routes alipay_qr/),
    ).toBeInTheDocument();
  });

  it("requires the payer identity a product declares before creating it", async () => {
    const handlers = renderConsole();

    fireEvent.click(screen.getByRole("combobox", { name: "Payment product" }));
    const options = await screen.findAllByText("WeChat Pay · wechat_jsapi");
    // Radix keeps a hidden duplicate of the selected item for the trigger.
    fireEvent.click(options[options.length - 1]);

    const submit = screen.getByRole("button", { name: "Create test payment" });
    fireEvent.click(submit);

    expect(
      await screen.findByRole("alert"),
    ).toHaveTextContent(/payer openid/);
    expect(handlers.onCreateTestPayment).not.toHaveBeenCalled();

    fireEvent.change(screen.getByRole("textbox", { name: /payer openid/ }), {
      target: { value: "o-payer-1" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create test payment" }));

    await waitFor(() => {
      expect(handlers.onCreateTestPayment).toHaveBeenCalledWith({
        methodKey: "wechat_jsapi",
        amount: "0.01",
        currencyCode: "CNY",
        payerIdentity: { openid: "o-payer-1" },
      });
    });
  });

  it("shows the launch surface and keeps simulated settlement gated per product", () => {
    renderConsole({
      payment: {
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
      },
    });

    expect(screen.getByRole("link", { name: "weixin://wxpay/bizpayurl?pr=1" })).toBeInTheDocument();
    // A product that moves real money must never offer a simulated settlement.
    expect(screen.getByRole("button", { name: "Simulate success" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Simulate failure" })).toBeDisabled();
    // Refund stays gated until the payment is actually settled.
    expect(screen.getByRole("button", { name: "Refund test payment" })).toBeDisabled();
  });

  it("enables refund and simulated settlement once the payment settled", () => {
    const payment = {
      paymentId: "attempt-1",
      paymentIntentId: "intent-1",
      attemptId: "attempt-1",
      outTradeNo: "TP-1",
      methodKey: "wechat_jsapi",
      providerCode: "wechat_pay",
      amount: "0.01",
      currencyCode: "CNY",
      status: "pending",
      launchKind: "jsapi" as const,
      launchPayload: "{\"appId\":\"wx\",\"paySign\":\"x\"}",
      settleSupported: true,
      providerParamKeys: ["nextAction"],
      requiredPayerFields: [],
      createdAt: "2026-07-17T00:00:00.000Z",
    };
    const handlers = renderConsole({
      payment,
      settled: {
        paymentIntentId: "intent-1",
        attemptId: "attempt-1",
        previousStatus: "pending",
        localStatus: "succeeded",
        providerCode: "wechat_pay",
        webhookEventId: "event-1",
      },
    });

    expect(screen.getByRole("button", { name: "Simulate success" })).toBeEnabled();
    const refund = screen.getByRole("button", { name: "Refund test payment" });
    expect(refund).toBeEnabled();
    fireEvent.click(refund);
    expect(handlers.onRefund).toHaveBeenCalledWith("intent-1");
  });

  it("reports the notification verification outcome instead of a generic success", async () => {
    const handlers = renderConsole({
      notification: {
        providerAccountId: "account-1",
        providerCode: "wechat_pay",
        verification: "rejected",
        accepted: false,
        nativeSignature: "rejected",
        documentSignature: "rejected",
        diagnostic: "the stored API v3 key does not match",
        deliveredAt: "2026-07-17T00:00:00.000Z",
      },
    });

    expect(screen.getByText("Notification rejected")).toBeInTheDocument();
    expect(screen.getByText(/the stored API v3 key does not match/)).toBeInTheDocument();

    const area = document.getElementById("test-payment-notification-payload") as HTMLTextAreaElement;
    fireEvent.change(area, { target: { value: "{\"paymentStatus\":\"succeeded\"}" } });
    fireEvent.click(screen.getByRole("button", { name: "Deliver notification" }));
    await waitFor(() => {
      expect(handlers.onTestNotification).toHaveBeenCalledWith({
        providerAccountId: "account-1",
        payload: "{\"paymentStatus\":\"succeeded\"}",
      });
    });
  });
});
