/**
 * Provider account list with status badges, environment/mode indicators, and
 * credential test/replace (rotate) action shortcuts. Designed for the admin workspace.
 */

import { Activity, CheckCircle2, CircleSlash2, Pencil, Power, PowerOff, RotateCcw, Trash2 } from "lucide-react";
import { Badge, Button, IconButton } from "@sdkwork/ui-pc-react";
import { useSdkworkI18n } from "@sdkwork/i18n-pc-react";
import {
  SdkworkPaymentListPaginationControls,
  ADMIN_PROVIDER_LABEL,
  formatAdminTimestamp,
  PaymentProviderIcon,
} from "@sdkwork/payment-pc-admin-core";
import type {
  PaymentProviderAccountView,
  PaymentLastTestStatus,
} from "../types/provider-admin-types";
import { resolveProviderAccountName } from "../types/provider-admin-types";

export interface ProviderAccountListProps {
  accounts: readonly PaymentProviderAccountView[];
  pageInfo?: import("@sdkwork/payment-contracts").SdkWorkPageInfo;
  selectedId?: string;
  busy?: boolean;
  canCreate: boolean;
  canEdit: boolean;
  canRotate: boolean;
  canTest: boolean;
  canDelete: boolean;
  /** Toggle the account between active and inactive (Enable/Disable). */
  onToggleStatus(account: PaymentProviderAccountView): void;
  onEdit(account: PaymentProviderAccountView): void;
  onTest(account: PaymentProviderAccountView): void;
  /** Replace (rotate) the account's credentials with new values. */
  onRotate(account: PaymentProviderAccountView): void;
  onDelete(account: PaymentProviderAccountView): void;
  // Empty-state inline create button callback; parent component wires it to the create dialog
  onCreate(): void;
  onLoadMore(): void;
}

const STATUS_LABEL: Record<PaymentProviderAccountView["status"], string> = {
  active: "Active",
  inactive: "Inactive",
  suspended: "Suspended",
  deprecated: "Deprecated",
};

const STATUS_TONE: Record<
  PaymentProviderAccountView["status"],
  "success" | "secondary" | "warning" | "danger"
> = {
  active: "success",
  inactive: "secondary",
  suspended: "warning",
  deprecated: "danger",
};

const ENV_LABEL: Record<PaymentProviderAccountView["environment"], string> = {
  development: "Dev",
  sandbox: "Sandbox",
  production: "Prod",
};

const TEST_STATUS_LABEL: Record<PaymentLastTestStatus, string> = {
  success: "Healthy",
  failure: "Failed",
  unknown: "Untested",
};

const TEST_STATUS_TONE: Record<PaymentLastTestStatus, "success" | "danger" | "warning"> = {
  success: "success",
  failure: "danger",
  unknown: "warning",
};

/** Compact readiness item: green check when configured, muted slash when not. */
function ReadinessItem({ ready, label }: { ready: boolean; label: string }) {
  return (
    <span className="inline-flex items-center gap-1">
      {ready ? (
        <CheckCircle2 className="h-3.5 w-3.5 text-[var(--sdk-color-text-success)]" />
      ) : (
        <CircleSlash2 className="h-3.5 w-3.5" />
      )}
      {label}
    </span>
  );
}

export function ProviderAccountList(props: ProviderAccountListProps) {
  const i18n = useSdkworkI18n();
  return (
    <div className="space-y-3" data-slot="provider-account-list">
      {props.accounts.length === 0 ? (
        <div className="rounded-md border border-dashed border-[var(--sdk-color-border-subtle)] p-8 text-center text-sm text-[var(--sdk-color-text-secondary)]">
          No provider accounts yet. Create one to configure payment channels.
          {/* Empty-state inline create button: guides users to create a provider account directly */}
          {props.canCreate ? <div className="mt-3">
            <Button
              type="button"
              variant="primary"
              size="sm"
              onClick={props.onCreate}
              disabled={props.busy}
            >
              Create provider account
            </Button>
          </div> : null}
        </div>
      ) : (
        <div className="space-y-3">
          <div className="flex items-center justify-between">
            <div className="text-xs font-semibold uppercase tracking-wider text-[var(--sdk-color-text-muted)]">
              Provider Accounts
            </div>
            {/* Persistent create button: keeps the add-provider-account entry visible
                even when the list already has records. */}
            {props.canCreate ? <Button
              type="button"
              variant="primary"
              size="sm"
              onClick={props.onCreate}
              disabled={props.busy}
            >
              Add provider account
            </Button> : null}
          </div>
          <ul className="divide-y divide-[var(--sdk-color-border-subtle)] rounded-md border border-[var(--sdk-color-border-subtle)]">
          {props.accounts.map((account) => {
            const isSelected = props.selectedId === account.id;
            return (
              <li
                key={account.id}
                aria-current={isSelected ? "true" : undefined}
                className={
                  "flex flex-col gap-3 p-4 sm:flex-row sm:items-center sm:justify-between " +
                  (isSelected ? "bg-[var(--sdk-color-bg-subtle)]" : "")
                }
                data-slot="provider-account-row"
              >
                <div className="flex min-w-0 flex-1 items-start gap-3">
                  <PaymentProviderIcon
                    label={ADMIN_PROVIDER_LABEL[account.providerCode]}
                    providerCode={account.providerCode}
                    size="md"
                  />
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="font-semibold text-[var(--sdk-color-text-primary)]">
                        {resolveProviderAccountName(account, i18n?.localeTag)}
                      </span>
                      <Badge variant="outline">{ADMIN_PROVIDER_LABEL[account.providerCode]}</Badge>
                      <Badge variant="outline">{ENV_LABEL[account.environment]}</Badge>
                      <Badge variant={STATUS_TONE[account.status]}>{STATUS_LABEL[account.status]}</Badge>
                    </div>
                    <p className="mt-1 truncate text-xs text-[var(--sdk-color-text-secondary)]">
                      <span>{account.accountMode === "partner" ? "Partner / ISV" : "Direct"}</span>
                      <span aria-hidden="true"> · </span>
                      <span>Merchant ID:</span>{" "}
                      <span className="font-medium text-[var(--sdk-color-text-primary)]">{account.merchantId ?? "--"}</span>
                      <span aria-hidden="true"> · </span>
                      <span>Settlement:</span>{" "}
                      <span className="font-medium text-[var(--sdk-color-text-primary)]">
                        {account.settlementCurrency}{account.countryCode ? ` / ${account.countryCode}` : ""}
                      </span>
                      <span aria-hidden="true"> · </span>
                      <span>Last test:</span>{" "}
                      <span className="font-medium text-[var(--sdk-color-text-primary)]">
                        {account.lastTestedAt ? formatAdminTimestamp(account.lastTestedAt) : "Run before activation"}
                      </span>
                    </p>
                    <div
                      className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-[var(--sdk-color-text-secondary)]"
                      aria-label="Credential readiness"
                    >
                      <ReadinessItem ready={account.hasPrimarySecret} label="Primary secret" />
                      <ReadinessItem ready={account.hasWebhookSecret} label="Webhook secret" />
                      <ReadinessItem ready={account.hasCertificate} label="Certificate" />
                      <Badge variant={TEST_STATUS_TONE[account.lastTestStatus ?? "unknown"]}>
                        {TEST_STATUS_LABEL[account.lastTestStatus ?? "unknown"]}
                      </Badge>
                    </div>
                  </div>
                </div>
                <div className="flex flex-wrap items-center justify-end gap-1 sm:self-center">
                  {props.canTest ? <IconButton
                    type="button"
                    variant="ghost"
                    aria-label="Test credentials"
                    onClick={() => props.onTest(account)}
                    disabled={props.busy}
                    title="Cannot test while another operation is in progress"
                  >
                    <Activity className="h-4 w-4" />
                  </IconButton> : null}
                  {props.canRotate ? <IconButton
                    type="button"
                    variant="ghost"
                    aria-label="Replace credentials"
                    onClick={() => props.onRotate(account)}
                    disabled={props.busy}
                    title="Cannot replace credentials while another operation is in progress"
                  >
                    <RotateCcw className="h-4 w-4" />
                  </IconButton> : null}
                  {/* Status toggle only applies to active/inactive accounts;
                      suspended/deprecated accounts are state changes handled
                      through the edit form. */}
                  {(account.status === "active" || account.status === "inactive") && props.canEdit ? <IconButton
                    type="button"
                    variant={account.status === "active" ? "danger" : "ghost"}
                    aria-label={account.status === "active" ? "Disable" : "Enable"}
                    onClick={() => props.onToggleStatus(account)}
                    disabled={props.busy}
                    title="Cannot change status while another operation is in progress"
                  >
                    {account.status === "active" ? <PowerOff className="h-4 w-4" /> : <Power className="h-4 w-4" />}
                  </IconButton> : null}
                  {props.canEdit ? <IconButton
                    type="button"
                    variant="ghost"
                    aria-label="Edit"
                    onClick={() => props.onEdit(account)}
                    disabled={props.busy}
                    title="Cannot edit while another operation is in progress"
                  >
                    <Pencil className="h-4 w-4" />
                  </IconButton> : null}
                  {props.canDelete ? <IconButton
                    type="button"
                    variant="danger"
                    aria-label="Delete"
                    onClick={() => props.onDelete(account)}
                    disabled={props.busy}
                    title="Cannot delete while another operation is in progress"
                  >
                    <Trash2 className="h-4 w-4" />
                  </IconButton> : null}
                </div>
              </li>
            );
          })}
          </ul>
        </div>
      )}
      <SdkworkPaymentListPaginationControls
        busy={props.busy ?? false}
        onLoadMore={props.onLoadMore}
        pageInfo={props.pageInfo}
      />
    </div>
  );
}
