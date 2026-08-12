-- Production keeps the sandbox configuration visible and immediately usable:
-- the sandbox adapter is local-only by nature (no real PSP call), so enabling
-- it does not route real payments anywhere; it lets operators verify the full
-- checkout → callback → status flow in any environment.
INSERT INTO commerce_payment_method (
    id, tenant_id, organization_id, method_key, display_name, provider_code,
    status, sort_order, scope, currency_code, country_code, metadata,
    idempotency_key, created_at, updated_at
)
VALUES
    ('bootstrap-payment-method-sandbox-test', '100001', '0', 'sandbox_test', 'Sandbox', 'sandbox', 'active', 900, 'organization', 'CNY', 'CN', '{"bootstrap":true}', 'bootstrap-payment-method-sandbox-test', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
ON CONFLICT DO NOTHING;

INSERT INTO commerce_payment_provider_account (
    id, tenant_id, organization_id, account_no, provider_code, merchant_id,
    account_name, environment, settlement_currency, secret_ref, webhook_secret_ref,
    certificate_ref, capabilities, status, metadata, created_at, updated_at
)
VALUES
    ('bootstrap-payment-provider-sandbox', '100001', '0', 'sandbox-primary', 'sandbox', '054065947793728', 'Sandbox Account', 'development', 'CNY', 'database:primary_secret', NULL, NULL, '{"pay":true,"refund":true,"close":true,"query":true}', 'active', '{"bootstrap":true}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
ON CONFLICT DO NOTHING;

INSERT INTO commerce_payment_channel (
    id, tenant_id, organization_id, channel_no, channel_name, provider_account_id,
    method_id, provider_code, scene_code, currency_code, country_code, status,
    priority, sort_order, metadata, created_at, updated_at
)
VALUES
    ('bootstrap-payment-channel-sandbox-test', '100001', '0', 'sandbox-channel', 'Sandbox', 'bootstrap-payment-provider-sandbox', 'bootstrap-payment-method-sandbox-test', 'sandbox', 'api', 'CNY', 'CN', 'active', 900, 900, '{"bootstrap":true}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
ON CONFLICT DO NOTHING;
