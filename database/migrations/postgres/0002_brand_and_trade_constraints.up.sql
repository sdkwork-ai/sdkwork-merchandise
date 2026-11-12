-- sdkwork:migration
-- id: 0002_brand_and_trade_constraints
-- engine: postgres
-- module: sdkwork-merchandise
-- purpose: Add the brand table family (commerce_product_brand and its
--   translation) with the SPU brand foreign key, the SKU trade constraint
--   columns (min/max order quantity, step, package unit), and the brand/logo
--   media vocabulary, on deployments initialized before the baseline
--   included them.
-- reversible: false
-- rollback: forward-fix (the brand tables are canonical master data; the
--   constraint drops are idempotent on replay)
-- transactional: true
-- lock: lightweight
-- lock_timeout: 2s
-- statement_timeout: 30s

-- 0002: brand table family + SKU trade constraint columns.
--
-- Companion to the same additions in the baseline DDL. The baseline strategy is
-- `baseline-plus-migrations`: fresh environments materialize the whole baseline,
-- existing environments replay this migration, and the two files must stay
-- statement-for-statement equivalent so a promoted environment cannot diverge
-- from a freshly bootstrapped one.

-- ---------------------------------------------------------------------------
-- commerce_product_brand (+ translation)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS commerce_product_brand (
    id BIGINT NOT NULL,
    uuid UUID NOT NULL DEFAULT gen_random_uuid(),
    tenant_id BIGINT NOT NULL,
    organization_id BIGINT NOT NULL DEFAULT 0,
    brand_no TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    sort_order BIGINT NOT NULL DEFAULT 0,
    version BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by BIGINT,
    updated_by BIGINT,
    deleted_at TIMESTAMPTZ,
    deleted_by BIGINT,
    PRIMARY KEY (id),
    CONSTRAINT uk_commerce_product_brand_uuid UNIQUE (uuid),
    CONSTRAINT ck_commerce_product_brand_name_length
        CHECK (char_length(name) BETWEEN 1 AND 200),
    CONSTRAINT ck_commerce_product_brand_status
        CHECK (status IN ('active', 'inactive'))
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_commerce_product_brand_tenant_no
    ON commerce_product_brand (tenant_id, brand_no)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_commerce_product_brand_tenant_status_sort
    ON commerce_product_brand (tenant_id, organization_id, status, sort_order, id)
    WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS commerce_product_brand_translation (
    id BIGINT NOT NULL,
    tenant_id BIGINT NOT NULL,
    organization_id BIGINT NOT NULL DEFAULT 0,
    brand_id BIGINT NOT NULL,
    locale TEXT NOT NULL,
    field_name TEXT NOT NULL,
    value TEXT NOT NULL,
    version BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (id),
    CONSTRAINT fk_commerce_product_brand_translation_brand
        FOREIGN KEY (brand_id) REFERENCES commerce_product_brand (id) ON DELETE CASCADE,
    CONSTRAINT ck_commerce_product_brand_translation_locale
        CHECK (locale ~ '^[a-z]{2}(-[A-Z][a-z]{3})?(-[A-Z]{2})?$'),
    CONSTRAINT ck_commerce_product_brand_translation_field
        CHECK (field_name IN ('name', 'description')),
    CONSTRAINT ck_commerce_product_brand_translation_value
        CHECK (char_length(value) BETWEEN 1 AND 2000)
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_commerce_product_brand_translation
    ON commerce_product_brand_translation (brand_id, locale, field_name);

CREATE INDEX IF NOT EXISTS idx_commerce_product_brand_translation_lookup
    ON commerce_product_brand_translation (tenant_id, locale, brand_id);

ALTER TABLE commerce_product_spu
    ADD CONSTRAINT fk_commerce_product_spu_brand
        FOREIGN KEY (brand_id) REFERENCES commerce_product_brand (id);

-- ---------------------------------------------------------------------------
-- SKU trade constraint columns
-- ---------------------------------------------------------------------------
ALTER TABLE commerce_product_sku
    ADD COLUMN IF NOT EXISTS min_order_quantity BIGINT NOT NULL DEFAULT 1,
    ADD COLUMN IF NOT EXISTS max_order_quantity BIGINT,
    ADD COLUMN IF NOT EXISTS step_quantity BIGINT NOT NULL DEFAULT 1,
    ADD COLUMN IF NOT EXISTS package_unit TEXT,
    ADD COLUMN IF NOT EXISTS units_per_package BIGINT;

ALTER TABLE commerce_product_sku
    ADD CONSTRAINT ck_commerce_product_sku_min_order_quantity
        CHECK (min_order_quantity >= 1);
ALTER TABLE commerce_product_sku
    ADD CONSTRAINT ck_commerce_product_sku_step_quantity
        CHECK (step_quantity >= 1);
ALTER TABLE commerce_product_sku
    ADD CONSTRAINT ck_commerce_product_sku_max_order_quantity
        CHECK (max_order_quantity IS NULL OR max_order_quantity >= min_order_quantity);
ALTER TABLE commerce_product_sku
    ADD CONSTRAINT ck_commerce_product_sku_units_per_package
        CHECK (units_per_package IS NULL OR units_per_package >= 1);

-- Media vocabulary widens for brand logos; idempotent re-registration by
-- drop-and-recreate is deliberately NOT used because the CHECK carries no
-- independent state. These statements fail on replay, which is the signal the
-- migration ledger exists to deduplicate.
ALTER TABLE commerce_product_media DROP CONSTRAINT IF EXISTS ck_commerce_product_media_owner_type;
ALTER TABLE commerce_product_media
    ADD CONSTRAINT ck_commerce_product_media_owner_type
        CHECK (owner_type IN ('spu', 'sku', 'category', 'attribute_value', 'brand'));
ALTER TABLE commerce_product_media DROP CONSTRAINT IF EXISTS ck_commerce_product_media_role;
ALTER TABLE commerce_product_media
    ADD CONSTRAINT ck_commerce_product_media_role
        CHECK (media_role IN ('main_image', 'gallery_image', 'detail_image', 'sku_image', 'video', 'manual', 'certificate', 'logo'));
