-- sdkwork:migration
-- id: 0006_payment_cashier_token_secret
-- engine: postgres
-- module: sdkwork-payment
-- purpose: Store the anonymous-cashier signing secret in the payment database so
--   the payment center (支付中心 → 支付设置 → 收银台密钥) can generate and rotate
--   it. The secret is sealed with the deployment key-encryption key
--   (`SDKWORK_COMMERCE_CASHIER_TOKEN_SECRET`) before it is written, so a database
--   dump without the deployment KEK cannot mint cashier tokens. A short-lived
--   previous secret keeps in-flight links valid across a rotation.
-- reversible: true
-- rollback: down-migration
-- transactional: true
-- lock: lightweight
-- lock_timeout: 2s
-- statement_timeout: 30s

BEGIN;

ALTER TABLE commerce_payment_cashier_token_setting
    ADD COLUMN IF NOT EXISTS secret_ciphertext TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS secret_encryption_key_id TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS secret_encryption_algorithm TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS secret_fingerprint_sha256 TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS secret_updated_at TIMESTAMPTZ NULL,
    ADD COLUMN IF NOT EXISTS previous_secret_ciphertext TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS previous_secret_encryption_key_id TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS previous_secret_encryption_algorithm TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS previous_secret_expires_at TIMESTAMPTZ NULL;

COMMIT;
