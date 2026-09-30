/**
 * Integration logs.
 *
 * Full webhook event timeline with replay capability. Distinct from the
 * WebhookDebugger's "Recent events" panel (which surfaces only the latest 5
 * events for context). This component supports:
 *   - Filtering by provider / status / received-at range
 *   - Paginated list via `createSdkWorkPagedListSession` (server-side)
 *   - Per-row replay (capped at WEBHOOK_STORED_ADMIN_WEBHOOK_REPLAY_MAX_RETRIES=5 by backend)
 *   - Replay count + last replay error visibility
 *
 * Mirrors industry PSP admin consoles (Stripe Dashboard → Developers → Events,
 * Alipay open platform → webhook records, WeChat Pay merchant platform →
 * webhook logs).
 */

import * as React from "react";
import {
  Badge,
  Button,
  Input,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@sdkwork/ui-pc-react";
import {
  ADMIN_PROVIDER_FILTER_OPTIONS,
  ADMIN_PROVIDER_LABEL,
  ADMIN_WEBHOOK_REPLAY_MAX_RETRIES,
  AdminFieldLabel,
  formatAdminTimestamp,
  SdkworkPaymentListPaginationControls,
} from "@sdkwork/payment-pc-admin-core";
import type { SdkWorkPageInfo } from "@sdkwork/payment-contracts";
import { useDevConfigMessages } from "../i18n";
import type {
  PaymentProviderCode,
  PaymentWebhookEventListFilter,
  PaymentWebhookEventView,
  PaymentWebhookReplayResult,
} from "../types/devconfig-admin-types";

export interface IntegrationLogsProps {
  events: readonly PaymentWebhookEventView[];
  pageInfo?: SdkWorkPageInfo;
  busy?: boolean;
  lastReplayResult?: PaymentWebhookReplayResult;
  /**
   * Called when the user applies a new filter. The controller will reset the
   * webhook events session and reload from page 1.
   */
  onApplyFilter(filter: PaymentWebhookEventListFilter): Promise<void> | void;
  onLoadMore(): void;
  onReplay(eventId: string): Promise<void> | void;
}

const STATUS_VARIANT: Record<
  PaymentWebhookEventView["status"],
  "secondary" | "success" | "danger" | "warning"
> = {
  queued: "secondary",
  processing: "warning",
  processed: "success",
  failed: "danger",
  dead: "secondary",
};

export function IntegrationLogs(props: IntegrationLogsProps) {
  const m = useDevConfigMessages();
  const [providerCode, setProviderCode] = React.useState<PaymentProviderCode | "">("");
  const statusLabel: Record<PaymentWebhookEventView["status"], string> = {
    queued: m.logs.statusQueued,
    processing: m.logs.statusProcessing,
    processed: m.logs.statusProcessed,
    failed: m.logs.statusFailed,
    dead: m.logs.statusDead,
  };
  const statusOptions: ReadonlyArray<{ label: string; value: PaymentWebhookEventView["status"] | "" }> = [
    { label: m.logs.statusAll, value: "" },
    { label: m.logs.statusQueued, value: "queued" },
    { label: m.logs.statusProcessing, value: "processing" },
    { label: m.logs.statusProcessed, value: "processed" },
    { label: m.logs.statusFailed, value: "failed" },
    { label: m.logs.statusDead, value: "dead" },
  ];
  const [status, setStatus] = React.useState<PaymentWebhookEventView["status"] | "">("");
  const [receivedFrom, setReceivedFrom] = React.useState("");
  const [receivedTo, setReceivedTo] = React.useState("");

  async function handleApplyFilter() {
    const next: PaymentWebhookEventListFilter = {
      ...(providerCode ? { providerCode: providerCode as PaymentProviderCode } : {}),
      ...(status ? { status: status as PaymentWebhookEventView["status"] } : {}),
      ...(receivedFrom ? { receivedFrom: toIsoStartOfDay(receivedFrom) } : {}),
      ...(receivedTo ? { receivedTo: toIsoEndOfDay(receivedTo) } : {}),
    };
    await props.onApplyFilter(next);
  }

  function handleResetFilter() {
    setProviderCode("");
    setStatus("");
    setReceivedFrom("");
    setReceivedTo("");
    void props.onApplyFilter({});
  }

  return (
    <div className="space-y-4" data-slot="integration-logs">
      <section className="rounded-md border border-[var(--sdk-color-border-subtle)] p-4">
        <header className="mb-3">
          <div className="text-xs font-semibold uppercase tracking-wider text-[var(--sdk-color-text-muted)]">
            {m.logs.filterHeader}
          </div>
          <p className="mt-1 text-xs text-[var(--sdk-color-text-secondary)]">
            {m.logs.filterIntro}
          </p>
        </header>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
          <AdminFieldLabel label={m.common.provider} htmlFor="integration-logs-filter-provider">
            <Select
              value={providerCode}
              onValueChange={(value) =>
                setProviderCode(value as PaymentProviderCode | "")
              }
              disabled={props.busy}
            >
              <SelectTrigger id="integration-logs-filter-provider">
                <SelectValue placeholder={m.logs.providerPlaceholder} />
              </SelectTrigger>
              <SelectContent>
                {ADMIN_PROVIDER_FILTER_OPTIONS.map((option) => (
                  <SelectItem key={option.value || "all"} value={option.value || "all"}>
                    {option.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </AdminFieldLabel>
          <AdminFieldLabel label={m.common.status} htmlFor="integration-logs-filter-status">
            <Select
              value={status}
              onValueChange={(value) =>
                setStatus(value as PaymentWebhookEventView["status"] | "")
              }
              disabled={props.busy}
            >
              <SelectTrigger id="integration-logs-filter-status">
                <SelectValue placeholder={m.logs.statusAll} />
              </SelectTrigger>
              <SelectContent>
                {statusOptions.map((option) => (
                  <SelectItem key={option.value || "all"} value={option.value || "all"}>
                    {option.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </AdminFieldLabel>
          <AdminFieldLabel label={m.logs.receivedFrom} htmlFor="integration-logs-filter-from">
            <Input
              id="integration-logs-filter-from"
              type="date"
              value={receivedFrom}
              onChange={(event) => setReceivedFrom(event.target.value)}
              disabled={props.busy}
            />
          </AdminFieldLabel>
          <AdminFieldLabel label={m.logs.receivedTo} htmlFor="integration-logs-filter-to">
            <Input
              id="integration-logs-filter-to"
              type="date"
              value={receivedTo}
              onChange={(event) => setReceivedTo(event.target.value)}
              disabled={props.busy}
            />
          </AdminFieldLabel>
        </div>
        <div className="mt-3 flex justify-end gap-2">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={handleResetFilter}
            disabled={props.busy}
          >
            {m.logs.reset}
          </Button>
          <Button
            type="button"
            size="sm"
            onClick={() => void handleApplyFilter()}
            disabled={props.busy}
          >
            {m.logs.applyFilter}
          </Button>
        </div>
      </section>

      {props.lastReplayResult ? (
        <div
          role="status"
          className={
            "rounded-md border p-3 text-sm " +
            (props.lastReplayResult.ok
              ? "border-[var(--sdk-color-border-success)] bg-[var(--sdk-color-bg-success-subtle)] text-[var(--sdk-color-text-success)]"
              : "border-[var(--sdk-color-border-error)] bg-[var(--sdk-color-bg-error-subtle)] text-[var(--sdk-color-text-error)]")
          }
        >
          <div className="font-medium">
            {props.lastReplayResult.ok ? m.logs.replaySuccess : m.logs.replayFailed}
          </div>
          <div className="mt-1 text-xs">
            {m.logs.eventIdLabel} {props.lastReplayResult.eventId}
            {` · ${m.logs.replayedAtLabel} ${formatAdminTimestamp(props.lastReplayResult.replayedAt)}`}
            {props.lastReplayResult.diagnostic
              ? ` · ${props.lastReplayResult.diagnostic}`
              : ""}
          </div>
        </div>
      ) : null}

      {props.events.length === 0 ? (
        <div className="rounded-md border border-dashed border-[var(--sdk-color-border-subtle)] p-8 text-center text-sm text-[var(--sdk-color-text-secondary)]">
          {m.logs.emptyState}
          <div className="mt-3">
            <Button type="button" variant="ghost" size="sm" onClick={handleResetFilter} disabled={props.busy}>
              {m.logs.clearFilters}
            </Button>
          </div>
        </div>
      ) : (
        <ul className="divide-y divide-[var(--sdk-color-border-subtle)] rounded-md border border-[var(--sdk-color-border-subtle)]">
          {props.events.map((event) => {
            const replayExhausted = event.retries >= ADMIN_WEBHOOK_REPLAY_MAX_RETRIES || event.status === "dead";
            return (
              <li
                key={event.id}
                className="space-y-2 p-4"
                data-slot="integration-logs-row"
              >
                <div className="flex flex-wrap items-center gap-2">
                  <Badge variant="outline">{ADMIN_PROVIDER_LABEL[event.providerCode]}</Badge>
                  <span className="font-mono text-sm text-[var(--sdk-color-text)]">
                    {event.eventType}
                  </span>
                  <Badge variant={STATUS_VARIANT[event.status]}>{statusLabel[event.status]}</Badge>
                  {event.retries > 0 ? (
                    <Badge variant="secondary">
                      {m.logs.retriesBadge.replace("{retries}", String(event.retries))}
                      {replayExhausted ? m.logs.retriesMaxSuffix : ""}
                    </Badge>
                  ) : null}
                </div>
                <dl className="grid grid-cols-1 gap-x-6 gap-y-1 text-xs text-[var(--sdk-color-text-secondary)] sm:grid-cols-3">
                  <div>
                    <dt className="inline">{m.logs.eventIdLabel}</dt>{" "}
                    <dd className="inline font-mono">
                      {event.eventId ?? event.id}
                    </dd>
                  </div>
                  <div>
                    <dt className="inline">{m.logs.receivedLabel}</dt>{" "}
                    <dd className="inline">{formatAdminTimestamp(event.receivedAt)}</dd>
                  </div>
                  <div>
                    <dt className="inline">{m.logs.processedLabel}</dt>{" "}
                    <dd className="inline">
                      {event.processedAt ? formatAdminTimestamp(event.processedAt) : "—"}
                    </dd>
                  </div>
                  {event.lastError ? (
                    <div className="sm:col-span-2">
                      <dt className="inline text-[var(--sdk-color-text-error)]">
                        {m.logs.lastErrorLabel}
                      </dt>{" "}
                      <dd className="inline font-mono text-[var(--sdk-color-text-error)]">
                        {event.lastError}
                      </dd>
                    </div>
                  ) : null}
                </dl>
                <div className="flex justify-end">
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    onClick={() => props.onReplay(event.id)}
                    disabled={props.busy || replayExhausted}
                    title={
                      replayExhausted
                        ? event.status === "dead"
                          ? m.logs.replayDeadTitle
                          : m.logs.replayCapTitle.replace("{max}", String(ADMIN_WEBHOOK_REPLAY_MAX_RETRIES))
                        : m.logs.replayTitle
                    }
                  >
                    {m.logs.replayButton}
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
    </div>
  );
}

function toIsoStartOfDay(value: string): string {
  const parsed = new Date(`${value}T00:00:00`);
  return Number.isNaN(parsed.getTime()) ? value : parsed.toISOString();
}

function toIsoEndOfDay(value: string): string {
  const parsed = new Date(`${value}T23:59:59.999`);
  return Number.isNaN(parsed.getTime()) ? value : parsed.toISOString();
}
