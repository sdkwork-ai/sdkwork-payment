export interface UpdatePaymentChannelCommand {
  channelNo: string;
  channelName?: string;
  providerAccountId: string;
  methodId: string;
  providerCode?: 'stripe' | 'alipay' | 'wechat_pay' | 'sandbox';
  sceneCode: 'app' | 'web' | 'mini_program' | 'api';
  currencyCode: string;
  countryCode: string;
  status?: 'active' | 'inactive' | 'deprecated';
  priority?: number;
  sortOrder?: number;
  metadata?: Record<string, unknown>;
}
