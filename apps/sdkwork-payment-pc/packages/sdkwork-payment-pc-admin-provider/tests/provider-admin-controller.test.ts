import { describe, expect, it, vi } from "vitest";

import { createPaymentProviderAdminController } from "../src/services/provider-admin-controller";

const providerAccount = {
  id: "provider-account-1",
  accountNo: "stripe-primary",
  providerCode: "stripe",
  merchantId: "merchant-1",
  accountMode: "partner",
  environment: "sandbox",
  countryCode: "US",
  settlementCurrency: "USD",
  hasPrimarySecret: true,
  hasWebhookSecret: true,
  hasCertificate: false,
  credentialStorage: "database_encrypted",
  capabilities: { pay: true },
  status: "inactive",
  createdAt: "2026-07-31T00:00:00.000Z",
  updatedAt: "2026-07-31T00:00:00.000Z",
};

const subMerchant = {
  id: "sub-merchant-1",
  providerAccountId: providerAccount.id,
  subMerchantNo: "sub-1",
  providerCode: "stripe",
  status: "active",
  createdAt: "2026-07-31T00:00:00.000Z",
  updatedAt: "2026-07-31T00:00:00.000Z",
};

function createBackendService() {
  const providerAccountsList = vi.fn(async () => ({
    items: [providerAccount],
    pageInfo: {
      mode: "offset",
      page: 1,
      pageSize: 20,
      totalItems: "1",
      totalPages: 1,
      hasMore: false,
    },
  }));
  const subMerchantsList = vi.fn(async (..._args: unknown[]) => ({
    items: [subMerchant],
    pageInfo: {
      mode: "offset",
      page: 1,
      pageSize: 20,
      totalItems: "1",
      totalPages: 1,
      hasMore: false,
    },
  }));
  const providerAccountsCreate = vi.fn(async (..._args: unknown[]) => providerAccount);
  const providerAccountsUpdate = vi.fn(async (..._args: unknown[]) => providerAccount);
  const providerAccountsDelete = vi.fn(async () => undefined);
  const providerAccountsTest = vi.fn(async (..._args: unknown[]) => ({
    ok: true,
    providerCode: "stripe",
    environment: "sandbox",
    testedAt: "2026-07-31T00:00:00.000Z",
  }));
  const providerAccountsRotate = vi.fn(async (..._args: unknown[]) => providerAccount);
  const subMerchantsCreate = vi.fn(async (..._args: unknown[]) => subMerchant);
  const subMerchantsUpdate = vi.fn(async (..._args: unknown[]) => subMerchant);

  return {
    calls: {
      providerAccountsCreate,
      providerAccountsDelete,
      providerAccountsList,
      providerAccountsRotate,
      providerAccountsTest,
      providerAccountsUpdate,
      subMerchantsCreate,
      subMerchantsList,
      subMerchantsUpdate,
    },
    service: {
      providerAccounts: {
        list: providerAccountsList,
        create: providerAccountsCreate,
        update: providerAccountsUpdate,
        delete: providerAccountsDelete,
        test: providerAccountsTest,
        credentials: { rotate: providerAccountsRotate },
      },
      subMerchants: {
        list: subMerchantsList,
        create: subMerchantsCreate,
        retrieve: vi.fn(),
        update: subMerchantsUpdate,
        delete: vi.fn(),
      },
    } as never,
  };
}

describe("payment provider admin controller", () => {
  it("uses SDK pageSize and filters the initial sub-merchant page by partner account", async () => {
    const { calls, service } = createBackendService();
    const controller = createPaymentProviderAdminController({ service });

    await controller.load();

    expect(calls.providerAccountsList).toHaveBeenCalledWith({ pageSize: 20 });
    expect(calls.subMerchantsList).toHaveBeenCalledWith({
      pageSize: 20,
      providerAccountId: providerAccount.id,
    });
    expect(controller.getState().listPageInfo).toEqual({
      providerAccounts: expect.objectContaining({ mode: "offset", page: 1, pageSize: 20 }),
      subMerchants: expect.objectContaining({ mode: "offset", page: 1, pageSize: 20 }),
    });
    expect(controller.getState().selectedProviderAccount?.id).toBe(providerAccount.id);
  });

  it("passes a fresh idempotency key through every sensitive generated SDK command", async () => {
    const { calls, service } = createBackendService();
    const controller = createPaymentProviderAdminController({ service });
    await controller.load();

    await controller.createProviderAccount({
      accountNo: providerAccount.accountNo,
      providerCode: "stripe",
      merchantId: providerAccount.merchantId,
      accountMode: "partner",
      environment: "sandbox",
      countryCode: "US",
      settlementCurrency: "USD",
      primarySecret: "write-only-secret",
      status: "inactive",
    });
    await controller.updateProviderAccount(providerAccount.id, { status: "inactive" });
    await controller.testProviderAccount(providerAccount.id, { dryRun: true });
    await controller.rotateProviderAccountCredentials(providerAccount.id, {
      primarySecret: "replacement-secret",
    });
    await controller.createSubMerchant({
      providerAccountId: providerAccount.id,
      providerCode: "stripe",
      subMerchantNo: subMerchant.subMerchantNo,
    });
    await controller.updateSubMerchant(subMerchant.id, { status: "active" });

    expect(calls.providerAccountsCreate.mock.calls[0]?.[1]).toEqual({
      idempotencyKey: expect.stringMatching(/^provider-account-create-/),
    });
    expect(calls.providerAccountsUpdate.mock.calls[0]?.[2]).toEqual({
      idempotencyKey: expect.stringMatching(/^provider-account-update-/),
    });
    expect(calls.providerAccountsTest.mock.calls[0]?.[1]).toEqual({
      idempotencyKey: expect.stringMatching(/^provider-account-test-/),
    });
    // 测试选项（含 dryRun）必须走 SDK body（第 3 参数），而不是被当作查询参数。
    expect(calls.providerAccountsTest.mock.calls[0]?.[2]).toEqual({ dryRun: true });
    expect(calls.providerAccountsRotate.mock.calls[0]?.[2]).toEqual({
      idempotencyKey: expect.stringMatching(/^provider-account-rotate-/),
    });
    expect(calls.subMerchantsCreate.mock.calls[0]?.[1]).toEqual({
      idempotencyKey: expect.stringMatching(/^sub-merchant-create-/),
    });
    expect(calls.subMerchantsUpdate.mock.calls[0]?.[2]).toEqual({
      idempotencyKey: expect.stringMatching(/^sub-merchant-update-/),
    });
    const idempotencyKeyOf = (value: unknown) =>
      (value as { idempotencyKey?: string } | undefined)?.idempotencyKey;
    const idempotencyKeys = [
      idempotencyKeyOf(calls.providerAccountsCreate.mock.calls[0]?.[1]),
      idempotencyKeyOf(calls.providerAccountsUpdate.mock.calls[0]?.[2]),
      idempotencyKeyOf(calls.providerAccountsTest.mock.calls[0]?.[1]),
      idempotencyKeyOf(calls.providerAccountsRotate.mock.calls[0]?.[2]),
      idempotencyKeyOf(calls.subMerchantsCreate.mock.calls[0]?.[1]),
      idempotencyKeyOf(calls.subMerchantsUpdate.mock.calls[0]?.[2]),
    ];
    expect(new Set(idempotencyKeys).size).toBe(idempotencyKeys.length);
    expect(calls.subMerchantsList.mock.calls).toHaveLength(3);
    for (const [query] of calls.subMerchantsList.mock.calls) {
      expect(query).toEqual({
        pageSize: 20,
        providerAccountId: providerAccount.id,
      });
    }
  });

  it("deletes a provider account through the generated SDK and reloads the list", async () => {
    const { calls, service } = createBackendService();
    const controller = createPaymentProviderAdminController({ service });
    await controller.load();
    const listCallsBeforeDelete = calls.providerAccountsList.mock.calls.length;

    await controller.deleteProviderAccount(providerAccount.id);

    expect(calls.providerAccountsDelete).toHaveBeenCalledTimes(1);
    expect(calls.providerAccountsDelete).toHaveBeenCalledWith(providerAccount.id);
    // 删除成功后刷新列表。
    expect(calls.providerAccountsList.mock.calls.length).toBe(listCallsBeforeDelete + 1);
  });

  it("clears the selected provider account after it is deleted", async () => {
    const { service } = createBackendService();
    const controller = createPaymentProviderAdminController({ service });
    await controller.load();
    expect(controller.getState().selectedProviderAccount?.id).toBe(providerAccount.id);

    await controller.deleteProviderAccount(providerAccount.id);

    expect(controller.getState().selectedProviderAccount).toBeUndefined();
  });
});
