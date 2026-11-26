export interface CheckoutScenePolicyUpsertRequest {
  scene?: string;
  appArchitecture?: string;
  osFamily?: string;
  paymentMethod: string;
  productCode: string;
  status?: 'active' | 'inactive';
  priority?: number;
}
