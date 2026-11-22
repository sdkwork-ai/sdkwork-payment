-- The merchant trade number (`out_trade_no`) a payment provider echoes back must
-- identify exactly one attempt within a tenant and provider. Without this, a
-- duplicate could be settled or refunded by the wrong channel callback. The
-- previous index omitted `provider_code` and was not unique, so it enforced
-- nothing.
--
-- Concurrent duplicates are unlikely (out_trade_no is generated per attempt), but
-- the index creation fails loudly rather than silently keeping bad data, which is
-- the correct outcome for a reconciliation-breaking defect.
CREATE UNIQUE INDEX IF NOT EXISTS ux_commerce_payment_attempt_provider_trade
    ON commerce_payment_attempt (tenant_id, provider_code, out_trade_no)
    WHERE out_trade_no IS NOT NULL AND deleted_at IS NULL;
