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
  Textarea,
} from "@sdkwork/ui-pc-react";
import {
  AdminFieldLabel,
  ADMIN_PROVIDER_FORM_OPTIONS,
  BaseDataSelectField,
  PaymentProviderIcon,
  PemFilePicker,
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
} from "../types/provider-admin-types";
import { generateCredentials } from "../services/credential-mock";

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

// Backend credential length limits (maxLength) for uploaded files:
// 32768 bytes for secret keys, 65536 bytes for certificates.
const MAX_SECRET_FILE_BYTES = 32768;
const MAX_CERTIFICATE_FILE_BYTES = 65536;

export interface ProviderAccountFormProps {
  initial?: Partial<PaymentProviderAccountView>;
  mode: "create" | "update";
  partnerAccountOptions?: readonly PaymentProviderAccountView[];
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
  const [state, setState] = React.useState<FormState>(() =>
    deriveInitialState(props.initial),
  );
  const [submitting, setSubmitting] = React.useState(false);
  const [error, setError] = React.useState<string | undefined>();
  /** Which credential field is currently generating (or "all"); disables the
   *  other generator buttons so concurrent Web Crypto key generation cannot
   *  race each other. */
  const [generating, setGenerating] = React.useState<"primarySecret" | "webhookSecret" | "certificate" | "all" | null>(null);

  const isCreate = props.mode === "create";

  function update<K extends keyof FormState>(key: K, value: FormState[K]) {
    setState((prev) => ({ ...prev, [key]: value }));
  }

  function buildMetadata(): Record<string, unknown> {
    const metadata: Record<string, unknown> = { ...(props.initial?.metadata ?? {}) };
    delete metadata.configurationState;
    delete metadata.configureBeforeActivation;
    delete metadata.appId;
    delete metadata.merchantSerialNo;
    delete metadata.signType;
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
    setError(undefined);
    if (!state.accountNo.trim() || !state.merchantId.trim() || (isCreate && !state.primarySecret.trim())) {
      setError("Account no, merchant id, and primary credential are required.");
      return;
    }
    if (state.accountMode === "partner" && !state.partnerProviderAccountId.trim() && isCreate) {
      setError("Partner provider account is required when account mode is partner.");
      return;
    }
    if (isCreate && state.status === "active") {
      setError("Create the account as inactive, validate it, then activate it.");
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
      setError(err instanceof Error ? err.message : "Failed to submit provider account form.");
    } finally {
      setSubmitting(false);
    }
  }

  const showAlipayFields = state.providerCode === "alipay";
  const showWeChatFields = state.providerCode === "wechat_pay";
  const showStripeFields = state.providerCode === "stripe";
  const showSandboxFields = state.providerCode === "sandbox";
  const showPartnerFields = state.accountMode === "partner";

  return (
    <form
      className="space-y-4"
      onSubmit={handleSubmit}
      aria-label="Provider account form"
    >
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        {/* Left column: account basics, partner config, provider metadata */}
        <div className="space-y-4">
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        <AdminFieldLabel label="Account No" htmlFor="provider-account-no" required>
          <Input
            id="provider-account-no"
            value={state.accountNo}
            onChange={(event) => update("accountNo", event.target.value)}
            disabled={!isCreate}
            placeholder="e.g., stripe-live-primary"
            required
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
              <SelectTrigger id="provider-partner-account-id">
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
            Sub-merchants (Alipay sub_appid / WeChat sub_mch_id / Stripe Connected Account)
            are managed under the partner account in the Sub-Merchants tab.
          </p>
        </div>
      ) : null}

      {showAlipayFields || showWeChatFields ? (
        <div className="rounded-md border border-[var(--sdk-color-border-subtle)] p-4">
          <div className="mb-3 text-xs font-semibold uppercase tracking-wider text-[var(--sdk-color-text-muted)]">
            Provider Metadata
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
          Sandbox provider requires only the primary credential.
        </p>
      ) : null}
        </div>
        {/* Right column: credentials, capabilities */}
        <div className="space-y-4">

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
              {generating === "all" ? "Generating..." : "Generate all credentials"}
            </Button>
          ) : null}
        </div>
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
          <AdminFieldLabel
            label={primarySecretLabel(state.providerCode)}
            htmlFor="provider-primary-secret"
            required={isCreate}
            className="sm:col-span-2"
          >
            <Textarea
              id="provider-primary-secret"
              value={state.primarySecret}
              onChange={(event) => update("primarySecret", event.target.value)}
              placeholder={credentialPlaceholder(isCreate, props.initial?.hasPrimarySecret)}
              required={isCreate}
              rows={showAlipayFields || showWeChatFields ? 4 : 3}
              className="resize-y font-mono"
              autoComplete="new-password"
            />
            <div className="flex items-center justify-between gap-2">
              <PemFilePicker
                maxBytes={MAX_SECRET_FILE_BYTES}
                disabled={submitting || generating !== null}
                onContent={(content) => update("primarySecret", content)}
              />
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="shrink-0 text-xs underline underline-offset-2"
                onClick={() => handleGenerateField("primarySecret")}
                disabled={submitting || generating !== null}
                title="Generate a key for this field for debugging"
              >
                {generateButtonLabel("primarySecret", "Generate key")}
              </Button>
            </div>
          </AdminFieldLabel>
          {showStripeFields || showWeChatFields ? (
            <AdminFieldLabel
              label={webhookSecretLabel(state.providerCode)}
              htmlFor="provider-webhook-secret"
              className="sm:col-span-2"
            >
              <Textarea
                id="provider-webhook-secret"
                value={state.webhookSecret}
                onChange={(event) => update("webhookSecret", event.target.value)}
                placeholder={credentialPlaceholder(isCreate, props.initial?.hasWebhookSecret)}
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
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  className="shrink-0 text-xs underline underline-offset-2"
                  onClick={() => handleGenerateField("webhookSecret")}
                  disabled={submitting || generating !== null}
                  title="Generate a secret for this field for debugging"
                >
                  {generateButtonLabel("webhookSecret", "Generate secret")}
                </Button>
              </div>
            </AdminFieldLabel>
          ) : null}
          {showAlipayFields || showWeChatFields ? (
            <AdminFieldLabel
              label={certificateLabel(state.providerCode)}
              htmlFor="provider-certificate"
              className="sm:col-span-2"
            >
              <Textarea
                id="provider-certificate"
                value={state.certificate}
                onChange={(event) => update("certificate", event.target.value)}
                placeholder={credentialPlaceholder(isCreate, props.initial?.hasCertificate)}
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
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  className="shrink-0 text-xs underline underline-offset-2"
                  onClick={() => handleGenerateField("certificate")}
                  disabled={submitting || generating !== null}
                  title="Generate a certificate for this field for debugging"
                >
                  {generateButtonLabel("certificate", "Generate certificate")}
                </Button>
              </div>
            </AdminFieldLabel>
          ) : null}
        </div>
        <p className="mt-3 text-xs text-[var(--sdk-color-text-secondary)]">
          {props.initial?.credentialStorage === "legacy_reference"
            ? "Legacy credential reference detected. Saving a replacement migrates it to encrypted database storage."
            : "Credential values are write-only and encrypted before database persistence."}
        </p>
        {isCreate && state.environment !== "production" ? (
          <p className="mt-1 text-xs text-[var(--sdk-color-text-secondary)]">
            Generated credentials are for sandbox and development debugging. Use PSP-issued credentials before production.
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
        </div>
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
        <Button type="button" variant="ghost" onClick={props.onCancel} disabled={submitting} title="Saving in progress...">
          Cancel
        </Button>
        <Button type="submit" disabled={submitting} title="Saving in progress...">
          {submitting ? "Saving..." : isCreate ? "Create Account" : "Update Account"}
        </Button>
      </div>
    </form>
  );
}

export function primarySecretLabel(providerCode: PaymentProviderCode): string {
  if (providerCode === "stripe") return "Stripe Secret Key";
  if (providerCode === "alipay") return "Alipay Merchant Private Key";
  if (providerCode === "wechat_pay") return "WeChat Merchant Private Key";
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

export function certificateLabel(providerCode: PaymentProviderCode): string {
  if (providerCode === "alipay") return "Alipay Public Key";
  if (providerCode === "wechat_pay") return "WeChat Platform Certificate";
  return "Certificate";
}
