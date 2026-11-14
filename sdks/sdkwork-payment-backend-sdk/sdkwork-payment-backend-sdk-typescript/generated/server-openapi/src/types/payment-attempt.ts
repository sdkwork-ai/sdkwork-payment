export interface PaymentAttempt {
  id?: string;
  paymentIntentId?: string;
  attemptNo?: string;
  providerCode?: string;
  channelId?: string;
  /** Attempt amount as an integer in the currency's smallest unit (minor units, e.g. cents). */
  amount?: string;
  currencyCode?: string;
  status?: 'created' | 'pending' | 'processing' | 'succeeded' | 'failed' | 'canceled' | 'closed';
  providerTransactionId?: string;
  outTradeNo?: string;
  paidAt?: string;
  createdAt?: string;
}
