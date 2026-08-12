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

const CERTIFICATE_TYPE_LABEL: Record<PaymentCertificateKind, string> = {
  merchant_private_key: "Merchant private key",
  provider_public_key: "Provider public key",
  platform_certificate: "Platform certificate",
  // The webhook_secret certificate kind carries the WeChat API v3 decryption
  // key (callback resource decryption), so it is labeled "API v3 Key".
  webhook_secret: "API v3 Key",
};

const CERTIFICATE_TYPE_OPTIONS: ReadonlyArray<{ label: string; value: PaymentCertificateKind }> = [
  { label: "Merchant private key", value: "merchant_private_key" },
  { label: "Provider public key", value: "provider_public_key" },
  { label: "Platform certificate", value: "platform_certificate" },
  { label: "API v3 Key", value: "webhook_secret" },
];

const STATUS_VARIANT: Record<PaymentCertificateView["status"], "success" | "warning" | "danger" | "secondary"> = {
  active: "success",
  pending_rotation: "warning",
  expired: "danger",
  revoked: "secondary",
};

const STATUS_LABEL: Record<PaymentCertificateView["status"], string> = {
  active: "Active",
  pending_rotation: "Pending rotation",
  expired: "Expired",
  revoked: "Revoked",
};

const EXPIRY_WARNING_DAYS = 30;

// Backend certificate length limit (maxLength) for uploaded files.
const MAX_CERTIFICATE_FILE_BYTES = 65536;

export function CertificateManager(props: CertificateManagerProps) {
  const [dialogOpen, setDialogOpen] = React.useState(false);
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
      setError(err instanceof Error ? err.message : "Failed to create certificate.");
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
      setError(err instanceof Error ? err.message : "Failed to delete certificate.");
    }
    setPendingDelete(null);
  }

  return (
    <div className="space-y-3" data-slot="certificate-manager">
      <div className="flex items-center justify-between">
        <div>
          <div className="text-xs font-semibold uppercase tracking-wider text-[var(--sdk-color-text-muted)]">
            PEM certificate references
          </div>
          <div className="mt-1 text-xs text-[var(--sdk-color-text-secondary)]">
            Env var references only — plaintext PEM content never persists in DB.
          </div>
        </div>
        <Button
          type="button"
          size="sm"
          onClick={() => setDialogOpen(true)}
          disabled={props.busy}
          title={props.busy ? "Cannot register while another operation is in progress" : "Register a new certificate reference"}
        >
          Register certificate
        </Button>
      </div>

      {props.certificates.length === 0 ? (
        <div className="rounded-md border border-dashed border-[var(--sdk-color-border-subtle)] p-8 text-center text-sm text-[var(--sdk-color-text-secondary)]">
          No certificates registered. Register a PEM reference to enable provider authentication.
          <div className="mt-3">
            <Button type="button" variant="primary" size="sm" onClick={() => setDialogOpen(true)} disabled={props.busy}>
              Register certificate
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
                    <Badge variant="outline">{CERTIFICATE_TYPE_LABEL[certificate.certificateType]}</Badge>
                    {certificate.providerCode ? (
                      <Badge variant="secondary">{certificate.providerCode}</Badge>
                    ) : null}
                    <Badge variant={STATUS_VARIANT[certificate.status]}>
                      {STATUS_LABEL[certificate.status]}
                    </Badge>
                    {expiry.kind === "expired" ? (
                      <Badge variant="danger">Expired {expiry.days}d ago</Badge>
                    ) : expiry.kind === "expiring" ? (
                      <Badge variant="warning">Expires in {expiry.days}d</Badge>
                    ) : null}
                  </div>
                  <dl className="mt-2 grid grid-cols-1 gap-x-6 gap-y-1 text-xs text-[var(--sdk-color-text-secondary)] sm:grid-cols-3">
                    <div>
                      <dt className="inline">Subject:</dt>{" "}
                      <dd className="inline">{certificate.subject ?? "—"}</dd>
                    </div>
                    <div>
                      <dt className="inline">Issuer:</dt>{" "}
                      <dd className="inline">{certificate.issuer ?? "—"}</dd>
                    </div>
                    <div>
                      <dt className="inline">Expires:</dt>{" "}
                      <dd className="inline">
                        {certificate.expiresAt ? formatAdminTimestamp(certificate.expiresAt) : "—"}
                      </dd>
                    </div>
                    <div>
                      <dt className="inline">Content:</dt>{" "}
                      <dd className="inline">{certificate.hasContent ? "Encrypted" : "Missing"}</dd>
                    </div>
                    <div>
                      <dt className="inline">Fingerprint:</dt>{" "}
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
                    title="Delete this certificate reference"
                  >
                    Delete
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
            <DialogTitle>Register certificate reference</DialogTitle>
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
        title="Delete certificate reference?"
        description={
          pendingDelete
            ? `Delete certificate ${pendingDelete.certificateNo}? The underlying PEM content in your secret store is not affected; only the reference is removed.`
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
      setFormError("Certificate no and PEM content are required.");
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
      setFormError(err instanceof Error ? err.message : "Failed to register certificate.");
    }
  }

  return (
    <form className="space-y-4" onSubmit={handleSubmit}>
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        <AdminFieldLabel label="Certificate No" htmlFor="cert-certificate-no" required>
          <Input
            id="cert-certificate-no"
            value={state.certificateNo}
            onChange={(event) => update("certificateNo", event.target.value)}
            placeholder="e.g., alipay-public-key-prod"
            required
          />
        </AdminFieldLabel>
        <AdminFieldLabel label="Type" htmlFor="cert-type" required>
          <Select value={state.certificateType} onValueChange={(value) => update("certificateType", value as PaymentCertificateKind)}>
            <SelectTrigger id="cert-type">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {CERTIFICATE_TYPE_OPTIONS.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  {option.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </AdminFieldLabel>
        <AdminFieldLabel label="Provider (optional)" htmlFor="cert-provider-code">
          <Select value={state.providerCode} onValueChange={(value) => update("providerCode", value)}>
            <SelectTrigger id="cert-provider-code">
              <SelectValue placeholder="Any provider" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="">Any provider</SelectItem>
              {ADMIN_PROVIDER_FORM_OPTIONS.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  {option.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </AdminFieldLabel>
      </div>
      <AdminFieldLabel label="PEM Content" htmlFor="cert-pem-content" required>
        <Textarea
          id="cert-pem-content"
          className="min-h-32 resize-y font-mono"
          value={state.certificate}
          onChange={(event) => update("certificate", event.target.value)}
          placeholder="Paste PEM content"
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
        <Button type="button" variant="ghost" onClick={props.onCancel} disabled={props.submitting} title="Cancel certificate registration">
          Cancel
        </Button>
        <Button type="submit" disabled={props.submitting} title="Register this certificate">
          {props.submitting ? "Registering..." : "Register certificate"}
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
