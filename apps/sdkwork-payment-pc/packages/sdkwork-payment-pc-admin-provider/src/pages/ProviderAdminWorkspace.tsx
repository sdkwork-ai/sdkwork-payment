/**
 * Provider admin workspace.
 *
 * Two-tab workspace:
 *   1. Provider Accounts — list + create/edit form + test/replace actions
 *   2. Sub-Merchants — manage sub-merchants under a selected partner account
 *
 * Uses an external store subscription pattern (subscribe/getState) so the host
 * app can wire it into React's useSyncExternalStore if needed.
 */

import * as React from "react";
import {
  Button,
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  Switch,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
  Textarea,
  Toaster,
  toast,
} from "@sdkwork/ui-pc-react";
import {
  AdminFieldLabel,
  ConfirmDialog,
  PaymentAdminI18nBoundary,
  PaymentAdminTabsContent,
  PaymentAdminTabsList,
  PaymentAdminTabsTrigger,
  PaymentAdminWorkspace,
  PemFilePicker,
} from "@sdkwork/payment-pc-admin-core";
import {
  ProviderAccountForm,
  certificateLabel,
  primarySecretLabel,
  webhookSecretLabel,
} from "../components/ProviderAccountForm";
import {
  ProviderAccountFillInGuide,
  ProviderAccountFillInGuideLink,
} from "../components/ProviderAccountFillInGuide";
import { ProviderAccountList } from "../components/ProviderAccountList";
import { SubMerchantManager } from "../components/SubMerchantManager";
import { useSdkworkI18n } from "@sdkwork/i18n-pc-react";
import type {
  PaymentBaseDataOption,
  PaymentCredentialRotateDraft,
  PaymentProviderAccountDraft,
  PaymentProviderAccountUpdateDraft,
  PaymentProviderAdminController,
  PaymentProviderAdminState,
  PaymentProviderAccountView,
} from "../types/provider-admin-types";
import { resolveProviderAccountName } from "../types/provider-admin-types";

export interface PaymentProviderAdminWorkspaceProps {
  controller: PaymentProviderAdminController;
  capabilities: PaymentProviderAdminCapabilities;
  section?: PaymentProviderAdminSection;
  title?: string;
  description?: string;
  /** Optional actions rendered at the far right of the tab list row. */
  tabActions?: React.ReactNode;
  /**
   * Base-data options resolved by the host app (sdkwork-appbase base-data
   * capability). When empty/omitted the account form degrades to free-text
   * country/currency inputs.
   */
  countryOptions?: readonly PaymentBaseDataOption[];
  currencyOptions?: readonly PaymentBaseDataOption[];
}

export type PaymentProviderAdminSection = "accounts" | "submerchants";

export interface PaymentProviderAdminCapabilities {
  canCreateProviderAccount: boolean;
  canUpdateProviderAccount: boolean;
  canDeleteProviderAccount: boolean;
  canTestProviderAccount: boolean;
  canRotateProviderCredentials: boolean;
  canCreateSubMerchant: boolean;
  canUpdateSubMerchant: boolean;
  canDeleteSubMerchant: boolean;
}

type DialogState =
  | { kind: "closed" }
  | { kind: "create" }
  | { kind: "edit"; account: PaymentProviderAccountView }
  | { kind: "delete"; account: PaymentProviderAccountView }
  | { kind: "test"; account: PaymentProviderAccountView }
  | { kind: "rotate"; account: PaymentProviderAccountView };

export function PaymentProviderAdminWorkspace(
  props: PaymentProviderAdminWorkspaceProps,
) {
  const { controller } = props;
  const i18n = useSdkworkI18n();
  const [state, setState] = React.useState<PaymentProviderAdminState>(() => controller.getState());
  const [tab, setTab] = React.useState<PaymentProviderAdminSection>("accounts");
  const activeSection = props.section ?? tab;
  const [dialog, setDialog] = React.useState<DialogState>({ kind: "closed" });
  const [guideOpen, setGuideOpen] = React.useState(false);
  const [disableTarget, setDisableTarget] = React.useState<PaymentProviderAccountView | null>(null);
  // Guards the enable/disable chains against double-clicks and concurrent
  // invocations: React state (and therefore the row busy prop) updates
  // asynchronously, so a second click can slip in before the first mutation
  // reaches the controller. Handlers catch their own errors, so the lock is
  // always released in `finally`.
  const statusMutationLockRef = React.useRef(false);

  function runStatusMutation(action: () => Promise<void>) {
    if (statusMutationLockRef.current) {
      return;
    }
    statusMutationLockRef.current = true;
    void action().finally(() => {
      statusMutationLockRef.current = false;
    });
  }

  React.useEffect(() => {
    return controller.subscribe(() => {
      setState(controller.getState());
    });
  }, [controller]);

  React.useEffect(() => {
    void controller.load().then(setState).catch((error) => {
      toast.error(
        error instanceof Error ? error.message : "Failed to load provider admin data.",
      );
    });
  }, [controller]);

  const partnerAccounts = React.useMemo(
    () => state.providerAccounts.filter((account) => account.accountMode === "partner"),
    [state.providerAccounts],
  );

  const selectedPartnerAccount = state.selectedProviderAccount?.accountMode === "partner"
    ? state.selectedProviderAccount
    : partnerAccounts[0];

  const visibleSubMerchants = React.useMemo(() => {
    if (selectedPartnerAccount) {
      return state.subMerchants.filter(
        (merchant) => merchant.providerAccountId === selectedPartnerAccount.id,
      );
    }
    return state.subMerchants;
  }, [state.subMerchants, selectedPartnerAccount]);

  async function handleCreate(draft: PaymentProviderAccountDraft) {
    await controller.createProviderAccount(draft);
    setDialog({ kind: "closed" });
    toast.success("Provider account created.");
  }

  async function handleUpdate(draft: PaymentProviderAccountUpdateDraft) {
    if (dialog.kind !== "edit") {
      return;
    }
    if (draft.status === "active") {
      await controller.updateProviderAccount(dialog.account.id, {
        ...draft,
        status: "inactive",
      });
      const result = await controller.testProviderAccount(dialog.account.id, {
        environment: draft.environment ?? dialog.account.environment,
        dryRun: true,
      });
      if (!result.ok) {
        throw new Error(result.diagnostic ?? "Provider account readiness validation failed.");
      }
      await controller.updateProviderAccount(dialog.account.id, { status: "active" });
    } else {
      await controller.updateProviderAccount(dialog.account.id, draft);
    }
    setDialog({ kind: "closed" });
    toast.success("Provider account updated.");
  }

  async function handleTest() {
    if (dialog.kind !== "test") {
      return;
    }
    const account = dialog.account;
    setDialog({ kind: "closed" });
    const loadingToast = toast.loading("Testing...");
    try {
      const result = await controller.testProviderAccount(account.id, {
        environment: account.environment,
        dryRun: true,
      });
      if (result.ok) {
        toast.success("Credentials verified", { id: loadingToast });
      } else {
        toast.error("Credential test failed", {
          id: loadingToast,
          description: result.diagnostic,
        });
      }
    } catch (error) {
      toast.error(
        error instanceof Error ? error.message : "Failed to test provider account credentials.",
        { id: loadingToast },
      );
    }
  }

  async function handleRotate(draft: PaymentCredentialRotateDraft) {
    if (dialog.kind !== "rotate") {
      return;
    }
    await controller.rotateProviderAccountCredentials(dialog.account.id, draft);
    setDialog({ kind: "closed" });
    toast.success("Credentials replaced.");
  }

  async function handleDelete() {
    if (dialog.kind !== "delete") {
      return;
    }
    try {
      await controller.deleteProviderAccount(dialog.account.id);
      setDialog({ kind: "closed" });
      toast.success("Provider account deleted.");
    } catch (error) {
      toast.error(
        error instanceof Error ? error.message : "Failed to delete provider account.",
      );
    }
  }

  // Row status toggle entry point: disabling an active account asks for
  // confirmation first (it takes live payment routing offline); enabling runs
  // the validated activation flow below.
  function handleToggleStatus(account: PaymentProviderAccountView) {
    if (account.status === "active") {
      setDisableTarget(account);
      return;
    }
    runStatusMutation(() => handleEnable(account));
  }

  async function handleEnable(account: PaymentProviderAccountView) {
    const loadingToast = toast.loading("Enabling...");
    try {
      // The toggle only renders for inactive accounts, so no status patch is
      // needed first: a dry-run test refreshes last_tested_at, then the
      // activation patch triggers the backend readiness guard, which
      // atomically validates test freshness, mock config, credential
      // completeness, and the one-active-account-per-provider rule.
      const result = await controller.testProviderAccount(account.id, {
        environment: account.environment,
        dryRun: true,
      });
      if (!result.ok) {
        throw new Error("Provider account readiness validation failed.", {
          cause: result.diagnostic,
        });
      }
      await controller.updateProviderAccount(account.id, { status: "active" });
      toast.success("Provider account enabled.", { id: loadingToast });
    } catch (error) {
      // The backend readiness guard rejects activation when another account of
      // the same provider is already active; report that conflict precisely
      // instead of the generic guard message when we can see the duplicate.
      const duplicateActive = state.providerAccounts.some(
        (other) => other.id !== account.id
          && other.providerCode === account.providerCode
          && other.status === "active",
      );
      const diagnostic = error instanceof Error
        ? typeof error.cause === "string"
          ? error.cause
          : undefined
        : undefined;
      toast.error(
        duplicateActive
          ? "Another active account for this provider exists. Disable it before enabling this one."
          : error instanceof Error
            ? error.message
            : "Failed to change provider account status.",
        {
          id: loadingToast,
          ...(diagnostic ? { description: diagnostic } : {}),
        },
      );
    }
  }

  async function handleDisable(account: PaymentProviderAccountView) {
    const loadingToast = toast.loading("Disabling...");
    try {
      await controller.updateProviderAccount(account.id, { status: "inactive" });
      toast.success("Provider account disabled.", { id: loadingToast });
    } catch (error) {
      toast.error(
        error instanceof Error
          ? error.message
          : "Failed to change provider account status.",
        { id: loadingToast },
      );
    } finally {
      setDisableTarget(null);
    }
  }

  function loadProviderAccounts() {
    void controller.loadMoreProviderAccounts().catch((error) => {
      toast.error(
        error instanceof Error ? error.message : "Failed to load more provider accounts.",
      );
    });
  }

  function loadSubMerchants(providerAccountId?: string) {
    void controller.loadMoreSubMerchants(providerAccountId).catch((error) => {
      toast.error(
        error instanceof Error ? error.message : "Failed to load more sub-merchants.",
      );
    });
  }

  return (
    <PaymentAdminI18nBoundary>
      <Toaster />
      <PaymentAdminWorkspace
        className="flex h-full min-h-0 flex-col"
        data-slot="payment-provider-admin-workspace"
        description={props.description}
        title={props.title ?? "Provider accounts & sub-merchants"}
      >
        <Tabs
          className="flex min-h-0 flex-1 flex-col"
          value={activeSection}
          onValueChange={(value) => {
            if (!props.section) {
              setTab(value as PaymentProviderAdminSection);
            }
          }}
        >
          {!props.section ? (
            <div className="flex items-center justify-between gap-2">
              <PaymentAdminTabsList aria-label="Payment provider sections">
                <PaymentAdminTabsTrigger value="accounts">Provider accounts</PaymentAdminTabsTrigger>
                <PaymentAdminTabsTrigger value="submerchants">Sub-merchants</PaymentAdminTabsTrigger>
              </PaymentAdminTabsList>
              {props.tabActions ?? null}
            </div>
          ) : null}
          <PaymentAdminTabsContent className="min-h-0 flex-1 overflow-y-auto" value="accounts">
            {/* "testing" keeps row actions disabled during the dry-run step of
                the enable flow so the buttons do not flicker back mid-sequence. */}
            <ProviderAccountList
              accounts={state.providerAccounts}
              pageInfo={state.listPageInfo?.providerAccounts}
              selectedId={state.selectedProviderAccount?.id}
              busy={state.status === "saving" || state.status === "loading" || state.status === "testing"}
              canCreate={props.capabilities.canCreateProviderAccount}
              canEdit={props.capabilities.canUpdateProviderAccount}
              canRotate={props.capabilities.canRotateProviderCredentials}
              canTest={props.capabilities.canTestProviderAccount}
              canDelete={props.capabilities.canDeleteProviderAccount}
              onToggleStatus={(account) => void handleToggleStatus(account)}
              onEdit={(account) => setDialog({ kind: "edit", account })}
              onTest={(account) => setDialog({ kind: "test", account })}
              onRotate={(account) => setDialog({ kind: "rotate", account })}
              onDelete={(account) => setDialog({ kind: "delete", account })}
              onLoadMore={loadProviderAccounts}
              onCreate={() => setDialog({ kind: "create" })}
            />
          </PaymentAdminTabsContent>
          <PaymentAdminTabsContent className="min-h-0 flex-1 overflow-y-auto" value="submerchants">
            <div className="space-y-4">
              {partnerAccounts.length > 0 ? (
                <div className="flex flex-col gap-2 sm:max-w-sm">
                  <label
                    className="text-xs font-medium text-[var(--sdk-color-text-secondary)]"
                    htmlFor="payment-provider-partner-account"
                  >
                    Selected partner account
                  </label>
                  <select
                    className="h-9 w-full rounded-[var(--sdk-radius-control)] border border-[var(--sdk-color-border-default)] bg-[var(--sdk-color-surface-panel)] px-3 text-sm text-[var(--sdk-color-text-primary)] outline-none focus:border-[var(--sdk-color-border-focus)] focus:ring-2 focus:ring-[var(--sdk-color-border-focus)]"
                    id="payment-provider-partner-account"
                    value={selectedPartnerAccount?.id ?? ""}
                    onChange={(event) => {
                      const nextId = event.target.value;
                      if (!nextId) {
                        controller.selectProviderAccount(undefined);
                        return;
                      }
                      const account = partnerAccounts.find((item) => item.id === nextId);
                      if (account) {
                        controller.selectProviderAccount(account.id);
                        loadSubMerchants(account.id);
                      }
                    }}
                  >
                    {partnerAccounts.map((account) => (
                      <option key={account.id} value={account.id}>
                        {resolveProviderAccountName(account, i18n?.localeTag)} ({account.providerCode})
                      </option>
                    ))}
                  </select>
                </div>
              ) : null}
              <SubMerchantManager
                partnerAccount={selectedPartnerAccount}
                subMerchants={visibleSubMerchants}
                pageInfo={state.listPageInfo?.subMerchants}
                busy={state.status === "saving" || state.status === "loading"}
                canCreate={props.capabilities.canCreateSubMerchant}
                canDelete={props.capabilities.canDeleteSubMerchant}
                canUpdate={props.capabilities.canUpdateSubMerchant}
                onCreate={(draft) => void controller.createSubMerchant(draft)}
                onUpdate={(id, draft) => void controller.updateSubMerchant(id, draft)}
                onDelete={(id) => void controller.deleteSubMerchant(id)}
                onLoadMore={() => loadSubMerchants(selectedPartnerAccount?.id)}
              />
            </div>
          </PaymentAdminTabsContent>
        </Tabs>

      <Dialog
        open={dialog.kind === "create" || dialog.kind === "edit"}
        onOpenChange={(open) => {
          if (!open) {
            setDialog({ kind: "closed" });
            setGuideOpen(false);
          }
        }}
      >
        <DialogContent
          className="max-h-[calc(100dvh-3rem)] overflow-y-auto"
          style={{ width: "60vw" }}
        >
          <DialogHeader>
            <div className="flex items-center justify-between gap-2">
              <DialogTitle>
                {dialog.kind === "create" ? "Create provider account" : "Edit provider account"}
              </DialogTitle>
              <ProviderAccountFillInGuideLink onClick={() => setGuideOpen(true)} />
            </div>
          </DialogHeader>
          {dialog.kind === "create" || dialog.kind === "edit" ? (
            <ProviderAccountForm
              mode={dialog.kind === "create" ? "create" : "update"}
              initial={dialog.kind === "edit" ? dialog.account : undefined}
              partnerAccountOptions={partnerAccounts}
              countryOptions={props.countryOptions}
              currencyOptions={props.currencyOptions}
              onCancel={() => setDialog({ kind: "closed" })}
              onSubmit={
                dialog.kind === "create"
                  ? (draft) => handleCreate(draft as PaymentProviderAccountDraft)
                  : (draft) => handleUpdate(draft as PaymentProviderAccountUpdateDraft)
              }
            />
          ) : null}
        </DialogContent>
      </Dialog>

      <ProviderAccountFillInGuide open={guideOpen} onOpenChange={setGuideOpen} />

      <Dialog
        open={dialog.kind === "test"}
        onOpenChange={(open) => {
          if (!open) {
            setDialog({ kind: "closed" });
          }
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Test provider credentials</DialogTitle>
          </DialogHeader>
          {dialog.kind === "test" ? (
            <div className="space-y-3">
              <p className="text-sm text-[var(--sdk-color-text-secondary)]">
                Validate the saved credentials and provider adapter for{" "}
                <strong>{resolveProviderAccountName(dialog.account, i18n?.localeTag)}</strong> ({dialog.account.providerCode} /{" "}
                {dialog.account.environment}). The result updates the provider account's
                <code className="mx-1 rounded bg-[var(--sdk-color-bg-subtle)] px-1 text-xs">
                  last_tested_at
                </code>
                and
                <code className="mx-1 rounded bg-[var(--sdk-color-bg-subtle)] px-1 text-xs">
                  last_test_status
                </code>
                fields.
              </p>
              <div className="flex justify-end gap-2">
                <Button
                  type="button"
                  variant="ghost"
                  onClick={() => setDialog({ kind: "closed" })}
                >
                  Cancel
                </Button>
                <Button
                  type="button"
                  onClick={() => void handleTest()}
                  disabled={state.status === "testing"}
                >
                  {state.status === "testing" ? "Testing..." : "Run test"}
                </Button>
              </div>
            </div>
          ) : null}
        </DialogContent>
      </Dialog>

      <ConfirmDialog
        open={dialog.kind === "delete"}
        title="Delete provider account?"
        description={
          dialog.kind === "delete"
            ? `Delete provider account ${resolveProviderAccountName(dialog.account, i18n?.localeTag)} (${dialog.account.providerCode} / ${dialog.account.environment})? The account is soft-deleted and no longer listed. Accounts still referenced by payment channels or sub-merchants cannot be deleted.`
            : ""
        }
        confirmLabel="Delete"
        variant="danger"
        busy={state.status === "saving"}
        onConfirm={() => void handleDelete()}
        onOpenChange={(open) => {
          if (!open) {
            setDialog({ kind: "closed" });
          }
        }}
      />

      <ConfirmDialog
        open={disableTarget !== null}
        title="Disable provider account?"
        description="Payments routed through this account will fail until it is re-enabled."
        confirmLabel="Disable"
        variant="danger"
        busy={state.status === "saving"}
        onConfirm={() => {
          if (disableTarget) runStatusMutation(() => handleDisable(disableTarget));
        }}
        onOpenChange={(open) => {
          if (!open) {
            setDisableTarget(null);
          }
        }}
      />

      <Dialog
        open={dialog.kind === "rotate"}
        onOpenChange={(open) => {
          if (!open) {
            setDialog({ kind: "closed" });
          }
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Replace provider credentials</DialogTitle>
          </DialogHeader>
          {dialog.kind === "rotate" ? (
            <RotateCredentialsDialog
              account={dialog.account}
              busy={state.status === "saving"}
              onCancel={() => setDialog({ kind: "closed" })}
              onSubmit={handleRotate}
            />
          ) : null}
        </DialogContent>
      </Dialog>
      </PaymentAdminWorkspace>
    </PaymentAdminI18nBoundary>
  );
}

// Credential rotation form: replaces the legacy window.prompt anti-pattern with a structured
// Dialog + write-only credential fields. Existing values are never loaded into browser state.
interface RotateCredentialsDialogProps {
  account: PaymentProviderAccountView;
  busy: boolean;
  onCancel(): void;
  onSubmit(draft: PaymentCredentialRotateDraft): Promise<void> | void;
}

// Backend credential length limits (maxLength) for uploaded files:
// 32768 bytes for secret keys, 65536 bytes for certificates.
const MAX_SECRET_FILE_BYTES = 32768;
const MAX_CERTIFICATE_FILE_BYTES = 65536;

interface RotateFormState {
  primarySecret: string;
  webhookSecret: string;
  certificate: string;
  invalidatePrevious: boolean;
}

function RotateCredentialsDialog(props: RotateCredentialsDialogProps) {
  const [state, setState] = React.useState<RotateFormState>(() => ({
    primarySecret: "",
    webhookSecret: "",
    certificate: "",
    invalidatePrevious: true,
  }));
  const [submitting, setSubmitting] = React.useState(false);
  const [error, setError] = React.useState<string | undefined>();

  const showPemFields =
    props.account.providerCode === "alipay" || props.account.providerCode === "wechat_pay";

  async function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError(undefined);
    setSubmitting(true);
    try {
      const draft: PaymentCredentialRotateDraft = {
        primarySecret: state.primarySecret.trim(),
        ...(state.webhookSecret.trim()
          ? { webhookSecret: state.webhookSecret.trim() }
          : {}),
        ...(state.certificate.trim()
          ? { certificate: state.certificate.trim() }
          : {}),
        invalidatePrevious: state.invalidatePrevious,
      };
      await props.onSubmit(draft);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to replace credentials.");
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <form
      className="space-y-4"
      onSubmit={handleSubmit}
      aria-label="Replace credentials form"
    >
      <p className="text-sm text-[var(--sdk-color-text-secondary)]">
        New credential versions are encrypted in the database. Previous active versions
        are superseded after this operation succeeds.
      </p>
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        <AdminFieldLabel
          label={primarySecretLabel(props.account.providerCode)}
          htmlFor="rotate-primary-secret"
          required
          className="sm:col-span-2"
        >
          <Textarea
            id="rotate-primary-secret"
            value={state.primarySecret}
            onChange={(event) =>
              setState((prev) => ({ ...prev, primarySecret: event.target.value }))
            }
            placeholder="Enter new credential value"
            required
            rows={showPemFields ? 5 : 3}
            className="resize-y font-mono"
            autoComplete="new-password"
          />
          <PemFilePicker
            maxBytes={MAX_SECRET_FILE_BYTES}
            disabled={submitting || props.busy}
            onContent={(content) =>
              setState((prev) => ({ ...prev, primarySecret: content }))
            }
          />
        </AdminFieldLabel>
        <AdminFieldLabel
          label={webhookSecretLabel(props.account.providerCode)}
          htmlFor="rotate-webhook-secret"
          className="sm:col-span-2"
        >
          <Textarea
            id="rotate-webhook-secret"
            value={state.webhookSecret}
            onChange={(event) =>
              setState((prev) => ({ ...prev, webhookSecret: event.target.value }))
            }
            placeholder="Enter new secret value"
            rows={2}
            className="resize-y font-mono"
            autoComplete="new-password"
          />
          <PemFilePicker
            maxBytes={MAX_SECRET_FILE_BYTES}
            disabled={submitting || props.busy}
            onContent={(content) =>
              setState((prev) => ({ ...prev, webhookSecret: content }))
            }
          />
        </AdminFieldLabel>
        <AdminFieldLabel
          label={certificateLabel(props.account.providerCode)}
          htmlFor="rotate-certificate"
          className="sm:col-span-2"
        >
          <Textarea
            id="rotate-certificate"
            value={state.certificate}
            onChange={(event) =>
              setState((prev) => ({ ...prev, certificate: event.target.value }))
            }
            placeholder="Enter new PEM value"
            rows={5}
            className="resize-y font-mono"
            autoComplete="new-password"
          />
          <PemFilePicker
            maxBytes={MAX_CERTIFICATE_FILE_BYTES}
            disabled={submitting || props.busy}
            onContent={(content) =>
              setState((prev) => ({ ...prev, certificate: content }))
            }
          />
        </AdminFieldLabel>
        <AdminFieldLabel
          label="Invalidate previous credentials"
          htmlFor="rotate-invalidate-previous"
          className="sm:col-span-2"
        >
          <Switch
            id="rotate-invalidate-previous"
            checked={state.invalidatePrevious}
            onCheckedChange={(checked) =>
              setState((prev) => ({ ...prev, invalidatePrevious: checked }))
            }
          />
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
          onClick={props.onCancel}
          disabled={submitting || props.busy}
        >
          Cancel
        </Button>
        <Button type="submit" disabled={submitting || props.busy}>
          {submitting || props.busy ? "Replacing..." : "Replace credentials"}
        </Button>
      </div>
    </form>
  );
}

// Re-export commonly used Tabs sub-components for host apps that want to wrap them.
export { Tabs as PaymentProviderAdminTabs, TabsList, TabsTrigger, TabsContent };
