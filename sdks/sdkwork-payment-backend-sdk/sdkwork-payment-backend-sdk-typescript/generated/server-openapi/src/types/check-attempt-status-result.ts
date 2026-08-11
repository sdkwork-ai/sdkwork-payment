export interface CheckAttemptStatusResult {
  paymentIntentId: string;
  attemptId: string;
  /** Raw provider status from the PSP query (e.g. WeChat SUCCESS, Alipay TRADE_SUCCESS); null when the attempt was already terminal */
  providerStatus?: string;
  /** Local attempt status after applying the provider result */
  localStatus: string;
  /** True when the provider reports the payment as successful */
  paid: boolean;
}
