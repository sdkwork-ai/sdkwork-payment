-- sdkwork:migration
-- id: 0007_payment_cashier_token_enabled
-- engine: postgres
-- module: sdkwork-payment
-- purpose: Add the per-tenant anonymous-cashier enable switch. Disabling stops
--   new capability-token minting immediately; already-issued tokens remain valid
--   only until their own expiry because verification reads the same setting.
-- reversible: true
-- rollback: down-migration
-- transactional: true
-- lock: lightweight
-- lock_timeout: 2s
-- statement_timeout: 30s

BEGIN;

ALTER TABLE commerce_payment_cashier_token_setting
    ADD COLUMN IF NOT EXISTS enabled BOOLEAN NOT NULL DEFAULT TRUE;

COMMIT;
