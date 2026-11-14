export interface PaymentIntent {
  id?: string;
  paymentIntentNo?: string;
  orderId?: string;
  ownerUserId?: string;
  paymentMethod?: string;
  providerCode?: string;
  /** Payment amount as an integer in the currency's smallest unit (minor units, e.g. cents). */
  amount?: string;
  currencyCode?: string;
  status?: 'created' | 'pending' | 'processing' | 'succeeded' | 'failed' | 'canceled' | 'closed';
  createdAt?: string;
  updatedAt?: string;
}
