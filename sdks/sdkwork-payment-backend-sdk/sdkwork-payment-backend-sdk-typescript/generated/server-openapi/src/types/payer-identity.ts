/** Payer identity for products that address a specific payer instead of whoever scans the QR code. Required keys are reported by the product coverage listing and enforced before any order is created. */
export interface PayerIdentity {
  /** Payer openid (wechat_jsapi) */
  openid?: string;
  /** Payer browser IP the H5 trade is scoped to (wechat_h5) */
  clientIp?: string;
  /** Alipay buyer user_id (alipay_jsapi) */
  buyerId?: string;
  /** Alipay buyer open_id alternative to buyerId (alipay_jsapi) */
  buyerOpenId?: string;
}
