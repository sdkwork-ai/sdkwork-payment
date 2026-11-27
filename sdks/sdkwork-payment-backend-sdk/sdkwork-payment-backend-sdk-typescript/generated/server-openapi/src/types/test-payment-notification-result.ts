export interface TestPaymentNotificationResult {
  providerAccountId: string;
  providerCode: string;
  verification: 'verified' | 'rejected';
  /** True when the signed delivery verified, normalized and was ingested */
  accepted: boolean;
  /** Result of the adapter's own signature verification */
  nativeSignature: string;
  /** Result of document-level signature verification, when the channel defines one */
  documentSignature: string;
  /** Signature header name the delivery carried */
  signatureHeader?: string;
  providerEventId?: string;
  /** Merchant trade number the notification resolved to */
  outTradeNo?: string;
  /** Webhook event row created by the delivery */
  webhookEventId?: string;
  /** Status the status machine applied (null when nothing transitioned) */
  appliedStatus?: string;
  paymentAttemptId?: string;
  diagnostic?: string;
  deliveredAt: string;
}
