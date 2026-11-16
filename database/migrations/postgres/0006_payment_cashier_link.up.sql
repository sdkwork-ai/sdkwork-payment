-- sdkwork:migration
-- id: 0006_payment_cashier_link
-- engine: postgres
-- module: sdkwork-payment
-- purpose: Add `commerce_payment_cashier_link`, the server-side short-id mapping
--   for anonymous cashier links. The QR/URL carries only a 128-bit short id; the
--   real signed capability token is stored encrypted here and resolved by the
--   order/IAM services. This keeps the existing `/cashier/{segment}` URL shape
--   while cutting the QR payload from ~460 to ~64 characters.
--   One active link per (tenant, order) keeps QR re-issues idempotent.
-- reversible: true
-- rollback: down-migration
-- transactional: true
-- lock: lightweight
-- lock_timeout: 2s
-- statement_timeout: 30s

BEGIN;

CREATE TABLE IF NOT EXISTS commerce_payment_cashier_link (
    short_id          TEXT PRIMARY KEY,
    tenant_id         TEXT NOT NULL,
    order_id          TEXT NOT NULL,
    token_id          TEXT NOT NULL,
    key_epoch         BIGINT NOT NULL CHECK (key_epoch >= 1),
    token_ciphertext  TEXT NOT NULL,
    status            TEXT NOT NULL DEFAULT 'active'
                      CHECK (status IN ('active', 'revoked', 'expired')),
    expires_at        TIMESTAMPTZ NOT NULL,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS ux_commerce_payment_cashier_link_active_order
    ON commerce_payment_cashier_link (tenant_id, order_id)
    WHERE status = 'active';

CREATE INDEX IF NOT EXISTS idx_commerce_payment_cashier_link_expiry
    ON commerce_payment_cashier_link (expires_at);

COMMIT;
