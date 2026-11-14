export interface PaymentAmountBucket {
  currencyCode: string;
  /** Aggregated amount as an integer in the currency's smallest unit (minor units, e.g. cents). */
  amount: string;
  count: string;
}
