import { FlaskConical } from "lucide-react";

import { paymentProviderBrandInk, resolvePaymentProviderBrand } from "./payment-provider-brand";

export type PaymentProviderBrandMarkSize = "xs" | "sm" | "md";

export interface PaymentProviderBrandMarkProps {
  className?: string;
  /** Value for `data-provider`, so callers keep a stable selector. */
  dataProvider?: string;
  /** Accessible name. When omitted the mark is decorative (a name sits next to it). */
  label?: string;
  providerCode: string;
  size?: PaymentProviderBrandMarkSize;
}

/**
 * The official brand mark of a payment provider in a fixed "app tile".
 *
 * Marks come from the CC0 `simple-icons` set as inert SVG paths: no per-row
 * network request, no `<img>` failure mode, crisp at any DPI. Providers without
 * an upstream mark fall back to a deterministic monogram, and a test/sandbox
 * endpoint never borrows a real provider's mark.
 *
 * Full-bleed artwork (an entire app-icon square such as Alipay or PhonePe) is
 * drawn edge to edge and clipped by the tile; a floating mark is inset. Without
 * that split a full-bleed square renders as a sticker inside a second square.
 */
const TILE_SIZES: Record<
  PaymentProviderBrandMarkSize,
  { tile: string; glyph: string; bleed: string; monogram: string }
> = {
  xs: {
    tile: "h-6 w-6 rounded-[5px]",
    glyph: "h-[14px] w-[14px]",
    bleed: "h-[27px] w-[27px]",
    monogram: "text-[8px]",
  },
  sm: {
    tile: "h-8 w-8 rounded-md",
    glyph: "h-[19px] w-[19px]",
    bleed: "h-[36px] w-[36px]",
    monogram: "text-[10px]",
  },
  md: {
    tile: "h-10 w-10 rounded-[10px]",
    glyph: "h-[23px] w-[23px]",
    bleed: "h-[45px] w-[45px]",
    monogram: "text-[12px]",
  },
};

export function PaymentProviderBrandMark({
  className,
  dataProvider,
  label,
  providerCode,
  size = "sm",
}: PaymentProviderBrandMarkProps) {
  const brand = resolvePaymentProviderBrand(providerCode);
  const dimensions = TILE_SIZES[size];
  const isBleed = brand.kind === "brand" && brand.icon.bleed === true;

  return (
    <span
      aria-hidden={label ? undefined : "true"}
      aria-label={label}
      className={[
        "relative inline-flex shrink-0 items-center justify-center bg-white ring-1 ring-slate-900/[0.08] shadow-[0_1px_2px_rgb(15_23_42_/_0.06)] dark:ring-white/15 dark:shadow-[0_1px_2px_rgb(0_0_0_/_0.45)]",
        dimensions.tile,
        isBleed ? "overflow-hidden" : "",
        className,
      ]
        .filter(Boolean)
        .join(" ")}
      data-payment-provider-brand={brand.kind}
      data-provider={dataProvider}
      role={label ? "img" : undefined}
      title={label}
    >
      {brand.kind === "brand" ? (
        <svg
          aria-hidden="true"
          className={isBleed ? dimensions.bleed : dimensions.glyph}
          fill="currentColor"
          style={{ color: paymentProviderBrandInk(brand.icon) }}
          viewBox="0 0 24 24"
          xmlns="http://www.w3.org/2000/svg"
        >
          <path d={brand.icon.path} />
        </svg>
      ) : brand.kind === "sandbox" ? (
        <FlaskConical
          aria-hidden="true"
          className={`${dimensions.glyph} text-amber-500`}
          strokeWidth={2}
        />
      ) : (
        <span
          aria-hidden="true"
          className={`font-semibold tracking-tight text-slate-500 ${dimensions.monogram}`}
        >
          {brand.monogram}
        </span>
      )}
    </span>
  );
}
