import {
  getSdkworkPaymentService,
  hasSdkworkPaymentSession,
  requireSdkworkPaymentSession,
  toNullableSdkworkPaymentNumber,
  toSdkworkPaymentNumber,
  toSdkworkPaymentOptionalString,
  unwrapSdkworkPaymentResponse,
  readSdkworkMediaResource,
  toExternalSdkworkPaymentMediaResource,
  type SdkworkPaymentAppService,
} from "@sdkwork/payment-service";
import {
  createSdkworkPaymentMessages,
  type SdkworkPaymentMessages,
  type SdkworkPaymentMessagesOverrides,
} from "./payment-copy";
import {
  summarizeSdkworkPayments,
  type SdkworkPaymentClientType,
  type SdkworkPaymentDetail,
  type SdkworkPaymentMethod,
  type SdkworkPaymentProductType,
  type SdkworkPaymentProductTypeOption,
  type SdkworkPaymentStatus,
  type SdkworkPaymentStatusDigest,
  type SdkworkPaymentSummary,
} from "./payment";

export interface SdkworkPaymentStatistics {
  closedCount: number;
  failedCount: number;
  pendingCount: number;
  succeededCount: number;
  timeoutCount: number;
  totalCount: number;
}

export interface SdkworkPaymentDashboardData {
  clientType: SdkworkPaymentClientType;
  digest: SdkworkPaymentStatusDigest;
  methods: SdkworkPaymentMethod[];
  records: SdkworkPaymentSummary[];
  statistics: SdkworkPaymentStatistics;
  pageInfo?: SdkworkPaymentPageInfo;
}

export interface SdkworkPaymentPageInfo {
  page: number;
  pageSize: number;
  totalItems: number;
  totalPages: number;
  hasNextPage: boolean;
}

export interface SdkworkPaymentCreateInput {
  amountCny?: number | null;
  businessOrderId?: string;
  businessType?: string;
  clientIp?: string;
  orderId: string;
  paymentMethod: string;
  paymentProvider?: string;
  paymentScene?: string;
  productType?: SdkworkPaymentProductType;
}

export interface SdkworkPaymentReconcileInput {
  orderId?: string;
  outTradeNo?: string;
  reconcileType?: "ORDER_ID" | "OUT_TRADE_NO";
}

export interface SdkworkPaymentCloseResult {
  closed: true;
  paymentId: string;
}

export interface CreateSdkworkPaymentServiceOptions {
  clientType?: SdkworkPaymentClientType;
  paymentAppService?: SdkworkPaymentAppService;
  locale?: string | null;
  messages?: SdkworkPaymentMessagesOverrides;
  pageSize?: number;
}

export interface SdkworkPaymentListRecordsInput {
  page?: number;
  pageSize?: number;
  status?: SdkworkPaymentStatus | "all";
  sortField?: string;
  sortDirection?: "asc" | "desc";
}

export interface SdkworkPaymentListRecordsResult {
  records: SdkworkPaymentSummary[];
  pageInfo: SdkworkPaymentPageInfo;
}

export interface SdkworkPaymentService {
  closePayment(paymentId: string): Promise<SdkworkPaymentCloseResult>;
  createPayment(input: SdkworkPaymentCreateInput): Promise<SdkworkPaymentDetail>;
  getDashboard(): Promise<SdkworkPaymentDashboardData>;
  getEmptyDashboard(): SdkworkPaymentDashboardData;
  getPaymentDetail(paymentId: string): Promise<SdkworkPaymentDetail>;
  getPaymentStatus(paymentId: string): Promise<SdkworkPaymentSummary>;
  getPaymentStatusByOutTradeNo(outTradeNo: string): Promise<SdkworkPaymentSummary>;
  listOrderPayments(orderId: string): Promise<SdkworkPaymentSummary[]>;
  listRecords(input?: SdkworkPaymentListRecordsInput): Promise<SdkworkPaymentListRecordsResult>;
  reconcilePayment(input: SdkworkPaymentReconcileInput): Promise<SdkworkPaymentSummary>;
}

interface RemotePageInfo {
  mode?: "offset" | "cursor";
  page?: number;
  pageSize?: number;
  totalItems?: number;
  totalPages?: number;
  hasNextPage?: boolean;
  cursor?: string | null;
}

interface RemoteListPage<T> {
  items?: T[];
  pageInfo?: RemotePageInfo;
}

interface RemotePaymentMethodProductType {
  available?: boolean;
  code?: string;
  name?: string;
}

interface RemotePaymentMethod {
  available?: boolean;
  code?: string;
  icon?: unknown;
  methodIcon?: unknown;
  methodId?: string;
  methodName?: string;
  productTypes?: RemotePaymentMethodProductType[];
  sort?: number | string;
}

interface RemotePaymentStatus {
  amount?: number | string;
  createdAt?: string;
  expireTime?: string;
  orderId?: number | string;
  outTradeNo?: string;
  paymentId?: number | string;
  paymentMethod?: string;
  paymentProvider?: string;
  paymentProviderName?: string;
  paymentSn?: string;
  productType?: string;
  status?: string;
  statusName?: string;
  successTime?: string;
  transactionId?: string;
}

interface RemotePaymentRecord extends RemotePaymentStatus {}

interface RemotePaymentDetail extends RemotePaymentStatus {
  merchantOrderId?: string;
  needQuery?: boolean;
  paymentOrderId?: string;
  paymentParams?: Record<string, unknown>;
  paymentUrl?: string;
  qrCode?: unknown;
  qrImage?: unknown;
  queryInterval?: number | string;
  remark?: string;
  subject?: string;
}

interface RemotePaymentStatistics {
  closedCount?: number | string;
  failedCount?: number | string;
  pendingCount?: number | string;
  succeededCount?: number | string;
  timeoutCount?: number | string;
  totalCount?: number | string;
}

type SdkworkPaymentCopyContext = Pick<SdkworkPaymentMessages, "common" | "productType" | "status">;
type SdkworkPaymentServiceCopy = SdkworkPaymentMessages["service"];

function mapPaymentStatus(status: string | undefined): SdkworkPaymentStatus {
  const normalized = (status || "").trim().toUpperCase();
  if (normalized === "DEFAULT") {
    return "default";
  }

  if (normalized === "PENDING") {
    return "pending";
  }

  if (normalized === "SUCCESS" || normalized === "PAID" || normalized === "COMPLETED") {
    return "success";
  }

  if (normalized === "FAILED") {
    return "failed";
  }

  if (normalized === "TIMEOUT") {
    return "timeout";
  }

  if (normalized === "CLOSED") {
    return "closed";
  }

  return "unknown";
}

function formatStatusLabel(status: SdkworkPaymentStatus, messages: SdkworkPaymentCopyContext): string {
  if (status === "default") {
    return messages.status.default;
  }

  if (status === "pending") {
    return messages.status.pending;
  }

  if (status === "success") {
    return messages.status.success;
  }

  if (status === "failed") {
    return messages.status.failed;
  }

  if (status === "timeout") {
    return messages.status.timeout;
  }

  if (status === "closed") {
    return messages.status.closed;
  }

  return messages.status.unknown;
}

function mapProductType(code: string | undefined): SdkworkPaymentProductType {
  const normalized = (code || "").trim().toLowerCase();
  if (
    normalized === "app"
    || normalized === "h5"
    || normalized === "jsapi"
    || normalized === "miniapp"
    || normalized === "native"
    || normalized === "online_bank"
    || normalized === "pc"
  ) {
    return normalized;
  }

  return "unknown";
}

function formatProductTypeLabel(code: string | undefined, messages: SdkworkPaymentCopyContext): string {
  const productType = mapProductType(code);
  if (productType === "app") {
    return messages.productType.app;
  }

  if (productType === "h5") {
    return messages.productType.h5;
  }

  if (productType === "jsapi") {
    return messages.productType.jsapi;
  }

  if (productType === "miniapp") {
    return messages.productType.miniapp;
  }

  if (productType === "native") {
    return messages.productType.native;
  }

  if (productType === "online_bank") {
    return messages.productType.onlineBank;
  }

  if (productType === "pc") {
    return messages.productType.pc;
  }

  return messages.productType.unknown;
}

function createMethodId(method: RemotePaymentMethod): string {
  const methodId = toSdkworkPaymentOptionalString(method.methodId);
  if (methodId) {
    return methodId;
  }

  return (toSdkworkPaymentOptionalString(method.code) || "payment-method").toLowerCase().replaceAll("_", "-");
}

function chooseRecommendedProductType(
  productTypes: readonly SdkworkPaymentProductTypeOption[],
): SdkworkPaymentProductType {
  const available = productTypes.filter((item) => item.available);
  const preferredOrder: SdkworkPaymentProductType[] = [
    "native",
    "pc",
    "app",
    "h5",
    "jsapi",
    "miniapp",
    "online_bank",
  ];

  for (const productType of preferredOrder) {
    if (available.some((item) => item.code === productType)) {
      return productType;
    }
  }

  return available[0]?.code ?? productTypes[0]?.code ?? "unknown";
}

function mapProductTypes(
  productTypes: RemotePaymentMethodProductType[] | undefined,
  messages: SdkworkPaymentCopyContext,
): SdkworkPaymentProductTypeOption[] {
  return (productTypes ?? []).map((item) => ({
    available: item.available !== false,
    code: mapProductType(item.code),
    label: toSdkworkPaymentOptionalString(item.name) || formatProductTypeLabel(item.code, messages),
  }));
}

function mapMethod(method: RemotePaymentMethod, messages: SdkworkPaymentCopyContext): SdkworkPaymentMethod {
  const productTypes = mapProductTypes(method.productTypes, messages);

  return {
    available: method.available !== false,
    code: toSdkworkPaymentOptionalString(method.code) || "UNKNOWN",
    icon: readSdkworkMediaResource(method.methodIcon) || readSdkworkMediaResource(method.icon),
    id: createMethodId(method),
    label: toSdkworkPaymentOptionalString(method.methodName) || messages.common.payment,
    productTypes,
    recommendedProductType: chooseRecommendedProductType(productTypes),
    sort: toSdkworkPaymentNumber(method.sort),
  };
}

function sortMethods(methods: SdkworkPaymentMethod[]): SdkworkPaymentMethod[] {
  return [...methods].sort(
    (left, right) =>
      Number(right.available) - Number(left.available)
      || right.sort - left.sort
      || left.label.localeCompare(right.label),
  );
}

const ALLOWED_PAYMENT_URL_SCHEMES = new Set(["http:", "https:"]);

function isSafePaymentUrl(value: string | undefined | null): value is string {
  if (!value) {
    return false;
  }
  const trimmed = value.trim();
  if (!trimmed) {
    return false;
  }
  // 防御 javascript:/data:/vbscript: 等 XSS 向量，仅放行 http/https。
  try {
    const url = new URL(trimmed);
    return ALLOWED_PAYMENT_URL_SCHEMES.has(url.protocol.toLowerCase());
  } catch {
    return false;
  }
}

function derivePaymentUrl(detail: RemotePaymentDetail | null | undefined): string | undefined {
  const paymentParams = detail?.paymentParams ?? {};
  const candidates = [
    toSdkworkPaymentOptionalString(detail?.paymentUrl),
    toSdkworkPaymentOptionalString(paymentParams.payUrl),
    toSdkworkPaymentOptionalString(paymentParams.mwebUrl),
  ];
  return candidates.find((candidate) => isSafePaymentUrl(candidate));
}

function isQrImageLocator(value: string | undefined): boolean {
  return Boolean(value && /^(?:data:image\/|https?:\/\/).+/i.test(value));
}

function deriveQrContent(detail: RemotePaymentDetail | null | undefined): string | undefined {
  const paymentParams = detail?.paymentParams ?? {};
  const value = toSdkworkPaymentOptionalString(detail?.qrCode)
    || toSdkworkPaymentOptionalString(paymentParams.qrCode)
    || toSdkworkPaymentOptionalString(paymentParams.codeUrl);
  return isQrImageLocator(value) ? undefined : value;
}

function deriveQrImage(detail: RemotePaymentDetail | null | undefined) {
  const paymentParams = detail?.paymentParams ?? {};
  const imageResource = readSdkworkMediaResource(detail?.qrImage)
    || readSdkworkMediaResource(detail?.qrCode)
    || readSdkworkMediaResource(paymentParams.qrImage);
  if (imageResource) {
    return imageResource;
  }

  const imageLocator = [
    toSdkworkPaymentOptionalString(detail?.qrCode),
    toSdkworkPaymentOptionalString(paymentParams.qrCode),
    toSdkworkPaymentOptionalString(paymentParams.qrImage),
  ].find(isQrImageLocator);

  return toExternalSdkworkPaymentMediaResource(imageLocator, "image");
}

function mapSummary(
  payment: RemotePaymentStatus | null | undefined,
  messages: SdkworkPaymentCopyContext,
  fallback: Partial<SdkworkPaymentSummary> = {},
): SdkworkPaymentSummary {
  const status = mapPaymentStatus(toSdkworkPaymentOptionalString(payment?.status));

  return {
    amountCny: toNullableSdkworkPaymentNumber(payment?.amount) ?? fallback.amountCny ?? null,
    canClose: status === "default" || status === "pending",
    canReconcile: status === "default" || status === "pending" || status === "failed" || status === "timeout",
    canRefreshStatus: status === "default" || status === "pending",
    createdAt: toSdkworkPaymentOptionalString(payment?.createdAt) || fallback.createdAt || new Date(0).toISOString(),
    expireTime: toSdkworkPaymentOptionalString(payment?.expireTime) || fallback.expireTime,
    id: toSdkworkPaymentOptionalString(payment?.paymentId) || fallback.id || "unknown-payment",
    orderId: toSdkworkPaymentOptionalString(payment?.orderId) || fallback.orderId,
    outTradeNo: toSdkworkPaymentOptionalString(payment?.outTradeNo) || fallback.outTradeNo,
    paymentMethod: toSdkworkPaymentOptionalString(payment?.paymentMethod) || fallback.paymentMethod,
    paymentProvider: toSdkworkPaymentOptionalString(payment?.paymentProvider) || fallback.paymentProvider,
    paymentProviderLabel: toSdkworkPaymentOptionalString(payment?.paymentProviderName) || fallback.paymentProviderLabel,
    paymentSn: toSdkworkPaymentOptionalString(payment?.paymentSn) || fallback.paymentSn,
    productType: mapProductType(toSdkworkPaymentOptionalString(payment?.productType) || fallback.productType),
    status,
    statusLabel: toSdkworkPaymentOptionalString(payment?.statusName) || formatStatusLabel(status, messages),
    successTime: toSdkworkPaymentOptionalString(payment?.successTime) || fallback.successTime,
    transactionId: toSdkworkPaymentOptionalString(payment?.transactionId) || fallback.transactionId,
  };
}

function mapDetail(
  payment: RemotePaymentDetail | null | undefined,
  messages: SdkworkPaymentCopyContext,
  fallback: Partial<SdkworkPaymentDetail> = {},
): SdkworkPaymentDetail {
  const summary = mapSummary(payment, messages, fallback);

  return {
    ...summary,
    needQuery: Boolean(payment?.needQuery ?? fallback.needQuery ?? summary.canRefreshStatus),
    paymentOrderId:
      toSdkworkPaymentOptionalString(payment?.paymentOrderId)
      || toSdkworkPaymentOptionalString(payment?.merchantOrderId)
      || fallback.paymentOrderId,
    paymentParams: (payment?.paymentParams ?? fallback.paymentParams ?? {}) as Record<string, unknown>,
    paymentUrl: derivePaymentUrl(payment) || fallback.paymentUrl,
    qrContent: deriveQrContent(payment) || fallback.qrContent,
    qrImage: deriveQrImage(payment) || fallback.qrImage,
    queryIntervalSeconds: toNullableSdkworkPaymentNumber(payment?.queryInterval) ?? fallback.queryIntervalSeconds ?? undefined,
    remark: toSdkworkPaymentOptionalString(payment?.remark) || fallback.remark,
    subject: toSdkworkPaymentOptionalString(payment?.subject) || fallback.subject,
  };
}

function mapStatistics(statistics: RemotePaymentStatistics | null | undefined): SdkworkPaymentStatistics {
  return {
    closedCount: toSdkworkPaymentNumber(statistics?.closedCount),
    failedCount: toSdkworkPaymentNumber(statistics?.failedCount),
    pendingCount: toSdkworkPaymentNumber(statistics?.pendingCount),
    succeededCount: toSdkworkPaymentNumber(statistics?.succeededCount),
    timeoutCount: toSdkworkPaymentNumber(statistics?.timeoutCount),
    totalCount: toSdkworkPaymentNumber(statistics?.totalCount),
  };
}

function createEmptyDashboard(clientType: SdkworkPaymentClientType): SdkworkPaymentDashboardData {
  return {
    clientType,
    digest: summarizeSdkworkPayments([]),
    methods: [],
    records: [],
    statistics: {
      closedCount: 0,
      failedCount: 0,
      pendingCount: 0,
      succeededCount: 0,
      timeoutCount: 0,
      totalCount: 0,
    },
  };
}

function derivePageInfo(
  page: RemoteListPage<unknown> | null | undefined,
  fallbackPage: number,
  fallbackPageSize: number,
): SdkworkPaymentPageInfo {
  const remotePageInfo = page?.pageInfo;
  const totalItems = toSdkworkPaymentNumber(remotePageInfo?.totalItems, 0);
  const pageSize = toSdkworkPaymentNumber(remotePageInfo?.pageSize, fallbackPageSize);
  const pageNumber = toSdkworkPaymentNumber(remotePageInfo?.page, fallbackPage);
  const totalPages = pageSize > 0 ? Math.max(1, Math.ceil(totalItems / pageSize)) : 1;
  return {
    page: pageNumber,
    pageSize,
    totalItems,
    totalPages,
    hasNextPage: pageNumber < totalPages,
  };
}

function resolveReconcilePayload(
  input: SdkworkPaymentReconcileInput,
  copy: SdkworkPaymentServiceCopy,
): {
  orderId?: string;
  outTradeNo?: string;
  reconcileType: "ORDER_ID" | "OUT_TRADE_NO";
} {
  if (toSdkworkPaymentOptionalString(input.orderId)) {
    return {
      orderId: toSdkworkPaymentOptionalString(input.orderId),
      outTradeNo: undefined,
      reconcileType: "ORDER_ID",
    };
  }

  if (toSdkworkPaymentOptionalString(input.outTradeNo)) {
    return {
      orderId: undefined,
      outTradeNo: toSdkworkPaymentOptionalString(input.outTradeNo),
      reconcileType: "OUT_TRADE_NO",
    };
  }

  if (input.reconcileType === "ORDER_ID") {
    throw new Error(copy.reconcileOrderIdRequired);
  }

  throw new Error(copy.reconcileInputRequired);
}

export function createSdkworkPaymentService(
  options: CreateSdkworkPaymentServiceOptions = {},
): SdkworkPaymentService {
  const messages = createSdkworkPaymentMessages(options.locale, options.messages);
  const copy = messages.service;
  const clientType = options.clientType ?? "WEB";
  const pageSize = options.pageSize ?? 20;
  const getPaymentAppService = () => options.paymentAppService ?? getSdkworkPaymentService();

  return {
    async closePayment(paymentId) {
      requireSdkworkPaymentSession(copy.signInRequired);
      await unwrapSdkworkPaymentResponse<void>(
        await getPaymentAppService().payments.close(paymentId),
        copy.closeFailed,
      );

      return {
        closed: true,
        paymentId,
      };
    },

    async createPayment(input) {
      requireSdkworkPaymentSession(copy.signInRequired);
      const payload = {
        amount: input.amountCny ?? undefined,
        businessOrderId: toSdkworkPaymentOptionalString(input.businessOrderId),
        businessType: toSdkworkPaymentOptionalString(input.businessType),
        clientIp: toSdkworkPaymentOptionalString(input.clientIp),
        orderId: input.orderId,
        paymentMethod: input.paymentMethod,
        paymentProvider: toSdkworkPaymentOptionalString(input.paymentProvider),
        paymentScene: toSdkworkPaymentOptionalString(input.paymentScene),
        productType: input.productType === "unknown" ? undefined : input.productType,
      };
      const result = unwrapSdkworkPaymentResponse<RemotePaymentDetail>(
        await getPaymentAppService().payments.create(payload),
        copy.createFailed,
      );

      return mapDetail(result, messages, {
        orderId: input.orderId,
        paymentMethod: input.paymentMethod,
        productType: input.productType,
      });
    },

    async getDashboard() {
      if (!hasSdkworkPaymentSession()) {
        return createEmptyDashboard(clientType);
      }

      const [methodsPayload, statisticsPayload, pagePayload] = await Promise.all([
        getPaymentAppService().payments.methods.list({ clientType }),
        getPaymentAppService().payments.statistics.summary.retrieve(),
        getPaymentAppService().payments.records.list({
            page: 1,
            pageSize,
            sortDirection: "desc",
            sortField: "createdAt",
        }),
      ]);
      const methods = unwrapSdkworkPaymentResponse<RemotePaymentMethod[]>(
        methodsPayload,
        copy.requestFailed,
      );
      const statistics = unwrapSdkworkPaymentResponse<RemotePaymentStatistics | null>(
        statisticsPayload,
        copy.requestFailed,
      );
      const page = unwrapSdkworkPaymentResponse<RemoteListPage<RemotePaymentRecord>>(
        pagePayload,
        copy.requestFailed,
      );

      const records = (page?.items ?? [])
        .map((payment) => mapSummary(payment, messages))
        .sort((left, right) => new Date(right.createdAt).getTime() - new Date(left.createdAt).getTime());
      const pageInfo = derivePageInfo(page, 1, pageSize);

      return {
        clientType,
        digest: summarizeSdkworkPayments(records),
        methods: sortMethods(methods.map((method) => mapMethod(method, messages))),
        records,
        statistics: mapStatistics(statistics),
        pageInfo,
      };
    },

    getEmptyDashboard() {
      return createEmptyDashboard(clientType);
    },

    async getPaymentDetail(paymentId) {
      requireSdkworkPaymentSession(copy.signInRequired);
      const result = unwrapSdkworkPaymentResponse<RemotePaymentDetail>(
        await getPaymentAppService().payments.records.retrieve(paymentId),
        copy.detailFailed,
      );

      return mapDetail(result, messages);
    },

    async getPaymentStatus(paymentId) {
      requireSdkworkPaymentSession(copy.signInRequired);
      const result = unwrapSdkworkPaymentResponse<RemotePaymentStatus>(
        await getPaymentAppService().payments.status.retrieve(paymentId),
        copy.statusFailed,
      );

      return mapSummary(result, messages);
    },

    async getPaymentStatusByOutTradeNo(outTradeNo) {
      requireSdkworkPaymentSession(copy.signInRequired);
      const result = unwrapSdkworkPaymentResponse<RemotePaymentStatus>(
        await getPaymentAppService().payments.status.outTradeNo.retrieve(outTradeNo),
        copy.statusByOutTradeNoFailed,
      );

      return mapSummary(result, messages);
    },

    async listOrderPayments(orderId) {
      requireSdkworkPaymentSession(copy.signInRequired);
      const result = unwrapSdkworkPaymentResponse<RemoteListPage<RemotePaymentStatus>>(
        await getPaymentAppService().payments.records.list({
          page: 1,
          pageSize: 200,
          orderId,
        }),
        copy.historyFailed,
      );

      return (result?.items ?? [])
        .map((payment) => mapSummary(payment, messages))
        .sort((left, right) => new Date(right.createdAt).getTime() - new Date(left.createdAt).getTime());
    },

    async listRecords(input) {
      requireSdkworkPaymentSession(copy.signInRequired);
      const pageNumber = Math.max(1, toSdkworkPaymentNumber(input?.page, 1));
      const pageSizeNumber = Math.min(200, Math.max(1, toSdkworkPaymentNumber(input?.pageSize, pageSize)));
      const statusFilter = input?.status && input.status !== "all" ? input.status : undefined;
      const sortField = toSdkworkPaymentOptionalString(input?.sortField) ?? "createdAt";
      const sortDirection = input?.sortDirection === "asc" ? "asc" : "desc";
      const result = unwrapSdkworkPaymentResponse<RemoteListPage<RemotePaymentRecord>>(
        await getPaymentAppService().payments.records.list({
          page: pageNumber,
          pageSize: pageSizeNumber,
          sortField,
          sortDirection,
          ...(statusFilter ? { status: statusFilter } : {}),
        }),
        copy.requestFailed,
      );

      const records = (result?.items ?? [])
        .map((payment) => mapSummary(payment, messages))
        .sort((left, right) => new Date(right.createdAt).getTime() - new Date(left.createdAt).getTime());
      const pageInfo = derivePageInfo(result, pageNumber, pageSizeNumber);

      return {
        records,
        pageInfo,
      };
    },

    async reconcilePayment(input) {
      requireSdkworkPaymentSession(copy.signInRequired);
      const payload = resolveReconcilePayload(input, copy);
      const result = unwrapSdkworkPaymentResponse<RemotePaymentStatus>(
        await getPaymentAppService().payments.reconcile(payload),
        copy.reconcileFailed,
      );

      return mapSummary(result, messages, {
        orderId: payload.orderId,
        outTradeNo: payload.outTradeNo,
      });
    },
  };
}

export const sdkworkPaymentService = createSdkworkPaymentService();
