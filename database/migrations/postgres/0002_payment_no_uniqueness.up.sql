-- sdkwork:migration
-- id: 0002_payment_no_uniqueness
-- engine: postgres
-- module: sdkwork-payment
-- purpose: Enforce business-number uniqueness at the database boundary and
--   support provider-transaction lookups. `refund_no` is the webhook
--   resolution key (per-tenant) and `payment_intent_no` is the operator-facing
--   intent number; both previously had no unique constraint, so a collision
--   made refund webhook resolution nondeterministic. The partial index on
--   `provider_transaction_id` serves PSP native-id lookups without a seq scan.
--   The indexes are IF NOT EXISTS idempotent so they also apply on top of the
--   updated baseline on fresh installs.
-- reversible: true
-- rollback: drop the three indexes
-- transactional: true
-- lock: lightweight
-- lock_timeout: 2s
-- statement_timeout: 30s
-- note: creation fails loudly if legacy data already contains duplicate
--   numbers; dedup is a data fix that must precede this migration.

BEGIN;

CREATE UNIQUE INDEX IF NOT EXISTS ux_commerce_refund_tenant_refund_no
    ON commerce_refund (tenant_id, refund_no)
    WHERE deleted_at IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS ux_commerce_payment_intent_tenant_intent_no
    ON commerce_payment_intent (tenant_id, payment_intent_no)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS ix_commerce_payment_attempt_provider_transaction
    ON commerce_payment_attempt (tenant_id, provider_code, provider_transaction_id)
    WHERE provider_transaction_id IS NOT NULL AND deleted_at IS NULL;

COMMIT;
