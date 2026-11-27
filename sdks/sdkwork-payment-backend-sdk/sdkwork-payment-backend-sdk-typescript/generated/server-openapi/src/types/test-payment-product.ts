import type { TestPaymentPayerField } from './test-payment-payer-field';

export interface TestPaymentProduct {
  providerCode: string;
  providerLabel: string;
  methodKey: string;
  /** Tenant payment-method catalog state: active, inactive, or missing */
  methodStatus: string;
  /** Launch surface the product produces (qr_code, redirect, form_redirect, card, jsapi, app, local_sandbox, unsupported) */
  launchKind: string;
  /** Settlement can be simulated from the console (sandbox and client-payload products) */
  settleSupported: boolean;
  /** A refund can be driven against the product */
  refundSupported: boolean;
  /** The product is part of the admin one-cent test surface */
  testEnabled: boolean;
  requiredPayerFields?: TestPaymentPayerField[];
  /** Active channels of this product bound to an active provider account */
  activeProviderAccounts: number;
  /** Active channels routing this product */
  activeChannelCount: number;
  /** Ordered findings; the single entry `ready` means the product can be exercised now */
  readiness: string[];
  ready: boolean;
}
