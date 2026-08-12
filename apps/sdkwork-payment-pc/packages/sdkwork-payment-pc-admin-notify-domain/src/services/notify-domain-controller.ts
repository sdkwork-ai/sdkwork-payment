/**
 * Payment notify domain admin controller.
 *
 * Consumes `SdkworkPaymentBackendService` via injected dependency — NEVER the
 * generated SDK directly (APP_SDK_INTEGRATION_SPEC §9). Owns the notify
 * domain list session and CRUD mutations (create/update/delete, set-default
 * via update). Exposes a React-friendly external store contract
 * (subscribe/getState), mirroring the provider-admin controller.
 */

import { extractSdkWorkResourceItem } from "@sdkwork/payment-contracts";
import type { SdkworkPaymentBackendService } from "@sdkwork/payment-service";
import { uuid } from "@sdkwork/utils";

export type NotifyDomainProtocol = "https" | "http";
export type NotifyDomainStatus = "active" | "inactive";

export interface NotifyDomainView {
  id: string;
  organizationId?: string | null;
  protocol: NotifyDomainProtocol;
  hostname: string;
  port?: number | null;
  isDefault: boolean;
  status: NotifyDomainStatus;
  sortOrder: number;
  paymentNotifyUrl: string;
  refundNotifyUrl: string;
}

export interface NotifyDomainDraft {
  protocol: NotifyDomainProtocol;
  hostname: string;
  port?: number | null;
  isDefault: boolean;
  status: NotifyDomainStatus;
  sortOrder: number;
}

export interface NotifyDomainAdminState {
  domains: NotifyDomainView[];
  loading: boolean;
  saving: boolean;
  error: string | null;
  notice: string | null;
}

export interface NotifyDomainAdminController {
  subscribe: (listener: () => void) => () => void;
  getState: () => NotifyDomainAdminState;
  refresh: () => Promise<void>;
  createDomain: (draft: NotifyDomainDraft) => Promise<NotifyDomainView>;
  updateDomain: (id: string, draft: NotifyDomainDraft) => Promise<NotifyDomainView>;
  deleteDomain: (id: string) => Promise<void>;
  setDefault: (id: string) => Promise<void>;
}

export interface CreateNotifyDomainAdminControllerInput {
  service: SdkworkPaymentBackendService;
}

const EMPTY_STATE: NotifyDomainAdminState = {
  domains: [],
  loading: false,
  saving: false,
  error: null,
  notice: null,
};

function asProtocol(value: unknown): NotifyDomainProtocol {
  return value === "http" ? "http" : "https";
}

function asStatus(value: unknown): NotifyDomainStatus {
  return value === "inactive" ? "inactive" : "active";
}

function asInt(value: unknown, fallback: number): number {
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}

function asDomainView(item: unknown): NotifyDomainView {
  const record = (item ?? {}) as Record<string, unknown>;
  return {
    id: String(record.id ?? ""),
    organizationId: record.organizationId == null ? null : String(record.organizationId),
    protocol: asProtocol(record.protocol),
    hostname: String(record.hostname ?? ""),
    port: record.port == null ? null : asInt(record.port, 0),
    isDefault: record.isDefault === true,
    status: asStatus(record.status),
    sortOrder: asInt(record.sortOrder, 0),
    paymentNotifyUrl: String(record.paymentNotifyUrl ?? ""),
    refundNotifyUrl: String(record.refundNotifyUrl ?? ""),
  };
}

export function createNotifyDomainAdminController(
  input: CreateNotifyDomainAdminControllerInput,
): NotifyDomainAdminController {
  const backend = input.service.notifyDomains;
  let state: NotifyDomainAdminState = { ...EMPTY_STATE };
  const listeners = new Set<() => void>();

  function setState(patch: Partial<NotifyDomainAdminState>) {
    state = { ...state, ...patch };
    listeners.forEach((listener) => listener());
  }

  function writeHeaders() {
    return { requestNo: `nd-${uuid()}`, idempotencyKey: uuid() };
  }

  async function updateDomainInternal(id: string, draft: NotifyDomainDraft) {
    setState({ saving: true, error: null });
    try {
      const response = await backend.update(id, draft, writeHeaders());
      const item = extractSdkWorkResourceItem(response);
      const view = asDomainView(item);
      setState({
        saving: false,
        domains: state.domains.map((domain) => (domain.id === id ? view : domain)),
      });
      return view;
    } catch (error) {
      setState({ saving: false, error: String(error) });
      throw error;
    }
  }

  return {
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    getState: () => state,
    async refresh() {
      setState({ loading: true, error: null });
      try {
        const page = await backend.list({});
        const items = (page as { data?: { items?: unknown[] } }).data?.items ?? [];
        setState({ domains: items.map(asDomainView), loading: false });
      } catch (error) {
        setState({ loading: false, error: String(error) });
      }
    },
    async createDomain(draft) {
      setState({ saving: true, error: null });
      try {
        const response = await backend.create(draft, writeHeaders());
        const item = extractSdkWorkResourceItem(response);
        const view = asDomainView(item);
        setState({ saving: false, domains: [...state.domains, view] });
        return view;
      } catch (error) {
        setState({ saving: false, error: String(error) });
        throw error;
      }
    },
    async updateDomain(id, draft) {
      return updateDomainInternal(id, draft);
    },
    async deleteDomain(id) {
      setState({ saving: true, error: null });
      try {
        await backend.delete(id);
        setState({
          saving: false,
          domains: state.domains.filter((domain) => domain.id !== id),
        });
      } catch (error) {
        setState({ saving: false, error: String(error) });
        throw error;
      }
    },
    async setDefault(id) {
      const domain = state.domains.find((candidate) => candidate.id === id);
      if (!domain) {
        throw new Error("notify domain not found");
      }
      await updateDomainInternal(id, {
        protocol: domain.protocol,
        hostname: domain.hostname,
        port: domain.port,
        isDefault: true,
        status: domain.status,
        sortOrder: domain.sortOrder,
      });
    },
  };
}
