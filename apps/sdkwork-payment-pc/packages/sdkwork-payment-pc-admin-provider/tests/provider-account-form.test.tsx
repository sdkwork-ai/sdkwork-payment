import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

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

describe("ProviderAccountForm credential generation", () => {
  it("fills all credential fields with one click in sandbox", async () => {
    renderCreateForm();

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

    fireEvent.click(screen.getByRole("button", { name: "Generate certificate" }));

    await waitFor(() => {
      const certificate = screen.getByLabelText(/Alipay Public Key/i) as HTMLTextAreaElement;
      expect(certificate.value).toMatch(/^-----BEGIN (PUBLIC|CERTIFICATE) KEY-----/u);
    });
  });

  it("renders the debugging note for non-production environments", () => {
    renderCreateForm();
    expect(
      screen.getByText(/Generated credentials are for sandbox and development debugging/u),
    ).toBeInTheDocument();
  });
});
