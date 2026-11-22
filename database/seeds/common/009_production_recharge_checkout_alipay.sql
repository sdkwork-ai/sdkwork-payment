-- The order checkout surfaces (membership, points recharge, community join) send
-- the stable `alipay` method key while the payment catalog keeps provider-product
-- methods such as `alipay_qr`. Without this compatibility row every
-- `paymentMethod=alipay` order fails channel selection with
-- "payment method has no eligible channel" (409) even though the Alipay provider
-- account is fully configured — Alipay was unreachable on every client.
--
-- The alias resolves to the in-store QR product (`alipay.trade.precreate`)
-- because desktop and H5 checkout render the returned `qrCodePayload`/`cashierUrl`
-- for the buyer to scan.
INSERT INTO commerce_payment_method (
    id, tenant_id, organization_id, method_key, display_name, provider_code,
    status, sort_order, scope, currency_code, country_code, metadata,
    idempotency_key, created_at, updated_at
)
VALUES
    ('bootstrap-payment-method-recharge-alipay', '100001', '0', 'alipay', 'Alipay', 'alipay', 'active', 205, 'organization', 'CNY', 'CN', '{"bootstrap":true,"checkoutAliasFor":"alipay_qr"}', 'bootstrap-payment-method-recharge-alipay', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
ON CONFLICT DO NOTHING;

INSERT INTO commerce_payment_channel (
    id, tenant_id, organization_id, channel_no, channel_name, provider_account_id,
    method_id, provider_code, scene_code, currency_code, country_code, status,
    priority, sort_order, metadata, created_at, updated_at
)
VALUES
    ('bootstrap-payment-channel-recharge-alipay', '100001', '0', 'recharge-alipay', 'Alipay Recharge', 'bootstrap-payment-provider-alipay', 'bootstrap-payment-method-recharge-alipay', 'alipay', 'web', 'CNY', 'CN', 'active', 205, 205, '{"bootstrap":true,"checkoutAliasFor":"alipay_qr"}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
ON CONFLICT DO NOTHING;
