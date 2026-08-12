/**
 * Sub-merchant manager for partner/ISV provider accounts.
 *
 * Renders a list of sub-merchants under the selected partner provider account
 * (Alipay sub_appid / WeChat sub_mch_id / Stripe Connected Account). Supports
 * create/update/delete with a lightweight inline form.
 *
 * Field names mirror the backend OpenAPI `SubMerchant` /
 * `CreateSubMerchantCommand` / `UpdateSubMerchantCommand` schemas.
 */

import * as React from "react";
import { usePaymentAdminMessages } from "@sdkwork/payment-pc-admin-core";
import {
  Badge,
  Button,
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  Input,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@sdkwork/ui-pc-react";
import {
  SdkworkPaymentListPaginationControls,
  AdminFieldLabel,
  ConfirmDialog,
  PaymentProviderIcon,
} from "@sdkwork/payment-pc-admin-core";
import type {
  PaymentProviderAccountView,
  PaymentSubMerchantStatus,
  PaymentSubMerchantView,
  PaymentSubMerchantDraft,
  PaymentSubMerchantUpdateDraft,
} from "../types/provider-admin-types";

const STATUS_OPTIONS: readonly { label: string; value: PaymentSubMerchantStatus }[] = [
  { label: "Active", value: "active" },
  { label: "Inactive", value: "inactive" },
  { label: "Suspended", value: "suspended" },
  { label: "Deprecated", value: "deprecated" },
];

const STATUS_LABEL: Readonly<Record<PaymentSubMerchantStatus, string>> = STATUS_OPTIONS.reduce(
  (acc, option) => {
    acc[option.value] = option.label;
    return acc;
  },
  {} as Record<PaymentSubMerchantStatus, string>,
);

const STATUS_TONE: Record<
  PaymentSubMerchantStatus,
  "success" | "secondary" | "warning" | "danger"
> = {
  active: "success",
  inactive: "secondary",
  suspended: "warning",
  deprecated: "danger",
};

export interface SubMerchantManagerProps {
  partnerAccount?: PaymentProviderAccountView;
  subMerchants: readonly PaymentSubMerchantView[];
  pageInfo?: import("@sdkwork/payment-contracts").SdkWorkPageInfo;
  busy?: boolean;
  canCreate: boolean;
  canDelete: boolean;
  canUpdate: boolean;
  onCreate(draft: PaymentSubMerchantDraft): Promise<void> | void;
  onUpdate(id: string, draft: PaymentSubMerchantUpdateDraft): Promise<void> | void;
  onDelete(id: string): Promise<void> | void;
  onLoadMore(): void;
}

interface FormState {
  subMerchantNo: string;
  subMerchantName: string;
  subAppId: string;
  subMchId: string;
  stripeConnectedAccountId: string;
  status: PaymentSubMerchantStatus;
}

function emptyFormState(): FormState {
  return {
    subMerchantNo: "",
    subMerchantName: "",
    subAppId: "",
    subMchId: "",
    stripeConnectedAccountId: "",
    status: "active",
  };
}

function fromSubMerchant(view: PaymentSubMerchantView): FormState {
  return {
    subMerchantNo: view.subMerchantNo,
    subMerchantName: view.subMerchantName ?? "",
    subAppId: view.subAppId ?? "",
    subMchId: view.subMchId ?? "",
    stripeConnectedAccountId: view.stripeConnectedAccountId ?? "",
    status: view.status,
  };
}

export function SubMerchantManager(props: SubMerchantManagerProps) {
  const phrases = usePaymentAdminMessages().legacy.phrases;
  const t = (key: string) => phrases[key] ?? key;
  const [dialogOpen, setDialogOpen] = React.useState(false);
  const [editing, setEditing] = React.useState<PaymentSubMerchantView | undefined>();
  const [formState, setFormState] = React.useState<FormState>(emptyFormState);
  const [submitting, setSubmitting] = React.useState(false);
  const [error, setError] = React.useState<string | undefined>();
  const [pendingDelete, setPendingDelete] = React.useState<PaymentSubMerchantView | null>(null);

  const partnerAccount = props.partnerAccount;
  const providerCode = partnerAccount?.providerCode;

  function openCreate() {
    setEditing(undefined);
    setFormState(emptyFormState());
    setError(undefined);
    setDialogOpen(true);
  }

  function openEdit(view: PaymentSubMerchantView) {
    setEditing(view);
    setFormState(fromSubMerchant(view));
    setError(undefined);
    setDialogOpen(true);
  }

  function closeDialog() {
    setDialogOpen(false);
    setEditing(undefined);
    setError(undefined);
  }

  async function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!partnerAccount) {
      setError(t("Select a partner provider account first."));
      return;
    }
    if (!formState.subMerchantNo.trim()) {
      setError("Sub-merchant number is required.");
      return;
    }
    setSubmitting(true);
    setError(undefined);
    try {
      const trimmedNo = formState.subMerchantNo.trim();
      const trimmedName = formState.subMerchantName.trim();
      const trimmedAppId = formState.subAppId.trim();
      const trimmedMchId = formState.subMchId.trim();
      const trimmedStripeId = formState.stripeConnectedAccountId.trim();

      if (editing) {
        const draft: PaymentSubMerchantUpdateDraft = {
          ...(trimmedName ? { subMerchantName: trimmedName } : {}),
          ...(trimmedAppId ? { subAppId: trimmedAppId } : {}),
          ...(trimmedMchId ? { subMchId: trimmedMchId } : {}),
          ...(trimmedStripeId ? { stripeConnectedAccountId: trimmedStripeId } : {}),
          status: formState.status,
        };
        await props.onUpdate(editing.id, draft);
      } else {
        const draft: PaymentSubMerchantDraft = {
          providerAccountId: partnerAccount.id,
          subMerchantNo: trimmedNo,
          providerCode: partnerAccount.providerCode,
          ...(trimmedName ? { subMerchantName: trimmedName } : {}),
          ...(trimmedAppId ? { subAppId: trimmedAppId } : {}),
          ...(trimmedMchId ? { subMchId: trimmedMchId } : {}),
          ...(trimmedStripeId ? { stripeConnectedAccountId: trimmedStripeId } : {}),
          status: formState.status,
        };
        await props.onCreate(draft);
      }
      closeDialog();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to save sub-merchant.");
    } finally {
      setSubmitting(false);
    }
  }

  async function handleConfirmDelete() {
    if (!pendingDelete) return;
    setError(undefined);
    try {
      await props.onDelete(pendingDelete.id);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to delete sub-merchant.");
    }
    setPendingDelete(null);
  }

  if (!partnerAccount) {
    return (
      <div className="rounded-md border border-dashed border-[var(--sdk-color-border-subtle)] p-8 text-center text-sm text-[var(--sdk-color-text-secondary)]">
        Select a partner provider account above to manage its sub-merchants.
      </div>
    );
  }

  const providerHint = providerCode === "alipay"
    ? t("Alipay sub_appid (offline merchant expansion)")
    : providerCode === "wechat_pay"
      ? "WeChat sub_mch_id (sub-merchant under service provider)"
      : providerCode === "stripe"
        ? "Stripe Connected Account id"
        : t("Sandbox sub-merchant id");

  return (
    <div className="space-y-3" data-slot="sub-merchant-manager">
      <div className="flex items-center justify-between">
        <div className="flex min-w-0 items-start gap-3">
          <PaymentProviderIcon providerCode={partnerAccount.providerCode} size="md" />
          <div className="min-w-0">
            <div className="text-xs font-semibold uppercase text-[var(--sdk-color-text-muted)]">
              Sub-Merchants · {partnerAccount.accountNo}
            </div>
            <div className="mt-1 text-xs text-[var(--sdk-color-text-secondary)]">
              {providerHint}
            </div>
          </div>
        </div>
        {props.canCreate ? <Button type="button" size="sm" onClick={openCreate}>
          Add sub-merchant
        </Button> : null}
      </div>

      {props.subMerchants.length === 0 ? (
        <div className="rounded-md border border-dashed border-[var(--sdk-color-border-subtle)] p-6 text-center text-sm text-[var(--sdk-color-text-secondary)]">
          No sub-merchants configured under this partner account.
          {props.canCreate ? <div className="mt-3">
            <Button type="button" variant="primary" size="sm" onClick={openCreate} disabled={submitting}>
              Create sub-merchant
            </Button>
          </div> : null}
        </div>
      ) : (
        <ul className="divide-y divide-[var(--sdk-color-border-subtle)] rounded-md border border-[var(--sdk-color-border-subtle)]">
          {props.subMerchants.map((merchant) => (
            <li
              key={merchant.id}
              className="flex flex-col gap-3 p-4 sm:flex-row sm:items-center sm:justify-between"
              data-slot="sub-merchant-row"
            >
              <div className="flex min-w-0 flex-1 items-start gap-3">
                <PaymentProviderIcon providerCode={merchant.providerCode} size="sm" />
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="font-medium text-[var(--sdk-color-text)]">
                      {merchant.subMerchantName || merchant.subMerchantNo}
                    </span>
                    <Badge variant="outline">{merchant.subMerchantNo}</Badge>
                    {merchant.subAppId ? (
                      <Badge variant="secondary">sub_appid: {merchant.subAppId}</Badge>
                    ) : null}
                    {merchant.subMchId ? (
                      <Badge variant="secondary">sub_mch_id: {merchant.subMchId}</Badge>
                    ) : null}
                    {merchant.stripeConnectedAccountId ? (
                      <Badge variant="secondary">stripe: {merchant.stripeConnectedAccountId}</Badge>
                    ) : null}
                    <Badge variant={STATUS_TONE[merchant.status]}>{STATUS_LABEL[merchant.status]}</Badge>
                  </div>
                </div>
              </div>
              <div className="flex items-center gap-2">
                {props.canUpdate ? <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  onClick={() => openEdit(merchant)}
                  disabled={props.busy}
                  title="Cannot edit while another operation is in progress"
                >
                  Edit
                </Button> : null}
                {props.canDelete ? <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  onClick={() => setPendingDelete(merchant)}
                  disabled={props.busy}
                  title="Cannot delete while another operation is in progress"
                >
                  Delete
                </Button> : null}
              </div>
            </li>
          ))}
        </ul>
      )}

      <SdkworkPaymentListPaginationControls
        busy={props.busy ?? false}
        onLoadMore={props.onLoadMore}
        pageInfo={props.pageInfo}
      />

      <Dialog open={(props.canCreate || props.canUpdate) && dialogOpen} onOpenChange={setDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {editing ? t("Edit sub-merchant") : t("Create sub-merchant")}
            </DialogTitle>
          </DialogHeader>
          <form className="space-y-4" onSubmit={handleSubmit}>
            <AdminFieldLabel label="Sub-Merchant No" htmlFor="sub-merchant-no" required>
              <Input
                id="sub-merchant-no"
                value={formState.subMerchantNo}
                onChange={(event) =>
                  setFormState((prev) => ({ ...prev, subMerchantNo: event.target.value }))
                }
                placeholder={providerHint}
                disabled={Boolean(editing)}
                required
              />
            </AdminFieldLabel>
            <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
              <AdminFieldLabel label="Sub-Merchant Name" htmlFor="sub-merchant-name">
                <Input
                  id="sub-merchant-name"
                  value={formState.subMerchantName}
                  onChange={(event) =>
                    setFormState((prev) => ({ ...prev, subMerchantName: event.target.value }))
                  }
                  placeholder="Human-friendly label"
                />
              </AdminFieldLabel>
              <AdminFieldLabel label="Sub AppID" htmlFor="sub-merchant-sub-app-id">
                <Input
                  id="sub-merchant-sub-app-id"
                  value={formState.subAppId}
                  onChange={(event) =>
                    setFormState((prev) => ({ ...prev, subAppId: event.target.value }))
                  }
                  placeholder={
                    providerCode === "alipay"
                      ? t("Alipay sub_appid")
                      : providerCode === "wechat_pay"
                        ? "WeChat sub appid"
                        : t("Optional sub app id")
                  }
                />
              </AdminFieldLabel>
              <AdminFieldLabel label="Sub Merchant ID" htmlFor="sub-merchant-sub-mch-id">
                <Input
                  id="sub-merchant-sub-mch-id"
                  value={formState.subMchId}
                  onChange={(event) =>
                    setFormState((prev) => ({ ...prev, subMchId: event.target.value }))
                  }
                  placeholder={
                    providerCode === "wechat_pay" ? t("WeChat sub_mch_id") : t("Optional sub merchant id")
                  }
                />
              </AdminFieldLabel>
              <AdminFieldLabel
                label="Stripe Connected Account ID"
                htmlFor="sub-merchant-stripe-id"
              >
                <Input
                  id="sub-merchant-stripe-id"
                  value={formState.stripeConnectedAccountId}
                  onChange={(event) =>
                    setFormState((prev) => ({
                      ...prev,
                      stripeConnectedAccountId: event.target.value,
                    }))
                  }
                  placeholder={
                    providerCode === "stripe" ? "acct_..." : "Optional (Stripe only)"
                  }
                />
              </AdminFieldLabel>
              <AdminFieldLabel label="Status" htmlFor="sub-merchant-status">
                <Select
                  value={formState.status}
                  onValueChange={(value) =>
                    setFormState((prev) => ({
                      ...prev,
                      status: value as PaymentSubMerchantStatus,
                    }))
                  }
                >
                  <SelectTrigger id="sub-merchant-status">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {STATUS_OPTIONS.map((option) => (
                      <SelectItem key={option.value} value={option.value}>
                        {option.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </AdminFieldLabel>
            </div>
            {error ? (
              <div
                role="alert"
                className="rounded-md border border-[var(--sdk-color-border-error)] bg-[var(--sdk-color-bg-error-subtle)] p-3 text-sm text-[var(--sdk-color-text-error)]"
              >
                {error}
              </div>
            ) : null}
            <div className="flex justify-end gap-2">
              <Button
                type="button"
                variant="ghost"
                onClick={closeDialog}
                disabled={submitting}
                title="Saving in progress..."
              >
                Cancel
              </Button>
              <Button type="submit" disabled={submitting} title="Saving in progress...">
                {submitting ? t("Saving...") : editing ? t("Save changes") : t("Create sub-merchant")}
              </Button>
            </div>
          </form>
        </DialogContent>
      </Dialog>

      <ConfirmDialog
        open={props.canDelete && pendingDelete !== null}
        title="Delete sub-merchant?"
        description={
          pendingDelete
            ? `Delete sub-merchant ${pendingDelete.subMerchantNo}? This action cannot be undone.`
            : ""
        }
        confirmLabel="Delete"
        variant="danger"
        busy={props.busy}
        onConfirm={handleConfirmDelete}
        onOpenChange={(open) => {
          if (!open) setPendingDelete(null);
        }}
      />
    </div>
  );
}
