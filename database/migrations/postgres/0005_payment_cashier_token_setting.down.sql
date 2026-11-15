-- sdkwork:migration
-- id: 0005_payment_cashier_token_setting_down
-- engine: postgres
-- module: sdkwork-payment
-- purpose: Roll back the per-tenant cashier-token key epoch setting.
-- reversible: false
-- rollback: not-applicable
-- transactional: true
-- lock: lightweight
-- lock_timeout: 2s
-- statement_timeout: 30s

BEGIN;

DROP TABLE IF EXISTS commerce_payment_cashier_token_setting;

COMMIT;
