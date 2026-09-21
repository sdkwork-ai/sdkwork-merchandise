-- SDKWork merchandise baseline (commerce product master-data tables)
-- The commerce_shop / commerce_shop_application / commerce_shop_status_event
-- tables are owned by sdkwork-shop (shop domain) and must not be duplicated
-- here; see ../sdkwork-shop/database/ddl/baseline/postgres.
-- Physical ownership per the catalog contract: sdkwork-catalog declares
-- commerce_product_spu/sku as reference-only with write_owner = commerce
-- platform bootstrap; the merchandise module is the physical owner of the
-- commerce_ product tables so order/payment/catalog flows can reference them
-- in every deployment.
-- Column shape mirrors the merchandise repository SQL
-- (crates/sdkwork-merchandise-repository-sqlx: postgres_catalog.rs and
-- single_sku_merchandise/postgres.rs): text ids/amounts, text timestamps
-- (read back through string_cell), plus sales_status so order module queries
-- resolve sku sales state through the same COALESCE fallback used elsewhere.
CREATE TABLE IF NOT EXISTS commerce_product_spu (
    id TEXT NOT NULL PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    organization_id TEXT NOT NULL DEFAULT '0',
    spu_no TEXT NOT NULL,
    name TEXT,
    title TEXT,
    subtitle TEXT,
    description TEXT,
    product_type TEXT,
    category_id TEXT,
    status TEXT NOT NULL DEFAULT 'draft',
    visible_surfaces TEXT,
    published_at TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS commerce_product_sku (
    id TEXT NOT NULL PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    organization_id TEXT NOT NULL DEFAULT '0',
    spu_id TEXT NOT NULL,
    sku_no TEXT NOT NULL,
    name TEXT,
    title TEXT,
    price_amount TEXT,
    original_price_amount TEXT,
    currency_code TEXT,
    fulfillment_type TEXT,
    inventory_tracking TEXT,
    status TEXT NOT NULL DEFAULT 'draft',
    sales_status TEXT NOT NULL DEFAULT 'active',
    published_at TEXT,
    spec_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_commerce_product_sku_spu
    ON commerce_product_sku (tenant_id, spu_id, status);

CREATE INDEX IF NOT EXISTS idx_commerce_product_sku_tenant_status
    ON commerce_product_sku (tenant_id, organization_id, status, sales_status);

-- Commerce product master-data tables (categories/attributes/price lists).
-- Column shape mirrors the merchandise repository SQL
-- (crates/sdkwork-merchandise-repository-sqlx/src/postgres_catalog.rs):
-- text ids/amounts, text timestamps read back through string_cell.
CREATE TABLE IF NOT EXISTS commerce_product_category (
    id TEXT NOT NULL PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    organization_id TEXT NOT NULL DEFAULT '0',
    category_no TEXT NOT NULL,
    parent_id TEXT,
    path TEXT,
    level_no BIGINT NOT NULL DEFAULT 0,
    name TEXT NOT NULL,
    sort_order BIGINT NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS commerce_product_attribute (
    id TEXT NOT NULL PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    organization_id TEXT NOT NULL DEFAULT '0',
    attribute_no TEXT NOT NULL,
    name TEXT NOT NULL,
    value_type TEXT NOT NULL DEFAULT 'enum',
    scope TEXT NOT NULL DEFAULT 'product',
    status TEXT NOT NULL DEFAULT 'active',
    sort_order BIGINT NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS commerce_product_attribute_value (
    id TEXT NOT NULL PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    organization_id TEXT NOT NULL DEFAULT '0',
    attribute_id TEXT NOT NULL,
    value_code TEXT NOT NULL,
    display_value TEXT NOT NULL,
    sort_order BIGINT NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT ux_commerce_product_attribute_value
        UNIQUE (tenant_id, attribute_id, value_code)
);

CREATE TABLE IF NOT EXISTS commerce_price_list (
    id TEXT NOT NULL PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    organization_id TEXT NOT NULL DEFAULT '0',
    price_list_no TEXT NOT NULL,
    currency_code TEXT NOT NULL DEFAULT 'CNY',
    market_code TEXT,
    customer_segment TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    starts_at TEXT,
    ends_at TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_commerce_price_list_segment
    ON commerce_price_list (tenant_id, customer_segment, status);
