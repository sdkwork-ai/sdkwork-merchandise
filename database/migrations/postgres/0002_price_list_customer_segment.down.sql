-- sdkwork:migration
-- id: 0002_price_list_customer_segment
-- engine: postgres
-- module: merchandise
-- purpose: Roll back the `customer_segment` column on `commerce_price_list`.
-- reversible: true
-- rollback: down-migration
-- transactional: true
-- lock: exclusive
-- lock_timeout: 2s
-- statement_timeout: 30s

BEGIN;

DROP INDEX IF EXISTS idx_commerce_price_list_segment;
ALTER TABLE commerce_price_list DROP COLUMN IF EXISTS customer_segment;

COMMIT;
