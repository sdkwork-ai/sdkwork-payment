/**
 * Provider account dynamic form.
 *
 * Renders different credential fields based on:
 *   1. providerCode (stripe / alipay / wechat_pay / sandbox)
 *   2. accountMode (direct / partner)
 *
 * Direct mode fields per provider:
 *   - stripe: primarySecret (sk_live_... / sk_test_...), webhookSecret
 *   - alipay: primarySecret (merchantPrivateKey PEM), certificate (alipayPublicKey PEM),
 *             metadata.appId, metadata.signType (RSA2/RSA)
 *   - wechat_pay: primarySecret (merchant private key PEM), webhookSecret (API v3 key),
 *                 certificate (platform cert PEM), metadata.merchantSerialNo
 *   - sandbox: primarySecret only
 *
 * Partner (ISV) mode fields:
 *   - All direct fields PLUS:
 *   - partnerProviderAccountId (select from existing partner accounts)
 *   - Sub-merchant management is delegated to <SubMerchantManager/>
 *
 * Credential fields are write-only. Existing values are never loaded into the
 * browser; the backend encrypts replacements before database persistence.
 */

import * as React from "react";
import {
  Button,
  Input,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Switch,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
  Textarea,
} from "@sdkwork/ui-pc-react";
import { CheckCircle2, Circle } from "lucide-react";
import {
  AdminFieldLabel,
  ADMIN_PROVIDER_FORM_OPTIONS,
  BaseDataSelectField,
  PaymentProviderIcon,
  PemFilePicker,
  usePaymentAdminMessages,
} from "@sdkwork/payment-pc-admin-core";
import type {
  PaymentBaseDataOption,
  PaymentProviderAccountDraft,
  PaymentProviderAccountMode,
  PaymentProviderAccountUpdateDraft,
  PaymentProviderAccountView,
  PaymentProviderCode,
  PaymentProviderEnvironment,
  PaymentProviderAccountStatus,
  ProviderAccountCredentialsView,
} from "../types/provider-admin-types";
import { generateCredentials } from "../services/credential-generator";

const ACCOUNT_MODE_OPTIONS: readonly { label: string; value: PaymentProviderAccountMode }[] = [
  { label: "Direct (merchant self-connection)", value: "direct" },
  { label: "Partner / ISV (with sub-merchants)", value: "partner" },
];

const ENVIRONMENT_OPTIONS: readonly { label: string; value: PaymentProviderEnvironment }[] = [
  { label: "Development", value: "development" },
  { label: "Sandbox", value: "sandbox" },
  { label: "Production", value: "production" },
];

const STATUS_OPTIONS: readonly { label: string; value: PaymentProviderAccountStatus }[] = [
  { label: "Active", value: "active" },
  { label: "Inactive", value: "inactive" },
  { label: "Suspended", value: "suspended" },
  { label: "Deprecated", value: "deprecated" },
];

const CAPABILITY_KEYS = ["pay", "refund", "close", "query", "reconcile", "download"] as const;

/** Form sections (top tabs). */
type SectionKey = "basics" | "credentials" | "capabilities";

/** Non-state tab styling: muted gray text with a hover tint. The active tab
 *  styling is applied through inline styles (ACTIVE_TAB_STYLE) because the
 *  TabsTrigger base class ships its own `data-[state=active]` background and
 *  text rules — same-specificity Tailwind overrides depend on generated CSS
 *  order and can silently lose to the base classes. */
const SECTION_TAB_CLASS_NAME =
  "min-w-fit justify-center gap-2 rounded-md border border-transparent px-4 py-1.5 text-left text-[var(--sdk-color-text-muted)] transition-colors hover:bg-[var(--sdk-color-surface-panel-muted)] hover:text-[var(--sdk-color-text-secondary)]";

/** Active tab: solid tech-blue background with white semibold text — applied
 *  inline so the selected state can never be overridden by base-class rules. */
const ACTIVE_TAB_STYLE: React.CSSProperties = {
  backgroundColor: "var(--sdk-color-brand-primary)",
  borderColor: "var(--sdk-color-brand-primary)",
  color: "white",
  fontWeight: 600,
};

// Backend credential length limits (maxLength) for uploaded files:
// 32768 bytes for secret keys, 65536 bytes for certificates.
const MAX_SECRET_FILE_BYTES = 32768;
const MAX_CERTIFICATE_FILE_BYTES = 65536;

export interface ProviderAccountFormProps {
  initial?: Partial<PaymentProviderAccountView>;
  mode: "create" | "update";
  partnerAccountOptions?: readonly PaymentProviderAccountView[];
  /**
   * Loads the account's saved credentials (decrypted server-side) so the edit
   * dialog can display, copy, and download them. Only wired in update mode;
   * create mode has nothing to load.
   */
  readCredentials?(): Promise<ProviderAccountCredentialsView>;
  /**
   * Base-data options resolved by the host app (served by the
   * sdkwork-appbase base-data capability). When empty or omitted the
   * corresponding field degrades to a free-text input.
   */
  countryOptions?: readonly PaymentBaseDataOption[];
  currencyOptions?: readonly PaymentBaseDataOption[];
  onCancel(): void;
  onSubmit(
    draft: PaymentProviderAccountDraft | PaymentProviderAccountUpdateDraft,
  ): Promise<void> | void;
}

interface FormState {
  accountNo: string;
  providerCode: PaymentProviderCode;
  merchantId: string;
  accountName: string;
  accountMode: PaymentProviderAccountMode;
  partnerProviderAccountId: string;
  environment: PaymentProviderEnvironment;
  countryCode: string;
  settlementCurrency: string;
  primarySecret: string;
  webhookSecret: string;
  certificate: string;
  status: PaymentProviderAccountStatus;
  metadataAppId: string;
  metadataMerchantSerialNo: string;
  metadataSignType: string;
  /** WeChat Pay API v3 verification credential mode (official two-option
   *  system): "wechatpay_public_key" (recommended, new-merchant default) or
   *  "platform_certificate". */
  metadataSignVerifyMode: string;
  /** WeChat Pay public key ID (PUB_KEY_ID_ prefix) for public key mode. */
  metadataWechatpayPublicKeyId: string;
  /** Platform certificate serial number for platform certificate mode. */
  metadataPlatformCertificateSerialNo: string;
  metadataNotifyUrl: string;
  metadataReturnUrl: string;
  capabilities: Record<string, boolean>;
}

function deriveInitialState(
  initial: Partial<PaymentProviderAccountView> | undefined,
): FormState {
  const metadata = initial?.metadata ?? {};
  return {
    accountNo: initial?.accountNo ?? "",
    providerCode: initial?.providerCode ?? "stripe",
    merchantId: initial?.merchantId ?? "",
    accountName: initial?.accountName ?? "",
    accountMode: initial?.accountMode ?? "direct",
    partnerProviderAccountId: initial?.partnerProviderAccountId ?? "",
    environment: initial?.environment ?? "sandbox",
    countryCode: initial?.countryCode ?? "CN",
    settlementCurrency: initial?.settlementCurrency ?? "CNY",
    primarySecret: "",
    webhookSecret: "",
    certificate: "",
    status: initial?.status ?? "inactive",
    metadataAppId: typeof metadata.appId === "string" ? metadata.appId : "",
    metadataMerchantSerialNo: typeof metadata.merchantSerialNo === "string" ? metadata.merchantSerialNo : "",
    metadataSignType: typeof metadata.signType === "string" ? metadata.signType : "RSA2",
    metadataSignVerifyMode: typeof metadata.signVerifyMode === "string" ? metadata.signVerifyMode : "wechatpay_public_key",
    metadataWechatpayPublicKeyId: typeof metadata.wechatpayPublicKeyId === "string" ? metadata.wechatpayPublicKeyId : "",
    metadataPlatformCertificateSerialNo: typeof metadata.platformCertificateSerialNo === "string" ? metadata.platformCertificateSerialNo : "",
    metadataNotifyUrl: typeof metadata.notifyUrl === "string" ? metadata.notifyUrl : "",
    metadataReturnUrl: typeof metadata.returnUrl === "string" ? metadata.returnUrl : "",
    capabilities: {
      pay: initial?.capabilities?.pay ?? true,
      refund: initial?.capabilities?.refund ?? true,
      close: initial?.capabilities?.close ?? true,
      query: initial?.capabilities?.query ?? true,
      reconcile: initial?.capabilities?.reconcile ?? false,
      download: initial?.capabilities?.download ?? false,
    },
  };
}

export function ProviderAccountForm(props: ProviderAccountFormProps) {
  const phrases = usePaymentAdminMessages().legacy.phrases;
  const t = (key: string) => phrases[key] ?? key;
  const [state, setState] = React.useState<FormState>(() =>
    deriveInitialState(props.initial),
  );
  const [submitting, setSubmitting] = React.useState(false);
  const [error, setError] = React.useState<string | undefined>();
  /** Active left-hand section (vertical tabs). */
  const [activeSection, setActiveSection] = React.useState<SectionKey>("basics");
  /** Fields that failed local validation on the last submit attempt; drives
   *  per-field error styling until the operator edits them again. */
  const [invalidFields, setInvalidFields] = React.useState<readonly string[]>([]);
  /** Which credential field is currently generating (or "all"); disables the
   *  other generator buttons so concurrent Web Crypto key generation cannot
   *  race each other. */
  const [generating, setGenerating] = React.useState<"primarySecret" | "webhookSecret" | "certificate" | "all" | null>(null);
  /** Which field was last copied (drives the transient "Copied!" label). */
  const [copiedField, setCopiedField] = React.useState<"primarySecret" | "webhookSecret" | "certificate" | null>(null);

  function clearInvalid(field: string) {
    setInvalidFields((prev) =>
      prev.includes(field) ? prev.filter((item) => item !== field) : prev,
    );
  }

  function markInvalid(fields: readonly string[]) {
    setInvalidFields((prev) => Array.from(new Set([...prev, ...fields])));
  }

  // Edit mode: load the saved credential values (decrypted server-side) so the
  // operator sees exactly what is configured and can copy or download it.
  React.useEffect(() => {
    if (props.mode !== "update" || !props.initial?.id || !props.readCredentials) {
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
        // Credential load failures surface through the workspace error toast;
        // keep the fields blank so the operator can still edit other values.
      });
    return () => {
      cancelled = true;
    };
  }, [props.mode, props.initial?.id, props.readCredentials]);

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

  const isCreate = props.mode === "create";

  function update<K extends keyof FormState>(key: K, value: FormState[K]) {
    setState((prev) => ({ ...prev, [key]: value }));
    // Editing a field clears its local validation error highlight.
    clearInvalid(key as string);
  }

  function buildMetadata(): Record<string, unknown> {
    const metadata: Record<string, unknown> = { ...(props.initial?.metadata ?? {}) };
    delete metadata.configurationState;
    delete metadata.configureBeforeActivation;
    delete metadata.appId;
    delete metadata.merchantSerialNo;
    delete metadata.signType;
    delete metadata.signVerifyMode;
    delete metadata.wechatpayPublicKeyId;
    delete metadata.platformCertificateSerialNo;
    delete metadata.notifyUrl;
    delete metadata.returnUrl;
    if (state.metadataAppId) {
      metadata.appId = state.metadataAppId;
    }
    if (state.metadataMerchantSerialNo) {
      metadata.merchantSerialNo = state.metadataMerchantSerialNo;
    }
    if (state.metadataSignType) {
      metadata.signType = state.metadataSignType;
    }
    if (state.providerCode === "wechat_pay" && state.metadataSignVerifyMode) {
      metadata.signVerifyMode = state.metadataSignVerifyMode;
      if (state.metadataSignVerifyMode === "wechatpay_public_key") {
        if (state.metadataWechatpayPublicKeyId) {
          metadata.wechatpayPublicKeyId = state.metadataWechatpayPublicKeyId;
        }
      } else if (state.metadataPlatformCertificateSerialNo) {
        metadata.platformCertificateSerialNo = state.metadataPlatformCertificateSerialNo;
      }
    }
    if (state.metadataNotifyUrl) {
      metadata.notifyUrl = state.metadataNotifyUrl;
    }
    if (state.metadataReturnUrl) {
      metadata.returnUrl = state.metadataReturnUrl;
    }
    return metadata;
  }

  function buildCapabilities() {
    const capabilities: Record<string, boolean> = {};
    for (const key of CAPABILITY_KEYS) {
      capabilities[key] = state.capabilities[key] ?? false;
    }
    return capabilities;
  }

  function handleGenerateField(field: "primarySecret" | "webhookSecret" | "certificate") {
    setGenerating(field);
    void generateCredentials(state.providerCode)
      .then((values) => {
        setState((prev) => ({
          ...prev,
          [field]: values[field] ?? "",
        }));
      })
      .finally(() => setGenerating(null));
  }

  function handleGenerateAll() {
    setGenerating("all");
    void generateCredentials(state.providerCode)
      .then((values) => {
        setState((prev) => ({
          ...prev,
          primarySecret: values.primarySecret,
          webhookSecret: values.webhookSecret ?? "",
          certificate: values.certificate ?? "",
          ...(values.wechatpayPublicKeyId
            ? {
                metadataSignVerifyMode: "wechatpay_public_key",
                metadataWechatpayPublicKeyId: values.wechatpayPublicKeyId,
              }
            : {}),
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

  async function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    // The drawer footer save button submits this form through the HTML `form`
    // attribute; guard against double submission while a save is in flight.
    if (submitting) return;
    setError(undefined);
    setInvalidFields([]);
    const missingBasics: string[] = [];
    if (!state.accountNo.trim()) {
      missingBasics.push("accountNo");
    }
    if (!state.merchantId.trim()) {
      missingBasics.push("merchantId");
    }
    if (state.accountMode === "partner" && !state.partnerProviderAccountId.trim() && isCreate) {
      missingBasics.push("partnerProviderAccountId");
    }
    if (missingBasics.length > 0) {
      markInvalid(missingBasics);
      setActiveSection("basics");
      setError(t("Account no and merchant id are required."));
      return;
    }
    if (isCreate && !state.primarySecret.trim()) {
      markInvalid(["primarySecret"]);
      setActiveSection("credentials");
      setError(t("Primary credential is required before creating the account."));
      return;
    }
    setSubmitting(true);
    try {
      const metadata = buildMetadata();
      const capabilities = buildCapabilities();
      if (isCreate) {
        const draft: PaymentProviderAccountDraft = {
          accountNo: state.accountNo.trim(),
          providerCode: state.providerCode,
          merchantId: state.merchantId.trim(),
          accountName: state.accountName.trim() || undefined,
          accountMode: state.accountMode,
          ...(state.partnerProviderAccountId.trim()
            ? { partnerProviderAccountId: state.partnerProviderAccountId.trim() }
            : {}),
          environment: state.environment,
          countryCode: state.countryCode.trim().toUpperCase() || "CN",
          settlementCurrency: state.settlementCurrency.trim().toUpperCase() || "CNY",
          primarySecret: state.primarySecret.trim(),
          ...(state.webhookSecret.trim() ? { webhookSecret: state.webhookSecret.trim() } : {}),
          ...(state.certificate.trim() ? { certificate: state.certificate.trim() } : {}),
          capabilities,
          status: state.status,
          metadata,
        };
        await props.onSubmit(draft);
      } else {
        const draft: PaymentProviderAccountUpdateDraft = {
          merchantId: state.merchantId.trim(),
          ...(state.accountName.trim() ? { accountName: state.accountName.trim() } : {}),
          accountMode: state.accountMode,
          ...(state.partnerProviderAccountId.trim()
            ? { partnerProviderAccountId: state.partnerProviderAccountId.trim() }
            : {}),
          environment: state.environment,
          countryCode: state.countryCode.trim().toUpperCase() || "CN",
          settlementCurrency: state.settlementCurrency.trim().toUpperCase() || "CNY",
          ...(state.primarySecret.trim() ? { primarySecret: state.primarySecret.trim() } : {}),
          ...(state.webhookSecret.trim() ? { webhookSecret: state.webhookSecret.trim() } : {}),
          ...(state.certificate.trim() ? { certificate: state.certificate.trim() } : {}),
          capabilities,
          status: state.status,
          metadata,
        };
        await props.onSubmit(draft);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : t("Failed to submit provider account form."));
    } finally {
      setSubmitting(false);
    }
  }

  const showAlipayFields = state.providerCode === "alipay";
  const showWeChatFields = state.providerCode === "wechat_pay";
  const showStripeFields = state.providerCode === "stripe";
  const showSandboxFields = state.providerCode === "sandbox";
  const showPartnerFields = state.accountMode === "partner";
  const showProviderMetadataFields = showAlipayFields || showWeChatFields;

  /** Completion state per section, driving the tab status badges. Basics
   *  covers the required account identity (account no + merchant id) plus the
   *  provider metadata fields (App ID, verification serials) for Alipay and
   *  WeChat Pay — everything needed for a quick first-page fill. */
  const basicsComplete =
    Boolean(state.accountNo.trim()) &&
    Boolean(state.merchantId.trim()) &&
    (state.accountMode !== "partner" || Boolean(state.partnerProviderAccountId.trim())) &&
    (!showProviderMetadataFields ||
      (Boolean(state.metadataAppId.trim()) &&
        (!showWeChatFields ||
          (Boolean(state.metadataMerchantSerialNo.trim()) &&
            (state.metadataSignVerifyMode === "platform_certificate"
              ? Boolean(state.metadataPlatformCertificateSerialNo.trim())
              : Boolean(state.metadataWechatpayPublicKeyId.trim()))))));
  const credentialsComplete = isCreate
    ? Boolean(state.primarySecret.trim())
    : Boolean(state.primarySecret.trim()) || Boolean(props.initial?.hasPrimarySecret);

  return (
    <form
      id="provider-account-form"
      className="flex min-h-0 flex-1 flex-col gap-4"
      onSubmit={handleSubmit}
      aria-label="Provider account form"
      noValidate
    >
      {/* Top section tabs with a fixed-height scrolling content area; the
          footer action bar stays fixed at the bottom. */}
      <Tabs
        value={activeSection}
        onValueChange={(value) => setActiveSection(value as SectionKey)}
        className="flex min-h-0 flex-1 flex-col"
      >
        <TabsList className="flex h-9 w-full items-center justify-start gap-0 border-b border-[var(--sdk-color-border-subtle)] bg-transparent p-0">
          <TabsTrigger
            value="basics"
            className={SECTION_TAB_CLASS_NAME}
            style={activeSection === "basics" ? ACTIVE_TAB_STYLE : undefined}
          >
            <span>{t("Account Basics")}</span>
            <SectionStatusBadge complete={basicsComplete} active={activeSection === "basics"} />
          </TabsTrigger>
          <TabsTrigger
            value="credentials"
            className={SECTION_TAB_CLASS_NAME}
            style={activeSection === "credentials" ? ACTIVE_TAB_STYLE : undefined}
          >
            <span>{t("Credentials")}</span>
            <SectionStatusBadge
              complete={credentialsComplete}
              active={activeSection === "credentials"}
            />
          </TabsTrigger>
          <TabsTrigger
            value="capabilities"
            className={SECTION_TAB_CLASS_NAME}
            style={activeSection === "capabilities" ? ACTIVE_TAB_STYLE : undefined}
          >
            <span>{t("Capabilities")}</span>
            <SectionStatusBadge complete active={activeSection === "capabilities"} />
          </TabsTrigger>
        </TabsList>
        <TabsContent
          value="basics"
          className="mt-3 min-h-0 flex-1 space-y-4 overflow-y-auto rounded-md border border-[var(--sdk-color-border-subtle)] p-5"
        >
        <div className="rounded-md border border-[var(--sdk-color-border-subtle)] p-4">
          <div className="mb-3 text-xs font-semibold uppercase tracking-wider text-[var(--sdk-color-text-muted)]">
            {t("Account Basics")}
          </div>
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        <AdminFieldLabel label="Account No" htmlFor="provider-account-no" required>
          <Input
            id="provider-account-no"
            value={state.accountNo}
            onChange={(event) => update("accountNo", event.target.value)}
            disabled={!isCreate}
            placeholder="e.g., stripe-live-primary"
            required
            aria-invalid={invalidFields.includes("accountNo")}
          />
        </AdminFieldLabel>
        <AdminFieldLabel label="Account Name" htmlFor="provider-account-name">
          <Input
            id="provider-account-name"
            value={state.accountName}
            onChange={(event) => update("accountName", event.target.value)}
            placeholder="e.g., Stripe Production Account"
            maxLength={128}
          />
        </AdminFieldLabel>
        <AdminFieldLabel label="Provider" htmlFor="provider-code" required>
          <Select
            value={state.providerCode}
            onValueChange={(value) => update("providerCode", value as PaymentProviderCode)}
            disabled={!isCreate}
          >
            <SelectTrigger id="provider-code">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {ADMIN_PROVIDER_FORM_OPTIONS.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  <span className="inline-flex items-center gap-2">
                    <PaymentProviderIcon providerCode={option.value} size="xs" />
                    {option.label}
                  </span>
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </AdminFieldLabel>
        <AdminFieldLabel label="Merchant ID" htmlFor="provider-merchant-id" required>
          <Input
            id="provider-merchant-id"
            value={state.merchantId}
            onChange={(event) => update("merchantId", event.target.value)}
            placeholder="e.g., merchant_001 or acct_xxx"
            required
            aria-invalid={invalidFields.includes("merchantId")}
          />
        </AdminFieldLabel>
        <AdminFieldLabel label="Environment" htmlFor="provider-environment" required>
          <Select
            value={state.environment}
            onValueChange={(value) =>
              update("environment", value as PaymentProviderEnvironment)
            }
          >
            <SelectTrigger id="provider-environment">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {ENVIRONMENT_OPTIONS.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  {option.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </AdminFieldLabel>
        <AdminFieldLabel label="Account Mode" htmlFor="provider-account-mode" required>
          <Select
            value={state.accountMode}
            onValueChange={(value) =>
              update("accountMode", value as PaymentProviderAccountMode)
            }
          >
            <SelectTrigger id="provider-account-mode">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {ACCOUNT_MODE_OPTIONS.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  {option.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </AdminFieldLabel>
        <AdminFieldLabel label="Status" htmlFor="provider-status">
          <Select
            value={state.status}
            onValueChange={(value) =>
              update("status", value as PaymentProviderAccountStatus)
            }
          >
            <SelectTrigger id="provider-status">
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
        <BaseDataSelectField
          id="provider-country-code"
          label="Country Code"
          options={props.countryOptions}
          value={state.countryCode}
          maxLength={2}
          placeholder="CN"
          onChange={(value) => update("countryCode", value)}
        />
        <BaseDataSelectField
          id="provider-settlement-currency"
          label="Settlement Currency"
          options={props.currencyOptions}
          value={state.settlementCurrency}
          maxLength={3}
          placeholder="CNY"
          onChange={(value) => update("settlementCurrency", value)}
        />
      </div>
        </div>

      {showPartnerFields ? (
        <div className="rounded-md border border-[var(--sdk-color-border-subtle)] bg-[var(--sdk-color-bg-subtle)] p-4">
          <div className="mb-2 text-xs font-semibold uppercase tracking-wider text-[var(--sdk-color-text-muted)]">
            Partner / ISV Configuration
          </div>
          <AdminFieldLabel
            label="Partner Provider Account"
            htmlFor="provider-partner-account-id"
            required={isCreate}
          >
            <Select
              value={state.partnerProviderAccountId}
              onValueChange={(value) => update("partnerProviderAccountId", value)}
              disabled={!isCreate && Boolean(props.initial?.partnerProviderAccountId)}
            >
              <SelectTrigger
                id="provider-partner-account-id"
                className={
                  invalidFields.includes("partnerProviderAccountId")
                    ? "border-[var(--sdk-color-state-danger)]"
                    : undefined
                }
              >
                <SelectValue placeholder="Select partner account..." />
              </SelectTrigger>
              <SelectContent>
                {(props.partnerAccountOptions ?? []).map((account) => (
                  <SelectItem key={account.id} value={account.id}>
                    {account.accountNo} ({account.providerCode})
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </AdminFieldLabel>
          <p className="mt-2 text-xs text-[var(--sdk-color-text-secondary)]">
            {t(
              "Sub-merchants (Alipay sub_appid / WeChat sub_mch_id / Stripe Connected Account) are managed under the partner account in the Sub-Merchants tab.",
            )}
          </p>
        </div>
      ) : null}

      {showProviderMetadataFields ? (
        <div className="rounded-md border border-[var(--sdk-color-border-subtle)] p-4">
          <div className="mb-3 text-xs font-semibold uppercase tracking-wider text-[var(--sdk-color-text-muted)]">
            {t("Provider Metadata")}
          </div>
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
            <AdminFieldLabel label="App ID" htmlFor="provider-metadata-app-id">
              <Input
                id="provider-metadata-app-id"
                value={state.metadataAppId}
                onChange={(event) => update("metadataAppId", event.target.value)}
                placeholder={showAlipayFields ? "Alipay open platform app id" : "WeChat mini program app id"}
              />
            </AdminFieldLabel>
            {showAlipayFields ? (
              <AdminFieldLabel label="Sign Type" htmlFor="provider-metadata-sign-type">
                <Select
                  value={state.metadataSignType}
                  onValueChange={(value) => update("metadataSignType", value)}
                >
                  <SelectTrigger id="provider-metadata-sign-type">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="RSA2">RSA2 (recommended)</SelectItem>
                    <SelectItem value="RSA">RSA</SelectItem>
                  </SelectContent>
                </Select>
              </AdminFieldLabel>
            ) : null}
            {showWeChatFields ? (
              <AdminFieldLabel
                label="Merchant Serial No"
                htmlFor="provider-metadata-merchant-serial-no"
              >
                <Input
                  id="provider-metadata-merchant-serial-no"
                  value={state.metadataMerchantSerialNo}
                  onChange={(event) => update("metadataMerchantSerialNo", event.target.value)}
                  placeholder="WeChat API v3 merchant certificate serial number"
                />
              </AdminFieldLabel>
            ) : null}
            {showWeChatFields ? (
              <AdminFieldLabel
                label="Sign Verify Mode"
                htmlFor="provider-metadata-sign-verify-mode"
              >
                <Select
                  value={state.metadataSignVerifyMode}
                  onValueChange={(value) => update("metadataSignVerifyMode", value)}
                >
                  <SelectTrigger id="provider-metadata-sign-verify-mode">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="wechatpay_public_key">
                      WeChat Pay Public Key (recommended)
                    </SelectItem>
                    <SelectItem value="platform_certificate">Platform Certificate</SelectItem>
                  </SelectContent>
                </Select>
              </AdminFieldLabel>
            ) : null}
            {showWeChatFields && state.metadataSignVerifyMode === "wechatpay_public_key" ? (
              <AdminFieldLabel
                label="WeChat Pay Public Key ID"
                htmlFor="provider-metadata-public-key-id"
              >
                <Input
                  id="provider-metadata-public-key-id"
                  value={state.metadataWechatpayPublicKeyId}
                  onChange={(event) => update("metadataWechatpayPublicKeyId", event.target.value)}
                  placeholder="PUB_KEY_ID_... (from merchant platform API Security)"
                />
              </AdminFieldLabel>
            ) : null}
            {showWeChatFields && state.metadataSignVerifyMode === "platform_certificate" ? (
              <AdminFieldLabel
                label="Platform Certificate Serial No"
                htmlFor="provider-metadata-cert-serial-no"
              >
                <Input
                  id="provider-metadata-cert-serial-no"
                  value={state.metadataPlatformCertificateSerialNo}
                  onChange={(event) =>
                    update("metadataPlatformCertificateSerialNo", event.target.value)
                  }
                  placeholder="Platform certificate serial number"
                />
              </AdminFieldLabel>
            ) : null}
            <AdminFieldLabel label="Return URL" htmlFor="provider-metadata-return-url">
              <Input
                id="provider-metadata-return-url"
                value={state.metadataReturnUrl}
                onChange={(event) => update("metadataReturnUrl", event.target.value)}
                placeholder="Optional override return URL"
              />
            </AdminFieldLabel>
            <AdminFieldLabel label="Notify URL" htmlFor="provider-metadata-notify-url">
              <Input
                id="provider-metadata-notify-url"
                value={state.metadataNotifyUrl}
                onChange={(event) => update("metadataNotifyUrl", event.target.value)}
                placeholder={`https://pay.example.com/app/v3/api/orders/payments/webhooks/${state.providerCode}`}
              />
            </AdminFieldLabel>
          </div>
        </div>
      ) : null}

      {showSandboxFields ? (
        <p className="text-xs text-[var(--sdk-color-text-secondary)]">
          {t("Sandbox provider requires only the primary credential.")}
        </p>
      ) : null}
        </TabsContent>

      <TabsContent
        value="credentials"
        className="mt-3 min-h-0 flex-1 space-y-4 overflow-y-auto rounded-md border border-[var(--sdk-color-border-subtle)] p-5"
      >
      <div className="rounded-md border border-[var(--sdk-color-border-subtle)] p-4">
        <div className="mb-3 flex items-center justify-between gap-2">
          <div className="text-xs font-semibold uppercase tracking-wider text-[var(--sdk-color-text-muted)]">
            Database Credentials
          </div>
          {/* One-click credential generation for quick debug-account setup.
              Hidden for production so real credentials are never accidentally
              replaced. Generates real RSA keys via Web Crypto so the dry-run
              test can reach the provider adapter. */}
          {isCreate && state.environment !== "production" ? (
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={handleGenerateAll}
              disabled={submitting || generating !== null}
              title="Fill all credential fields with generated values for debugging"
            >
              {generating === "all" ? t("Generating...") : t("Generate all credentials")}
            </Button>
          ) : null}
        </div>
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
          <AdminFieldLabel
            label={t(primarySecretLabel(state.providerCode))}
            htmlFor="provider-primary-secret"
            required={isCreate}
            className="sm:col-span-2"
            hint={t(
              credentialFieldHint(state.providerCode, "primarySecret") ?? "",
            )}
          >
            <Textarea
              id="provider-primary-secret"
              value={state.primarySecret}
              onChange={(event) => update("primarySecret", event.target.value)}
              placeholder={t(credentialPlaceholder(isCreate, props.initial?.hasPrimarySecret))}
              required={isCreate}
              rows={showAlipayFields || showWeChatFields ? 4 : 3}
              className={`resize-y font-mono${
                invalidFields.includes("primarySecret")
                  ? " border-[var(--sdk-color-state-danger)]"
                  : ""
              }`}
              autoComplete="new-password"
            />
            <div className="flex items-center justify-between gap-2">
              <PemFilePicker
                maxBytes={MAX_SECRET_FILE_BYTES}
                disabled={submitting || generating !== null}
                onContent={(content) => update("primarySecret", content)}
              />
              <div className="flex shrink-0 items-center gap-1">
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  className="text-xs underline underline-offset-2"
                  onClick={() => copyToClipboard("primarySecret", state.primarySecret)}
                  disabled={!state.primarySecret || submitting || generating !== null}
                  title="Copy the saved credential value"
                >
                  {copiedField === "primarySecret" ? t("Copied!") : t("Copy")}
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  className="text-xs underline underline-offset-2"
                  onClick={() => handleGenerateField("primarySecret")}
                  disabled={submitting || generating !== null}
                  title="Generate a key for this field for debugging"
                >
                  {generateButtonLabel("primarySecret", t("Generate key"))}
                </Button>
              </div>
            </div>
          </AdminFieldLabel>
          {showStripeFields || showWeChatFields ? (
            <AdminFieldLabel
              label={t(webhookSecretLabel(state.providerCode))}
              htmlFor="provider-webhook-secret"
              className="sm:col-span-2"
              hint={t(
                credentialFieldHint(state.providerCode, "webhookSecret") ?? "",
              )}
            >
              <Textarea
                id="provider-webhook-secret"
                value={state.webhookSecret}
                onChange={(event) => update("webhookSecret", event.target.value)}
                placeholder={t(credentialPlaceholder(isCreate, props.initial?.hasWebhookSecret))}
                rows={2}
                className="resize-y font-mono"
                autoComplete="new-password"
              />
              <div className="flex items-center justify-between gap-2">
                <PemFilePicker
                  maxBytes={MAX_SECRET_FILE_BYTES}
                  disabled={submitting || generating !== null}
                  onContent={(content) => update("webhookSecret", content)}
                />
                <div className="flex shrink-0 items-center gap-1">
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    className="text-xs underline underline-offset-2"
                    onClick={() => copyToClipboard("webhookSecret", state.webhookSecret)}
                    disabled={!state.webhookSecret || submitting || generating !== null}
                    title="Copy the saved secret value"
                  >
                    {copiedField === "webhookSecret" ? t("Copied!") : t("Copy")}
                  </Button>
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    className="text-xs underline underline-offset-2"
                    onClick={() => handleGenerateField("webhookSecret")}
                    disabled={submitting || generating !== null}
                    title="Generate a secret for this field for debugging"
                  >
                    {generateButtonLabel("webhookSecret", t("Generate secret"))}
                  </Button>
                </div>
              </div>
            </AdminFieldLabel>
          ) : null}
          {showAlipayFields || showWeChatFields ? (
            <AdminFieldLabel
              label={t(certificateLabel(state.providerCode, state.metadataSignVerifyMode))}
              htmlFor="provider-certificate"
              className="sm:col-span-2"
              hint={t(
                credentialFieldHint(
                  state.providerCode,
                  "certificate",
                  state.metadataSignVerifyMode,
                ) ?? "",
              )}
            >
              <Textarea
                id="provider-certificate"
                value={state.certificate}
                onChange={(event) => update("certificate", event.target.value)}
                placeholder={
                  showWeChatFields && state.metadataSignVerifyMode === "platform_certificate"
                    ? t("wechatpay_cert.pem content")
                    : t(credentialPlaceholder(isCreate, props.initial?.hasCertificate))
                }
                rows={4}
                className="resize-y font-mono"
                autoComplete="new-password"
              />
              <div className="flex items-center justify-between gap-2">
                <PemFilePicker
                  maxBytes={MAX_CERTIFICATE_FILE_BYTES}
                  disabled={submitting || generating !== null}
                  onContent={(content) => update("certificate", content)}
                />
                <div className="flex shrink-0 items-center gap-1">
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    className="text-xs underline underline-offset-2"
                    onClick={() => copyToClipboard("certificate", state.certificate)}
                    disabled={!state.certificate || submitting || generating !== null}
                    title="Copy the saved certificate value"
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
                      `${state.accountNo || state.providerCode}-certificate.pem`,
                    )}
                    disabled={!state.certificate || submitting || generating !== null}
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
                    disabled={submitting || generating !== null}
                    title="Generate a certificate for this field for debugging"
                  >
                    {generateButtonLabel("certificate", t("Generate certificate"))}
                  </Button>
                </div>
              </div>
            </AdminFieldLabel>
          ) : null}
        </div>
        <p className="mt-3 text-xs text-[var(--sdk-color-text-secondary)]">
          {props.initial?.credentialStorage === "legacy_reference"
            ? t(
                "Legacy credential reference detected. Saving a replacement migrates it to encrypted database storage.",
              )
            : t(
                "Credential values are encrypted before database persistence and can be viewed, copied, and downloaded here by operators with credential management access.",
              )}
        </p>
        {showWeChatFields ? (
          <p className="mt-1 text-xs text-[var(--sdk-color-text-secondary)]">
            {t(
              "WeChat Pay API v3 verification credential is one of two official modes: WeChat Pay Public Key (pub_key.pem, no expiry, recommended) or Platform Certificate (wechatpay_cert.pem, 5-year validity, rotate before expiry). The Wechatpay-Serial header must match the configured public key ID / certificate serial number.",
            )}
          </p>
        ) : null}
        {isCreate && state.environment !== "production" ? (
          <p className="mt-1 text-xs text-[var(--sdk-color-text-secondary)]">
            {t(
              "Generated credentials are for sandbox and development debugging. Use PSP-issued credentials before production.",
            )}
          </p>
        ) : null}
      </div>

      <div className="rounded-md border border-[var(--sdk-color-border-subtle)] p-4">
        <div className="mb-3 text-xs font-semibold uppercase tracking-wider text-[var(--sdk-color-text-muted)]">
          Capabilities
        </div>
        <div className="grid grid-cols-2 gap-3 sm:grid-cols-3">
          {CAPABILITY_KEYS.map((key) => (
            <label
              key={key}
              className="flex items-center gap-2 text-sm"
              htmlFor={`provider-capability-${key}`}
            >
              <Switch
                id={`provider-capability-${key}`}
                checked={state.capabilities[key] ?? false}
                onCheckedChange={(checked) =>
                  setState((prev) => ({
                    ...prev,
                    capabilities: { ...prev.capabilities, [key]: checked },
                  }))
                }
              />
              <span className="capitalize">{key}</span>
            </label>
          ))}
        </div>
      </div>
        </TabsContent>
      </Tabs>

      {error ? (
        <div
          role="alert"
          className="rounded-md border border-[var(--sdk-color-border-error)] bg-[var(--sdk-color-bg-error-subtle)] p-3 text-sm text-[var(--sdk-color-text-error)]"
        >
          {error}
        </div>
      ) : null}
    </form>
  );
}

export function primarySecretLabel(providerCode: PaymentProviderCode): string {
  if (providerCode === "stripe") return "Stripe Secret Key";
  if (providerCode === "alipay") return "Alipay Merchant Private Key";
  // Official WeChat Pay API v3 terminology: the signing credential is the
  // "Merchant API Certificate" (apiclient_cert.pem + apiclient_key.pem); the
  // field stores the apiclient_key.pem private key PEM.
  if (providerCode === "wechat_pay") return "WeChat Pay Merchant API Certificate";
  return "Primary Credential";
}

function credentialPlaceholder(isCreate: boolean, configured?: boolean): string {
  if (!isCreate && configured) return "Configured";
  return "Enter credential value";
}

export function webhookSecretLabel(providerCode: PaymentProviderCode): string {
  if (providerCode === "stripe") return "Stripe Webhook Signing Secret";
  if (providerCode === "wechat_pay") return "WeChat API v3 Key";
  return "Webhook Secret";
}

export function certificateLabel(
  providerCode: PaymentProviderCode,
  signVerifyMode?: string,
): string {
  if (providerCode === "alipay") return "Alipay Public Key";
  if (providerCode === "wechat_pay") {
    return signVerifyMode === "platform_certificate"
      ? "WeChat Platform Certificate"
      : "WeChat Pay Public Key";
  }
  return "Certificate";
}

/** One-line explanation under each credential field label, rendered through
 *  the admin message catalog so both the account form and the replace
 *  credentials dialog share the same copy. Returns a catalog key (or
 *  undefined when the provider has no hint for that field). */
export function credentialFieldHint(
  providerCode: PaymentProviderCode,
  field: "primarySecret" | "webhookSecret" | "certificate",
  signVerifyMode?: string,
): string | undefined {
  if (field === "primarySecret") {
    if (providerCode === "stripe") {
      return "Secret key (sk_live_… / sk_test_…) from Dashboard → Developers → API keys; signs every API request.";
    }
    if (providerCode === "alipay") {
      return "RSA2 application private key (PKCS#8 PEM) from Alipay Open Platform; signs payment requests.";
    }
    if (providerCode === "wechat_pay") {
      return "Merchant API certificate apiclient_key.pem from the merchant platform (API Security → Merchant API Certificate); signs API v3 requests.";
    }
    return "Sandbox accepts any credential value; used for local testing.";
  }
  if (field === "webhookSecret") {
    if (providerCode === "stripe") {
      return "Webhook signing secret whsec_… from the webhook endpoint details page; verifies Stripe callbacks.";
    }
    if (providerCode === "wechat_pay") {
      return "32-character API v3 key (API Security → APIv3 key setting); decrypts encrypted callback resources.";
    }
    return undefined;
  }
  if (field === "certificate") {
    if (providerCode === "alipay") {
      return "Alipay public key PEM shown in the app console; verifies Alipay callbacks.";
    }
    if (providerCode === "wechat_pay") {
      return signVerifyMode === "platform_certificate"
        ? "Platform certificate wechatpay_cert.pem (5-year validity, rotate before expiry); verifies webhook and response signatures. Its serial must match the Wechatpay-Serial header."
        : "WeChat Pay signs callbacks and responses with the WeChat Pay private key; the merchant verifies them with the WeChat Pay public key to confirm WeChat Pay's identity. The public key ID must match the Wechatpay-Serial header.";
    }
    return undefined;
  }
  return undefined;
}

/** Completion badge shown next to each section label: a green check once the
 *  section's required fields are satisfied, a muted circle otherwise. On the
 *  solid brand-blue active tab the badge renders in white for contrast. */
function SectionStatusBadge({ complete, active }: { complete: boolean; active?: boolean }) {
  return (
    <span data-complete={complete} className="shrink-0" aria-hidden="true">
      {complete ? (
        <CheckCircle2
          className={`h-4 w-4 shrink-0 ${
            active ? "text-white/90" : "text-[var(--sdk-color-state-success)]"
          }`}
          aria-hidden="true"
        />
      ) : (
        <Circle
          className={`h-4 w-4 shrink-0 ${
            active ? "text-white/70" : "text-[var(--sdk-color-text-muted)]"
          }`}
          aria-hidden="true"
        />
      )}
    </span>
  );
}
