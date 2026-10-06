-- sdkwork:migration
-- id: 0003_payment_attempt_provider_code
-- engine: postgres
-- module: sdkwork-payment
-- purpose: Backfill the `provider_code` column on `commerce_payment_attempt`
--   for long-lived databases created before the column existed in the
--   baseline. The reconciliation/compensation claim query selects
--   `provider_code` from this table, so a database created from an older
--   baseline fails every compensation pass with "column provider_code does
--   not exist". The column is nullable on backfill: existing rows describe
--   attempts whose provider channel is no longer recoverable, and new rows
--   insert it explicitly (the baseline enforces NOT NULL on fresh installs).
--   The IF NOT EXISTS form is idempotent so it also applies on top of the
--   updated baseline on fresh installs.
ALTER TABLE commerce_payment_attempt
    ADD COLUMN IF NOT EXISTS provider_code TEXT;
