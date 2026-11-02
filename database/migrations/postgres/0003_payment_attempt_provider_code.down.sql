-- sdkwork:migration
-- id: 0003_payment_attempt_provider_code
-- engine: postgres
-- module: sdkwork-payment
-- purpose: Mark 0003 as non-destructive on reversal. Dropping the backfilled
--   provider_code column would destroy provider attribution for existing
--   payment attempts, so this down migration deliberately performs no DDL.
-- reversible: true
-- rollback: down-migration
-- transactional: true
-- lock: lightweight
-- lock_timeout: 2s
-- statement_timeout: 30s

BEGIN;

-- Intentionally no-op: provider_code is retained.
SELECT 1;

COMMIT;
