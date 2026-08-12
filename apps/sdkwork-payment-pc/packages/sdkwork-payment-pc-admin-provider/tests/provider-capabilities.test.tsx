import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { ProviderAccountList } from "../src/components/ProviderAccountList";
import { SubMerchantManager } from "../src/components/SubMerchantManager";

const providerAccount = {
  id: "provider-1",
  accountNo: "stripe-main",
  providerCode: "stripe",
  accountMode: "partner",
  environment: "production",
  status: "active",
  capabilities: {},
  createdAt: "2026-07-17T00:00:00.000Z",
  updatedAt: "2026-07-17T00:00:00.000Z",
} as const;

afterEach(cleanup);

describe("payment provider capabilities", () => {
  it("hides provider mutation controls for read-only operators", () => {
    render(
      <ProviderAccountList
        accounts={[providerAccount as never]}
        canCreate={false}
        canEdit={false}
        canRotate={false}
        canTest={false}
        canDelete={false}
        onCreate={vi.fn()}
        onEdit={vi.fn()}
        onLoadMore={vi.fn()}
        onRotate={vi.fn()}
        onToggleStatus={vi.fn()}
        onTest={vi.fn()}
        onDelete={vi.fn()}
      />,
    );

    expect(screen.getByText("stripe-main")).toBeInTheDocument();
    expect(document.querySelector('[data-provider="stripe"]')).not.toBeNull();
    expect(screen.getByLabelText("Credential readiness")).toBeInTheDocument();
    for (const action of ["Add provider account", "Create provider account", "Edit", "Replace credentials", "Test credentials", "Delete", "Disable"]) {
      expect(screen.queryByRole("button", { name: action })).not.toBeInTheDocument();
    }
  });

  it("shows the accountName field verbatim instead of a localized override", () => {
    render(
      <ProviderAccountList
        accounts={[{
          ...providerAccount,
          accountName: "Stripe Main",
          accountNameI18n: { "zh-CN": "本地化旧名", "en-US": "Localized Old Name" },
        } as never]}
        canCreate={false}
        canEdit={false}
        canRotate={false}
        canTest={false}
        canDelete={false}
        onCreate={vi.fn()}
        onEdit={vi.fn()}
        onLoadMore={vi.fn()}
        onRotate={vi.fn()}
        onToggleStatus={vi.fn()}
        onTest={vi.fn()}
        onDelete={vi.fn()}
      />,
    );

    // Operator-edited accountName always wins; i18n overrides are not applied.
    expect(screen.getByText("Stripe Main")).toBeInTheDocument();
    expect(screen.queryByText("本地化旧名")).not.toBeInTheDocument();
    expect(screen.queryByText("Localized Old Name")).not.toBeInTheDocument();
  });

  it("shows an enable/disable status toggle and forwards it to the workspace", () => {
    const onToggleStatus = vi.fn();
    render(
      <ProviderAccountList
        accounts={[providerAccount as never]}
        canCreate={false}
        canEdit
        canRotate={false}
        canTest={false}
        canDelete={false}
        onCreate={vi.fn()}
        onEdit={vi.fn()}
        onLoadMore={vi.fn()}
        onRotate={vi.fn()}
        onToggleStatus={onToggleStatus}
        onTest={vi.fn()}
        onDelete={vi.fn()}
      />,
    );

    expect(screen.queryByRole("button", { name: "Enable" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Disable" }));
    expect(onToggleStatus).toHaveBeenCalledWith(providerAccount);
  });

  it("shows Enable for inactive accounts and hides the toggle for suspended ones", () => {
    const commonProps = {
      canCreate: false,
      canEdit: true,
      canRotate: false,
      canTest: false,
      canDelete: false,
      onCreate: vi.fn(),
      onEdit: vi.fn(),
      onLoadMore: vi.fn(),
      onRotate: vi.fn(),
      onToggleStatus: vi.fn(),
      onTest: vi.fn(),
      onDelete: vi.fn(),
    } as const;

    const { rerender } = render(
      <ProviderAccountList
        {...commonProps}
        accounts={[{ ...providerAccount, status: "inactive" } as never]}
      />,
    );
    expect(screen.queryByRole("button", { name: "Disable" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Enable" })).toBeInTheDocument();

    // Suspended/deprecated accounts are managed through the edit form, so the
    // quick status toggle must not appear for them.
    rerender(
      <ProviderAccountList
        {...commonProps}
        accounts={[{ ...providerAccount, status: "suspended" } as never]}
      />,
    );
    expect(screen.queryByRole("button", { name: "Enable" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Disable" })).not.toBeInTheDocument();
  });

  it("hides sub-merchant create, edit, and delete controls for read-only operators", () => {
    render(
      <SubMerchantManager
        canCreate={false}
        canDelete={false}
        canUpdate={false}
        onCreate={vi.fn()}
        onDelete={vi.fn()}
        onLoadMore={vi.fn()}
        onUpdate={vi.fn()}
        partnerAccount={providerAccount as never}
        subMerchants={[{
          id: "merchant-1",
          providerAccountId: "provider-1",
          subMerchantNo: "merchant-main",
          status: "active",
          createdAt: "2026-07-17T00:00:00.000Z",
          updatedAt: "2026-07-17T00:00:00.000Z",
        } as never]}
      />,
    );

    expect(screen.getAllByText("merchant-main")).toHaveLength(2);
    expect(screen.queryByRole("button", { name: /sub-merchant/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Edit" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Delete" })).not.toBeInTheDocument();
  });
});
