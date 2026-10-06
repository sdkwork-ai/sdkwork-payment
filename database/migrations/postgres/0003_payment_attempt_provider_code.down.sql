-- sdkwork:migration
-- id: 0003_payment_attempt_provider_code
-- engine: postgres
-- module: sdkwork-payment
-- purpose: Reverse 0003 by dropping the backfilled provider_code column.
ALTER TABLE commerce_payment_attempt
    DROP COLUMN IF EXISTS provider_code;
