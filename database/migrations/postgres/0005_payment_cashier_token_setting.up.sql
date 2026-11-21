-- sdkwork:migration
-- id: 0005_payment_cashier_token_setting
-- engine: postgres
-- module: sdkwork-payment
-- purpose: Add `commerce_payment_cashier_token_setting`, the per-tenant key epoch
--   for anonymous-cashier capability-token rotation (支付中心 → 支付设置 →
--   收银台密钥). Only the public epoch counter and rotation metadata are stored;
--   the root signing secret stays a deployment secret, so neither a database
--   dump nor a read-only database account can mint cashier tokens.
--   Resolution: tenant row -> platform '0' row -> epoch 1 (legacy derivation).
-- reversible: true
-- rollback: down-migration
-- transactional: true
-- lock: lightweight
-- lock_timeout: 2s
-- statement_timeout: 30s

BEGIN;

CREATE TABLE IF NOT EXISTS commerce_payment_cashier_token_setting (
    tenant_id   TEXT PRIMARY KEY,
    key_epoch   BIGINT NOT NULL DEFAULT 1 CHECK (key_epoch >= 1),
    rotated_at  TIMESTAMPTZ NULL,
    rotated_by  TEXT NOT NULL DEFAULT '',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

COMMIT;
