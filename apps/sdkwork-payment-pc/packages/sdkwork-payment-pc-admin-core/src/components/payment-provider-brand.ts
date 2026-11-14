import { PAYMENT_PROVIDER_BRAND_ICONS, type PaymentProviderBrandIcon } from "./payment-provider-brand-icons.generated";

/**
 * Brand identity for a payment provider, resolved from its wire `providerCode`.
 *
 * GENERATED DATA + PURE LOGIC: no React, so the resolution rules are unit
 * testable. The tile component renders the result.
 *
 * `payment-provider-brand-icons.generated.ts` is produced by the canonical
 * generator in the Cloud Router workspace:
 * `apps/sdkwork-cloudrouter-pc/scripts/generate-payment-provider-brand-icons.mjs`
 * (`--out <path>` writes this copy). One brand table, two consumer workspaces,
 * so a provider can never show a different mark or colour in the two consoles.
 */
const BRAND_ICONS: Readonly<Record<string, PaymentProviderBrandIcon>> = PAYMENT_PROVIDER_BRAND_ICONS;

/** Providers that are not brands and must never borrow a brand mark. */
const NON_BRAND_PROVIDER_CODES = new Set(["sandbox", "sandbox_test", "test", "mock"]);

export type ResolvedPaymentProviderBrand =
  | { kind: "brand"; icon: PaymentProviderBrandIcon }
  | { kind: "sandbox" }
  | { kind: "monogram"; monogram: string };

export function resolvePaymentProviderBrand(providerCode: string): ResolvedPaymentProviderBrand {
  const code = providerCode.trim().toLowerCase();
  if (NON_BRAND_PROVIDER_CODES.has(code)) {
    return { kind: "sandbox" };
  }
  const icon = BRAND_ICONS[code];
  if (icon) {
    return { kind: "brand", icon };
  }
  return { kind: "monogram", monogram: providerMonogram(code) };
}

/**
 * Monogram for a provider code: `jd_pay` -> `JP`, `unionpay` -> `UN`. Stable and
 * language-neutral on purpose — the localized display name sits next to it.
 */
export function providerMonogram(providerCode: string): string {
  const segments = providerCode
    .split(/[^a-z0-9]+/i)
    .map((segment) => segment.trim())
    .filter(Boolean);
  if (segments.length === 0) {
    return "?";
  }
  if (segments.length === 1) {
    return segments[0].slice(0, 2).toUpperCase();
  }
  return segments
    .slice(0, 2)
    .map((segment) => segment[0])
    .join("")
    .toUpperCase();
}

/** The colour the mark is drawn in: brand colour, or the legible-on-white ink. */
export function paymentProviderBrandInk(icon: PaymentProviderBrandIcon): string {
  return `#${icon.ink ?? icon.hex}`;
}

/**
 * Operator-facing name for any provider code. `ADMIN_PROVIDER_LABEL` only covers
 * the four codes of the first-party integrations, so a newer provider returned
 * by the backend used to render as the literal string "undefined".
 */
export function resolveProviderDisplayLabel(providerCode: string): string {
  const known: Record<string, string> = {
    stripe: "Stripe",
    alipay: "Alipay",
    wechat_pay: "WeChat Pay",
    sandbox: "Sandbox",
  };
  const code = providerCode.trim();
  if (known[code]) {
    return known[code];
  }
  if (!code) {
    return "Unknown provider";
  }
  // `wechat_pay` -> `Wechat Pay`, `apple_pay` -> `Apple Pay`.
  return code
    .split("_")
    .filter(Boolean)
    .map((segment) =>
      segment === "pay"
        ? "Pay"
        : segment.charAt(0).toUpperCase() + segment.slice(1).toLowerCase(),
    )
    .join(" ");
}
