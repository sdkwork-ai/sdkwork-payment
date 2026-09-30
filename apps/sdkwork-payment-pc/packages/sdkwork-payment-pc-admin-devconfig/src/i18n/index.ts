import { createSdkworkMessageCatalog, useSdkworkModuleMessages } from "@sdkwork/i18n-pc-react";

import { devConfigAdminMessages as enDevConfig } from "./en-US/commerce/devconfig-admin/messages";
import { devConfigAdminMessages as zhDevConfig } from "./zh-CN/commerce/devconfig-admin/messages";
import type { DevConfigAdminMessages } from "../types/devconfig-admin-i18n";

export const PAYMENT_DEVCONFIG_I18N_CATALOG = createSdkworkMessageCatalog<DevConfigAdminMessages>({
  defaultLocale: "en-US",
  locales: {
    "en-US": enDevConfig,
    "zh-CN": zhDevConfig,
  },
  namespace: "commerce.payment.devConfigAdmin",
});

export function useDevConfigMessages(): DevConfigAdminMessages {
  return useSdkworkModuleMessages(PAYMENT_DEVCONFIG_I18N_CATALOG);
}
