-- Configurable checkout product resolution: which provider product serves a buyer
-- context (scene x app architecture x os x payment method). Replaces the
-- compile-time mapping so a new platform needs configuration, not a release.
CREATE TABLE IF NOT EXISTS commerce_payment_checkout_scene_policy (
    id                 TEXT PRIMARY KEY,
    tenant_id          TEXT NOT NULL,
    organization_id    TEXT NOT NULL DEFAULT '0',
    scene              TEXT NOT NULL,
    app_architecture   TEXT NOT NULL DEFAULT 'any',
    os_family          TEXT NOT NULL DEFAULT 'any',
    payment_method     TEXT NOT NULL,
    product_code       TEXT NOT NULL,
    status             TEXT NOT NULL DEFAULT 'active'
                       CHECK (status IN ('active', 'inactive')),
    priority           INTEGER NOT NULL DEFAULT 100,
    metadata           JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at         TIMESTAMPTZ NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS ux_commerce_payment_checkout_scene_policy
    ON commerce_payment_checkout_scene_policy
       (tenant_id, organization_id, LOWER(scene), LOWER(app_architecture), LOWER(os_family), LOWER(payment_method))
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_commerce_payment_checkout_scene_policy_lookup
    ON commerce_payment_checkout_scene_policy
       (tenant_id, organization_id, payment_method, status)
    WHERE deleted_at IS NULL;
