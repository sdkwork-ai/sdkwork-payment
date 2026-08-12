/**
 * Payment notify domain manager: list, create, update, delete, set-default.
 * Displays the full payment/refund notify URL templates per domain and
 * highlights the default domain.
 */

import { useEffect, useMemo, useState } from "react";
import type { SdkworkPaymentBackendService } from "@sdkwork/payment-service";
import {
  createNotifyDomainAdminController,
  type NotifyDomainAdminController,
  type NotifyDomainDraft,
  type NotifyDomainView,
} from "../services/notify-domain-controller";

export interface NotifyDomainManagerProps {
  service: SdkworkPaymentBackendService;
}

interface DraftState {
  id: string | null;
  protocol: "https" | "http";
  hostname: string;
  port: string;
  isDefault: boolean;
  status: "active" | "inactive";
  sortOrder: string;
}

const EMPTY_DRAFT: DraftState = {
  id: null,
  protocol: "https",
  hostname: "",
  port: "",
  isDefault: false,
  status: "active",
  sortOrder: "0",
};

function draftToPayload(draft: DraftState): NotifyDomainDraft {
  return {
    protocol: draft.protocol,
    hostname: draft.hostname.trim(),
    port: draft.port.trim() === "" ? null : Number(draft.port),
    isDefault: draft.isDefault,
    status: draft.status,
    sortOrder: Number(draft.sortOrder || 0),
  };
}

export function NotifyDomainManager({ service }: NotifyDomainManagerProps) {
  const controller = useMemo<NotifyDomainAdminController>(
    () => createNotifyDomainAdminController({ service }),
    [service],
  );
  const [domains, setDomains] = useState<NotifyDomainView[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [draft, setDraft] = useState<DraftState | null>(null);

  useEffect(() => {
    let mounted = true;
    const unsubscribe = controller.subscribe(() => {
      if (!mounted) {
        return;
      }
      const snapshot = controller.getState();
      setDomains(snapshot.domains);
      setLoading(snapshot.loading);
      setError(snapshot.error);
    });
    void controller.refresh();
    return () => {
      mounted = false;
      unsubscribe();
    };
  }, [controller]);

  async function saveDraft() {
    if (!draft) {
      return;
    }
    setError(null);
    setNotice(null);
    try {
      const payload = draftToPayload(draft);
      if (draft.id) {
        await controller.updateDomain(draft.id, payload);
      } else {
        await controller.createDomain(payload);
      }
      setNotice(draft.id ? "Notified domain updated." : "Notified domain created.");
      setDraft(null);
    } catch (cause) {
      setError(String(cause));
    }
  }

  async function setDefault(id: string) {
    setError(null);
    setNotice(null);
    try {
      await controller.setDefault(id);
      setNotice("Default notify domain set.");
    } catch (cause) {
      setError(String(cause));
    }
  }

  async function removeDomain(id: string) {
    setError(null);
    setNotice(null);
    try {
      await controller.deleteDomain(id);
      setNotice("Notify domain deleted.");
    } catch (cause) {
      setError(String(cause));
    }
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center justify-between">
        <h2 className="text-base font-semibold text-slate-900 dark:text-white">
          Payment notify domains
        </h2>
        <button
          className="rounded-md bg-blue-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-blue-700"
          onClick={() => setDraft({ ...EMPTY_DRAFT })}
          type="button"
        >
          + Add domain
        </button>
      </div>

      {error ? <div className="text-sm text-red-600 dark:text-red-400" role="alert">{error}</div> : null}
      {notice ? <div className="text-sm text-emerald-600 dark:text-emerald-400" role="status">{notice}</div> : null}

      {loading ? (
        <div className="text-sm text-slate-500 dark:text-slate-400">Loading domains...</div>
      ) : domains.length === 0 ? (
        <div className="rounded-md border border-dashed border-slate-300 p-6 text-sm text-slate-500 dark:border-white/15 dark:text-slate-400">
          No notify domains configured. Add one to build the PSP callback URLs.
        </div>
      ) : (
        <table className="w-full text-left text-sm">
          <thead>
            <tr className="border-b border-slate-200 text-xs uppercase text-slate-500 dark:border-white/10 dark:text-slate-400">
              <th className="py-2 pr-2">Domain</th>
              <th className="py-2 pr-2">Default</th>
              <th className="py-2 pr-2">Status</th>
              <th className="py-2 pr-2">Payment notify URL</th>
              <th className="py-2 pr-2">Refund notify URL</th>
              <th className="py-2">Actions</th>
            </tr>
          </thead>
          <tbody>
            {domains.map((domain) => (
              <tr key={domain.id} className="border-b border-slate-100 dark:border-white/5">
                <td className="py-2 pr-2 font-medium text-slate-800 dark:text-slate-100">
                  {domain.protocol}://{domain.hostname}
                  {domain.port ? `:${domain.port}` : ""}
                </td>
                <td className="py-2 pr-2">
                  {domain.isDefault ? (
                    <span className="rounded bg-emerald-100 px-1.5 py-0.5 text-xs font-medium text-emerald-700 dark:bg-emerald-900/40 dark:text-emerald-300">
                      default
                    </span>
                  ) : (
                    <button className="text-xs text-blue-600 hover:underline dark:text-blue-400" onClick={() => void setDefault(domain.id)} type="button">
                      set default
                    </button>
                  )}
                </td>
                <td className="py-2 pr-2 capitalize text-slate-600 dark:text-slate-300">{domain.status}</td>
                <td className="max-w-[16rem] truncate py-2 pr-2 font-mono text-xs text-slate-600 dark:text-slate-300" title={domain.paymentNotifyUrl}>
                  {domain.paymentNotifyUrl}
                </td>
                <td className="max-w-[16rem] truncate py-2 pr-2 font-mono text-xs text-slate-600 dark:text-slate-300" title={domain.refundNotifyUrl}>
                  {domain.refundNotifyUrl}
                </td>
                <td className="py-2">
                  <button
                    className="mr-2 text-xs text-blue-600 hover:underline dark:text-blue-400"
                    onClick={() =>
                      setDraft({
                        id: domain.id,
                        protocol: domain.protocol,
                        hostname: domain.hostname,
                        port: domain.port == null ? "" : String(domain.port),
                        isDefault: domain.isDefault,
                        status: domain.status,
                        sortOrder: String(domain.sortOrder),
                      })
                    }
                    type="button"
                  >
                    edit
                  </button>
                  <button className="text-xs text-red-600 hover:underline dark:text-red-400" onClick={() => void removeDomain(domain.id)} type="button">
                    delete
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      {draft ? (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/55 p-4" role="presentation">
          <div className="w-full max-w-md rounded-lg border border-slate-200 bg-white p-5 shadow-2xl dark:border-white/10 dark:bg-[#181818]" role="dialog" aria-modal="true">
            <h3 className="mb-4 text-sm font-semibold text-slate-900 dark:text-white">
              {draft.id ? "Edit notify domain" : "Add notify domain"}
            </h3>
            <div className="grid gap-3 text-sm">
              <label className="grid gap-1">
                <span className="text-xs text-slate-500 dark:text-slate-400">Protocol</span>
                <select
                  className="rounded-md border border-slate-200 bg-white px-2 py-1.5 dark:border-white/10 dark:bg-[#202020]"
                  value={draft.protocol}
                  onChange={(event) => setDraft({ ...draft, protocol: event.target.value as "https" | "http" })}
                >
                  <option value="https">https</option>
                  <option value="http">http</option>
                </select>
              </label>
              <label className="grid gap-1">
                <span className="text-xs text-slate-500 dark:text-slate-400">Hostname</span>
                <input
                  className="rounded-md border border-slate-200 px-2 py-1.5 dark:border-white/10 dark:bg-[#202020]"
                  placeholder="pay.example.com"
                  value={draft.hostname}
                  onChange={(event) => setDraft({ ...draft, hostname: event.target.value })}
                />
              </label>
              <label className="grid gap-1">
                <span className="text-xs text-slate-500 dark:text-slate-400">Port (optional)</span>
                <input
                  className="rounded-md border border-slate-200 px-2 py-1.5 dark:border-white/10 dark:bg-[#202020]"
                  placeholder="443"
                  value={draft.port}
                  onChange={(event) => setDraft({ ...draft, port: event.target.value })}
                />
              </label>
              <label className="flex items-center gap-2">
                <input
                  className="h-4 w-4"
                  type="checkbox"
                  checked={draft.isDefault}
                  onChange={(event) => setDraft({ ...draft, isDefault: event.target.checked })}
                />
                <span className="text-xs text-slate-600 dark:text-slate-300">Set as default domain</span>
              </label>
              <label className="grid gap-1">
                <span className="text-xs text-slate-500 dark:text-slate-400">Status</span>
                <select
                  className="rounded-md border border-slate-200 bg-white px-2 py-1.5 dark:border-white/10 dark:bg-[#202020]"
                  value={draft.status}
                  onChange={(event) => setDraft({ ...draft, status: event.target.value as "active" | "inactive" })}
                >
                  <option value="active">active</option>
                  <option value="inactive">inactive</option>
                </select>
              </label>
            </div>
            <div className="mt-5 flex justify-end gap-2">
              <button
                className="rounded-md border border-slate-200 px-3 py-1.5 text-sm text-slate-700 hover:bg-slate-50 dark:border-white/10 dark:text-slate-200 dark:hover:bg-white/5"
                onClick={() => setDraft(null)}
                type="button"
              >
                Cancel
              </button>
              <button
                className="rounded-md bg-blue-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-60"
                disabled={draft.hostname.trim() === ""}
                onClick={() => void saveDraft()}
                type="button"
              >
                Save
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}
