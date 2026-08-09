-- sdkwork:migration
-- id: 0001_organization_id_not_null
-- engine: postgres
-- module: sdkwork-payment
-- purpose: Enforce organization_id NOT NULL DEFAULT on all tables in the
--   consolidated baseline. NULL rows (pre-standard data anomalies) are
--   backfilled with the platform sentinel before NOT NULL is set, and
--   NOT NULL columns without an explicit default receive the sentinel
--   default, keeping existing deployments consistent with fresh baseline
--   installs.
-- reversible: false
-- rollback: forward-fix (sentinel backfill is the canonical fix; NULL
--   organization rows are data anomalies)
-- transactional: true
-- lock: lightweight
-- lock_timeout: 2s
-- statement_timeout: 30s

BEGIN;

UPDATE commerce_payment_method SET organization_id = '0' WHERE organization_id IS NULL;
ALTER TABLE commerce_payment_method ALTER COLUMN organization_id SET DEFAULT '0';
ALTER TABLE commerce_payment_method ALTER COLUMN organization_id SET NOT NULL;

UPDATE commerce_payment_intent SET organization_id = '0' WHERE organization_id IS NULL;
ALTER TABLE commerce_payment_intent ALTER COLUMN organization_id SET DEFAULT '0';
ALTER TABLE commerce_payment_intent ALTER COLUMN organization_id SET NOT NULL;

UPDATE commerce_payment_attempt SET organization_id = '0' WHERE organization_id IS NULL;
ALTER TABLE commerce_payment_attempt ALTER COLUMN organization_id SET DEFAULT '0';
ALTER TABLE commerce_payment_attempt ALTER COLUMN organization_id SET NOT NULL;

UPDATE commerce_refund SET organization_id = '0' WHERE organization_id IS NULL;
ALTER TABLE commerce_refund ALTER COLUMN organization_id SET DEFAULT '0';
ALTER TABLE commerce_refund ALTER COLUMN organization_id SET NOT NULL;

UPDATE commerce_refund_event SET organization_id = '0' WHERE organization_id IS NULL;
ALTER TABLE commerce_refund_event ALTER COLUMN organization_id SET DEFAULT '0';
ALTER TABLE commerce_refund_event ALTER COLUMN organization_id SET NOT NULL;

UPDATE commerce_payment_channel SET organization_id = '0' WHERE organization_id IS NULL;
ALTER TABLE commerce_payment_channel ALTER COLUMN organization_id SET DEFAULT '0';
ALTER TABLE commerce_payment_channel ALTER COLUMN organization_id SET NOT NULL;

UPDATE commerce_payment_provider_account SET organization_id = '0' WHERE organization_id IS NULL;
ALTER TABLE commerce_payment_provider_account ALTER COLUMN organization_id SET DEFAULT '0';
ALTER TABLE commerce_payment_provider_account ALTER COLUMN organization_id SET NOT NULL;

UPDATE commerce_payment_provider_credential SET organization_id = '0' WHERE organization_id IS NULL;
ALTER TABLE commerce_payment_provider_credential ALTER COLUMN organization_id SET DEFAULT '0';
ALTER TABLE commerce_payment_provider_credential ALTER COLUMN organization_id SET NOT NULL;

UPDATE commerce_payment_sub_merchant SET organization_id = '0' WHERE organization_id IS NULL;
ALTER TABLE commerce_payment_sub_merchant ALTER COLUMN organization_id SET DEFAULT '0';
ALTER TABLE commerce_payment_sub_merchant ALTER COLUMN organization_id SET NOT NULL;

UPDATE commerce_payment_certificate SET organization_id = '0' WHERE organization_id IS NULL;
ALTER TABLE commerce_payment_certificate ALTER COLUMN organization_id SET DEFAULT '0';
ALTER TABLE commerce_payment_certificate ALTER COLUMN organization_id SET NOT NULL;

UPDATE commerce_payment_route_rule SET organization_id = '0' WHERE organization_id IS NULL;
ALTER TABLE commerce_payment_route_rule ALTER COLUMN organization_id SET DEFAULT '0';
ALTER TABLE commerce_payment_route_rule ALTER COLUMN organization_id SET NOT NULL;

UPDATE commerce_payment_webhook_event SET organization_id = '0' WHERE organization_id IS NULL;
ALTER TABLE commerce_payment_webhook_event ALTER COLUMN organization_id SET DEFAULT '0';
ALTER TABLE commerce_payment_webhook_event ALTER COLUMN organization_id SET NOT NULL;

UPDATE commerce_payment_webhook_delivery SET organization_id = '0' WHERE organization_id IS NULL;
ALTER TABLE commerce_payment_webhook_delivery ALTER COLUMN organization_id SET DEFAULT '0';
ALTER TABLE commerce_payment_webhook_delivery ALTER COLUMN organization_id SET NOT NULL;

UPDATE commerce_payment_reconciliation_run SET organization_id = '0' WHERE organization_id IS NULL;
ALTER TABLE commerce_payment_reconciliation_run ALTER COLUMN organization_id SET DEFAULT '0';
ALTER TABLE commerce_payment_reconciliation_run ALTER COLUMN organization_id SET NOT NULL;

UPDATE commerce_payment_provider SET organization_id = '0' WHERE organization_id IS NULL;
ALTER TABLE commerce_payment_provider ALTER COLUMN organization_id SET DEFAULT '0';
ALTER TABLE commerce_payment_provider ALTER COLUMN organization_id SET NOT NULL;

COMMIT;
