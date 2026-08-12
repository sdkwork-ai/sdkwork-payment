// === App API method tree (app/v3/api) ===

export const APP_PAYMENT_METHOD_TREE = {
  payments: {
    methods: { list: true },
    intents: {
      create: true,
      retrieve: true,
      cancel: true,
      attempts: { create: true },
    },
    attempts: { retrieve: true },
    checkout: { retrieve: true },
    close: true,
    create: true,
    records: {
      list: true,
      retrieve: true,
    },
    reconcile: true,
    statistics: { summary: { retrieve: true } },
    status: {
      retrieve: true,
      outTradeNo: { retrieve: true },
    },
  },
  refunds: {
    create: true,
    list: true,
    retrieve: true,
  },
} as const;

// === Backend API method tree (backend/v3/api) ===
//
// 对齐 `apis/backend-api/payment/sdkwork-payment-backend-api.openapi.yaml` 的
// operationId 结构。每个叶子节点 `true` 对应一个 backend SDK 方法。SDK 生成器
// 会基于 OpenAPI 的 operationId 生成 `client.payments.<resource>.<action>(...)`
// 形态的方法。

export const BACKEND_PAYMENT_METHOD_TREE = {
  intents: {
    list: true,
    retrieve: true,
  },
  refunds: {
    list: true,
    create: true,
    retrieve: true,
    retry: true,
  },
  methods: {
    list: true,
    create: true,
    update: true,
  },
  providerAccounts: {
    list: true,
    create: true,
    update: true,
    test: true,
    delete: true,
    credentials: { rotate: true, read: true },
  },
  channels: {
    list: true,
    create: true,
    update: true,
    delete: true,
  },
  routeRules: {
    list: true,
    create: true,
    update: true,
    delete: true,
  },
  subMerchants: {
    list: true,
    create: true,
    retrieve: true,
    update: true,
    delete: true,
  },
  certificates: {
    list: true,
    create: true,
    retrieve: true,
    delete: true,
  },
  notifyDomains: {
    list: true,
    create: true,
    update: true,
    delete: true,
  },
  attempts: {
    list: true,
  },
  webhookEvents: {
    list: true,
    replay: true,
  },
  reconciliationRuns: {
    list: true,
    create: true,
  },
  dev: {
    sandboxTrigger: true,
    webhookSignatureTest: true,
  },
} as const;

export type PaymentRequestParams = Record<string, unknown>;
export type PaymentSdkResponse<T> = Promise<T>;
export type PaymentSdkMethod = (...args: any[]) => PaymentSdkResponse<any>;

type MethodTree = {
  readonly [key: string]: true | MethodTree;
};

export type ClientFromMethodTree<TTree extends MethodTree> = {
  readonly [TKey in keyof TTree]: TTree[TKey] extends true
    ? PaymentSdkMethod
    : TTree[TKey] extends MethodTree
      ? ClientFromMethodTree<TTree[TKey]>
      : never;
};

export type PaymentAppSdkClient = {
  commerce: ClientFromMethodTree<{ payments: (typeof APP_PAYMENT_METHOD_TREE)["payments"] }>;
  refunds: ClientFromMethodTree<(typeof APP_PAYMENT_METHOD_TREE)["refunds"]>;
};

// Backend SDK 客户端端口：由 `@sdkwork/payment-backend-sdk` 的组合 facade 实现，
// 在运行时由 bootstrap / shell 注入。Admin UI 通过 `@sdkwork/payment-service`
// 的 `service.backend` 访问此客户端，而非直接导入 SDK 包。
export type PaymentBackendSdkClient = {
  payments: ClientFromMethodTree<(typeof BACKEND_PAYMENT_METHOD_TREE)>;
};
