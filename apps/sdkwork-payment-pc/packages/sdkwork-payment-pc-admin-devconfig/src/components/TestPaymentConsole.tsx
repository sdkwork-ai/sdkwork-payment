/**
 * One-cent test console.
 *
 * The operator-facing acceptance surface for "configure a service provider,
 * then exercise the whole payment function":
 *
 *   1. Coverage board — every product the shipped adapters declare, with the
 *      exact configuration still missing before a one-cent test can run.
 *   2. Test payment — create a 0.01 payment for the selected product and get
 *      the launch surface (QR / redirect / form / card / client payload).
 *   3. Settlement, status and close — poll the provider, simulate settlement
 *      for sandbox and client-payload products, or close the trade.
 *   4. Refund — refund a settled test payment through the production path.
 *   5. Notification — deliver a notification the deployment signed with the
 *      account's own credential through the real verification + ingest pipeline.
 *
 * Every action reports its own outcome inline; nothing is inferred from a
 * previous step, so a failing link in the chain is always the visible one.
 */

import * as React from "react";
import {
  Badge,
  Button,
  Input,
  Label,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@sdkwork/ui-pc-react";
import {
  ADMIN_PROVIDER_LABEL,
  AdminFieldLabel,
  formatAdminAmount,
  formatAdminTimestamp,
} from "@sdkwork/payment-pc-admin-core";
import { useDevConfigMessages } from "../i18n";
import type {
  PaymentProviderAccountView,
  PaymentTestConsoleResults,
  PaymentTestLaunchKind,
  PaymentTestNotificationDraft,
  PaymentTestPaymentDraft,
  PaymentTestPayerField,
  PaymentTestProductView,
  PaymentTestSigner,
} from "../types/devconfig-admin-types";

export interface TestPaymentConsoleProps {
  products?: import("../types/devconfig-admin-types").PaymentTestProductsView;
  accounts: readonly PaymentProviderAccountView[];
  results: PaymentTestConsoleResults;
  busy?: boolean;
  onReloadProducts(): Promise<void> | void;
  onCreateTestPayment(draft: PaymentTestPaymentDraft): Promise<void> | void;
  onCheckStatus(paymentIntentId: string): Promise<void> | void;
  onSettle(paymentIntentId: string, outcome: "success" | "failed"): Promise<void> | void;
  onCancel(paymentIntentId: string): Promise<void> | void;
  onRefund(paymentIntentId: string): Promise<void> | void;
  onTestNotification(draft: PaymentTestNotificationDraft): Promise<void> | void;
}

const LAUNCH_KIND_LABEL: Record<PaymentTestLaunchKind, string> = {
  qr_code: "QR code",
  redirect: "Redirect",
  form_redirect: "Cashier form",
  card: "Card element",
  jsapi: "Client payload",
  app: "App SDK payload",
  local_sandbox: "Local simulation",
  unsupported: "No launch surface",
};

/** Reason codes the backend accepts for a test refund. */
const REFUND_REASON_CODES = [
  "customer_request",
  "duplicate",
  "fraud",
  "service_failure",
  "other",
] as const;

function payerFieldInputId(field: string): string {
  return `test-payment-payer-${field.replace(/_/g, "-")}`;
}

function isPayerFieldName(field: string): field is "openid" | "clientIp" | "buyerId" {
  return field === "openid" || field === "client_ip" || field === "buyer_id";
}

export function TestPaymentConsole(props: TestPaymentConsoleProps) {
  const m = useDevConfigMessages();
  const products = props.products?.products ?? [];

  // The selected product drives the payer fields and the settlement rules.
  const [methodKey, setMethodKey] = React.useState("");
  const [amount, setAmount] = React.useState("0.01");
  const [currencyCode, setCurrencyCode] = React.useState("CNY");
  const [payer, setPayer] = React.useState<Record<string, string>>({});
  const [error, setError] = React.useState<string | undefined>();

  // Notification delivery form.
  const [notificationAccountId, setNotificationAccountId] = React.useState("");
  const [notificationPayload, setNotificationPayload] = React.useState("");
  const [notificationSigner, setNotificationSigner] = React.useState<PaymentTestSigner | "">("");

  React.useEffect(() => {
    if (!methodKey && products.length > 0) {
      const firstUsable = products.find((product) => product.ready) ?? products[0];
      setMethodKey(firstUsable.methodKey);
    }
  }, [products, methodKey]);

  React.useEffect(() => {
    if (!notificationAccountId && props.accounts.length > 0) {
      setNotificationAccountId(props.accounts[0].id);
    }
  }, [props.accounts, notificationAccountId]);

  const selected: PaymentTestProductView | undefined = products.find(
    (product) => product.methodKey === methodKey,
  );
  const payment = props.results.payment;

  function updatePayer(field: string, value: string): void {
    setPayer((current) => ({ ...current, [field]: value }));
  }

  async function handleCreate(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError(undefined);
    if (!methodKey) {
      setError(m.testPay.needsMethod);
      return;
    }
    // Payer inputs are declared by the product; the dialog never guesses which
    // ones a product needs.
    const missing = (selected?.requiredPayerFields ?? []).filter(
      (field) => !(payer[field.field] ?? "").trim(),
    );
    if (missing.length > 0) {
      setError(
        `${m.testPay.needsPayerFields}: ${missing.map((field) => field.label).join(", ")}`,
      );
      return;
    }
    const payerIdentity: PaymentTestPaymentDraft["payerIdentity"] = {};
    for (const [field, value] of Object.entries(payer)) {
      if (!value.trim()) {
        continue;
      }
      if (isPayerFieldName(field)) {
        Object.assign(payerIdentity, { [field]: value.trim() });
      }
    }
    try {
      await props.onCreateTestPayment({
        methodKey,
        ...(amount.trim() ? { amount: amount.trim() } : {}),
        ...(currencyCode.trim() ? { currencyCode: currencyCode.trim() } : {}),
        ...(Object.keys(payerIdentity).length > 0 ? { payerIdentity } : {}),
      });
    } catch (err) {
      setError(err instanceof Error ? err.message : m.testPay.createFailed);
    }
  }

  async function handleNotification(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError(undefined);
    if (!notificationAccountId) {
      setError(m.common.errorAccountRequired);
      return;
    }
    if (!notificationPayload.trim()) {
      setError(m.testPay.notificationPayloadRequired);
      return;
    }
    try {
      await props.onTestNotification({
        providerAccountId: notificationAccountId,
        payload: notificationPayload.trim(),
        ...(notificationSigner ? { signer: notificationSigner } : {}),
      });
    } catch (err) {
      setError(err instanceof Error ? err.message : m.testPay.notificationFailed);
    }
  }

  return (
    <div className="space-y-6" data-slot="one-cent-test-console">
      <CoverageBoard
        products={props.products}
        busy={props.busy}
        onReload={props.onReloadProducts}
      />

      <section className="rounded-md border border-[var(--sdk-color-border-subtle)] p-4">
        <header className="mb-3">
          <div className="text-xs font-semibold uppercase tracking-wider text-[var(--sdk-color-text-muted)]">
            {m.testPay.createHeader}
          </div>
          <p className="mt-1 text-xs text-[var(--sdk-color-text-secondary)]">
            {m.testPay.createIntro}
          </p>
        </header>
        <form className="space-y-3" onSubmit={handleCreate}>
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
            <AdminFieldLabel label={m.testPay.method} htmlFor="test-payment-method" required>
              <Select value={methodKey} onValueChange={setMethodKey} disabled={props.busy}>
                <SelectTrigger id="test-payment-method" aria-label={m.testPay.method}>
                  <SelectValue placeholder={m.testPay.methodPlaceholder} />
                </SelectTrigger>
                <SelectContent>
                  {products.map((product) => (
                    <SelectItem key={product.methodKey} value={product.methodKey}>
                      {product.providerLabel} · {product.methodKey}
                      {product.ready ? "" : ` (${m.testPay.notReady})`}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </AdminFieldLabel>
            <AdminFieldLabel label={m.testPay.amount} htmlFor="test-payment-amount">
              <Input
                id="test-payment-amount"
                value={amount}
                onChange={(event) => setAmount(event.target.value)}
                placeholder="0.01"
                disabled={props.busy}
              />
            </AdminFieldLabel>
            <AdminFieldLabel label={m.testPay.currency} htmlFor="test-payment-currency">
              <Input
                id="test-payment-currency"
                value={currencyCode}
                onChange={(event) => setCurrencyCode(event.target.value)}
                placeholder="CNY"
                maxLength={3}
                disabled={props.busy}
              />
            </AdminFieldLabel>
          </div>

          {(selected?.requiredPayerFields ?? []).length > 0 ? (
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
              {selected?.requiredPayerFields.map((field: PaymentTestPayerField) => (
                <AdminFieldLabel
                  key={field.field}
                  label={field.label}
                  htmlFor={payerFieldInputId(field.field)}
                  required
                >
                  <Input
                    id={payerFieldInputId(field.field)}
                    value={payer[field.field] ?? ""}
                    onChange={(event) => updatePayer(field.field, event.target.value)}
                    placeholder={field.field}
                    disabled={props.busy}
                  />
                </AdminFieldLabel>
              ))}
            </div>
          ) : null}

          {error ? (
            <div
              role="alert"
              className="rounded-md border border-[var(--sdk-color-border-error)] bg-[var(--sdk-color-bg-error-subtle)] p-3 text-sm text-[var(--sdk-color-text-error)]"
            >
              {error}
            </div>
          ) : null}

          <div className="flex justify-end">
            <Button type="submit" disabled={props.busy || products.length === 0}>
              {props.busy ? m.testPay.creating : m.testPay.createSubmit}
            </Button>
          </div>
        </form>

        {payment ? <LaunchPanel payment={payment} /> : null}
        {payment ? (
          <SettlementPanel
            payment={payment}
            busy={props.busy}
            results={props.results}
            onCheck={props.onCheckStatus}
            onSettle={props.onSettle}
            onCancel={props.onCancel}
          />
        ) : null}
        {payment ? <RefundPanel payment={payment} busy={props.busy} results={props.results} onRefund={props.onRefund} /> : null}
      </section>

      <section className="rounded-md border border-[var(--sdk-color-border-subtle)] p-4">
        <header className="mb-3">
          <div className="text-xs font-semibold uppercase tracking-wider text-[var(--sdk-color-text-muted)]">
            {m.testPay.notificationHeader}
          </div>
          <p className="mt-1 text-xs text-[var(--sdk-color-text-secondary)]">
            {m.testPay.notificationIntro}
          </p>
        </header>
        <form className="space-y-3" onSubmit={handleNotification}>
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <AdminFieldLabel
              label={m.common.providerAccount}
              htmlFor="test-payment-notification-account"
              required
            >
              <Select
                value={notificationAccountId}
                onValueChange={setNotificationAccountId}
                disabled={props.busy}
              >
                <SelectTrigger id="test-payment-notification-account" aria-label={m.common.providerAccount}>
                  <SelectValue
                    placeholder={
                      props.accounts.length === 0
                        ? m.common.noEligibleAccounts
                        : m.common.accountPlaceholder
                    }
                  />
                </SelectTrigger>
                <SelectContent>
                  {props.accounts.map((account) => (
                    <SelectItem key={account.id} value={account.id}>
                      {account.accountNo} ({ADMIN_PROVIDER_LABEL[account.providerCode]} ·{" "}
                      {account.environment})
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </AdminFieldLabel>
            <AdminFieldLabel
              label={m.testPay.notificationSigner}
              htmlFor="test-payment-notification-signer"
            >
              <Select
                value={notificationSigner || "auto"}
                onValueChange={(value) =>
                  setNotificationSigner(value === "auto" ? "" : (value as PaymentTestSigner))
                }
                disabled={props.busy}
              >
                <SelectTrigger id="test-payment-notification-signer" aria-label={m.testPay.notificationSigner}>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="auto">{m.testPay.notificationSignerAuto}</SelectItem>
                  <SelectItem value="hmac_sha256">HMAC-SHA256</SelectItem>
                  <SelectItem value="wechat_pay_rsa_sha256">WeChat Pay RSA-SHA256</SelectItem>
                  <SelectItem value="alipay_rsa_sha256">Alipay RSA-SHA256</SelectItem>
                  <SelectItem value="none">{m.testPay.notificationSignerNone}</SelectItem>
                </SelectContent>
              </Select>
            </AdminFieldLabel>
          </div>
          <AdminFieldLabel
            label={m.testPay.notificationPayload}
            htmlFor="test-payment-notification-payload"
            required
          >
            <textarea
              id="test-payment-notification-payload"
              className="min-h-[8rem] w-full rounded-md border border-[var(--sdk-color-border-default)] bg-[var(--sdk-color-surface-panel)] px-3 py-2 font-mono text-sm shadow-[var(--sdk-shadow-sm)] outline-none focus:ring-2 focus:ring-[var(--sdk-color-border-focus)]"
              value={notificationPayload}
              onChange={(event) => setNotificationPayload(event.target.value)}
              placeholder={m.testPay.notificationPayloadPlaceholder}
              disabled={props.busy}
              required
            />
          </AdminFieldLabel>
          <div className="flex justify-end">
            <Button
              type="submit"
              disabled={props.busy || props.accounts.length === 0}
            >
              {props.busy ? m.testPay.notificationSending : m.testPay.notificationSubmit}
            </Button>
          </div>
        </form>
        {props.results.notification ? (
          <NotificationResultCard result={props.results.notification} />
        ) : null}
      </section>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Coverage board
// ---------------------------------------------------------------------------

function CoverageBoard(props: {
  products?: import("../types/devconfig-admin-types").PaymentTestProductsView;
  busy?: boolean;
  onReload(): Promise<void> | void;
}) {
  const m = useDevConfigMessages();
  return (
    <section className="rounded-md border border-[var(--sdk-color-border-subtle)] p-4">
      <header className="mb-3 flex flex-wrap items-start justify-between gap-3">
        <div>
          <div className="text-xs font-semibold uppercase tracking-wider text-[var(--sdk-color-text-muted)]">
            {m.testPay.coverageHeader}
          </div>
          <p className="mt-1 text-xs text-[var(--sdk-color-text-secondary)]">
            {m.testPay.coverageIntro}
          </p>
        </div>
        <div className="flex items-center gap-2">
          {props.products ? (
            <Badge variant="outline">
              {m.testPay.coverageReady
                .replace("{ready}", String(props.products.readyCount))
                .replace("{total}", String(props.products.totalCount))}
            </Badge>
          ) : null}
          <Button type="button" variant="outline" onClick={() => void props.onReload()} disabled={props.busy}>
            {m.testPay.coverageReload}
          </Button>
        </div>
      </header>
      {props.products ? (
        <ul className="divide-y divide-[var(--sdk-color-border-subtle)]" aria-label={m.testPay.coverageHeader}>
          {props.products.products.map((product) => (
            <li key={product.methodKey} className="py-2" data-slot="test-coverage-row">
              <div className="flex flex-wrap items-center gap-2 text-xs">
                <Badge variant={product.ready ? "success" : "warning"}>
                  {product.ready ? m.testPay.ready : m.testPay.blocked}
                </Badge>
                <span className="font-mono">{product.methodKey}</span>
                <span className="text-[var(--sdk-color-text-secondary)]">{product.providerLabel}</span>
                <Badge variant="outline">{LAUNCH_KIND_LABEL[product.launchKind]}</Badge>
                {product.settleSupported ? (
                  <Badge variant="secondary">{m.testPay.badgeSettle}</Badge>
                ) : null}
                {product.refundSupported ? (
                  <Badge variant="secondary">{m.testPay.badgeRefund}</Badge>
                ) : null}
                {product.requiredPayerFields.length > 0 ? (
                  <Badge variant="outline">
                    {m.testPay.badgePayer}:{" "}
                    {product.requiredPayerFields.map((field) => field.field).join(", ")}
                  </Badge>
                ) : null}
              </div>
              <ul className="mt-1 space-y-0.5 text-xs text-[var(--sdk-color-text-secondary)]">
                {product.readiness.map((finding, index) => (
                  <li key={`${product.methodKey}-${index}`}>{finding}</li>
                ))}
              </ul>
            </li>
          ))}
        </ul>
      ) : (
        <p className="text-xs text-[var(--sdk-color-text-secondary)]">{m.testPay.coverageEmpty}</p>
      )}
      {props.products && props.products.unconfiguredProviders.length > 0 ? (
        <p className="mt-2 text-xs text-[var(--sdk-color-text-secondary)]">
          {m.testPay.unconfiguredProviders}: {props.products.unconfiguredProviders.join(", ")}
        </p>
      ) : null}
    </section>
  );
}

// ---------------------------------------------------------------------------
// Launch surface
// ---------------------------------------------------------------------------

function LaunchPanel(props: { payment: NonNullable<PaymentTestConsoleResults["payment"]> }) {
  const m = useDevConfigMessages();
  const { payment } = props;
  return (
    <div
      className="mt-4 rounded-md border border-[var(--sdk-color-border-subtle)] p-3"
      role="status"
      data-slot="test-payment-launch"
    >
      <div className="flex flex-wrap items-center gap-2 text-xs">
        <Badge variant="outline">{LAUNCH_KIND_LABEL[payment.launchKind]}</Badge>
        <span className="font-mono">{payment.outTradeNo}</span>
        <span>{formatAdminAmount(payment.amount, payment.currencyCode)}</span>
        <Badge variant="secondary">{payment.status}</Badge>
      </div>
      <dl className="mt-2 grid grid-cols-1 gap-1 text-xs sm:grid-cols-2">
        <div>
          <dt className="inline text-[var(--sdk-color-text-muted)]">{m.testPay.intentLabel}: </dt>
          <dd className="inline font-mono">{payment.paymentIntentId}</dd>
        </div>
        <div>
          <dt className="inline text-[var(--sdk-color-text-muted)]">{m.testPay.nextActionLabel}: </dt>
          <dd className="inline">{payment.nextAction ?? "—"}</dd>
        </div>
      </dl>

      {payment.launchKind === "qr_code" && payment.qrCodeUrl ? (
        <p className="mt-2 break-all text-xs">
          <span className="text-[var(--sdk-color-text-muted)]">{m.testPay.qrLabel}: </span>
          <a
            className="font-mono text-[var(--sdk-color-text-link)] underline"
            href={payment.qrCodeUrl}
            rel="noreferrer noopener"
            target="_blank"
          >
            {payment.qrCodeUrl}
          </a>
        </p>
      ) : null}
      {payment.payUrl ? (
        <p className="mt-2 break-all text-xs">
          <span className="text-[var(--sdk-color-text-muted)]">{m.testPay.redirectLabel}: </span>
          <a
            className="font-mono text-[var(--sdk-color-text-link)] underline"
            href={payment.payUrl}
            rel="noreferrer noopener"
            target="_blank"
          >
            {payment.payUrl}
          </a>
        </p>
      ) : null}
      {payment.cashierUrl ? (
        <p className="mt-2 break-all text-xs">
          <span className="text-[var(--sdk-color-text-muted)]">{m.testPay.cashierLabel}: </span>
          <a
            className="font-mono text-[var(--sdk-color-text-link)] underline"
            href={payment.cashierUrl}
            rel="noreferrer noopener"
            target="_blank"
          >
            {payment.cashierUrl}
          </a>
        </p>
      ) : null}
      {payment.payForm ? (
        <details className="mt-2 text-xs">
          <summary className="cursor-pointer">{m.testPay.formLabel}</summary>
          <pre className="mt-1 max-h-40 overflow-auto rounded bg-[var(--sdk-color-bg-subtle)] p-2 font-mono">
            {payment.payForm}
          </pre>
        </details>
      ) : null}
      {payment.clientSecret ? (
        <p className="mt-2 break-all text-xs">
          <span className="text-[var(--sdk-color-text-muted)]">{m.testPay.clientSecretLabel}: </span>
          <span className="font-mono">{payment.clientSecret}</span>
        </p>
      ) : null}
      {payment.launchPayload ? (
        <details className="mt-2 text-xs">
          <summary className="cursor-pointer">{m.testPay.payloadLabel}</summary>
          <pre className="mt-1 max-h-40 overflow-auto rounded bg-[var(--sdk-color-bg-subtle)] p-2 font-mono">
            {payment.launchPayload}
          </pre>
        </details>
      ) : null}
      {payment.launchKind === "unsupported" ? (
        <p className="mt-2 text-xs text-[var(--sdk-color-text-error)]">{m.testPay.unsupportedLaunch}</p>
      ) : null}
      {payment.providerParamKeys.length > 0 ? (
        <p className="mt-2 break-all text-xs text-[var(--sdk-color-text-muted)]">
          {m.testPay.paramsLabel}: {payment.providerParamKeys.join(", ")}
        </p>
      ) : null}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Settlement / status / close
// ---------------------------------------------------------------------------

function SettlementPanel(props: {
  payment: NonNullable<PaymentTestConsoleResults["payment"]>;
  busy?: boolean;
  results: PaymentTestConsoleResults;
  onCheck(paymentIntentId: string): Promise<void> | void;
  onSettle(paymentIntentId: string, outcome: "success" | "failed"): Promise<void> | void;
  onCancel(paymentIntentId: string): Promise<void> | void;
}) {
  const m = useDevConfigMessages();
  const { payment } = props;
  const intentId = payment.paymentIntentId;
  return (
    <div className="mt-4 rounded-md border border-[var(--sdk-color-border-subtle)] p-3">
      <div className="text-xs font-semibold uppercase tracking-wider text-[var(--sdk-color-text-muted)]">
        {m.testPay.settlementHeader}
      </div>
      <p className="mt-1 text-xs text-[var(--sdk-color-text-secondary)]">
        {payment.settleSupported ? m.testPay.settlementSimulatable : m.testPay.settlementProviderOnly}
      </p>
      <div className="mt-2 flex flex-wrap gap-2">
        <Button
          type="button"
          variant="outline"
          disabled={props.busy}
          onClick={() => void props.onCheck(intentId)}
        >
          {m.testPay.checkStatus}
        </Button>
        <Button
          type="button"
          variant="secondary"
          disabled={props.busy || !payment.settleSupported}
          title={payment.settleSupported ? undefined : m.testPay.settlementDisabledTitle}
          onClick={() => void props.onSettle(intentId, "success")}
        >
          {m.testPay.settleSuccess}
        </Button>
        <Button
          type="button"
          variant="outline"
          disabled={props.busy || !payment.settleSupported}
          title={payment.settleSupported ? undefined : m.testPay.settlementDisabledTitle}
          onClick={() => void props.onSettle(intentId, "failed")}
        >
          {m.testPay.settleFailure}
        </Button>
        <Button
          type="button"
          variant="outline"
          disabled={props.busy}
          onClick={() => void props.onCancel(intentId)}
        >
          {m.testPay.closeTrade}
        </Button>
      </div>
      {props.results.status ? (
        <p className="mt-2 text-xs" role="status">
          {m.testPay.statusResult}: {props.results.status.localStatus}
          {props.results.status.providerStatus
            ? ` · ${m.testPay.providerStatusLabel}: ${props.results.status.providerStatus}`
            : ""}
          {props.results.status.paid === undefined
            ? ""
            : ` · ${props.results.status.paid ? m.testPay.paid : m.testPay.notPaid}`}
        </p>
      ) : null}
      {props.results.settled ? (
        <p className="mt-1 text-xs" role="status">
          {m.testPay.settledResult}: {props.results.settled.previousStatus ?? "→"} →{" "}
          {props.results.settled.localStatus}
          {props.results.settled.webhookEventId
            ? ` · ${m.testPay.webhookEventLabel}: ${props.results.settled.webhookEventId}`
            : ""}
        </p>
      ) : null}
      {props.results.cancelled ? (
        <p className="mt-1 text-xs" role="status">
          {m.testPay.closedResult}: {props.results.cancelled.previousStatus ?? "→"} →{" "}
          {props.results.cancelled.localStatus}
        </p>
      ) : null}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Refund
// ---------------------------------------------------------------------------

function RefundPanel(props: {
  payment: NonNullable<PaymentTestConsoleResults["payment"]>;
  busy?: boolean;
  results: PaymentTestConsoleResults;
  onRefund(paymentIntentId: string): Promise<void> | void;
}) {
  const m = useDevConfigMessages();
  const refund = props.results.refund;
  const status = props.results.settled?.localStatus ?? props.results.status?.localStatus ?? props.payment.status;
  const refundable = status === "succeeded";
  return (
    <div className="mt-4 rounded-md border border-[var(--sdk-color-border-subtle)] p-3">
      <div className="text-xs font-semibold uppercase tracking-wider text-[var(--sdk-color-text-muted)]">
        {m.testPay.refundHeader}
      </div>
      <p className="mt-1 text-xs text-[var(--sdk-color-text-secondary)]">
        {refundable ? m.testPay.refundIntro : m.testPay.refundNotSettled}
      </p>
      <div className="mt-2 flex flex-wrap items-center gap-2">
        <Button
          type="button"
          variant="outline"
          disabled={props.busy || !refundable}
          onClick={() => void props.onRefund(props.payment.paymentIntentId)}
        >
          {m.testPay.refundSubmit}
        </Button>
        <span className="text-xs text-[var(--sdk-color-text-muted)]">
          {m.testPay.refundReasonHint}: {REFUND_REASON_CODES.join(", ")}
        </span>
      </div>
      {refund ? (
        <div className="mt-2 text-xs" role="status">
          <div className="flex flex-wrap items-center gap-2">
            <Badge variant={refund.refunded ? "success" : refund.status === "processing" ? "warning" : "danger"}>
              {refund.status}
            </Badge>
            <span className="font-mono">{refund.refundNo}</span>
            <span>{formatAdminAmount(refund.amount, refund.currencyCode)}</span>
          </div>
          <p className="mt-1 text-[var(--sdk-color-text-secondary)]">{refund.nextStep}</p>
        </div>
      ) : null}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Notification result
// ---------------------------------------------------------------------------

function NotificationResultCard(props: {
  result: NonNullable<PaymentTestConsoleResults["notification"]>;
}) {
  const m = useDevConfigMessages();
  const { result } = props;
  return (
    <div
      className={
        "mt-3 rounded-md border p-3 text-sm " +
        (result.accepted
          ? "border-[var(--sdk-color-border-success)] bg-[var(--sdk-color-bg-success-subtle)] text-[var(--sdk-color-text-success)]"
          : "border-[var(--sdk-color-border-error)] bg-[var(--sdk-color-bg-error-subtle)] text-[var(--sdk-color-text-error)]")
      }
      role="status"
    >
      <div className="font-medium">
        {result.accepted ? m.testPay.notificationAccepted : m.testPay.notificationRejected}
      </div>
      <div className="mt-1 text-xs">
        {m.testPay.verificationLabel}: {result.verification} · {m.testPay.signatureLabel}:{" "}
        {result.nativeSignature} / {result.documentSignature}
        {result.signatureHeader ? ` · ${m.testPay.signatureHeaderLabel}: ${result.signatureHeader}` : ""}
        {result.outTradeNo ? ` · ${m.testPay.outTradeNoLabel}: ${result.outTradeNo}` : ""}
        {result.appliedStatus ? ` · ${m.testPay.appliedStatusLabel}: ${result.appliedStatus}` : ""}
        {result.webhookEventId ? ` · ${m.testPay.webhookEventLabel}: ${result.webhookEventId}` : ""}
        {` · ${formatAdminTimestamp(result.deliveredAt)}`}
      </div>
      {result.diagnostic ? <p className="mt-1 text-xs">{result.diagnostic}</p> : null}
    </div>
  );
}

// Re-exported for hosts that render the console pieces individually.
export { CoverageBoard, LaunchPanel, SettlementPanel, RefundPanel, NotificationResultCard };
