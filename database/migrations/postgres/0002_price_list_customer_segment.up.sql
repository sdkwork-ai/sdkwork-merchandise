-- sdkwork:migration
-- id: 0002_price_list_customer_segment
-- engine: postgres
-- module: merchandise
-- purpose: Add `customer_segment` to `commerce_price_list`.
--
--   The Cloud Router admin catalog price-list surface
--   (`list_price_lists` / `create_price_list` / `load_price_list`) selects,
--   inserts and updates `customer_segment`, but this baseline did not define the
--   column, so those operations failed with
--   `column "customer_segment" does not exist`.
--
--   The column is nullable with no default so existing rows stay valid, and the
--   index matches the read pattern `(tenant_id, customer_segment, status)`.
-- reversible: true
-- rollback: down-migration
-- transactional: true
-- lock: exclusive
-- lock_timeout: 2s
-- statement_timeout: 30s

BEGIN;

ALTER TABLE commerce_price_list ADD COLUMN IF NOT EXISTS customer_segment TEXT;

CREATE INDEX IF NOT EXISTS idx_commerce_price_list_segment
    ON commerce_price_list (tenant_id, customer_segment, status);

COMMIT;
