import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { toast } from "@sdkwork/ui-pc-react";
import { SdkworkI18nProvider } from "@sdkwork/i18n-pc-react";
import { afterEach, describe, expect, it, vi } from "vitest";

import {
  PaymentProviderAdminWorkspace,
  type PaymentProviderAdminCapabilities,
} from "../src/pages/ProviderAdminWorkspace";
import type {
  PaymentProviderAccountTestResult,
  PaymentProviderAccountView,
  PaymentProviderAdminController,
  PaymentProviderAdminState,
} from "../src/types/provider-admin-types";

const sandboxAccount: PaymentProviderAccountView = {
  id: "provider-1",
  accountNo: "stripe-main",
  providerCode: "stripe",
  merchantId: "merchant_001",
  accountName: "Stripe Main",
  accountMode: "direct",
  environment: "sandbox",
  countryCode: "CN",
  settlementCurrency: "CNY",
  hasPrimarySecret: true,
  hasWebhookSecret: true,
  hasCertificate: false,
  credentialStorage: "database_encrypted",
  metadata: {},
  capabilities: { pay: true },
  status: "inactive",
  createdAt: "2026-07-17T00:00:00.000Z",
  updatedAt: "2026-07-17T00:00:00.000Z",
};

const okTestResult: PaymentProviderAccountTestResult = {
  ok: true,
  providerCode: "stripe",
  environment: "sandbox",
  testedAt: "2026-07-17T00:00:00.000Z",
};

interface ControllerMock {
  controller: PaymentProviderAdminController;
  updateProviderAccount: ReturnType<typeof vi.fn>;
  testProviderAccount: ReturnType<typeof vi.fn>;
}

function createControllerMock(account: PaymentProviderAccountView): ControllerMock {
  const updateProviderAccount = vi.fn(async () => account);
  const testProviderAccount = vi.fn(async () => okTestResult);
  let state: PaymentProviderAdminState = {
    providerAccounts: [account],
    subMerchants: [],
    status: "ready",
  };
  const controller: PaymentProviderAdminController = {
    getState: () => state,
    subscribe: () => () => undefined,
    load: async () => state,
    loadMoreProviderAccounts: async () => [account],
    loadMoreSubMerchants: async () => [],
    selectProviderAccount: (id) => state.providerAccounts.find((item) => item.id === id),
    selectSubMerchant: () => undefined,
    createProviderAccount: async () => account,
    updateProviderAccount,
    deleteProviderAccount: async () => undefined,
    testProviderAccount,
    rotateProviderAccountCredentials: async () => account,
    readProviderAccountCredentials: async () => ({
      providerAccountId: account.id,
      primarySecret: "sk_test_saved",
      webhookSecret: "whsec_saved",
      certificate: "",
    }),
    createSubMerchant: async () => ({ id: "merchant-1" }) as never,
    updateSubMerchant: async () => ({ id: "merchant-1" }) as never,
    deleteSubMerchant: async () => undefined,
  };
  return { controller, updateProviderAccount, testProviderAccount };
}

const capabilities: PaymentProviderAdminCapabilities = {
  canCreateProviderAccount: true,
  canUpdateProviderAccount: true,
  canDeleteProviderAccount: true,
  canTestProviderAccount: true,
  canRotateProviderCredentials: true,
  canCreateSubMerchant: true,
  canUpdateSubMerchant: true,
  canDeleteSubMerchant: true,
};

afterEach(() => {
  toast.dismiss();
  cleanup();
});

describe("PaymentProviderAdminWorkspace status toggle", () => {
  it("enables an inactive account through the validated activation flow", async () => {
    const { controller, updateProviderAccount, testProviderAccount } = createControllerMock(sandboxAccount);
    render(
      <PaymentProviderAdminWorkspace controller={controller} capabilities={capabilities} />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Enable" }));

    await waitFor(() => {
      expect(testProviderAccount).toHaveBeenCalledWith("provider-1", {
        environment: "sandbox",
        dryRun: true,
      });
    });
    await waitFor(() => {
      expect(updateProviderAccount).toHaveBeenCalledWith("provider-1", { status: "active" });
    });
    expect(await screen.findByText("Provider account enabled.")).toBeInTheDocument();
  });

  it("surfaces the readiness diagnostic when the dry-run test fails", async () => {
    const { controller, updateProviderAccount, testProviderAccount } = createControllerMock(sandboxAccount);
    testProviderAccount.mockResolvedValueOnce({
      ok: false,
      providerCode: "stripe",
      environment: "sandbox",
      diagnostic: "primary provider credential is not configured",
      testedAt: "2026-07-17T00:00:00.000Z",
    });
    render(
      <PaymentProviderAdminWorkspace controller={controller} capabilities={capabilities} />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Enable" }));

    expect(await screen.findByText("Provider account readiness validation failed.")).toBeInTheDocument();
    expect(
      screen.getByText("primary provider credential is not configured"),
    ).toBeInTheDocument();
    expect(updateProviderAccount).not.toHaveBeenCalledWith("provider-1", { status: "active" });
  });

  it("disables an active account only after confirmation", async () => {
    const { controller, updateProviderAccount } = createControllerMock({
      ...sandboxAccount,
      status: "active",
    });
    render(
      <PaymentProviderAdminWorkspace controller={controller} capabilities={capabilities} />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Disable" }));
    expect(updateProviderAccount).not.toHaveBeenCalled();

    const confirmDialog = await screen.findByRole("dialog");
    fireEvent.click(within(confirmDialog).getByRole("button", { name: "Disable" }));

    await waitFor(() => {
      expect(updateProviderAccount).toHaveBeenCalledWith("provider-1", { status: "inactive" });
    });
    expect(await screen.findByText("Provider account disabled.")).toBeInTheDocument();
  });

  it("ignores a second click while a status mutation is in flight", async () => {
    const { controller, testProviderAccount } = createControllerMock(sandboxAccount);
    let resolveTest!: (result: PaymentProviderAccountTestResult) => void;
    testProviderAccount.mockImplementation(
      () => new Promise<PaymentProviderAccountTestResult>((resolve) => { resolveTest = resolve; }),
    );
    render(
      <PaymentProviderAdminWorkspace controller={controller} capabilities={capabilities} />,
    );

    const enableButton = screen.getByRole("button", { name: "Enable" });
    fireEvent.click(enableButton);
    fireEvent.click(enableButton);
    expect(testProviderAccount).toHaveBeenCalledTimes(1);

    resolveTest(okTestResult);
    await waitFor(() => {
      expect(testProviderAccount).toHaveBeenCalledTimes(1);
    });
  });
});

describe("PaymentProviderAdminWorkspace credential replacement", () => {
  it("echoes saved credentials and generates a key from the link button", async () => {
    const { controller } = createControllerMock(sandboxAccount);
    render(
      <PaymentProviderAdminWorkspace controller={controller} capabilities={capabilities} />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Replace credentials" }));
    const dialog = await screen.findByRole("dialog");

    // Saved credentials are loaded into the rotate form for comparison.
    await waitFor(() => {
      const primarySecret = within(dialog).getByLabelText(/Stripe Secret Key/i) as HTMLTextAreaElement;
      expect(primarySecret.value).toBe("sk_test_saved");
    });

    // Copy/Generate link buttons align under each credential textarea.
    expect(within(dialog).getAllByRole("button", { name: "Copy" })).toHaveLength(3);
    expect(within(dialog).getByRole("button", { name: "Generate key" })).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Generate secret" })).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Generate certificate" })).toBeInTheDocument();

    fireEvent.click(within(dialog).getByRole("button", { name: "Generate key" }));

    await waitFor(() => {
      const primarySecret = within(dialog).getByLabelText(/Stripe Secret Key/i) as HTMLTextAreaElement;
      expect(primarySecret.value).toMatch(/^sk_test_[A-Za-z0-9]{24}$/u);
    });
  });
});

describe("PaymentProviderAdminWorkspace localization", () => {
  it("localizes the replace credentials dialog under zh-CN", async () => {
    const { controller } = createControllerMock(sandboxAccount);
    render(
      <SdkworkI18nProvider locale="zh-CN">
        <PaymentProviderAdminWorkspace controller={controller} capabilities={capabilities} />
      </SdkworkI18nProvider>,
    );

    fireEvent.click(screen.getByRole("button", { name: /更换凭据/u }));
    const dialog = await screen.findByRole("dialog");

    expect(within(dialog).getByLabelText(/Stripe 密钥/u)).toBeInTheDocument();
    expect(within(dialog).getByLabelText(/Stripe Webhook 签名密钥/u)).toBeInTheDocument();
    expect(within(dialog).getByText(/新凭据版本/u)).toBeInTheDocument();
    expect(within(dialog).getByText(/作废旧凭据/u)).toBeInTheDocument();
    expect(
      within(dialog).getByRole("button", { name: /更换凭据/u }),
    ).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: /取消/u })).toBeInTheDocument();
  });
});

describe("PaymentProviderAdminWorkspace create drawer localization", () => {
  it("renders the WeChat Pay merchant API certificate label in Chinese under zh-CN", async () => {
    const { controller } = createControllerMock(sandboxAccount);
    render(
      <SdkworkI18nProvider locale="zh-CN">
        <PaymentProviderAdminWorkspace controller={controller} capabilities={capabilities} />
      </SdkworkI18nProvider>,
    );

    fireEvent.click(screen.getByRole("button", { name: /支付机构账户/u }));
    const drawer = await screen.findByRole("dialog");

    // Switch the provider to WeChat Pay inside the drawer, then verify the
    // credential label renders fully localized (no mixed English/Chinese).
    fireEvent.click(within(drawer).getByRole("combobox", { name: /支付机构/u }));
    const listbox = await screen.findByRole("listbox");
    fireEvent.click(within(listbox).getByText(/微信支付/u));
    fireEvent.mouseDown(within(drawer).getByRole("tab", { name: /密钥和安全凭据/u }));

    expect(within(drawer).getByLabelText(/微信支付商户API证书/u)).toBeInTheDocument();
  });
});
