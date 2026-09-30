export type SdkworkPaymentLocale = "en-US" | "zh-CN";

export type SdkworkPaymentMessagesOverrides = DeepPartial<SdkworkPaymentMessages>;

export interface SdkworkPaymentMessages {
  actions: {
    close: string;
    closePayment: string;
    createPayment: string;
    newPayment: string;
    reconcile: string;
    refresh: string;
    refreshStatus: string;
    viewDetails: string;
  };
  common: {
    emptyValue: string;
    payment: string;
    paymentAttempt: string;
  };
  controller: {
    bootstrapFailed: string;
    closeFailed: string;
    createFailed: string;
    detailFailed: string;
    reconcileContextRequired: string;
    reconcileFailed: string;
    selectPaymentMethodRequired: string;
    selectPaymentRequired: string;
    statusFailed: string;
  };
  createDialog: {
    description: string;
    errorTitle: string;
    orderIdLabel: string;
    paymentMethodLabel: string;
    paymentMethodPlaceholder: string;
    productTypeLabel: string;
    productTypePlaceholder: string;
    title: string;
  };
  detail: {
    amountMetricLabel: string;
    createdAtLabel: string;
    descriptionFallback: string;
    historyDescription: string;
    historyTitle: string;
    loading: string;
    methodMetricLabel: string;
    orderIdLabel: string;
    outTradeNoLabel: string;
    overviewDescription: string;
    overviewTitle: string;
    paymentIdLabel: string;
    paymentLinkLabel: string;
    paymentSerialLabel: string;
    pollingIdle: string;
    pollingLabel: string;
    productTypeLabel: string;
    providerLabel: string;
    qrImageAlt: string;
    qrPayloadLabel: string;
    qrUnavailable: string;
    scanDescription: string;
    scanTitle: string;
    statusMetricLabel: string;
    successTimeLabel: string;
    summaryLoading: string;
    title: string;
    transactionIdLabel: string;
  };
  empty: {
    paymentDescription: string;
    paymentTitle: string;
    relatedPayments: string;
  };
  filters: {
    actionable: string;
    all: string;
    failed: string;
    pending: string;
    success: string;
  };
  format: {
    paymentSummary: string;
    pollingRequired: string;
    recommendedProductTypeValue: string;
  };
  manifest: {
    description: string;
    title: string;
  };
  page: {
    description: string;
    errorTitle: string;
    eyebrow: string;
    loading: string;
    methodsEmpty: string;
    methodsEyebrow: string;
    methodsTitle: string;
    paginationLabel: string;
    paginationNext: string;
    paginationPrevious: string;
    paginationRecords: string;
    paginationSummary: string;
    recordsDescription: string;
    recordsEyebrow: string;
    recordsTitle: string;
    title: string;
  };
  productType: {
    app: string;
    h5: string;
    jsapi: string;
    miniapp: string;
    native: string;
    onlineBank: string;
    pc: string;
    unknown: string;
  };
  service: {
    clientMethodUnavailable: string;
    closeFailed: string;
    createFailed: string;
    detailFailed: string;
    historyFailed: string;
    reconcileFailed: string;
    reconcileInputRequired: string;
    reconcileOrderIdRequired: string;
    requestFailed: string;
    signInRequired: string;
    statusByOutTradeNoFailed: string;
    statusFailed: string;
  };
  stats: {
    actionablePayments: string;
    actionablePaymentsDescription: string;
    closedCount: string;
    closedCountDescription: string;
    failedCount: string;
    failedCountDescription: string;
    pendingCount: string;
    pendingCountDescription: string;
    succeededCount: string;
    succeededCountDescription: string;
    totalAttempts: string;
    totalAttemptsDescription: string;
  };
  status: {
    closed: string;
    default: string;
    failed: string;
    pending: string;
    success: string;
    timeout: string;
    unknown: string;
  };
}

type DeepPartial<T> = {
  [K in keyof T]?: T[K] extends (...args: never[]) => unknown
    ? T[K]
    : T[K] extends object
      ? DeepPartial<T[K]>
      : T[K];
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function mergeDeep<T>(base: T, overrides?: DeepPartial<T>): T {
  if (!overrides) {
    return base;
  }

  const output: Record<string, unknown> = {
    ...(base as Record<string, unknown>),
  };

  for (const [key, value] of Object.entries(overrides)) {
    if (value === undefined) {
      continue;
    }

    const baseValue = output[key];
    output[key] = isRecord(baseValue) && isRecord(value)
      ? mergeDeep(baseValue, value)
      : value;
  }

  return output as T;
}

const EN_US_MESSAGES: SdkworkPaymentMessages = {
  actions: {
    close: "Close",
    closePayment: "Close payment",
    createPayment: "Create payment",
    newPayment: "New payment",
    reconcile: "Reconcile",
    refresh: "Refresh",
    refreshStatus: "Refresh status",
    viewDetails: "View details",
  },
  common: {
    emptyValue: "--",
    payment: "Payment",
    paymentAttempt: "Payment attempt",
  },
  controller: {
    bootstrapFailed: "Failed to load payment center.",
    closeFailed: "Failed to close payment.",
    createFailed: "Failed to create payment.",
    detailFailed: "Failed to load payment detail.",
    reconcileContextRequired: "No payment context is available for reconciliation.",
    reconcileFailed: "Failed to reconcile payment.",
    selectPaymentMethodRequired: "Select a payment method before continuing.",
    selectPaymentRequired: "Select a payment before continuing.",
    statusFailed: "Failed to refresh payment status.",
  },
  createDialog: {
    description: "Start a new payment attempt for an existing commercial order and surface QR-aware payment material immediately.",
    errorTitle: "Payment create error",
    orderIdLabel: "Order id",
    paymentMethodLabel: "Payment method",
    paymentMethodPlaceholder: "Choose a payment method",
    productTypeLabel: "Product type",
    productTypePlaceholder: "Choose a product type",
    title: "Create payment",
  },
  detail: {
    amountMetricLabel: "Amount",
    createdAtLabel: "Created",
    descriptionFallback: "Inspect payment status, QR material, and related retry attempts.",
    historyDescription: "Related payment attempts for the same order.",
    historyTitle: "Order payment history",
    loading: "Loading payment detail...",
    methodMetricLabel: "Method",
    orderIdLabel: "Order id",
    outTradeNoLabel: "Out trade no",
    overviewDescription: "Core payment identifiers and channel state.",
    overviewTitle: "Overview",
    paymentIdLabel: "Payment id",
    paymentLinkLabel: "Payment link",
    paymentSerialLabel: "Payment serial",
    pollingIdle: "No additional status polling is required.",
    pollingLabel: "Polling",
    productTypeLabel: "Product type",
    providerLabel: "Provider",
    qrImageAlt: "Payment QR code",
    qrPayloadLabel: "QR payload",
    qrUnavailable: "QR material unavailable",
    scanDescription: "Desktop payment material synced from the active payment attempt.",
    scanTitle: "Scan to pay",
    statusMetricLabel: "Status",
    successTimeLabel: "Success time",
    summaryLoading: "Loading payment detail...",
    title: "Payment detail",
    transactionIdLabel: "Transaction id",
  },
  empty: {
    paymentDescription: "No payment attempts matched the current filter. Create a new payment or switch to another status view.",
    paymentTitle: "No payment attempts yet",
    relatedPayments: "No related payment attempts were returned for this order.",
  },
  filters: {
    actionable: "Actionable",
    all: "All",
    failed: "Failed",
    pending: "Pending",
    success: "Successful",
  },
  format: {
    paymentSummary: "Payment #{id}",
    pollingRequired: "Query every {value} seconds when waiting for settlement.",
    recommendedProductTypeValue: "Recommended: {value}",
  },
  manifest: {
    description: "Payment workspace for method selection, payment attempt tracking, and QR-aware billing operations.",
    title: "Payment",
  },
  page: {
    description: "Centralize payment methods, QR payment attempts, settlement tracking, and order retry flows in one reusable Sdkwork-grade workspace.",
    errorTitle: "Payment center error",
    eyebrow: "Payment orchestration",
    loading: "Loading payment center...",
    methodsEmpty: "No payment methods are currently available for this client type.",
    methodsEyebrow: "Payment methods",
    methodsTitle: "Provider rail",
    paginationLabel: "Page size",
    paginationNext: "Next",
    paginationPrevious: "Previous",
    paginationRecords: "records",
    paginationSummary: "Page {page} of {totalPages} · {totalItems} {records}",
    recordsDescription: "Recent payment attempts and settlement outcomes.",
    recordsEyebrow: "Payment records",
    recordsTitle: "Payment records",
    title: "Payment center",
  },
  productType: {
    app: "App",
    h5: "H5",
    jsapi: "JSAPI",
    miniapp: "Mini App",
    native: "Native",
    onlineBank: "Online Bank",
    pc: "PC Web",
    unknown: "Unknown",
  },
  service: {
    clientMethodUnavailable: "{name} is unavailable on the current app client.",
    closeFailed: "Failed to close payment.",
    createFailed: "Failed to create payment.",
    detailFailed: "Failed to load payment detail.",
    historyFailed: "Failed to load order payment history.",
    reconcileFailed: "Failed to reconcile payment.",
    reconcileInputRequired: "Provide orderId or outTradeNo to reconcile a payment.",
    reconcileOrderIdRequired: "orderId is required for ORDER_ID reconciliation.",
    requestFailed: "Request failed.",
    signInRequired: "Please sign in to manage payments.",
    statusByOutTradeNoFailed: "Failed to refresh payment status by out trade number.",
    statusFailed: "Failed to refresh payment status.",
  },
  stats: {
    actionablePayments: "Action required",
    actionablePaymentsDescription: "Attempts that still need user or backend follow-up.",
    closedCount: "Closed",
    closedCountDescription: "Attempts that were explicitly closed or expired.",
    failedCount: "Failed",
    failedCountDescription: "Attempts that failed before successful completion.",
    pendingCount: "Pending",
    pendingCountDescription: "Attempts that still need settlement confirmation.",
    succeededCount: "Successful",
    succeededCountDescription: "Payments confirmed by the active provider channel.",
    totalAttempts: "Total attempts",
    totalAttemptsDescription: "Payment attempts created across the current workspace.",
  },
  status: {
    closed: "Closed",
    default: "Default",
    failed: "Failed",
    pending: "Pending",
    success: "Successful",
    timeout: "Timed out",
    unknown: "Unknown",
  },
};

const ZH_CN_MESSAGES: SdkworkPaymentMessages = {
  actions: {
    close: "\u5173\u95ed",
    closePayment: "\u5173\u95ed\u652f\u4ed8",
    createPayment: "\u521b\u5efa\u652f\u4ed8",
    newPayment: "\u65b0\u5efa\u652f\u4ed8",
    reconcile: "\u5bf9\u8d26",
    refresh: "\u5237\u65b0",
    refreshStatus: "\u5237\u65b0\u72b6\u6001",
    viewDetails: "\u67e5\u770b\u8be6\u60c5",
  },
  common: {
    emptyValue: "--",
    payment: "\u652f\u4ed8",
    paymentAttempt: "\u652f\u4ed8\u5c1d\u8bd5",
  },
  controller: {
    bootstrapFailed: "\u52a0\u8f7d\u652f\u4ed8\u4e2d\u5fc3\u5931\u8d25\u3002",
    closeFailed: "\u5173\u95ed\u652f\u4ed8\u5931\u8d25\u3002",
    createFailed: "\u521b\u5efa\u652f\u4ed8\u5931\u8d25\u3002",
    detailFailed: "\u52a0\u8f7d\u652f\u4ed8\u8be6\u60c5\u5931\u8d25\u3002",
    reconcileContextRequired: "\u5f53\u524d\u6ca1\u6709\u53ef\u7528\u7684\u5bf9\u8d26\u4e0a\u4e0b\u6587\u3002",
    reconcileFailed: "\u5bf9\u8d26\u652f\u4ed8\u5931\u8d25\u3002",
    selectPaymentMethodRequired: "\u8bf7\u5148\u9009\u62e9\u652f\u4ed8\u65b9\u5f0f\u3002",
    selectPaymentRequired: "\u8bf7\u5148\u9009\u62e9\u4e00\u6761\u652f\u4ed8\u8bb0\u5f55\u3002",
    statusFailed: "\u5237\u65b0\u652f\u4ed8\u72b6\u6001\u5931\u8d25\u3002",
  },
  createDialog: {
    description: "\u4e3a\u73b0\u6709\u5546\u4e1a\u8ba2\u5355\u53d1\u8d77\u65b0\u7684\u652f\u4ed8\u5c1d\u8bd5\uff0c\u5e76\u7acb\u5373\u5c55\u793a\u4e8c\u7ef4\u7801\u4e0e\u652f\u4ed8\u6750\u6599\u3002",
    errorTitle: "\u521b\u5efa\u652f\u4ed8\u5f02\u5e38",
    orderIdLabel: "\u8ba2\u5355\u7f16\u53f7",
    paymentMethodLabel: "\u652f\u4ed8\u65b9\u5f0f",
    paymentMethodPlaceholder: "\u9009\u62e9\u652f\u4ed8\u65b9\u5f0f",
    productTypeLabel: "\u652f\u4ed8\u4ea7\u54c1",
    productTypePlaceholder: "\u9009\u62e9\u652f\u4ed8\u4ea7\u54c1",
    title: "\u65b0\u5efa\u652f\u4ed8",
  },
  detail: {
    amountMetricLabel: "\u91d1\u989d",
    createdAtLabel: "\u521b\u5efa\u65f6\u95f4",
    descriptionFallback: "\u67e5\u770b\u652f\u4ed8\u72b6\u6001\u3001\u4e8c\u7ef4\u7801\u6750\u6599\u548c\u76f8\u5173\u91cd\u8bd5\u8bb0\u5f55\u3002",
    historyDescription: "\u540c\u4e00\u8ba2\u5355\u4e0b\u7684\u5173\u8054\u652f\u4ed8\u5c1d\u8bd5\u3002",
    historyTitle: "\u8ba2\u5355\u652f\u4ed8\u5386\u53f2",
    loading: "\u6b63\u5728\u52a0\u8f7d\u652f\u4ed8\u8be6\u60c5...",
    methodMetricLabel: "\u65b9\u5f0f",
    orderIdLabel: "\u8ba2\u5355\u7f16\u53f7",
    outTradeNoLabel: "\u5546\u6237\u6d41\u6c34\u53f7",
    overviewDescription: "\u67e5\u770b\u6838\u5fc3\u652f\u4ed8\u6807\u8bc6\u4e0e\u6e20\u9053\u72b6\u6001\u3002",
    overviewTitle: "\u6982\u89c8",
    paymentIdLabel: "\u652f\u4ed8\u7f16\u53f7",
    paymentLinkLabel: "\u652f\u4ed8\u94fe\u63a5",
    paymentSerialLabel: "\u652f\u4ed8\u6d41\u6c34\u53f7",
    pollingIdle: "\u5f53\u524d\u65e0\u9700\u7ee7\u7eed\u8f6e\u8be2\u652f\u4ed8\u72b6\u6001\u3002",
    pollingLabel: "\u8f6e\u8be2\u7b56\u7565",
    productTypeLabel: "\u652f\u4ed8\u4ea7\u54c1",
    providerLabel: "\u652f\u4ed8\u901a\u9053",
    qrImageAlt: "\u652f\u4ed8\u4e8c\u7ef4\u7801",
    qrPayloadLabel: "\u4e8c\u7ef4\u7801\u8f7d\u8377",
    qrUnavailable: "\u6682\u672a\u8fd4\u56de\u4e8c\u7ef4\u7801\u6750\u6599",
    scanDescription: "\u684c\u9762\u7aef\u626b\u7801\u652f\u4ed8\u6750\u6599\u4e0e\u5f53\u524d\u652f\u4ed8\u5c1d\u8bd5\u5b9e\u65f6\u540c\u6b65\u3002",
    scanTitle: "\u626b\u7801\u652f\u4ed8",
    statusMetricLabel: "\u72b6\u6001",
    successTimeLabel: "\u6210\u529f\u65f6\u95f4",
    summaryLoading: "\u6b63\u5728\u52a0\u8f7d\u652f\u4ed8\u8be6\u60c5...",
    title: "\u652f\u4ed8\u8be6\u60c5",
    transactionIdLabel: "\u4ea4\u6613\u6d41\u6c34\u53f7",
  },
  empty: {
    paymentDescription: "\u5f53\u524d\u7b5b\u9009\u6761\u4ef6\u4e0b\u6ca1\u6709\u5339\u914d\u7684\u652f\u4ed8\u8bb0\u5f55\uff0c\u53ef\u4ee5\u65b0\u5efa\u652f\u4ed8\u6216\u5207\u6362\u5230\u5176\u4ed6\u72b6\u6001\u89c6\u56fe\u3002",
    paymentTitle: "\u6682\u65e0\u652f\u4ed8\u8bb0\u5f55",
    relatedPayments: "\u5f53\u524d\u8ba2\u5355\u6682\u672a\u8fd4\u56de\u5173\u8054\u652f\u4ed8\u5c1d\u8bd5\u3002",
  },
  filters: {
    actionable: "\u5f85\u5904\u7406",
    all: "\u5168\u90e8",
    failed: "\u5931\u8d25",
    pending: "\u5f85\u652f\u4ed8",
    success: "\u6210\u529f",
  },
  format: {
    paymentSummary: "\u652f\u4ed8 #{id}",
    pollingRequired: "\u6bcf {value} \u79d2\u67e5\u8be2\u4e00\u6b21\u652f\u4ed8\u72b6\u6001\u3002",
    recommendedProductTypeValue: "\u63a8\u8350\u4ea7\u54c1\uff1a{value}",
  },
  manifest: {
    description: "\u7528\u4e8e\u652f\u4ed8\u65b9\u5f0f\u9009\u62e9\u3001\u652f\u4ed8\u5c1d\u8bd5\u8ddf\u8e2a\u4e0e\u4e8c\u7ef4\u7801\u652f\u4ed8\u4e1a\u52a1\u7684\u652f\u4ed8\u5de5\u4f5c\u533a\u3002",
    title: "\u652f\u4ed8",
  },
  page: {
    description: "\u5c06\u652f\u4ed8\u65b9\u5f0f\u3001\u4e8c\u7ef4\u7801\u652f\u4ed8\u5c1d\u8bd5\u3001\u7ed3\u7b97\u8ddf\u8e2a\u4e0e\u8ba2\u5355\u91cd\u8bd5\u6d41\u7a0b\u96c6\u4e2d\u5230\u540c\u4e00\u4e2a\u53ef\u590d\u7528\u7684 Sdkwork \u98ce\u683c\u5de5\u4f5c\u533a\u4e2d\u3002",
    errorTitle: "\u652f\u4ed8\u4e2d\u5fc3\u5f02\u5e38",
    eyebrow: "\u652f\u4ed8\u7f16\u6392",
    loading: "\u6b63\u5728\u52a0\u8f7d\u652f\u4ed8\u4e2d\u5fc3...",
    methodsEmpty: "\u5f53\u524d\u5ba2\u6237\u7aef\u7c7b\u578b\u6682\u65e0\u53ef\u7528\u7684\u652f\u4ed8\u65b9\u5f0f\u3002",
    methodsEyebrow: "\u652f\u4ed8\u65b9\u5f0f",
    methodsTitle: "\u652f\u4ed8\u901a\u9053",
    paginationLabel: "\u6bcf\u9875\u6761\u6570",
    paginationNext: "\u4e0b\u4e00\u9875",
    paginationPrevious: "\u4e0a\u4e00\u9875",
    paginationRecords: "\u6761\u8bb0\u5f55",
    paginationSummary: "\u7b2c {page} \u9875 / \u5171 {totalPages} \u9875 \u00b7 {totalItems} {records}",
    recordsDescription: "\u67e5\u770b\u6700\u8fd1\u7684\u652f\u4ed8\u5c1d\u8bd5\u4e0e\u7ed3\u7b97\u7ed3\u679c\u3002",
    recordsEyebrow: "\u652f\u4ed8\u8bb0\u5f55",
    recordsTitle: "\u652f\u4ed8\u8bb0\u5f55",
    title: "\u652f\u4ed8\u4e2d\u5fc3",
  },
  productType: {
    app: "APP",
    h5: "H5",
    jsapi: "JSAPI",
    miniapp: "\u5c0f\u7a0b\u5e8f",
    native: "\u539f\u751f\u626b\u7801",
    onlineBank: "\u7f51\u94f6\u652f\u4ed8",
    pc: "PC \u7f51\u9875",
    unknown: "\u672a\u77e5\u4ea7\u54c1",
  },
  service: {
    clientMethodUnavailable: "\u5f53\u524d\u5e94\u7528\u5ba2\u6237\u7aef\u672a\u63d0\u4f9b {name} \u80fd\u529b\u3002",
    closeFailed: "\u5173\u95ed\u652f\u4ed8\u5931\u8d25\u3002",
    createFailed: "\u521b\u5efa\u652f\u4ed8\u5931\u8d25\u3002",
    detailFailed: "\u52a0\u8f7d\u652f\u4ed8\u8be6\u60c5\u5931\u8d25\u3002",
    historyFailed: "\u52a0\u8f7d\u8ba2\u5355\u652f\u4ed8\u5386\u53f2\u5931\u8d25\u3002",
    reconcileFailed: "\u5bf9\u8d26\u652f\u4ed8\u5931\u8d25\u3002",
    reconcileInputRequired: "\u8bf7\u63d0\u4f9b orderId \u6216 outTradeNo \u7528\u4e8e\u5bf9\u8d26\u3002",
    reconcileOrderIdRequired: "\u5f53\u5bf9\u8d26\u65b9\u5f0f\u4e3a ORDER_ID \u65f6\uff0c\u5fc5\u987b\u63d0\u4f9b orderId\u3002",
    requestFailed: "\u8bf7\u6c42\u5931\u8d25\u3002",
    signInRequired: "\u8bf7\u5148\u767b\u5f55\u540e\u518d\u7ba1\u7406\u652f\u4ed8\u3002",
    statusByOutTradeNoFailed: "\u901a\u8fc7\u5546\u6237\u6d41\u6c34\u53f7\u5237\u65b0\u652f\u4ed8\u72b6\u6001\u5931\u8d25\u3002",
    statusFailed: "\u5237\u65b0\u652f\u4ed8\u72b6\u6001\u5931\u8d25\u3002",
  },
  stats: {
    actionablePayments: "\u5f85\u5904\u7406\u652f\u4ed8",
    actionablePaymentsDescription: "\u4ecd\u9700\u7528\u6237\u6216\u540e\u7aef\u8ddf\u8fdb\u7684\u652f\u4ed8\u5c1d\u8bd5\u3002",
    closedCount: "\u5df2\u5173\u95ed\u652f\u4ed8",
    closedCountDescription: "\u5df2\u660e\u786e\u5173\u95ed\u6216\u5df2\u8fc7\u671f\u7684\u652f\u4ed8\u5c1d\u8bd5\u3002",
    failedCount: "\u652f\u4ed8\u5931\u8d25",
    failedCountDescription: "\u5728\u6210\u529f\u4e4b\u524d\u5df2\u7ecf\u5931\u8d25\u7684\u652f\u4ed8\u5c1d\u8bd5\u3002",
    pendingCount: "\u5f85\u652f\u4ed8",
    pendingCountDescription: "\u4ecd\u5728\u7b49\u5f85\u7ed3\u7b97\u786e\u8ba4\u7684\u652f\u4ed8\u5c1d\u8bd5\u3002",
    succeededCount: "\u652f\u4ed8\u6210\u529f",
    succeededCountDescription: "\u5df2\u88ab\u6d3b\u8dc3\u652f\u4ed8\u6e20\u9053\u786e\u8ba4\u6210\u529f\u7684\u8bb0\u5f55\u3002",
    totalAttempts: "\u603b\u652f\u4ed8\u6b21\u6570",
    totalAttemptsDescription: "\u5f53\u524d\u5de5\u4f5c\u533a\u5185\u521b\u5efa\u7684\u652f\u4ed8\u5c1d\u8bd5\u603b\u6570\u3002",
  },
  status: {
    closed: "\u5df2\u5173\u95ed",
    default: "\u5f85\u786e\u8ba4",
    failed: "\u5931\u8d25",
    pending: "\u5f85\u652f\u4ed8",
    success: "\u6210\u529f",
    timeout: "\u5df2\u8d85\u65f6",
    unknown: "\u672a\u77e5",
  },
};

const SDKWORK_PAYMENT_MESSAGES: Record<SdkworkPaymentLocale, SdkworkPaymentMessages> = {
  "en-US": EN_US_MESSAGES,
  "zh-CN": ZH_CN_MESSAGES,
};

export function normalizeSdkworkPaymentLocale(locale?: string | null): SdkworkPaymentLocale {
  const normalized = String(locale || "").trim().toLowerCase();
  if (normalized.startsWith("zh")) {
    return "zh-CN";
  }

  return "en-US";
}

export function createSdkworkPaymentMessages(
  locale?: string | null,
  overrides?: SdkworkPaymentMessagesOverrides,
): SdkworkPaymentMessages {
  return mergeDeep(
    SDKWORK_PAYMENT_MESSAGES[normalizeSdkworkPaymentLocale(locale)],
    overrides,
  );
}
