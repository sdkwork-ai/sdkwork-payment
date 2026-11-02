-- sdkwork:migration
-- id: 0002_payment_no_uniqueness
-- engine: postgres
-- module: sdkwork-payment
-- purpose: Reverse 0002 by dropping the business-number uniqueness indexes and
--   the provider-transaction lookup index.
-- reversible: true
-- rollback: down-migration
-- transactional: true
-- lock: lightweight
-- lock_timeout: 2s
-- statement_timeout: 30s

BEGIN;

DROP INDEX IF EXISTS ix_commerce_payment_attempt_provider_transaction;
DROP INDEX IF EXISTS ux_commerce_payment_intent_tenant_intent_no;
DROP INDEX IF EXISTS ux_commerce_refund_tenant_refund_no;

COMMIT;
