-- sdkwork:migration
-- id: 0006_payment_cashier_token_secret_down
-- engine: postgres
-- module: sdkwork-payment
-- purpose: Remove the encrypted cashier-token signing secret columns.
-- reversible: true
-- rollback: up-migration
-- transactional: true
-- lock: lightweight
-- lock_timeout: 2s
-- statement_timeout: 30s

BEGIN;

ALTER TABLE commerce_payment_cashier_token_setting
    DROP COLUMN IF EXISTS secret_ciphertext,
    DROP COLUMN IF EXISTS secret_encryption_key_id,
    DROP COLUMN IF EXISTS secret_encryption_algorithm,
    DROP COLUMN IF EXISTS secret_fingerprint_sha256,
    DROP COLUMN IF EXISTS secret_updated_at,
    DROP COLUMN IF EXISTS previous_secret_ciphertext,
    DROP COLUMN IF EXISTS previous_secret_encryption_key_id,
    DROP COLUMN IF EXISTS previous_secret_encryption_algorithm,
    DROP COLUMN IF EXISTS previous_secret_expires_at;

COMMIT;
