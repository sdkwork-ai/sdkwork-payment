-- sdkwork:migration
-- id: 0004_payment_domain_setting
-- engine: postgres
-- module: sdkwork-payment
-- purpose: Add `commerce_payment_domain_setting`, the typed settings store for
--   the admin payment center's domain configuration (支付设置 → 域名设置).
--   It holds the H5 payment domain used to build the order cashier URL / QR
--   code payload and the default H5 return URL for redirect-style PSP products.
--   The callback (notify) domain keeps its own multi-row table
--   `commerce_payment_notify_domain`. IF NOT EXISTS keeps the migration
--   idempotent on databases already created from the updated baseline.
-- reversible: true
-- rollback: down-migration
-- transactional: true
-- lock: lightweight
-- lock_timeout: 2s
-- statement_timeout: 30s

BEGIN;

CREATE TABLE IF NOT EXISTS commerce_payment_domain_setting (
    id                TEXT PRIMARY KEY,
    tenant_id         TEXT NOT NULL,
    organization_id   TEXT NOT NULL DEFAULT '0',
    h5_protocol       TEXT NOT NULL DEFAULT 'https'
                      CHECK (h5_protocol IN ('https', 'http')),
    h5_hostname       TEXT NOT NULL,
    h5_port           INTEGER,
    h5_status         TEXT NOT NULL DEFAULT 'active'
                      CHECK (h5_status IN ('active', 'inactive')),
    version           BIGINT NOT NULL DEFAULT 1,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at        TIMESTAMPTZ NULL,
    CHECK (h5_port IS NULL OR (h5_port >= 1 AND h5_port <= 65535))
);

CREATE UNIQUE INDEX IF NOT EXISTS ux_commerce_payment_domain_setting_scope
    ON commerce_payment_domain_setting (tenant_id, organization_id)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_commerce_payment_domain_setting_lookup
    ON commerce_payment_domain_setting (tenant_id, organization_id, h5_status, id)
    WHERE deleted_at IS NULL;

COMMIT;
