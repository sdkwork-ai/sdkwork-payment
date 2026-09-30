import {
  createSdkworkAppCapabilityManifest,
  type CreateSdkworkAppCapabilityManifestOptions,
  type SdkworkAppCapabilityManifest,
} from "@sdkwork/appbase-pc-react";
import type { SdkworkMediaResource } from "@sdkwork/payment-service";
import {
  createSdkworkPaymentMessages,
  type SdkworkPaymentMessagesOverrides,
} from "./payment-copy";

export type SdkworkPaymentStatus =
  | "closed"
  | "default"
  | "failed"
  | "pending"
  | "success"
  | "timeout"
  | "unknown";

export type SdkworkPaymentFilter = "actionable" | "all" | SdkworkPaymentStatus;
export type SdkworkPaymentProviderCode = "ALIPAY" | "UNION_PAY" | "WECHAT_PAY";
export type SdkworkPaymentClientType = "ANDROID" | "APP" | "H5" | "IOS" | "MINIAPP" | "WEB";
export type SdkworkPaymentProductType =
  | "app"
  | "h5"
  | "jsapi"
  | "miniapp"
  | "native"
  | "online_bank"
  | "pc"
  | "unknown";

export interface SdkworkPaymentProductTypeOption {
  available: boolean;
  code: SdkworkPaymentProductType;
  label: string;
}

export interface SdkworkPaymentMethod {
  available: boolean;
  code: string;
  icon?: SdkworkMediaResource;
  id: string;
  label: string;
  productTypes: SdkworkPaymentProductTypeOption[];
  recommendedProductType: SdkworkPaymentProductType;
  sort: number;
}

export interface SdkworkPaymentSummary {
  amountCny: number | null;
  canClose: boolean;
  canReconcile: boolean;
  canRefreshStatus: boolean;
  createdAt: string;
  expireTime?: string;
  id: string;
  orderId?: string;
  outTradeNo?: string;
  paymentMethod?: string;
  paymentProvider?: string;
  paymentProviderLabel?: string;
  paymentSn?: string;
  productType?: SdkworkPaymentProductType;
  status: SdkworkPaymentStatus;
  statusLabel: string;
  successTime?: string;
  transactionId?: string;
}

export interface SdkworkPaymentDetail extends SdkworkPaymentSummary {
  needQuery: boolean;
  paymentOrderId?: string;
  paymentParams: Record<string, unknown>;
  paymentUrl?: string;
  qrContent?: string;
  qrImage?: SdkworkMediaResource;
  queryIntervalSeconds?: number;
  remark?: string;
  subject?: string;
}

export interface SdkworkPaymentWorkspaceManifest extends SdkworkAppCapabilityManifest {
  capability: "payment";
  routePath: string;
}

export interface CreatePaymentWorkspaceManifestOptions
  extends Partial<
    Pick<CreateSdkworkAppCapabilityManifestOptions, "description" | "host" | "id" | "packageNames" | "theme" | "title">
  > {
  locale?: string | null;
  messages?: SdkworkPaymentMessagesOverrides;
  routePath?: string;
}

export interface SdkworkPaymentRouteIntent {
  filter?: SdkworkPaymentFilter;
  focusWindow: boolean;
  orderId?: string;
  paymentId?: string;
  route: string;
  source: "payment-workspace";
  type: "payment-route-intent";
}

export interface CreatePaymentRouteIntentOptions {
  basePath?: string;
  filter?: SdkworkPaymentFilter;
  focusWindow?: boolean;
  orderId?: string;
  paymentId?: string;
}

export interface SdkworkPaymentSummaryDigestInput {
  amountCny?: number | null;
  id: string;
  status: SdkworkPaymentStatus;
}

export interface SdkworkPaymentStatusDigest {
  actionablePayments: number;
  closedCount: number;
  failedCount: number;
  successfulPayments: number;
  timedOutPayments: number;
  totalAmountCny: number;
  totalCount: number;
}

function normalizeBasePath(basePath: string | undefined): string {
  const normalized = (basePath ?? "/payments").trim();
  if (!normalized || normalized === "/") {
    return "/payments";
  }

  return normalized.endsWith("/") ? normalized.slice(0, -1) : normalized;
}

function toSafeNumber(value: number | null | undefined): number {
  return typeof value === "number" && Number.isFinite(value) ? value : 0;
}

export function summarizeSdkworkPayments(
  payments: readonly SdkworkPaymentSummaryDigestInput[],
): SdkworkPaymentStatusDigest {
  return payments.reduce<SdkworkPaymentStatusDigest>(
    (summary, payment) => {
      summary.totalCount += 1;
      summary.totalAmountCny += toSafeNumber(payment.amountCny);

      if (payment.status === "default" || payment.status === "pending") {
        summary.actionablePayments += 1;
      }

      if (payment.status === "success") {
        summary.successfulPayments += 1;
      }

      if (payment.status === "failed") {
        summary.failedCount += 1;
      }

      if (payment.status === "timeout") {
        summary.timedOutPayments += 1;
      }

      if (payment.status === "closed") {
        summary.closedCount += 1;
      }

      return summary;
    },
    {
      actionablePayments: 0,
      closedCount: 0,
      failedCount: 0,
      successfulPayments: 0,
      timedOutPayments: 0,
      totalAmountCny: 0,
      totalCount: 0,
    },
  );
}

export function createPaymentWorkspaceManifest({
  description,
  host,
  id = "sdkwork-payment",
  locale,
  messages,
  packageNames = ["@sdkwork/payment-pc-payment"],
  routePath = "/payments",
  theme,
  title,
}: CreatePaymentWorkspaceManifestOptions = {}): SdkworkPaymentWorkspaceManifest {
  const copy = createSdkworkPaymentMessages(locale, messages).manifest;

  return {
    ...createSdkworkAppCapabilityManifest({
      description: description ?? copy.description,
      host,
      id,
      packageNames,
      theme,
      title: title ?? copy.title,
    }),
    capability: "payment",
    routePath: normalizeBasePath(routePath),
  };
}

export function createPaymentRouteIntent(
  options: CreatePaymentRouteIntentOptions = {},
): SdkworkPaymentRouteIntent {
  const basePath = normalizeBasePath(options.basePath);
  const queryParams = new URLSearchParams();

  if (options.filter) {
    queryParams.set("filter", options.filter);
  }

  if (options.paymentId) {
    queryParams.set("paymentId", options.paymentId);
  }

  if (options.orderId) {
    queryParams.set("orderId", options.orderId);
  }

  const querySuffix = queryParams.toString() ? `?${queryParams.toString()}` : "";

  return {
    focusWindow: options.focusWindow !== false,
    ...(options.filter ? { filter: options.filter } : {}),
    ...(options.paymentId ? { paymentId: options.paymentId } : {}),
    ...(options.orderId ? { orderId: options.orderId } : {}),
    route: `${basePath}${querySuffix}`,
    source: "payment-workspace",
    type: "payment-route-intent",
  };
}

export const paymentPackageMeta = {
  architecture: "pc-react",
  domain: "commerce",
  package: "@sdkwork/payment-pc-payment",
  status: "ready",
} as const;

export type PaymentPackageMeta = typeof paymentPackageMeta;
