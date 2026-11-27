export interface TestPaymentPayerField {
  /** Product the input belongs to */
  methodKey: string;
  /** Payer identity field name (openid, client_ip, buyer_id) as posted in payerIdentity */
  field: string;
  /** Operator-facing description of who the value identifies */
  label: string;
}
