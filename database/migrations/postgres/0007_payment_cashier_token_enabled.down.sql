-- sdkwork:migration
-- id: 0007_payment_cashier_token_enabled_down
-- engine: postgres
-- module: sdkwork-payment
-- purpose: Remove the per-tenant anonymous-cashier enable switch.
-- reversible: true
-- rollback: up-migration
-- transactional: true
-- lock: lightweight
-- lock_timeout: 2s
-- statement_timeout: 30s

BEGIN;

ALTER TABLE commerce_payment_cashier_token_setting
    DROP COLUMN IF EXISTS enabled;

COMMIT;
