-- Append-only audit trail for privileged payment-admin changes (credential
-- create/rotate/revoke and account mutations). Records who/what/which request and
-- never the secret material.
CREATE TABLE IF NOT EXISTS commerce_payment_admin_audit_log (
    id                TEXT PRIMARY KEY,
    tenant_id         TEXT NOT NULL,
    organization_id   TEXT NOT NULL DEFAULT '0',
    actor_user_id     TEXT NOT NULL,
    action            TEXT NOT NULL,
    resource_type     TEXT NOT NULL,
    resource_id       TEXT NOT NULL,
    request_no        TEXT,
    detail            JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_commerce_payment_admin_audit_tenant_time
    ON commerce_payment_admin_audit_log (tenant_id, created_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_commerce_payment_admin_audit_resource
    ON commerce_payment_admin_audit_log (tenant_id, resource_type, resource_id, created_at DESC);
