import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { SdkworkI18nProvider } from "@sdkwork/i18n-pc-react";

import { ProviderAccountForm } from "../src/components/ProviderAccountForm";

afterEach(cleanup);

function renderCreateForm() {
  return render(
    <ProviderAccountForm
      mode="create"
      onCancel={vi.fn()}
      onSubmit={vi.fn()}
    />,
  );
}

/** The cancel/save actions live in the drawer footer (outside this component);
 *  tests submit the form directly. */
function submitForm(view: ReturnType<typeof render>) {
  fireEvent.submit(view.container.querySelector("form")!);
}

/** The form uses top section tabs; non-active sections are unmounted by
 *  Radix, so field assertions must activate the owning section first. Radix
 *  activates a tab on `mousedown` (not click). */
function openSection(name: RegExp) {
  fireEvent.mouseDown(screen.getByRole("tab", { name }));
}

function sectionBadge(name: RegExp): boolean {
  const tab = screen.getByRole("tab", { name });
  return tab.querySelector('[data-complete="true"]') !== null;
}

describe("ProviderAccountForm section layout", () => {
  it("renders the left-hand sections with completion badges", () => {
    renderCreateForm();
    // Default provider (stripe): basics incomplete (empty required fields),
    // credentials incomplete (create mode requires a primary secret),
    // capabilities always complete; no metadata section for stripe.
    expect(screen.getByRole("tab", { name: /Account Basics/i })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /Credentials/i })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /Capabilities/i })).toBeInTheDocument();
    expect(screen.queryByRole("tab", { name: /Provider Metadata/i })).not.toBeInTheDocument();

    expect(sectionBadge(/Account Basics/i)).toBe(false);
    expect(sectionBadge(/Credentials/i)).toBe(false);
    expect(sectionBadge(/Capabilities/i)).toBe(true);
  });

  it("marks basics and credentials complete once required fields are filled", async () => {
    renderCreateForm();
    expect(sectionBadge(/Account Basics/i)).toBe(false);

    fireEvent.change(screen.getByLabelText(/Account No/i), {
      target: { value: "stripe-live-primary" },
    });
    fireEvent.change(screen.getByLabelText(/Merchant ID/i), {
      target: { value: "acct_123" },
    });
    expect(sectionBadge(/Account Basics/i)).toBe(true);
    expect(sectionBadge(/Credentials/i)).toBe(false);

    openSection(/Credentials/i);
    fireEvent.change(screen.getByLabelText(/Stripe Secret Key/i), {
      target: { value: "sk_test_mock_secret_key_for_ui_testing" },
    });
    expect(sectionBadge(/Credentials/i)).toBe(true);
  });

  it("shows the provider metadata fields inline for WeChat Pay", async () => {
    renderCreateForm();
    fireEvent.click(screen.getByRole("combobox", { name: /Provider/ }));
    const listbox = await screen.findByRole("listbox");
    fireEvent.click(within(listbox).getByText("WeChat Pay"));

    // The metadata fields live inside the basics section (no separate tab);
    // the basics badge stays incomplete until they are filled.
    expect(screen.queryByRole("tab", { name: /Provider Metadata/i })).not.toBeInTheDocument();
    expect(sectionBadge(/Account Basics/i)).toBe(false);

    fireEvent.change(screen.getByLabelText(/Account No/i), {
      target: { value: "wechat-dev-primary" },
    });
    fireEvent.change(screen.getByLabelText(/Merchant ID/i), {
      target: { value: "1900000109" },
    });
    expect(sectionBadge(/Account Basics/i)).toBe(false);

    fireEvent.change(screen.getByLabelText(/App ID/i), {
      target: { value: "wx2421b1c4370ec43b" },
    });
    fireEvent.change(screen.getByLabelText(/Merchant Serial No/i), {
      target: { value: "6EB892196BEAA85D5E59B06F077C8A2903683649" },
    });
    fireEvent.change(screen.getByLabelText(/WeChat Pay Public Key ID/i), {
      target: { value: "PUB_KEY_ID_00000000000000000000000000000001" },
    });
    expect(sectionBadge(/Account Basics/i)).toBe(true);
  });
});

describe("ProviderAccountForm validation navigation", () => {
  it("applies the tech-blue background to the active tab and clears it on others", () => {
    renderCreateForm();
    const basicsTab = screen.getByRole("tab", { name: /Account Basics/i });
    const credentialsTab = screen.getByRole("tab", { name: /Credentials/i });
    // Active tab carries the solid brand background via inline style; the
    // inactive tab does not. jsdom resolves `white` to rgb(255, 255, 255).
    expect(basicsTab).toHaveStyle({
      backgroundColor: "var(--sdk-color-brand-primary)",
      color: "rgb(255, 255, 255)",
    });
    expect(credentialsTab).not.toHaveStyle({
      backgroundColor: "var(--sdk-color-brand-primary)",
    });

    openSection(/Credentials/i);
    expect(credentialsTab).toHaveStyle({
      backgroundColor: "var(--sdk-color-brand-primary)",
      color: "rgb(255, 255, 255)",
    });
    expect(basicsTab).not.toHaveStyle({
      backgroundColor: "var(--sdk-color-brand-primary)",
    });
  });

  it("jumps to basics and highlights required fields when basics are missing", () => {
    const view = renderCreateForm();
    submitForm(view);

    expect(screen.getByRole("alert")).toHaveTextContent(
      /Account no and merchant id are required/u,
    );
    expect(screen.getByLabelText(/Account No/i)).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByLabelText(/Merchant ID/i)).toHaveAttribute("aria-invalid", "true");
    // The basics section stays active after the failed submit.
    expect(screen.getByRole("tab", { name: /Account Basics/i })).toHaveAttribute(
      "data-state",
      "active",
    );
  });

  it("jumps to credentials when the primary credential is missing", () => {
    const view = renderCreateForm();
    fireEvent.change(screen.getByLabelText(/Account No/i), {
      target: { value: "stripe-live-primary" },
    });
    fireEvent.change(screen.getByLabelText(/Merchant ID/i), {
      target: { value: "acct_123" },
    });
    submitForm(view);

    expect(screen.getByRole("alert")).toHaveTextContent(
      /Primary credential is required before creating the account/u,
    );
    expect(screen.getByRole("tab", { name: /Credentials/i })).toHaveAttribute(
      "data-state",
      "active",
    );
    expect(screen.getByLabelText(/Stripe Secret Key/i)).toHaveClass(
      "border-[var(--sdk-color-state-danger)]",
    );
  });

  it("clears the field error highlight once the field is edited", () => {
    const view = renderCreateForm();
    submitForm(view);
    expect(screen.getByLabelText(/Account No/i)).toHaveAttribute("aria-invalid", "true");

    fireEvent.change(screen.getByLabelText(/Account No/i), {
      target: { value: "stripe-live-primary" },
    });
    expect(screen.getByLabelText(/Account No/i)).toHaveAttribute("aria-invalid", "false");
  });
});

describe("ProviderAccountForm credential generation", () => {
  it("shows an explanatory hint under each credential field", async () => {
    renderCreateForm();
    openSection(/Credentials/i);
    expect(screen.getByText(/sk_live_… \/ sk_test_…/u)).toBeInTheDocument();
    expect(screen.getByText(/whsec_…/u)).toBeInTheDocument();
    expect(screen.getByText(/signs every API request/u)).toBeInTheDocument();
  });

  it("localizes the WeChat Pay verification hint under zh-CN", async () => {
    render(
      <SdkworkI18nProvider locale="zh-CN">
        <ProviderAccountForm mode="create" onCancel={vi.fn()} onSubmit={vi.fn()} />
      </SdkworkI18nProvider>,
    );
    // Tab labels are localized too.
    expect(screen.getByRole("tab", { name: /密钥和安全凭据/u })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("combobox", { name: /Provider/ }));
    const listbox = await screen.findByRole("listbox");
    fireEvent.click(within(listbox).getByText("WeChat Pay"));
    fireEvent.mouseDown(screen.getByRole("tab", { name: /密钥和安全凭据/u }));

    // The merchant API certificate label is fully localized (no mixed copy).
    expect(screen.getByLabelText(/微信支付商户API证书/u)).toBeInTheDocument();

    expect(
      screen.getByText(/微信支付 API v3 验签凭据采用官方两种模式二选一/u),
    ).toBeInTheDocument();
    expect(screen.getByText(/Wechatpay-Serial 响应头必须与已配置的公钥 ID/u)).toBeInTheDocument();
    // Public key hint explains the WeChat Pay signing/verification model.
    expect(screen.getByText(/商户需要使用微信支付公钥验签/u)).toBeInTheDocument();
  });

  it("fills all credential fields with one click in sandbox", async () => {
    renderCreateForm();
    openSection(/Credentials/i);

    const button = screen.getByRole("button", { name: "Generate all credentials" });
    expect(button).toBeInTheDocument();

    fireEvent.click(button);

    // Stripe default provider: sk_test_ secret + whsec_ webhook secret.
    await waitFor(() => {
      const primarySecret = screen.getByLabelText(/Stripe Secret Key/i) as HTMLTextAreaElement;
      expect(primarySecret.value).toMatch(/^sk_test_[A-Za-z0-9]{24}$/u);
    });
    const webhookSecret = screen.getByLabelText(/Stripe Webhook Signing Secret/i) as HTMLTextAreaElement;
    expect(webhookSecret.value).toMatch(/^whsec_[A-Za-z0-9]{32}$/u);
  });

  it("generates a single field from its link button", async () => {
    renderCreateForm();
    openSection(/Credentials/i);

    fireEvent.click(screen.getByRole("button", { name: "Generate key" }));

    await waitFor(() => {
      const primarySecret = screen.getByLabelText(/Stripe Secret Key/i) as HTMLTextAreaElement;
      expect(primarySecret.value).toMatch(/^sk_test_[A-Za-z0-9]{24}$/u);
    });
    // Only the primary field is filled; the webhook field stays untouched.
    const webhookSecret = screen.getByLabelText(/Stripe Webhook Signing Secret/i) as HTMLTextAreaElement;
    expect(webhookSecret.value).toBe("");
  });

  it("generates the certificate field for Alipay", async () => {
    renderCreateForm();
    // Switch provider to Alipay through the provider select.
    fireEvent.click(screen.getByRole("combobox", { name: /Provider/ }));
    const listbox = await screen.findByRole("listbox");
    fireEvent.click(within(listbox).getByText("Alipay"));
    openSection(/Credentials/i);

    fireEvent.click(screen.getByRole("button", { name: "Generate certificate" }));

    await waitFor(() => {
      const certificate = screen.getByLabelText(/Alipay Public Key/i) as HTMLTextAreaElement;
      expect(certificate.value).toMatch(/^-----BEGIN (PUBLIC|CERTIFICATE) KEY-----/u);
    });
  });

  it("renders the debugging note for non-production environments", () => {
    renderCreateForm();
    openSection(/Credentials/i);
    expect(
      screen.getByText(/Generated credentials are for sandbox and development debugging/u),
    ).toBeInTheDocument();
  });

  it("echoes the saved credentials back in edit mode", async () => {
    const readCredentials = vi.fn(async () => ({
      providerAccountId: "provider-1",
      primarySecret: "sk_test_saved_secret",
      webhookSecret: "whsec_saved_secret",
      certificate: "-----BEGIN PUBLIC KEY-----\nsaved\n-----END PUBLIC KEY-----",
    }));
    render(
      <ProviderAccountForm
        mode="update"
        initial={{ id: "provider-1", providerCode: "stripe" } as never}
        readCredentials={readCredentials}
        onCancel={vi.fn()}
        onSubmit={vi.fn()}
      />,
    );
    openSection(/Credentials/i);

    await waitFor(() => {
      const primarySecret = screen.getByLabelText(/Stripe Secret Key/i) as HTMLTextAreaElement;
      expect(primarySecret.value).toBe("sk_test_saved_secret");
    });
    const webhookSecret = screen.getByLabelText(/Stripe Webhook Signing Secret/i) as HTMLTextAreaElement;
    expect(webhookSecret.value).toBe("whsec_saved_secret");
    expect(readCredentials).toHaveBeenCalledTimes(1);
  });
});
