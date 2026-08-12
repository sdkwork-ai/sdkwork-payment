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
  OperationDrawer,
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
  usePaymentAdminMessages,
} from "@sdkwork/payment-pc-admin-core";
import {
  ProviderAccountForm,
  certificateLabel,
  credentialFieldHint,
  primarySecretLabel,
  webhookSecretLabel,
} from "../components/ProviderAccountForm";
import {
  ProviderAccountFillInGuide,
  ProviderAccountFillInGuideLink,
} from "../components/ProviderAccountFillInGuide";
import { ProviderAccountList } from "../components/ProviderAccountList";
import { SubMerchantManager } from "../components/SubMerchantManager";
import { generateCredentials } from "../services/credential-generator";
import type {
  PaymentBaseDataOption,
  PaymentCredentialRotateDraft,
  PaymentProviderAccountDraft,
  PaymentProviderAccountUpdateDraft,
  PaymentProviderAdminController,
  PaymentProviderAdminState,
  PaymentProviderAccountView,
  ProviderAccountCredentialsView,
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
  const phrases = usePaymentAdminMessages().legacy.phrases;
  const t = (key: string) => phrases[key] ?? key;
  const { controller } = props;
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
        error instanceof Error ? error.message : t("Failed to load provider admin data."),
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
    toast.success(t("Provider account created."));
  }

  async function handleUpdate(draft: PaymentProviderAccountUpdateDraft) {
    if (dialog.kind !== "edit") {
      return;
    }
    // Saving an account takes effect immediately: credentials are encrypted on
    // write, and the status saved is applied as-is (no Test → Activate gate).
    await controller.updateProviderAccount(dialog.account.id, draft);
    setDialog({ kind: "closed" });
    toast.success(t("Provider account updated."));
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
        toast.success(t("Credentials verified"), { id: loadingToast });
      } else {
        toast.error(t("Credential test failed"), {
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
    // Carried out of the try block so the failure toast can attach the raw
    // provider diagnostic as its description (ErrorOptions cause is not
    // available under every TypeScript lib target).
    let diagnostic: string | undefined;
    try {
      // The toggle only renders for inactive accounts, so no status patch is
      // needed first: a dry-run test refreshes last_tested_at, then the
      // activation patch triggers the backend readiness guard, which
      // atomically validates test freshness, credential completeness, and the
      // one-active-account-per-provider rule.
      const result = await controller.testProviderAccount(account.id, {
        environment: account.environment,
        dryRun: true,
      });
      diagnostic = result.diagnostic;
      if (!result.ok) {
        throw new Error("Provider account readiness validation failed.");
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
                        {resolveProviderAccountName(account)} ({account.providerCode})
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

      {/* Create/edit the provider account in a left-side operation drawer: the
          account list stays visible beside it, the drawer is full-height with
          a fixed width so section switching never resizes the surface. */}
      <OperationDrawer
        open={dialog.kind === "create" || dialog.kind === "edit"}
        onOpenChange={(open) => {
          if (!open) {
            setDialog({ kind: "closed" });
            setGuideOpen(false);
          }
        }}
        side="left"
        size="xl"
        title={dialog.kind === "create" ? t("Create provider account") : t("Edit provider account")}
        actions={<ProviderAccountFillInGuideLink onClick={() => setGuideOpen(true)} />}
        footer={
          <div className="flex items-center justify-between gap-2">
            <p className="text-xs text-[var(--sdk-color-text-secondary)]">
              {t("Fields marked with * are required.")}
            </p>
            <div className="flex items-center gap-2">
              <Button
                type="button"
                variant="ghost"
                onClick={() => setDialog({ kind: "closed" })}
              >
                Cancel
              </Button>
              <Button type="submit" form="provider-account-form">
                {dialog.kind === "create" ? t("Create Account") : t("Update Account")}
              </Button>
            </div>
          </div>
        }
      >
          {dialog.kind === "create" || dialog.kind === "edit" ? (
            <ProviderAccountForm
              mode={dialog.kind === "create" ? "create" : "update"}
              initial={dialog.kind === "edit" ? dialog.account : undefined}
              partnerAccountOptions={partnerAccounts}
              readCredentials={dialog.kind === "edit"
                ? () => controller.readProviderAccountCredentials(dialog.account.id)
                : undefined}
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
      </OperationDrawer>

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
                <strong>{resolveProviderAccountName(dialog.account)}</strong> ({dialog.account.providerCode} /{" "}
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
            ? `Delete provider account ${resolveProviderAccountName(dialog.account)} (${dialog.account.providerCode} / ${dialog.account.environment})? The account is soft-deleted and no longer listed. Accounts still referenced by payment channels or sub-merchants cannot be deleted.`
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
              readCredentials={() => controller.readProviderAccountCredentials(dialog.account.id)}
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

// Credential rotation form: structured Dialog with write-only credential
// fields. The currently saved values are loaded (decrypted server-side) so
// the operator sees what is configured before replacing it.
interface RotateCredentialsDialogProps {
  account: PaymentProviderAccountView;
  busy: boolean;
  readCredentials?(): Promise<ProviderAccountCredentialsView>;
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

/** Resolves the account's WeChat Pay verification mode for label consistency
 *  (defaults to the official recommended public key mode). */
function wechatSignVerifyMode(account: PaymentProviderAccountView): string {
  const mode = account.metadata?.signVerifyMode;
  return typeof mode === "string" && mode ? mode : "wechatpay_public_key";
}

function RotateCredentialsDialog(props: RotateCredentialsDialogProps) {
  const phrases = usePaymentAdminMessages().legacy.phrases;
  const t = (key: string) => phrases[key] ?? key;
  const [state, setState] = React.useState<RotateFormState>(() => ({
    primarySecret: "",
    webhookSecret: "",
    certificate: "",
    invalidatePrevious: true,
  }));
  const [submitting, setSubmitting] = React.useState(false);
  const [error, setError] = React.useState<string | undefined>();
  /** Which credential field is currently generating; disables the other
   *  generator buttons so concurrent Web Crypto key generation cannot race
   *  each other (mirrors the edit form). */
  const [generating, setGenerating] = React.useState<"primarySecret" | "webhookSecret" | "certificate" | null>(null);
  /** Which field was last copied (drives the transient "Copied!" label). */
  const [copiedField, setCopiedField] = React.useState<"primarySecret" | "webhookSecret" | "certificate" | null>(null);

  // Show the currently saved credential values so the operator can compare
  // before replacing them.
  React.useEffect(() => {
    if (!props.readCredentials) {
      return;
    }
    let cancelled = false;
    void props.readCredentials()
      .then((credentials) => {
        if (cancelled) return;
        setState((prev) => ({
          ...prev,
          primarySecret: credentials.primarySecret ?? "",
          webhookSecret: credentials.webhookSecret ?? "",
          certificate: credentials.certificate ?? "",
        }));
      })
      .catch(() => {
        // Load failures surface through the workspace error toast.
      });
    return () => {
      cancelled = true;
    };
  }, [props.readCredentials]);

  const showPemFields =
    props.account.providerCode === "alipay" || props.account.providerCode === "wechat_pay";

  function copyToClipboard(field: "primarySecret" | "webhookSecret" | "certificate", value: string) {
    if (!value) return;
    void navigator.clipboard.writeText(value).then(() => {
      setCopiedField(field);
      setTimeout(() => setCopiedField(null), 1500);
    }).catch(() => {
      // Clipboard access can be denied in non-secure contexts; the value
      // remains visible in the textarea for manual copying.
    });
  }

  function handleGenerateField(field: "primarySecret" | "webhookSecret" | "certificate") {
    setGenerating(field);
    void generateCredentials(props.account.providerCode)
      .then((values) => {
        setState((prev) => ({
          ...prev,
          [field]: values[field] ?? "",
        }));
      })
      .finally(() => setGenerating(null));
  }

  function generateButtonLabel(
    field: "primarySecret" | "webhookSecret" | "certificate",
    label: string,
  ) {
    return generating === field ? "Generating..." : label;
  }

  function downloadCertificate(value: string, fileName: string) {
    if (!value) return;
    const blob = new Blob([value], { type: "application/x-pem-file" });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = fileName;
    anchor.click();
    URL.revokeObjectURL(url);
  }

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
      setError(err instanceof Error ? err.message : t("Failed to replace credentials."));
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <form
      className="space-y-4"
      onSubmit={handleSubmit}
      aria-label={t("Replace credentials form")}
    >
      <p className="text-sm text-[var(--sdk-color-text-secondary)]">
        {t(
          "New credential versions are encrypted in the database. Previous active versions are superseded after this operation succeeds.",
        )}
      </p>
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        <AdminFieldLabel
          label={t(primarySecretLabel(props.account.providerCode))}
          htmlFor="rotate-primary-secret"
          required
          className="sm:col-span-2"
          hint={t(
            credentialFieldHint(
              props.account.providerCode,
              "primarySecret",
              wechatSignVerifyMode(props.account),
            ) ?? "",
          )}
        >
          <Textarea
            id="rotate-primary-secret"
            value={state.primarySecret}
            onChange={(event) =>
              setState((prev) => ({ ...prev, primarySecret: event.target.value }))
            }
            placeholder={t("Enter new credential value")}
            required
            rows={showPemFields ? 5 : 3}
            className="resize-y font-mono"
            autoComplete="new-password"
          />
          <div className="flex items-center justify-between gap-2">
            <PemFilePicker
              maxBytes={MAX_SECRET_FILE_BYTES}
              disabled={submitting || props.busy || generating !== null}
              onContent={(content) =>
                setState((prev) => ({ ...prev, primarySecret: content }))
              }
            />
            <div className="flex shrink-0 items-center gap-1">
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="text-xs underline underline-offset-2"
                onClick={() => copyToClipboard("primarySecret", state.primarySecret)}
                disabled={!state.primarySecret || submitting || props.busy || generating !== null}
                title="Copy the current credential value"
              >
                {copiedField === "primarySecret" ? t("Copied!") : t("Copy")}
              </Button>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="text-xs underline underline-offset-2"
                onClick={() => handleGenerateField("primarySecret")}
                disabled={submitting || props.busy || generating !== null}
                title="Generate a key for this field for debugging"
              >
                {generateButtonLabel("primarySecret", t("Generate key"))}
              </Button>
            </div>
          </div>
        </AdminFieldLabel>
        <AdminFieldLabel
          label={t(webhookSecretLabel(props.account.providerCode))}
          htmlFor="rotate-webhook-secret"
          className="sm:col-span-2"
          hint={t(
            credentialFieldHint(props.account.providerCode, "webhookSecret") ?? "",
          )}
        >
          <Textarea
            id="rotate-webhook-secret"
            value={state.webhookSecret}
            onChange={(event) =>
              setState((prev) => ({ ...prev, webhookSecret: event.target.value }))
            }
            placeholder={t("Enter new secret value")}
            rows={2}
            className="resize-y font-mono"
            autoComplete="new-password"
          />
          <div className="flex items-center justify-between gap-2">
            <PemFilePicker
              maxBytes={MAX_SECRET_FILE_BYTES}
              disabled={submitting || props.busy || generating !== null}
              onContent={(content) =>
                setState((prev) => ({ ...prev, webhookSecret: content }))
              }
            />
            <div className="flex shrink-0 items-center gap-1">
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="text-xs underline underline-offset-2"
                onClick={() => copyToClipboard("webhookSecret", state.webhookSecret)}
                disabled={!state.webhookSecret || submitting || props.busy || generating !== null}
                title="Copy the current secret value"
              >
                {copiedField === "webhookSecret" ? t("Copied!") : t("Copy")}
              </Button>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="text-xs underline underline-offset-2"
                onClick={() => handleGenerateField("webhookSecret")}
                disabled={submitting || props.busy || generating !== null}
                title="Generate a secret for this field for debugging"
              >
                {generateButtonLabel("webhookSecret", t("Generate secret"))}
              </Button>
            </div>
          </div>
        </AdminFieldLabel>
        <AdminFieldLabel
          label={t(certificateLabel(props.account.providerCode, wechatSignVerifyMode(props.account)))}
          htmlFor="rotate-certificate"
          className="sm:col-span-2"
          hint={t(
            credentialFieldHint(
              props.account.providerCode,
              "certificate",
              wechatSignVerifyMode(props.account),
            ) ?? "",
          )}
        >
          <Textarea
            id="rotate-certificate"
            value={state.certificate}
            onChange={(event) =>
              setState((prev) => ({ ...prev, certificate: event.target.value }))
            }
            placeholder={t("Enter new PEM value")}
            rows={5}
            className="resize-y font-mono"
            autoComplete="new-password"
          />
          <div className="flex items-center justify-between gap-2">
            <PemFilePicker
              maxBytes={MAX_CERTIFICATE_FILE_BYTES}
              disabled={submitting || props.busy || generating !== null}
              onContent={(content) =>
                setState((prev) => ({ ...prev, certificate: content }))
              }
            />
            <div className="flex shrink-0 items-center gap-1">
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="text-xs underline underline-offset-2"
                onClick={() => copyToClipboard("certificate", state.certificate)}
                disabled={!state.certificate || submitting || props.busy || generating !== null}
                title="Copy the current certificate value"
              >
                {copiedField === "certificate" ? t("Copied!") : t("Copy")}
              </Button>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="text-xs underline underline-offset-2"
                onClick={() => downloadCertificate(
                  state.certificate,
                  `${props.account.accountNo || props.account.providerCode}-certificate.pem`,
                )}
                disabled={!state.certificate || submitting || props.busy || generating !== null}
                title="Download the certificate as a PEM file"
              >
                Download
              </Button>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="text-xs underline underline-offset-2"
                onClick={() => handleGenerateField("certificate")}
                disabled={submitting || props.busy || generating !== null}
                title="Generate a certificate for this field for debugging"
              >
                {generateButtonLabel("certificate", t("Generate certificate"))}
              </Button>
            </div>
          </div>
        </AdminFieldLabel>
        {props.account.providerCode === "wechat_pay" ? (
          <p className="mt-1 text-xs text-[var(--sdk-color-text-secondary)] sm:col-span-2">
            {wechatSignVerifyMode(props.account) === "platform_certificate"
              ? t(
                  "This account verifies WeChat Pay signatures with the Platform Certificate (wechatpay_cert.pem, serial must match the Wechatpay-Serial header). Switching modes or updating the public key ID / certificate serial number is done in the account editor (Provider Metadata).",
                )
              : t(
                  "This account verifies WeChat Pay signatures with the WeChat Pay Public Key (pub_key.pem, ID must match the Wechatpay-Serial header). Switching modes or updating the public key ID / certificate serial number is done in the account editor (Provider Metadata).",
                )}
          </p>
        ) : null}
        <AdminFieldLabel
          label={t("Invalidate previous credentials")}
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
          {submitting || props.busy ? t("Replacing...") : t("Replace credentials")}
        </Button>
      </div>
    </form>
  );
}

// Re-export commonly used Tabs sub-components for host apps that want to wrap them.
export { Tabs as PaymentProviderAdminTabs, TabsList, TabsTrigger, TabsContent };
