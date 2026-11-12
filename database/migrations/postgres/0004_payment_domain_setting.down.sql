-- sdkwork:migration
-- id: 0004_payment_domain_setting
-- engine: postgres
-- module: sdkwork-payment
-- purpose: Mark 0004 as non-destructive on reversal. Dropping
--   `commerce_payment_domain_setting` would destroy operator-configured
--   payment domains (and silently re-point generated QR codes at the
--   deployment default), so this down migration deliberately performs no DDL.
-- reversible: true
-- rollback: down-migration
-- transactional: true
-- lock: lightweight
-- lock_timeout: 2s
-- statement_timeout: 30s

BEGIN;

-- Intentionally no-op: commerce_payment_domain_setting and its configuration
-- are retained on reversal.
SELECT 1;

COMMIT;
