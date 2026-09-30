/**
 * Channel manager.
 *
 * Lists payment channels (the bridge between PaymentMethod and ProviderAccount)
 * with create-only capability. Per OpenAPI contract, channels have NO update
 * or delete operation — the UI surfaces this honestly by hiding edit/delete
 * buttons and showing an explanatory note.
 *
 * A channel represents a concrete available payment pathway:
 *   PaymentMethod (what) + ProviderAccount (who) + SceneCode (where) +
 *   Currency + Country = a routable channel.
 *
 * Mirrors industry PSP channel registries (Stripe Dashboard → Payment method
 * availability per merchant, Alipay open platform → app channel binding,
 * WeChat Pay merchant platform → payment scene config).
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
  adminPaymentMethodKeyOption,
  BaseDataSelectField,
  PaymentMethodIcon,
  PaymentProviderIcon,
  PaymentSceneIcon,
  SdkworkPaymentListPaginationControls,
  type PaymentBaseDataOption,
} from "@sdkwork/payment-pc-admin-core";
import type { SdkWorkPageInfo } from "@sdkwork/payment-contracts";
import { useChannelAdminMessages } from "../i18n";
import type {
  PaymentChannelDraft,
  PaymentChannelView,
  PaymentEntityStatus,
  PaymentMethodView,
  PaymentProviderAccountView,
  PaymentProviderCode,
  PaymentSceneCode,
} from "../types/channel-admin-types";

export interface ChannelManagerProps {
  channels: readonly PaymentChannelView[];
  methods: readonly PaymentMethodView[];
  providerAccounts: readonly PaymentProviderAccountView[];
  pageInfo?: SdkWorkPageInfo;
  busy?: boolean;
  canCreate: boolean;
  /** Base-data options resolved by the host app; free-text fallback when empty. */
  countryOptions?: readonly PaymentBaseDataOption[];
  currencyOptions?: readonly PaymentBaseDataOption[];
  onCreate(draft: PaymentChannelDraft): Promise<void> | void;
  onLoadMore(): void;
}

const STATUS_VARIANT: Record<PaymentEntityStatus, "success" | "secondary" | "danger"> = {
  active: "success",
  inactive: "secondary",
  deprecated: "danger",
};

export function ChannelManager(props: ChannelManagerProps) {
  const m = useChannelAdminMessages();
  const [open, setOpen] = React.useState(false);

  const sceneLabel: Record<PaymentSceneCode, string> = {
    app: m.common.sceneApp,
    web: m.common.sceneWeb,
    mini_program: m.common.sceneMiniProgram,
    api: m.common.sceneApi,
  };
  const statusLabel: Record<PaymentEntityStatus, string> = {
    active: m.common.statusActive,
    inactive: m.common.statusInactive,
    deprecated: m.common.statusDeprecated,
  };

  return (
    <div className="space-y-4" data-slot="channel-manager">
      <div className="flex items-center justify-between">
        <p className="text-xs text-[var(--sdk-color-text-muted)]">
          {m.channel.intro}
        </p>
        {props.canCreate ? <Button
          type="button"
          size="sm"
          onClick={() => setOpen(true)}
          disabled={props.busy || props.methods.length === 0 || props.providerAccounts.length === 0}
          title={
            props.methods.length === 0 || props.providerAccounts.length === 0
              ? m.channel.createDisabledTitle
              : m.channel.createTitle
          }
        >
          {m.channel.createButton}
        </Button> : null}
      </div>

      {props.channels.length === 0 ? (
        <div className="rounded-md border border-dashed border-[var(--sdk-color-border-subtle)] p-8 text-center text-sm text-[var(--sdk-color-text-secondary)]">
          {m.channel.emptyState}
          {/* Empty-state inline create button: disabled when a payment method or provider account is missing, mirroring the header button logic */}
          {props.canCreate ? <div className="mt-3">
            <Button
              type="button"
              variant="primary"
              size="sm"
              onClick={() => setOpen(true)}
              disabled={props.busy || props.methods.length === 0 || props.providerAccounts.length === 0}
            >
              {m.channel.createButton}
            </Button>
          </div> : null}
        </div>
      ) : (
        <ul className="divide-y divide-[var(--sdk-color-border-subtle)] rounded-md border border-[var(--sdk-color-border-subtle)]">
          {props.channels.map((channel) => {
            const method = props.methods.find((item) => item.id === channel.methodId);
            const providerAccount = props.providerAccounts.find((item) => item.id === channel.providerAccountId);
            return (
              <li key={channel.id} className="flex flex-col gap-3 p-4 xl:flex-row xl:items-center xl:justify-between">
                <div className="flex min-w-0 flex-1 items-start gap-3">
                  <div className="flex shrink-0 items-center -space-x-1.5" aria-label={m.channel.routeAriaLabel}>
                    <PaymentMethodIcon
                      label={method ? adminPaymentMethodKeyOption(method.methodKey)?.label ?? method.displayName : m.channel.methodFallback}
                      methodKey={method?.methodKey ?? channel.methodId}
                      providerCode={method?.providerCode ?? channel.providerCode}
                      size="md"
                    />
                    <PaymentProviderIcon
                      className="ring-2 ring-[var(--sdk-color-surface-panel)]"
                      label={providerAccount?.providerCode ?? channel.providerCode ?? m.channel.providerFallback}
                      providerCode={providerAccount?.providerCode ?? channel.providerCode ?? "unknown"}
                      size="md"
                    />
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="font-semibold text-[var(--sdk-color-text-primary)]">
                        {channel.channelName ?? channel.channelNo}
                      </span>
                      <Badge variant="outline" className="font-mono">{channel.channelNo}</Badge>
                      <Badge variant={STATUS_VARIANT[channel.status]}>{statusLabel[channel.status]}</Badge>
                      <Badge variant="outline">{m.channel.priorityBadge.replace("{priority}", String(channel.priority))}</Badge>
                    </div>
                    <div className="mt-2 flex flex-wrap items-center gap-2 text-xs text-[var(--sdk-color-text-secondary)]">
                      <span className="font-medium text-[var(--sdk-color-text-primary)]">
                        {method ? method.displayName : channel.methodId}
                      </span>
                      <span aria-hidden="true" className="text-[var(--sdk-color-text-muted)]">→</span>
                      <span className="font-mono">
                        {providerAccount ? providerAccount.accountNo : channel.providerAccountId}
                      </span>
                    </div>
                  </div>
                </div>
                <div className="flex flex-wrap items-center gap-2 xl:justify-end">
                  <span className="inline-flex items-center gap-1.5 text-xs text-[var(--sdk-color-text-secondary)]">
                    <PaymentSceneIcon sceneCode={channel.sceneCode} size="xs" />
                    {sceneLabel[channel.sceneCode]}
                  </span>
                  <Badge variant="outline">{channel.currencyCode} · {channel.countryCode || m.common.global}</Badge>
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
        open={props.canCreate && open}
        onOpenChange={setOpen}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{m.channel.dialogTitle}</DialogTitle>
          </DialogHeader>
          <ChannelForm
            methods={props.methods}
            providerAccounts={props.providerAccounts}
            countryOptions={props.countryOptions}
            currencyOptions={props.currencyOptions}
            onCancel={() => setOpen(false)}
            onSubmit={async (draft) => {
              await props.onCreate(draft);
              setOpen(false);
            }}
          />
        </DialogContent>
      </Dialog>
    </div>
  );
}

interface ChannelFormProps {
  methods: readonly PaymentMethodView[];
  providerAccounts: readonly PaymentProviderAccountView[];
  countryOptions?: readonly PaymentBaseDataOption[];
  currencyOptions?: readonly PaymentBaseDataOption[];
  onCancel(): void;
  onSubmit(draft: PaymentChannelDraft): Promise<void> | void;
}

function ChannelForm(props: ChannelFormProps) {
  const m = useChannelAdminMessages();
  const [channelNo, setChannelNo] = React.useState("");
  const [channelName, setChannelName] = React.useState("");
  const [methodId, setMethodId] = React.useState("");
  const [providerAccountId, setProviderAccountId] = React.useState("");
  const [sceneCode, setSceneCode] = React.useState<PaymentSceneCode>("api");
  const [currencyCode, setCurrencyCode] = React.useState("CNY");
  const [countryCode, setCountryCode] = React.useState("CN");
  const [status, setStatus] = React.useState<PaymentEntityStatus>("active");
  const [priority, setPriority] = React.useState("0");
  const [sortOrder, setSortOrder] = React.useState("0");
  const [error, setError] = React.useState<string | undefined>();

  const sceneOptions: ReadonlyArray<{ label: string; value: PaymentSceneCode }> = [
    { label: m.common.sceneApp, value: "app" },
    { label: m.common.sceneWeb, value: "web" },
    { label: m.common.sceneMiniProgram, value: "mini_program" },
    { label: m.common.sceneApi, value: "api" },
  ];
  const statusOptions: ReadonlyArray<{ label: string; value: PaymentEntityStatus }> = [
    { label: m.common.statusActive, value: "active" },
    { label: m.common.statusInactive, value: "inactive" },
    { label: m.common.statusDeprecated, value: "deprecated" },
  ];

  React.useEffect(() => {
    if (!methodId && props.methods.length > 0) {
      setMethodId(props.methods[0].id);
    }
  }, [props.methods, methodId]);

  React.useEffect(() => {
    if (!providerAccountId && props.providerAccounts.length > 0) {
      setProviderAccountId(props.providerAccounts[0].id);
    }
  }, [props.providerAccounts, providerAccountId]);

  // Auto-fill currencyCode and countryCode from selected method when method changes.
  React.useEffect(() => {
    const method = props.methods.find((item) => item.id === methodId);
    if (method) {
      setCurrencyCode(method.currencyCode);
      if (method.countryCode) {
        setCountryCode(method.countryCode);
      }
    }
  }, [methodId, props.methods]);

  async function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError(undefined);
    if (!channelNo.trim()) {
      setError(m.channel.errorChannelNumberRequired);
      return;
    }
    if (!methodId) {
      setError(m.channel.errorMethodRequired);
      return;
    }
    if (!providerAccountId) {
      setError(m.channel.errorProviderRequired);
      return;
    }
    const priorityNum = Number.parseInt(priority, 10);
    const sortOrderNum = Number.parseInt(sortOrder, 10);
    if (Number.isNaN(priorityNum) || Number.isNaN(sortOrderNum)) {
      setError(m.channel.errorPriorityInteger);
      return;
    }
    const method = props.methods.find((item) => item.id === methodId);
    const providerAccount = props.providerAccounts.find((item) => item.id === providerAccountId);
    const draft: PaymentChannelDraft = {
      channelNo: channelNo.trim(),
      channelName: channelName.trim() || undefined,
      providerAccountId,
      methodId,
      providerCode: (method?.providerCode ?? providerAccount?.providerCode) as PaymentProviderCode | undefined,
      sceneCode,
      currencyCode: currencyCode.trim() || "CNY",
      countryCode: countryCode.trim(),
      status,
      priority: priorityNum,
      sortOrder: sortOrderNum,
    };
    try {
      await props.onSubmit(draft);
    } catch (err) {
      setError(err instanceof Error ? err.message : m.channel.errorCreateFailed);
    }
  }

  return (
    <form className="space-y-3" onSubmit={handleSubmit}>
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        <AdminFieldLabel label={m.channel.channelNumber} htmlFor="channel-form-no" required>
          <Input
            id="channel-form-no"
            value={channelNo}
            onChange={(event) => setChannelNo(event.target.value)}
            placeholder={m.channel.channelNumberPlaceholder}
            required
          />
        </AdminFieldLabel>
        <AdminFieldLabel label={m.channel.channelName} htmlFor="channel-form-name">
          <Input
            id="channel-form-name"
            value={channelName}
            onChange={(event) => setChannelName(event.target.value)}
            placeholder={m.channel.channelNamePlaceholder}
          />
        </AdminFieldLabel>
      </div>
      <AdminFieldLabel label={m.channel.paymentMethod} htmlFor="channel-form-method" required>
        <Select value={methodId} onValueChange={setMethodId}>
          <SelectTrigger id="channel-form-method">
            <SelectValue placeholder={m.channel.methodPlaceholder} />
          </SelectTrigger>
          <SelectContent>
            {props.methods.map((method) => (
              <SelectItem key={method.id} value={method.id}>
                {method.displayName} ({method.methodKey})
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </AdminFieldLabel>
      <AdminFieldLabel label={m.channel.providerAccount} htmlFor="channel-form-provider" required>
        <Select value={providerAccountId} onValueChange={setProviderAccountId}>
          <SelectTrigger id="channel-form-provider">
            <SelectValue placeholder={m.channel.providerPlaceholder} />
          </SelectTrigger>
          <SelectContent>
            {props.providerAccounts.map((account) => (
              <SelectItem key={account.id} value={account.id}>
                {account.accountNo} ({account.providerCode} · {account.environment})
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </AdminFieldLabel>
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        <AdminFieldLabel label={m.channel.scene} htmlFor="channel-form-scene">
          <Select
            value={sceneCode}
            onValueChange={(value) => setSceneCode(value as PaymentSceneCode)}
          >
            <SelectTrigger id="channel-form-scene">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {sceneOptions.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  {option.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </AdminFieldLabel>
        <AdminFieldLabel label={m.common.status} htmlFor="channel-form-status">
          <Select
            value={status}
            onValueChange={(value) => setStatus(value as PaymentEntityStatus)}
          >
            <SelectTrigger id="channel-form-status">
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
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-4">
        <BaseDataSelectField
          id="channel-form-currency"
          label={m.common.currency}
          options={props.currencyOptions}
          value={currencyCode}
          maxLength={3}
          placeholder="CNY"
          onChange={setCurrencyCode}
        />
        <BaseDataSelectField
          id="channel-form-country"
          label={m.common.country}
          options={props.countryOptions}
          value={countryCode}
          maxLength={2}
          placeholder="CN"
          onChange={setCountryCode}
        />
        <AdminFieldLabel label={m.common.priority} htmlFor="channel-form-priority">
          <Input
            id="channel-form-priority"
            type="number"
            value={priority}
            onChange={(event) => setPriority(event.target.value)}
            placeholder="0"
          />
        </AdminFieldLabel>
        <AdminFieldLabel label={m.common.sortOrder} htmlFor="channel-form-sort">
          <Input
            id="channel-form-sort"
            type="number"
            value={sortOrder}
            onChange={(event) => setSortOrder(event.target.value)}
            placeholder="0"
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
        <Button type="button" variant="ghost" onClick={props.onCancel}>
          {m.common.cancel}
        </Button>
        <Button type="submit">{m.channel.createButton}</Button>
      </div>
    </form>
  );
}
