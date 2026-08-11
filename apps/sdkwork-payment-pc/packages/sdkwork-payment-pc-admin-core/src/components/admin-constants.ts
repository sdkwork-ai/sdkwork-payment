/**
 * Shared admin constants.
 *
 * Provider labels, provider options (for Select dropdowns), and the webhook
 * replay retry cap. Extracted to admin-core to eliminate 8+ duplicate
 * `PROVIDER_LABEL` definitions, 8+ `PROVIDER_OPTIONS` definitions, and 2
 * `REPLAY_MAX_RETRIES` definitions across the 4 admin capability packages.
 */

import { formatMoney } from "@sdkwork/utils/money";

export const ADMIN_PROVIDER_CODES = ["stripe", "alipay", "wechat_pay", "sandbox"] as const;
export type AdminProviderCode = (typeof ADMIN_PROVIDER_CODES)[number];

export const ADMIN_PROVIDER_LABEL: Record<AdminProviderCode, string> = {
  stripe: "Stripe",
  alipay: "Alipay",
  wechat_pay: "WeChat Pay",
  sandbox: "Sandbox",
};

/** Provider options for forms (create/edit) — 4 values, no "All" placeholder. */
export const ADMIN_PROVIDER_FORM_OPTIONS: ReadonlyArray<{ label: string; value: AdminProviderCode }> = [
  { label: "Stripe", value: "stripe" },
  { label: "Alipay", value: "alipay" },
  { label: "WeChat Pay", value: "wechat_pay" },
  { label: "Sandbox", value: "sandbox" },
];

/** Provider options for filters — includes "All providers" empty-value option. */
export const ADMIN_PROVIDER_FILTER_OPTIONS: ReadonlyArray<{ label: string; value: AdminProviderCode | "" }> = [
  { label: "All providers", value: "" },
  { label: "Stripe", value: "stripe" },
  { label: "Alipay", value: "alipay" },
  { label: "WeChat Pay", value: "wechat_pay" },
  { label: "Sandbox", value: "sandbox" },
];

/** Webhook replay retry cap (backend `WEBHOOK_STORED_REPLAY_MAX_RETRIES`). */
export const ADMIN_WEBHOOK_REPLAY_MAX_RETRIES = 5;

// ---------------------------------------------------------------------------
// Payment method keys
// ---------------------------------------------------------------------------
// Mirrors backend `PaymentCreateIntentRequest.payment_scene` routing in
// `crates/sdkwork-payment-providers/src/{alipay,wechat_pay,stripe}.rs`.
// Each method_key routes to a specific PSP API endpoint.

export interface AdminPaymentMethodKeyOption {
  readonly methodKey: string;
  readonly label: string;
  readonly providerCode: AdminProviderCode;
  readonly description: string;
}

/**
 * Canonical payment method keys grouped by provider.
 *
 * - Stripe: card, Apple Pay, Google Pay (wallet-based card payments via Stripe
 *   Dashboard), plus cross-border alipay/wechat_pay via Stripe
 * - Alipay: qr (in-store QR), pc (desktop website), wap (mobile website),
 *   app (native SDK), jsapi (in-page JSAPI)
 * - WeChat Pay: native (merchant QR), jsapi (Official Account / Mini Program),
 *   h5 (mobile browser), app (native SDK)
 * - Sandbox: test (local cashier, no external HTTP)
 */
export const ADMIN_PAYMENT_METHOD_KEYS: ReadonlyArray<AdminPaymentMethodKeyOption> = [
  // Stripe
  { methodKey: "stripe_card", label: "Credit / Debit Card", providerCode: "stripe", description: "Visa, Mastercard, Amex, Discover, JCB, UnionPay via Stripe" },
  { methodKey: "stripe_apple_pay", label: "Apple Pay", providerCode: "stripe", description: "Apple Pay wallet via Stripe (requires Dashboard + domain verification)" },
  { methodKey: "stripe_google_pay", label: "Google Pay", providerCode: "stripe", description: "Google Pay wallet via Stripe (requires Dashboard configuration)" },
  { methodKey: "stripe_alipay", label: "Alipay (cross-border)", providerCode: "stripe", description: "Alipay via Stripe for cross-border CNY settlement" },
  { methodKey: "stripe_wechat_pay", label: "WeChat Pay (cross-border)", providerCode: "stripe", description: "WeChat Pay via Stripe for cross-border settlement" },
  // Alipay (direct)
  { methodKey: "alipay_qr", label: "Alipay In-store QR", providerCode: "alipay", description: "alipay.trade.precreate — merchant scans buyer QR" },
  { methodKey: "alipay_pc", label: "Alipay PC Website", providerCode: "alipay", description: "alipay.trade.page.pay — desktop browser redirect" },
  { methodKey: "alipay_wap", label: "Alipay WAP (Mobile)", providerCode: "alipay", description: "alipay.trade.wap.pay — mobile browser redirect" },
  { methodKey: "alipay_app", label: "Alipay App", providerCode: "alipay", description: "alipay.trade.app.pay — native App SDK" },
  { methodKey: "alipay_jsapi", label: "Alipay JSAPI", providerCode: "alipay", description: "alipay.trade.create — in-page JSAPI (requires buyer_id)" },
  // WeChat Pay (direct)
  { methodKey: "wechat_native", label: "WeChat Pay Native (QR)", providerCode: "wechat_pay", description: "/v3/pay/transactions/native — buyer scans merchant QR" },
  { methodKey: "wechat_jsapi", label: "WeChat Pay JSAPI", providerCode: "wechat_pay", description: "/v3/pay/transactions/jsapi — Official Account / Mini Program (requires openid)" },
  { methodKey: "wechat_h5", label: "WeChat Pay H5", providerCode: "wechat_pay", description: "/v3/pay/transactions/h5 — mobile browser (requires client_ip)" },
  { methodKey: "wechat_app", label: "WeChat Pay App", providerCode: "wechat_pay", description: "/v3/pay/transactions/app — native App SDK" },
  // Sandbox
  { methodKey: "sandbox_test", label: "Sandbox", providerCode: "sandbox", description: "Local cashier URL — no external HTTP" },
];

/** Filter payment method keys by provider code. */
export function adminPaymentMethodKeysForProvider(
  providerCode: AdminProviderCode,
): ReadonlyArray<AdminPaymentMethodKeyOption> {
  return ADMIN_PAYMENT_METHOD_KEYS.filter((option) => option.providerCode === providerCode);
}

/** Find a payment method key option by its key. */
export function adminPaymentMethodKeyOption(
  methodKey: string,
): AdminPaymentMethodKeyOption | undefined {
  return ADMIN_PAYMENT_METHOD_KEYS.find((option) => option.methodKey === methodKey);
}

/**
 * Format an ISO timestamp into a human-readable locale string.
 * Returns "—" for falsy or invalid values.
 */
export function formatAdminTimestamp(value: string | undefined | null): string {
  if (!value) {
    return "—";
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleString();
}

/**
 * Format a monetary amount with currency symbol, mirroring industry PSP
 * displays (Stripe Dashboard amount column, Adyen Customer Area balance).
 *
 * Payment amounts are typically strings matching `^[0-9]+(\.[0-9]{1,2})?$`
 * per OpenAPI; this helper parses to a number and applies the shared
 * `formatMoney` (symbol mode). Falls back to a plain number + currency code
 * suffix when the ISO 4217 code is unknown.
 *
 * Returns "—" for falsy or invalid amounts.
 */
export function formatAdminAmount(
  amount: string | number | undefined | null,
  currencyCode?: string | undefined | null,
): string {
  if (amount === undefined || amount === null || amount === "") {
    return "—";
  }
  const numeric = typeof amount === "number" ? amount : Number(amount);
  if (!Number.isFinite(numeric)) {
    return String(amount);
  }
  const code = currencyCode ?? undefined;
  if (!code) {
    return `${numeric.toFixed(2)} `.trim();
  }
  return formatMoney(numeric, { currency: code, mode: "symbol" }) ?? `${numeric.toFixed(2)} ${code}`.trim();
}

/**
 * Format a timestamp as a relative time string (e.g., "3 minutes ago",
 * "in 2 hours"), mirroring industry PSP list views (Stripe Dashboard event
 * timestamps, Alipay merchant platform relative time labels).
 *
 * Uses `Intl.RelativeTimeFormat` with the most appropriate unit
 * (seconds/minutes/hours/days). Returns "—" for falsy or invalid values,
 * and falls back to `formatAdminTimestamp` when the value is older than 30 days
 * (relative time is no longer useful at that range).
 */
export function formatAdminRelativeTime(
  value: string | number | Date | undefined | null,
): string {
  if (!value) {
    return "—";
  }
  const date = value instanceof Date ? value : new Date(value);
  if (Number.isNaN(date.getTime())) {
    return "—";
  }
  const now = Date.now();
  const diffMs = date.getTime() - now;
  const absDiffMs = Math.abs(diffMs);
  const seconds = Math.round(diffMs / 1000);
  const minutes = Math.round(diffMs / (1000 * 60));
  const hours = Math.round(diffMs / (1000 * 60 * 60));
  const days = Math.round(diffMs / (1000 * 60 * 60 * 24));

  const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" });

  if (absDiffMs < 60 * 1000) {
    return rtf.format(seconds, "second");
  }
  if (absDiffMs < 60 * 60 * 1000) {
    return rtf.format(minutes, "minute");
  }
  if (absDiffMs < 24 * 60 * 60 * 1000) {
    return rtf.format(hours, "hour");
  }
  if (absDiffMs < 30 * 24 * 60 * 60 * 1000) {
    return rtf.format(days, "day");
  }
  // Beyond 30 days, relative time is no longer useful — show absolute timestamp.
  return formatAdminTimestamp(value as string);
}
