import { createSdkworkMessageCatalog, useSdkworkModuleMessages } from "@sdkwork/i18n-pc-react";

import { channelAdminMessages as enChannelAdmin } from "./en-US/commerce/channel-admin/messages";
import { channelAdminMessages as zhChannelAdmin } from "./zh-CN/commerce/channel-admin/messages";
import type { ChannelAdminMessages } from "../types/channel-admin-i18n";

export const PAYMENT_CHANNEL_I18N_CATALOG = createSdkworkMessageCatalog<ChannelAdminMessages>({
  defaultLocale: "en-US",
  locales: {
    "en-US": enChannelAdmin,
    "zh-CN": zhChannelAdmin,
  },
  namespace: "commerce.payment.channelAdmin",
});

export function useChannelAdminMessages(): ChannelAdminMessages {
  return useSdkworkModuleMessages(PAYMENT_CHANNEL_I18N_CATALOG);
}
