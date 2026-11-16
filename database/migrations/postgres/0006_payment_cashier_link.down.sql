-- sdkwork:migration
-- id: 0006_payment_cashier_link_down
-- engine: postgres
-- module: sdkwork-payment
-- purpose: Roll back the anonymous cashier short-link mapping.
-- reversible: false
-- rollback: not-applicable
-- transactional: true
-- lock: lightweight
-- lock_timeout: 2s
-- statement_timeout: 30s

BEGIN;

DROP TABLE IF EXISTS commerce_payment_cashier_link;

COMMIT;
