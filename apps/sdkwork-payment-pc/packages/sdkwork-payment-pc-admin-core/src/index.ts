export function listSdkworkPaymentPcAdminSdkInventory() {
  return [
    "@sdkwork/payment-backend-sdk",
  ] as const;
}

export const PAYMENT_PC_ADMIN_CAPABILITY_PACKAGES = [
  "@sdkwork/payment-pc-admin-provider",
  "@sdkwork/payment-pc-admin-devconfig",
  "@sdkwork/payment-pc-admin-channel",
  "@sdkwork/payment-pc-admin-monitor",
  "@sdkwork/payment-pc-admin-notify-domain",
] as const;

export type PaymentPcAdminCapabilityPackage = (typeof PAYMENT_PC_ADMIN_CAPABILITY_PACKAGES)[number];

export interface PaymentPcAdminModuleRecord {
  additionalPermissionPrefixes?: readonly string[];
  capability: string;
  id: string;
  packageName: PaymentPcAdminCapabilityPackage;
  permissionPrefix: string;
  routeBasePath: string;
}

export function createSdkworkPaymentPcAdminModuleRegistry(): readonly PaymentPcAdminModuleRecord[] {
  return [
    {
      capability: "provider",
      id: "payment-provider",
      packageName: "@sdkwork/payment-pc-admin-provider",
      permissionPrefix: "commerce.payments.provider_accounts",
      routeBasePath: "/admin/payments/providers",
    },
    {
      additionalPermissionPrefixes: [
        "commerce.payments.certificates",
        "commerce.payments.webhook_events",
        "commerce.payments.dev",
        "commerce.payments.provider_accounts",
      ],
      capability: "devconfig",
      id: "payment-devconfig",
      packageName: "@sdkwork/payment-pc-admin-devconfig",
      permissionPrefix: "commerce.payments.devconfig",
      routeBasePath: "/admin/payments/devconfig",
    },
    {
      additionalPermissionPrefixes: [
        "commerce.payments.methods",
        "commerce.payments.route_rules",
      ],
      capability: "channel",
      id: "payment-channel",
      packageName: "@sdkwork/payment-pc-admin-channel",
      permissionPrefix: "commerce.payments.channels",
      routeBasePath: "/admin/payments/channels",
    },
    {
      "additionalPermissionPrefixes": [
        "commerce.payments.intents",
        "commerce.payments.attempts",
        "commerce.payments.refunds",
        "commerce.payments.webhook_events",
        "commerce.payments.reconciliation_runs",
      ],
      capability: "monitor",
      id: "payment-monitor",
      packageName: "@sdkwork/payment-pc-admin-monitor",
      permissionPrefix: "commerce.payments.intents",
      routeBasePath: "/admin/payments/monitor",
    },
    {
      capability: "notify-domain",
      id: "payment-notify-domain",
      packageName: "@sdkwork/payment-pc-admin-notify-domain",
      permissionPrefix: "commerce.payments.notify_domains",
      routeBasePath: "/admin/payments/notify-domains",
    },
  ] as const;
}

export {
  SdkworkPaymentListPaginationControls,
  type SdkworkPaymentListPaginationControlsProps,
} from "./components/SdkworkPaymentListPaginationControls";

export { AdminFieldLabel, type AdminFieldLabelProps } from "./components/AdminFieldLabel";

export {
  BaseDataSelectField,
  type BaseDataSelectFieldProps,
  type PaymentBaseDataOption,
} from "./components/BaseDataSelectField";

export {
  ConfirmDialog,
  useConfirmDialog,
  type ConfirmDialogProps,
  type ConfirmDialogState,
  type ConfirmDialogVariant,
  type UseConfirmDialogResult,
} from "./components/ConfirmDialog";

export {
  CopyButton,
  SecretRefField,
  maskSecretRef,
  type CopyButtonProps,
  type SecretRefFieldProps,
} from "./components/CopyButton";

export {
  PemFilePicker,
  type PemFilePickerProps,
} from "./components/PemFilePicker";

export {
  ADMIN_PROVIDER_CODES,
  ADMIN_PROVIDER_LABEL,
  ADMIN_PROVIDER_FORM_OPTIONS,
  ADMIN_PROVIDER_FILTER_OPTIONS,
  ADMIN_WEBHOOK_REPLAY_MAX_RETRIES,
  ADMIN_PAYMENT_METHOD_KEYS,
  adminPaymentMethodKeysForProvider,
  adminPaymentMethodKeyOption,
  formatAdminTimestamp,
  formatAdminAmount,
  formatAdminRelativeTime,
  type AdminProviderCode,
  type AdminPaymentMethodKeyOption,
} from "./components/admin-constants";

export { PaymentAdminI18nBoundary } from "./components/PaymentAdminI18nBoundary";
export {
  PaymentMethodIcon,
  PaymentProviderIcon,
  PaymentSceneIcon,
  type PaymentIdentityIconProps,
  type PaymentIdentityIconSize,
  type PaymentMethodIconProps,
  type PaymentProviderIconProps,
  type PaymentSceneIconProps,
} from "./components/PaymentIdentityIcon";
export {
  PaymentAdminTabsContent,
  PaymentAdminTabsList,
  PaymentAdminTabsTrigger,
  PaymentAdminWorkspace,
  type PaymentAdminWorkspaceProps,
} from "./components/PaymentAdminWorkspace";
export { PAYMENT_ADMIN_I18N_CATALOG, usePaymentAdminMessages } from "./i18n";

export {
  asString,
  asRequiredString,
  asStatus,
  asNumber,
  asRecord,
} from "./services/admin-coercion-helpers";
