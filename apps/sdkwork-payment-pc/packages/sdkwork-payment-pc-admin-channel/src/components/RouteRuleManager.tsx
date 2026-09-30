/**
 * Route rule manager.
 *
 * Full CRUD for routing rules. A route rule is the "traffic controller" of
 * payments — given a set of match conditions (purchase type, country, currency,
 * client platform, amount range, user segment, risk level), it routes the
 * payment to a specific channel (which in turn determines the provider account).
 *
 * API matrix: list + create + update + delete (no retrieve — detail loaded
 * from list items cache).
 *
 * Match conditions are flat fields on the rule (not a separate schema per
 * OpenAPI). The form groups them visually (Conditions / Action / Time window)
 * but the wire payload is a single object.
 *
 * Mirrors industry PSP routing rule surfaces (Stripe Dashboard → Routing rules,
 * Adyen → Payment routing, WeChat Pay → payment scene routing).
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
} from "@sdkwork/ui-pc-react";
import {
  AdminFieldLabel,
  BaseDataSelectField,
  ConfirmDialog,
  PaymentProviderIcon,
  PaymentSceneIcon,
  SdkworkPaymentListPaginationControls,
  type PaymentBaseDataOption,
} from "@sdkwork/payment-pc-admin-core";
import type { SdkWorkPageInfo } from "@sdkwork/payment-contracts";
import { useChannelAdminMessages } from "../i18n";
import type {
  PaymentChannelView,
  PaymentEntityStatus,
  PaymentRouteRuleDraft,
  PaymentRouteRuleUpdateDraft,
  PaymentRouteRuleView,
} from "../types/channel-admin-types";

export interface RouteRuleManagerProps {
  routeRules: readonly PaymentRouteRuleView[];
  channels: readonly PaymentChannelView[];
  pageInfo?: SdkWorkPageInfo;
  busy?: boolean;
  canCreate: boolean;
  canDelete: boolean;
  canUpdate: boolean;
  /** Base-data options resolved by the host app; free-text fallback when empty. */
  countryOptions?: readonly PaymentBaseDataOption[];
  currencyOptions?: readonly PaymentBaseDataOption[];
  onCreate(draft: PaymentRouteRuleDraft): Promise<void> | void;
  onUpdate(id: string, draft: PaymentRouteRuleUpdateDraft): Promise<void> | void;
  onDelete(id: string): Promise<void> | void;
  onLoadMore(): void;
}

const STATUS_VARIANT: Record<PaymentEntityStatus, "success" | "secondary" | "danger"> = {
  active: "success",
  inactive: "secondary",
  deprecated: "danger",
};

export function RouteRuleManager(props: RouteRuleManagerProps) {
  const m = useChannelAdminMessages();
  const [dialog, setDialog] = React.useState<
    | { kind: "closed" }
    | { kind: "create" }
    | { kind: "edit"; rule: PaymentRouteRuleView }
  >({ kind: "closed" });
  const [pendingDelete, setPendingDelete] = React.useState<PaymentRouteRuleView | null>(null);
  const [error, setError] = React.useState<string | undefined>();

  const statusLabel: Record<PaymentEntityStatus, string> = {
    active: m.common.statusActive,
    inactive: m.common.statusInactive,
    deprecated: m.common.statusDeprecated,
  };

  async function handleCreate(draft: PaymentRouteRuleDraft) {
    await props.onCreate(draft);
    setDialog({ kind: "closed" });
  }

  async function handleUpdate(draft: PaymentRouteRuleUpdateDraft) {
    if (dialog.kind !== "edit") {
      return;
    }
    await props.onUpdate(dialog.rule.id, draft);
    setDialog({ kind: "closed" });
  }

  async function handleConfirmDelete() {
    if (!pendingDelete) return;
    setError(undefined);
    try {
      await props.onDelete(pendingDelete.id);
    } catch (err) {
      setError(err instanceof Error ? err.message : m.routeRule.errorDeleteFailed);
    }
    setPendingDelete(null);
  }

  return (
    <div className="space-y-4" data-slot="route-rule-manager">
      <div className="flex justify-end">
        {props.canCreate ? <Button
          type="button"
          size="sm"
          onClick={() => setDialog({ kind: "create" })}
          disabled={props.busy || props.channels.length === 0}
          title={
            props.channels.length === 0
              ? m.routeRule.createDisabledTitle
              : m.routeRule.createTitle
          }
        >
          {m.routeRule.createButton}
        </Button> : null}
      </div>

      {props.routeRules.length === 0 ? (
        <div className="rounded-md border border-dashed border-[var(--sdk-color-border-subtle)] p-8 text-center text-sm text-[var(--sdk-color-text-secondary)]">
          {m.routeRule.emptyState}
          {/* Empty-state inline create button: disabled when no channels exist, mirroring the header button logic */}
          {props.canCreate ? <div className="mt-3">
            <Button
              type="button"
              variant="primary"
              size="sm"
              onClick={() => setDialog({ kind: "create" })}
              disabled={props.busy || props.channels.length === 0}
            >
              {m.routeRule.createButton}
            </Button>
          </div> : null}
        </div>
      ) : (
        <ul className="divide-y divide-[var(--sdk-color-border-subtle)] rounded-md border border-[var(--sdk-color-border-subtle)]">
          {/* Sort ascending by priority: lower number means higher priority; spread into a new array to avoid mutating props */}
          {[...props.routeRules]
            .sort((a, b) => a.priority - b.priority)
            .map((rule) => {
            const channel = props.channels.find((item) => item.id === rule.channelId);
            return (
              <li key={rule.id} className="grid gap-3 p-4 lg:grid-cols-[auto_minmax(0,1fr)_auto] lg:items-start">
                <div className="flex items-center gap-2">
                  <span className="inline-flex h-10 min-w-10 items-center justify-center rounded-md border border-[var(--sdk-color-border-default)] bg-[var(--sdk-color-surface-panel-muted)] px-2 text-sm font-bold tabular-nums text-[var(--sdk-color-text-primary)]" title={m.routeRule.priorityHint}>
                    {rule.priority}
                  </span>
                  {channel ? (
                    <PaymentProviderIcon providerCode={channel.providerCode ?? "unknown"} size="md" />
                  ) : null}
                </div>
                <div className="min-w-0 space-y-2">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="font-semibold text-[var(--sdk-color-text-primary)]">{rule.ruleNo}</span>
                    <Badge variant={STATUS_VARIANT[rule.status]}>{statusLabel[rule.status]}</Badge>
                    {channel ? (
                      <span className="inline-flex items-center gap-1.5 text-xs text-[var(--sdk-color-text-secondary)]">
                        <PaymentSceneIcon sceneCode={channel.sceneCode} size="xs" />
                        {channel.channelName ?? channel.channelNo}
                      </span>
                    ) : (
                      <span className="text-xs text-[var(--sdk-color-text-muted)]">{m.routeRule.missingChannel.replace("{channelId}", rule.channelId)}</span>
                    )}
                  </div>
                <dl className="grid grid-cols-1 gap-x-6 gap-y-1 text-xs text-[var(--sdk-color-text-secondary)] sm:grid-cols-3">
                  {rule.purchaseType ? (
                    <div>
                      <dt className="inline">{m.routeRule.purchaseTypeLabel}</dt>{" "}
                      <dd className="inline">{rule.purchaseType}</dd>
                    </div>
                  ) : null}
                  {rule.countryCode ? (
                    <div>
                      <dt className="inline">{m.routeRule.countryLabel}</dt>{" "}
                      <dd className="inline">{rule.countryCode}</dd>
                    </div>
                  ) : null}
                  {rule.currencyCode ? (
                    <div>
                      <dt className="inline">{m.routeRule.currencyLabel}</dt>{" "}
                      <dd className="inline">{rule.currencyCode}</dd>
                    </div>
                  ) : null}
                  {rule.clientPlatform ? (
                    <div>
                      <dt className="inline">{m.routeRule.clientPlatformLabel}</dt>{" "}
                      <dd className="inline">{rule.clientPlatform}</dd>
                    </div>
                  ) : null}
                  {rule.amountMin || rule.amountMax ? (
                    <div>
                      <dt className="inline">{m.routeRule.amountLabel}</dt>{" "}
                      <dd className="inline">
                        {rule.amountMin ?? "*"} ~ {rule.amountMax ?? "*"}
                      </dd>
                    </div>
                  ) : null}
                  {rule.userSegment ? (
                    <div>
                      <dt className="inline">{m.routeRule.userSegmentLabel}</dt>{" "}
                      <dd className="inline">{rule.userSegment}</dd>
                    </div>
                  ) : null}
                  {rule.riskLevel ? (
                    <div>
                      <dt className="inline">{m.routeRule.riskLevelLabel}</dt>{" "}
                      <dd className="inline">{rule.riskLevel}</dd>
                    </div>
                  ) : null}
                </dl>
                {rule.startsAt || rule.endsAt ? (
                  <div className="text-xs text-[var(--sdk-color-text-muted)]">
                    {m.routeRule.validWindow
                      .replace("{starts}", rule.startsAt ?? m.routeRule.validWindowNow)
                      .replace("{ends}", rule.endsAt ?? m.routeRule.validWindowForever)}
                  </div>
                ) : null}
                </div>
                <div className="flex justify-end gap-2 lg:self-center">
                  {props.canUpdate ? <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    onClick={() => setDialog({ kind: "edit", rule })}
                    disabled={props.busy}
                    title={m.routeRule.editBusyTitle}
                  >
                    {m.common.edit}
                  </Button> : null}
                  {props.canDelete ? <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    onClick={() => setPendingDelete(rule)}
                    disabled={props.busy}
                    title={m.routeRule.deleteBusyTitle}
                  >
                    {m.common.delete}
                  </Button> : null}
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

      <Dialog
        open={
          (props.canCreate && dialog.kind === "create")
          || (props.canUpdate && dialog.kind === "edit")
        }
        onOpenChange={(open) => {
          if (!open) setDialog({ kind: "closed" });
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {dialog.kind === "create" ? m.routeRule.createDialogTitle : m.routeRule.editDialogTitle}
            </DialogTitle>
          </DialogHeader>
          {dialog.kind === "create" || dialog.kind === "edit" ? (
            <RouteRuleForm
              mode={dialog.kind === "create" ? "create" : "update"}
              initial={dialog.kind === "edit" ? dialog.rule : undefined}
              channels={props.channels}
              countryOptions={props.countryOptions}
              currencyOptions={props.currencyOptions}
              onCancel={() => setDialog({ kind: "closed" })}
              onSubmit={
                dialog.kind === "create"
                  ? (draft) => handleCreate(draft as PaymentRouteRuleDraft)
                  : (draft) => handleUpdate(draft as PaymentRouteRuleUpdateDraft)
              }
            />
          ) : null}
        </DialogContent>
      </Dialog>

      {error ? (
        <div
          role="alert"
          className="rounded-md border border-[var(--sdk-color-border-error)] bg-[var(--sdk-color-bg-error-subtle)] p-3 text-sm text-[var(--sdk-color-text-error)]"
        >
          {error}
        </div>
      ) : null}

      <ConfirmDialog
        open={props.canDelete && pendingDelete !== null}
        title={m.routeRule.confirmDeleteTitle}
        description={
          pendingDelete
            ? m.routeRule.confirmDeleteDescription.replace("{ruleNo}", pendingDelete.ruleNo)
            : ""
        }
        confirmLabel={m.routeRule.confirmDeleteLabel}
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

interface RouteRuleFormProps {
  mode: "create" | "update";
  initial?: PaymentRouteRuleView;
  channels: readonly PaymentChannelView[];
  countryOptions?: readonly PaymentBaseDataOption[];
  currencyOptions?: readonly PaymentBaseDataOption[];
  onCancel(): void;
  onSubmit(draft: PaymentRouteRuleDraft | PaymentRouteRuleUpdateDraft): Promise<void> | void;
}

function RouteRuleForm(props: RouteRuleFormProps) {
  const m = useChannelAdminMessages();
  const { mode, initial } = props;
  const [ruleNo, setRuleNo] = React.useState(initial?.ruleNo ?? "");
  const [priority, setPriority] = React.useState(String(initial?.priority ?? 0));
  const [purchaseType, setPurchaseType] = React.useState(initial?.purchaseType ?? "");
  const [countryCode, setCountryCode] = React.useState(initial?.countryCode ?? "");
  const [currencyCode, setCurrencyCode] = React.useState(initial?.currencyCode ?? "");
  const [clientPlatform, setClientPlatform] = React.useState(initial?.clientPlatform ?? "");
  const [amountMin, setAmountMin] = React.useState(initial?.amountMin ?? "");
  const [amountMax, setAmountMax] = React.useState(initial?.amountMax ?? "");
  const [userSegment, setUserSegment] = React.useState(initial?.userSegment ?? "");
  const [riskLevel, setRiskLevel] = React.useState(initial?.riskLevel ?? "");
  const [channelId, setChannelId] = React.useState(initial?.channelId ?? "");
  const [status, setStatus] = React.useState<PaymentEntityStatus>(initial?.status ?? "active");
  const [startsAt, setStartsAt] = React.useState(initial?.startsAt?.slice(0, 16) ?? "");
  const [endsAt, setEndsAt] = React.useState(initial?.endsAt?.slice(0, 16) ?? "");
  const [error, setError] = React.useState<string | undefined>();

  const statusOptions: ReadonlyArray<{ label: string; value: PaymentEntityStatus }> = [
    { label: m.common.statusActive, value: "active" },
    { label: m.common.statusInactive, value: "inactive" },
    { label: m.common.statusDeprecated, value: "deprecated" },
  ];

  React.useEffect(() => {
    if (!channelId && props.channels.length > 0) {
      setChannelId(props.channels[0].id);
    }
  }, [props.channels, channelId]);

  async function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError(undefined);
    if (mode === "create" && !ruleNo.trim()) {
      setError(m.routeRule.errorRuleNumberRequired);
      return;
    }
    if (!channelId) {
      setError(m.routeRule.errorChannelRequired);
      return;
    }
    const priorityNum = Number.parseInt(priority, 10);
    if (Number.isNaN(priorityNum)) {
      setError(m.routeRule.errorPriorityInteger);
      return;
    }
    if ((amountMin && !isAmountValid(amountMin)) || (amountMax && !isAmountValid(amountMax))) {
      setError(m.routeRule.errorAmountPattern);
      return;
    }
    const startsAtIso = startsAt ? new Date(startsAt).toISOString() : undefined;
    const endsAtIso = endsAt ? new Date(endsAt).toISOString() : undefined;
    try {
      if (mode === "create") {
        await props.onSubmit({
          ruleNo: ruleNo.trim(),
          priority: priorityNum,
          purchaseType: purchaseType.trim() || undefined,
          countryCode: countryCode.trim() || undefined,
          currencyCode: currencyCode.trim() || undefined,
          clientPlatform: clientPlatform.trim() || undefined,
          amountMin: amountMin.trim() || undefined,
          amountMax: amountMax.trim() || undefined,
          userSegment: userSegment.trim() || undefined,
          riskLevel: riskLevel.trim() || undefined,
          channelId,
          status,
          startsAt: startsAtIso,
          endsAt: endsAtIso,
        } as PaymentRouteRuleDraft);
      } else {
        await props.onSubmit({
          priority: priorityNum,
          purchaseType: purchaseType.trim() || undefined,
          countryCode: countryCode.trim() || undefined,
          currencyCode: currencyCode.trim() || undefined,
          clientPlatform: clientPlatform.trim() || undefined,
          amountMin: amountMin.trim() || undefined,
          amountMax: amountMax.trim() || undefined,
          userSegment: userSegment.trim() || undefined,
          riskLevel: riskLevel.trim() || undefined,
          channelId,
          status,
          startsAt: startsAtIso,
          endsAt: endsAtIso,
        } as PaymentRouteRuleUpdateDraft);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : m.routeRule.errorSaveFailed);
    }
  }

  return (
    <form className="space-y-4" onSubmit={handleSubmit}>
      {mode === "create" ? (
        <AdminFieldLabel label={m.routeRule.ruleNumber} htmlFor="rule-form-no" required>
          <Input
            id="rule-form-no"
            value={ruleNo}
            onChange={(event) => setRuleNo(event.target.value)}
            placeholder={m.routeRule.ruleNumberPlaceholder}
            required
          />
        </AdminFieldLabel>
      ) : (
        <p className="text-xs text-[var(--sdk-color-text-muted)]">
          {m.routeRule.ruleNumberImmutable}
        </p>
      )}

      <AdminFieldLabel label={m.routeRule.targetChannel} htmlFor="rule-form-channel" required>
        <Select value={channelId} onValueChange={setChannelId}>
          <SelectTrigger id="rule-form-channel">
            <SelectValue placeholder={m.routeRule.channelPlaceholder} />
          </SelectTrigger>
          <SelectContent>
            {props.channels.map((channel) => (
              <SelectItem key={channel.id} value={channel.id}>
                {channel.channelNo} ({channel.sceneCode} · {channel.currencyCode})
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </AdminFieldLabel>

      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        <AdminFieldLabel label={m.common.priority} htmlFor="rule-form-priority">
          <Input
            id="rule-form-priority"
            type="number"
            value={priority}
            onChange={(event) => setPriority(event.target.value)}
            placeholder={m.routeRule.priorityPlaceholder}
          />
        </AdminFieldLabel>
        <AdminFieldLabel label={m.common.status} htmlFor="rule-form-status">
          <Select
            value={status}
            onValueChange={(value) => setStatus(value as PaymentEntityStatus)}
          >
            <SelectTrigger id="rule-form-status">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {statusOptions.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  {option.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </AdminFieldLabel>
      </div>

      <fieldset className="space-y-3 rounded-md border border-[var(--sdk-color-border-subtle)] p-3">
        <legend className="px-1 text-xs font-semibold uppercase tracking-wider text-[var(--sdk-color-text-muted)]">
          {m.routeRule.matchConditionsLegend}
        </legend>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <AdminFieldLabel label={m.routeRule.purchaseType} htmlFor="rule-form-purchase-type">
            <Input
              id="rule-form-purchase-type"
              value={purchaseType}
              onChange={(event) => setPurchaseType(event.target.value)}
              placeholder={m.routeRule.purchaseTypePlaceholder}
            />
          </AdminFieldLabel>
          <AdminFieldLabel label={m.routeRule.clientPlatform} htmlFor="rule-form-platform">
            <Input
              id="rule-form-platform"
              value={clientPlatform}
              onChange={(event) => setClientPlatform(event.target.value)}
              placeholder={m.routeRule.clientPlatformPlaceholder}
            />
          </AdminFieldLabel>
          <BaseDataSelectField
            id="rule-form-country"
            label={m.common.country}
            options={props.countryOptions}
            value={countryCode}
            maxLength={2}
            placeholder="CN"
            onChange={setCountryCode}
          />
          <BaseDataSelectField
            id="rule-form-currency"
            label={m.common.currency}
            options={props.currencyOptions}
            value={currencyCode}
            maxLength={3}
            placeholder="CNY"
            onChange={setCurrencyCode}
          />
          <AdminFieldLabel label={m.routeRule.amountMin} htmlFor="rule-form-amount-min">
            <Input
              id="rule-form-amount-min"
              value={amountMin}
              onChange={(event) => setAmountMin(event.target.value)}
              placeholder={m.routeRule.amountMinPlaceholder}
            />
          </AdminFieldLabel>
          <AdminFieldLabel label={m.routeRule.amountMax} htmlFor="rule-form-amount-max">
            <Input
              id="rule-form-amount-max"
              value={amountMax}
              onChange={(event) => setAmountMax(event.target.value)}
              placeholder={m.routeRule.amountMaxPlaceholder}
            />
          </AdminFieldLabel>
          <AdminFieldLabel label={m.routeRule.userSegment} htmlFor="rule-form-segment">
            <Input
              id="rule-form-segment"
              value={userSegment}
              onChange={(event) => setUserSegment(event.target.value)}
              placeholder={m.routeRule.userSegmentPlaceholder}
            />
          </AdminFieldLabel>
          <AdminFieldLabel label={m.routeRule.riskLevel} htmlFor="rule-form-risk">
            <Input
              id="rule-form-risk"
              value={riskLevel}
              onChange={(event) => setRiskLevel(event.target.value)}
              placeholder={m.routeRule.riskLevelPlaceholder}
            />
          </AdminFieldLabel>
        </div>
      </fieldset>

      <fieldset className="space-y-3 rounded-md border border-[var(--sdk-color-border-subtle)] p-3">
        <legend className="px-1 text-xs font-semibold uppercase tracking-wider text-[var(--sdk-color-text-muted)]">
          {m.routeRule.validityWindowLegend}
        </legend>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <AdminFieldLabel label={m.routeRule.startsAt} htmlFor="rule-form-starts">
            <Input
              id="rule-form-starts"
              type="datetime-local"
              value={startsAt}
              onChange={(event) => setStartsAt(event.target.value)}
            />
          </AdminFieldLabel>
          <AdminFieldLabel label={m.routeRule.endsAt} htmlFor="rule-form-ends">
            <Input
              id="rule-form-ends"
              type="datetime-local"
              value={endsAt}
              onChange={(event) => setEndsAt(event.target.value)}
            />
          </AdminFieldLabel>
        </div>
      </fieldset>

      {error ? (
        <div
          role="alert"
          className="rounded-md border border-[var(--sdk-color-border-error)] bg-[var(--sdk-color-bg-error-subtle)] p-3 text-sm text-[var(--sdk-color-text-error)]"
        >
          {error}
        </div>
      ) : null}
      <div className="flex justify-end gap-2">
        <Button type="button" variant="ghost" onClick={props.onCancel}>
          {m.common.cancel}
        </Button>
        <Button type="submit">
          {mode === "create" ? m.routeRule.createSubmit : m.common.saveChanges}
        </Button>
      </div>
    </form>
  );
}

function isAmountValid(value: string): boolean {
  return /^[0-9]+(\.[0-9]{1,2})?$/.test(value);
}
