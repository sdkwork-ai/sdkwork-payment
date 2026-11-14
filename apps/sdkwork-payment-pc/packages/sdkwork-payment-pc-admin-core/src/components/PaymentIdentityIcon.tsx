import type { LucideIcon } from "lucide-react";
import {
  AppWindow,
  Apple,
  BadgeDollarSign,
  Braces,
  CreditCard,
  FlaskConical,
  Globe2,
  MessageCircle,
  Monitor,
  QrCode,
  ScanLine,
  Smartphone,
  WalletCards,
} from "lucide-react";

import type { AdminProviderCode } from "./admin-constants";
import { PaymentProviderBrandMark } from "./PaymentProviderBrandMark";

export type PaymentIdentityIconSize = "xs" | "sm" | "md";

export interface PaymentIdentityIconProps {
  className?: string;
  label?: string;
  size?: PaymentIdentityIconSize;
}

export interface PaymentMethodIconProps extends PaymentIdentityIconProps {
  methodKey: string;
  providerCode?: AdminProviderCode | string;
}

export interface PaymentProviderIconProps extends PaymentIdentityIconProps {
  providerCode: AdminProviderCode | string;
}

export interface PaymentSceneIconProps extends PaymentIdentityIconProps {
  sceneCode: "api" | "app" | "mini_program" | "web" | string;
}

const SIZE_CLASS: Record<PaymentIdentityIconSize, string> = {
  xs: "h-6 w-6 rounded-[5px] [&_svg]:h-3 [&_svg]:w-3",
  sm: "h-8 w-8 rounded-md [&_svg]:h-4 [&_svg]:w-4",
  md: "h-10 w-10 rounded-md [&_svg]:h-5 [&_svg]:w-5",
};

const PROVIDER_TONE_CLASS: Record<AdminProviderCode, string> = {
  stripe: "border-[#635bff]/25 bg-[#635bff]/10 text-[#5145cd] dark:text-[#a9a4ff]",
  alipay: "border-[#1677ff]/25 bg-[#1677ff]/10 text-[#1169d8] dark:text-[#74adff]",
  wechat_pay: "border-[#07c160]/25 bg-[#07c160]/10 text-[#078b48] dark:text-[#45d88a]",
  sandbox: "border-amber-500/25 bg-amber-500/10 text-amber-700 dark:text-amber-300",
};

const PROVIDER_ICON: Record<AdminProviderCode, LucideIcon> = {
  stripe: CreditCard,
  alipay: BadgeDollarSign,
  wechat_pay: MessageCircle,
  sandbox: FlaskConical,
};

const METHOD_ICON: Record<string, LucideIcon> = {
  stripe_card: CreditCard,
  stripe_apple_pay: Apple,
  stripe_google_pay: WalletCards,
  stripe_alipay: BadgeDollarSign,
  stripe_wechat_pay: MessageCircle,
  alipay_qr: QrCode,
  alipay_pc: Monitor,
  alipay_wap: Globe2,
  alipay_app: Smartphone,
  alipay_jsapi: ScanLine,
  wechat_native: QrCode,
  wechat_jsapi: MessageCircle,
  wechat_h5: Globe2,
  wechat_app: Smartphone,
  sandbox_test: FlaskConical,
};

const SCENE_ICON: Record<string, LucideIcon> = {
  api: Braces,
  app: Smartphone,
  mini_program: AppWindow,
  web: Monitor,
};

function normalizeProviderCode(providerCode: string | undefined): AdminProviderCode | undefined {
  if (providerCode === "stripe" || providerCode === "alipay" || providerCode === "wechat_pay" || providerCode === "sandbox") {
    return providerCode;
  }
  return undefined;
}

function renderIdentityIcon(
  Icon: LucideIcon,
  toneClassName: string,
  props: PaymentIdentityIconProps,
  data: Record<string, string>,
) {
  const size = props.size ?? "sm";
  return (
    <span
      aria-hidden={props.label ? undefined : "true"}
      aria-label={props.label}
      className={[
        "inline-flex shrink-0 items-center justify-center border",
        SIZE_CLASS[size],
        toneClassName,
        props.className,
      ].filter(Boolean).join(" ")}
      role={props.label ? "img" : undefined}
      title={props.label}
      {...data}
    >
      <Icon aria-hidden="true" strokeWidth={1.8} />
    </span>
  );
}

/**
 * Provider identity: the provider's **official brand mark**, not a generic
 * credit-card/chat glyph. Operators recognise a provider by its logo, and a
 * generic icon made every non-first-party provider look identical (or, before
 * this change, like the same "dollar sign" placeholder).
 *
 * The mark set is shared with the Cloud Router payment console through the
 * canonical generator, so both consoles show the same artwork and brand colour.
 */
export function PaymentProviderIcon({ providerCode, ...props }: PaymentProviderIconProps) {
  return (
    <PaymentProviderBrandMark
      className={props.className}
      dataProvider={providerCode}
      label={props.label}
      providerCode={providerCode}
      size={props.size === "md" ? "md" : props.size === "xs" ? "xs" : "sm"}
    />
  );
}

export function PaymentMethodIcon({ methodKey, providerCode, ...props }: PaymentMethodIconProps) {
  const normalizedProvider = normalizeProviderCode(providerCode) ?? normalizeProviderCode(methodKey.split("_")[0]);
  const Icon = METHOD_ICON[methodKey] ?? (normalizedProvider ? PROVIDER_ICON[normalizedProvider] : WalletCards);
  const toneClassName = normalizedProvider
    ? PROVIDER_TONE_CLASS[normalizedProvider]
    : "border-[var(--sdk-color-border-default)] bg-[var(--sdk-color-surface-panel-muted)] text-[var(--sdk-color-text-secondary)]";
  return renderIdentityIcon(Icon, toneClassName, props, { "data-method-key": methodKey });
}

export function PaymentSceneIcon({ sceneCode, ...props }: PaymentSceneIconProps) {
  const Icon = SCENE_ICON[sceneCode] ?? AppWindow;
  return renderIdentityIcon(
    Icon,
    "border-[var(--sdk-color-border-default)] bg-[var(--sdk-color-surface-panel-muted)] text-[var(--sdk-color-text-secondary)]",
    props,
    { "data-scene": sceneCode },
  );
}
