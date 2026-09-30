/**
 * Certificate manager.
 *
 * Lists PEM certificate references with expiry metadata. The PEM content itself
 * is encrypted in the DB; only configuration status and
 * parsed metadata (subject, issuer, fingerprint, expiry) are persisted. This
 * view surfaces:
 *   - Expiry warnings (yellow when within 30 days, red when expired)
 *   - Create dialog — paste the PEM content and the backend parses
 *     subject/issuer/fingerprint/expiresAt server-side (mirrors Stripe
 *     Dashboard's "paste the key and we'll fill in the details" UX)
 *   - Delete with confirmation
 *
 * Mirrors industry PSP certificate management surfaces (Stripe Dashboard API
 * keys, Alipay open platform cert management, WeChat Pay merchant platform
 * cert list).
 */

import * as React from "react";
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
  Textarea,
} from "@sdkwork/ui-pc-react";
import {
  ADMIN_PROVIDER_FORM_OPTIONS,
  AdminFieldLabel,
  ConfirmDialog,
  formatAdminTimestamp,
  PemFilePicker,
  SdkworkPaymentListPaginationControls,
} from "@sdkwork/payment-pc-admin-core";
import type { SdkWorkPageInfo } from "@sdkwork/payment-contracts";
import { useDevConfigMessages } from "../i18n";
import type {
  PaymentCertificateDraft,
  PaymentCertificateKind,
  PaymentCertificateView,
  PaymentProviderCode,
} from "../types/devconfig-admin-types";

export interface CertificateManagerProps {
  certificates: readonly PaymentCertificateView[];
  pageInfo?: SdkWorkPageInfo;
  busy?: boolean;
  onCreate(draft: PaymentCertificateDraft): Promise<void> | void;
  onDelete(id: string): Promise<void> | void;
  onLoadMore(): void;
}

const STATUS_VARIANT: Record<PaymentCertificateView["status"], "success" | "warning" | "danger" | "secondary"> = {
  active: "success",
  pending_rotation: "warning",
  expired: "danger",
  revoked: "secondary",
};

const EXPIRY_WARNING_DAYS = 30;

// Backend certificate length limit (maxLength) for uploaded files.
const MAX_CERTIFICATE_FILE_BYTES = 65536;

export function CertificateManager(props: CertificateManagerProps) {
  const m = useDevConfigMessages();
  const [dialogOpen, setDialogOpen] = React.useState(false);

  const certificateTypeLabel: Record<PaymentCertificateKind, string> = {
    merchant_private_key: m.certificates.typeMerchantPrivateKey,
    provider_public_key: m.certificates.typeProviderPublicKey,
    platform_certificate: m.certificates.typePlatformCertificate,
    // The webhook_secret certificate kind carries the WeChat API v3 decryption
    // key (callback resource decryption), so it is labeled "API v3 Key".
    webhook_secret: m.certificates.typeWebhookSecret,
  };
  const certificateTypeOptions: ReadonlyArray<{ label: string; value: PaymentCertificateKind }> = [
    { label: m.certificates.typeMerchantPrivateKey, value: "merchant_private_key" },
    { label: m.certificates.typeProviderPublicKey, value: "provider_public_key" },
    { label: m.certificates.typePlatformCertificate, value: "platform_certificate" },
    { label: m.certificates.typeWebhookSecret, value: "webhook_secret" },
  ];
  const statusLabel: Record<PaymentCertificateView["status"], string> = {
    active: m.certificates.statusActive,
    pending_rotation: m.certificates.statusPendingRotation,
    expired: m.certificates.statusExpired,
    revoked: m.certificates.statusRevoked,
  };
  const [submitting, setSubmitting] = React.useState(false);
  const [error, setError] = React.useState<string | undefined>();
  const [pendingDelete, setPendingDelete] = React.useState<PaymentCertificateView | null>(null);

  async function handleCreate(draft: PaymentCertificateDraft) {
    setSubmitting(true);
    setError(undefined);
    try {
      await props.onCreate(draft);
      setDialogOpen(false);
    } catch (err) {
      setError(err instanceof Error ? err.message : m.certificates.errorCreateFailed);
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
      setError(err instanceof Error ? err.message : m.certificates.errorRegisterFailed);
    }
    setPendingDelete(null);
  }

  return (
    <div className="space-y-3" data-slot="certificate-manager">
      <div className="flex items-center justify-between">
        <div>
          <div className="text-xs font-semibold uppercase tracking-wider text-[var(--sdk-color-text-muted)]">
            {m.certificates.header}
          </div>
          <div className="mt-1 text-xs text-[var(--sdk-color-text-secondary)]">
            {m.certificates.headerHint}
          </div>
        </div>
        <Button
          type="button"
          size="sm"
          onClick={() => setDialogOpen(true)}
          disabled={props.busy}
          title={props.busy ? m.certificates.registerBusyTitle : m.certificates.registerTitle}
        >
          {m.certificates.registerButton}
        </Button>
      </div>

      {props.certificates.length === 0 ? (
        <div className="rounded-md border border-dashed border-[var(--sdk-color-border-subtle)] p-8 text-center text-sm text-[var(--sdk-color-text-secondary)]">
          {m.certificates.emptyState}
          <div className="mt-3">
            <Button type="button" variant="primary" size="sm" onClick={() => setDialogOpen(true)} disabled={props.busy}>
              {m.certificates.registerButton}
            </Button>
          </div>
        </div>
      ) : (
        <ul className="divide-y divide-[var(--sdk-color-border-subtle)] rounded-md border border-[var(--sdk-color-border-subtle)]">
          {props.certificates.map((certificate) => {
            const expiry = computeExpiryState(certificate.expiresAt);
            return (
              <li
                key={certificate.id}
                className="flex flex-col gap-3 p-4 sm:flex-row sm:items-center sm:justify-between"
                data-slot="certificate-row"
              >
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="font-medium text-[var(--sdk-color-text)]">
                      {certificate.certificateNo}
                    </span>
                    <Badge variant="outline">{certificateTypeLabel[certificate.certificateType]}</Badge>
                    {certificate.providerCode ? (
                      <Badge variant="secondary">{certificate.providerCode}</Badge>
                    ) : null}
                    <Badge variant={STATUS_VARIANT[certificate.status]}>
                      {statusLabel[certificate.status]}
                    </Badge>
                    {expiry.kind === "expired" ? (
                      <Badge variant="danger">{m.certificates.expiredDaysAgo.replace("{days}", String(expiry.days))}</Badge>
                    ) : expiry.kind === "expiring" ? (
                      <Badge variant="warning">{m.certificates.expiresInDays.replace("{days}", String(expiry.days))}</Badge>
                    ) : null}
                  </div>
                  <dl className="mt-2 grid grid-cols-1 gap-x-6 gap-y-1 text-xs text-[var(--sdk-color-text-secondary)] sm:grid-cols-3">
                    <div>
                      <dt className="inline">{m.certificates.subjectLabel}</dt>{" "}
                      <dd className="inline">{certificate.subject ?? "—"}</dd>
                    </div>
                    <div>
                      <dt className="inline">{m.certificates.issuerLabel}</dt>{" "}
                      <dd className="inline">{certificate.issuer ?? "—"}</dd>
                    </div>
                    <div>
                      <dt className="inline">{m.certificates.expiresLabel}</dt>{" "}
                      <dd className="inline">
                        {certificate.expiresAt ? formatAdminTimestamp(certificate.expiresAt) : "—"}
                      </dd>
                    </div>
                    <div>
                      <dt className="inline">{m.certificates.contentLabel}</dt>{" "}
                      <dd className="inline">{certificate.hasContent ? m.certificates.contentEncrypted : m.certificates.contentMissing}</dd>
                    </div>
                    <div>
                      <dt className="inline">{m.certificates.fingerprintLabel}</dt>{" "}
                      <dd className="inline font-mono">
                        {certificate.fingerprint ? truncateFingerprint(certificate.fingerprint) : "—"}
                      </dd>
                    </div>
                  </dl>
                </div>
                <div className="flex items-center gap-2">
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    onClick={() => setPendingDelete(certificate)}
                    disabled={props.busy}
                    title={m.certificates.deleteButtonTitle}
                  >
                    {m.common.delete}
                  </Button>
                </div>
              </li>
            );
          })}
        </ul>
      )}

      <SdkworkPaymentListPaginationControls
        busy={props.busy ?? false}
        onLoadMore={props.onLoadMore}
        pageInfo={props.pageInfo}
      />

      {error ? (
        <div
          role="alert"
          className="rounded-md border border-[var(--sdk-color-border-error)] bg-[var(--sdk-color-bg-error-subtle)] p-3 text-sm text-[var(--sdk-color-text-error)]"
        >
          {error}
        </div>
      ) : null}

      <Dialog open={dialogOpen} onOpenChange={setDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{m.certificates.dialogTitle}</DialogTitle>
          </DialogHeader>
          <CertificateForm
            onCancel={() => setDialogOpen(false)}
            onSubmit={handleCreate}
            submitting={submitting}
          />
        </DialogContent>
      </Dialog>

      <ConfirmDialog
        open={pendingDelete !== null}
        title={m.certificates.confirmDeleteTitle}
        description={
          pendingDelete
            ? m.certificates.confirmDeleteDescription.replace("{certificateNo}", pendingDelete.certificateNo)
            : ""
        }
        confirmLabel={m.common.delete}
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

interface CertificateFormProps {
  onCancel(): void;
  onSubmit(draft: PaymentCertificateDraft): Promise<void> | void;
  submitting: boolean;
}

interface CertificateFormState {
  certificateNo: string;
  providerCode: string;
  certificateType: PaymentCertificateKind;
  certificate: string;
}

function CertificateForm(props: CertificateFormProps) {
  const m = useDevConfigMessages();
  const certificateTypeOptions: ReadonlyArray<{ label: string; value: PaymentCertificateKind }> = [
    { label: m.certificates.typeMerchantPrivateKey, value: "merchant_private_key" },
    { label: m.certificates.typeProviderPublicKey, value: "provider_public_key" },
    { label: m.certificates.typePlatformCertificate, value: "platform_certificate" },
    { label: m.certificates.typeWebhookSecret, value: "webhook_secret" },
  ];
  const [state, setState] = React.useState<CertificateFormState>({
    certificateNo: "",
    providerCode: "",
    certificateType: "merchant_private_key",
    certificate: "",
  });
  const [formError, setFormError] = React.useState<string | undefined>();

  function update<K extends keyof CertificateFormState>(key: K, value: CertificateFormState[K]) {
    setState((prev) => ({ ...prev, [key]: value }));
  }

  async function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setFormError(undefined);
    if (!state.certificateNo.trim() || !state.certificate.trim()) {
      setFormError(m.certificates.errorRequired);
      return;
    }
    const draft: PaymentCertificateDraft = {
      certificateNo: state.certificateNo.trim(),
      certificateType: state.certificateType,
      certificate: state.certificate.trim(),
      ...(state.providerCode ? { providerCode: state.providerCode as PaymentProviderCode } : {}),
    };
    try {
      await props.onSubmit(draft);
    } catch (err) {
      setFormError(err instanceof Error ? err.message : m.certificates.errorRegisterFailed);
    }
  }

  return (
    <form className="space-y-4" onSubmit={handleSubmit}>
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        <AdminFieldLabel label={m.certificates.certificateNo} htmlFor="cert-certificate-no" required>
          <Input
            id="cert-certificate-no"
            value={state.certificateNo}
            onChange={(event) => update("certificateNo", event.target.value)}
            placeholder={m.certificates.certificateNoPlaceholder}
            required
          />
        </AdminFieldLabel>
        <AdminFieldLabel label={m.certificates.type} htmlFor="cert-type" required>
          <Select value={state.certificateType} onValueChange={(value) => update("certificateType", value as PaymentCertificateKind)}>
            <SelectTrigger id="cert-type">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {certificateTypeOptions.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  {option.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </AdminFieldLabel>
        <AdminFieldLabel label={m.certificates.providerOptional} htmlFor="cert-provider-code">
          <Select value={state.providerCode} onValueChange={(value) => update("providerCode", value)}>
            <SelectTrigger id="cert-provider-code">
              <SelectValue placeholder={m.certificates.anyProvider} />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="">{m.certificates.anyProvider}</SelectItem>
              {ADMIN_PROVIDER_FORM_OPTIONS.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  {option.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </AdminFieldLabel>
      </div>
      <AdminFieldLabel label={m.certificates.pemContent} htmlFor="cert-pem-content" required>
        <Textarea
          id="cert-pem-content"
          className="min-h-32 resize-y font-mono"
          value={state.certificate}
          onChange={(event) => update("certificate", event.target.value)}
          placeholder={m.certificates.pemPlaceholder}
          required
          autoComplete="new-password"
        />
        <PemFilePicker
          maxBytes={MAX_CERTIFICATE_FILE_BYTES}
          disabled={props.submitting}
          onContent={(content) => update("certificate", content)}
        />
      </AdminFieldLabel>
      {formError ? (
        <div
          role="alert"
          className="rounded-md border border-[var(--sdk-color-border-error)] bg-[var(--sdk-color-bg-error-subtle)] p-3 text-sm text-[var(--sdk-color-text-error)]"
        >
          {formError}
        </div>
      ) : null}
      <div className="flex justify-end gap-2">
        <Button type="button" variant="ghost" onClick={props.onCancel} disabled={props.submitting} title={m.certificates.cancelTitle}>
          {m.common.cancel}
        </Button>
        <Button type="submit" disabled={props.submitting} title={m.certificates.submitTitle}>
          {props.submitting ? m.certificates.registering : m.certificates.registerButton}
        </Button>
      </div>
    </form>
  );
}

function computeExpiryState(expiresAt: string | undefined):
  | { kind: "unknown" }
  | { kind: "valid" }
  | { kind: "expiring"; days: number }
  | { kind: "expired"; days: number } {
  if (!expiresAt) {
    return { kind: "unknown" };
  }
  const parsed = new Date(expiresAt);
  if (Number.isNaN(parsed.getTime())) {
    return { kind: "unknown" };
  }
  const now = new Date();
  const diffMs = parsed.getTime() - now.getTime();
  const days = Math.floor(diffMs / (1000 * 60 * 60 * 24));
  if (days < 0) {
    return { kind: "expired", days: Math.abs(days) };
  }
  if (days <= EXPIRY_WARNING_DAYS) {
    return { kind: "expiring", days };
  }
  return { kind: "valid" };
}

function truncateFingerprint(value: string): string {
  if (value.length <= 16) {
    return value;
  }
  return `${value.slice(0, 8)}…${value.slice(-8)}`;
}
