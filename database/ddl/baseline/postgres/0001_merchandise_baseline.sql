-- ============================================================================
-- SDKWork merchandise baseline (commerce product master data)
-- ============================================================================
-- Ownership
--   sdkwork-merchandise is the physical owner of the `commerce_product_*`,
--   `commerce_price_list*`, and `commerce_currency` table families. The
--   `commerce_shop*`, `commerce_cart*`, `commerce_user_address`, and
--   `commerce_inventory_*` families are owned by sdkwork-shop,
--   sdkwork-catalog, and sdkwork-inventory respectively and MUST NOT be
--   declared, read, or written here.
--
-- Spec anchors
--   DATABASE_SPEC.md  section 6.1 (BIGINT id), 6.2 (uuid), 6.4 (reserved seed
--     ids), 6.4.1 (translation tables), 6.5/6.6 (audit + lifecycle),
--     6.10 (tenant/organization subjects), 7 (naming), 8.2 (native PG types),
--     10 (indexes), 11 (constraints), 12 (enums), 13 (JSON),
--     14 (money/precision), 31 (minimum compliance example);
--     rules DB052, DB094, DB095, DB096, DB098, DB099.
--   API_SPEC.md section 13.6 (int64 wire contract: DB BIGINT <-> wire string).
--   MEDIA_RESOURCE_SPEC.md section 6 (catalog media profile).
--
-- Design intent (industry-standard product master data)
--   * SPU is product identity; SKU is the sellable unit identified by a
--     combination of `sales` attribute values. Identity is enforced by
--     `variant_signature` rather than by free-form JSON.
--   * Attribute roles (key / sales / parameter) are decided by the category
--     binding, not by the attribute itself: "colour" is a sales axis in
--     apparel and a plain parameter in furniture. Attribute rows therefore
--     carry no `scope` column.
--   * Category attribute templates are MATERIALIZED into child categories on
--     creation (`source_category_id` records provenance). Resolution is a
--     single-table lookup, never a recursive path walk.
--   * Money is stored as exact integer minor units plus a declared currency;
--     `minor_unit_exponent` comes from `commerce_currency`, never from a
--     per-table divisor such as `/ 100` (DATABASE_SPEC section 14).
--   * `*_no` columns are the stable external/business keys used by other
--     services; the BIGINT `id` is repository-local (API_SPEC section 13.6).
--   * Localized display text lives in `*_translation` tables so every locale
--     can coexist. Base tables keep machine fields plus the default-locale
--     display value.
--
-- Application is pre-launch: this is the single authoritative baseline.
-- Post-GA shape changes belong in database/migrations/postgres.
-- ============================================================================

-- ---------------------------------------------------------------------------
-- commerce_currency
-- ---------------------------------------------------------------------------
-- Global (cross-tenant) reference table. `tenant_id` is intentionally absent:
-- ISO 4217 is a platform-wide standard, not tenant data (DATABASE_SPEC 6.10).
-- `minor_unit_exponent` is the single authority for a currency's scale, so no
-- downstream layer may hardcode a divisor (DB095).
CREATE TABLE IF NOT EXISTS commerce_currency (
    id BIGINT NOT NULL,
    code TEXT NOT NULL,
    minor_unit_exponent SMALLINT NOT NULL,
    rounding_mode TEXT NOT NULL DEFAULT 'half_up',
    display_symbol TEXT,
    display_name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    sort_order BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (id),
    CONSTRAINT uk_commerce_currency_code UNIQUE (code),
    CONSTRAINT ck_commerce_currency_code_shape
        CHECK (code ~ '^[A-Z0-9]{3,8}$'),
    CONSTRAINT ck_commerce_currency_minor_unit_exponent
        CHECK (minor_unit_exponent BETWEEN 0 AND 8),
    CONSTRAINT ck_commerce_currency_rounding_mode
        CHECK (rounding_mode IN ('half_up', 'half_down', 'half_even', 'floor', 'ceiling', 'truncate')),
    CONSTRAINT ck_commerce_currency_status
        CHECK (status IN ('active', 'inactive'))
);

CREATE INDEX IF NOT EXISTS idx_commerce_currency_status_sort
    ON commerce_currency (status, sort_order, id);

-- ---------------------------------------------------------------------------
-- commerce_product_category
-- ---------------------------------------------------------------------------
-- Dual-tree model: `parent_id` is the merchandising (front-of-site) tree and
-- `back_parent_id` is the back-office (operational) tree. Both are optional so
-- a category can appear in either or both trees.
CREATE TABLE IF NOT EXISTS commerce_product_category (
    id BIGINT NOT NULL,
    uuid UUID NOT NULL DEFAULT gen_random_uuid(),
    tenant_id BIGINT NOT NULL,
    organization_id BIGINT NOT NULL DEFAULT 0,
    category_no TEXT NOT NULL,
    parent_id BIGINT,
    back_parent_id BIGINT,
    path TEXT NOT NULL DEFAULT '/',
    depth SMALLINT NOT NULL DEFAULT 0,
    name TEXT NOT NULL,
    is_leaf BOOLEAN NOT NULL DEFAULT TRUE,
    sort_order BIGINT NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'active',
    version BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by BIGINT,
    updated_by BIGINT,
    deleted_at TIMESTAMPTZ,
    deleted_by BIGINT,
    PRIMARY KEY (id),
    CONSTRAINT uk_commerce_product_category_uuid UNIQUE (uuid),
    CONSTRAINT fk_commerce_product_category_parent
        FOREIGN KEY (parent_id) REFERENCES commerce_product_category (id),
    CONSTRAINT fk_commerce_product_category_back_parent
        FOREIGN KEY (back_parent_id) REFERENCES commerce_product_category (id),
    CONSTRAINT ck_commerce_product_category_name_length
        CHECK (char_length(name) BETWEEN 1 AND 200),
    CONSTRAINT ck_commerce_product_category_depth
        CHECK (depth >= 0),
    CONSTRAINT ck_commerce_product_category_path_shape
        CHECK (path ~ '^/([0-9]+/)*$'),
    CONSTRAINT ck_commerce_product_category_self_parent
        CHECK (parent_id IS NULL OR parent_id <> id),
    CONSTRAINT ck_commerce_product_category_self_back_parent
        CHECK (back_parent_id IS NULL OR back_parent_id <> id),
    CONSTRAINT ck_commerce_product_category_status
        CHECK (status IN ('active', 'inactive'))
);

-- Business key: a category number is unique per tenant while live.
CREATE UNIQUE INDEX IF NOT EXISTS uk_commerce_product_category_tenant_no
    ON commerce_product_category (tenant_id, category_no)
    WHERE deleted_at IS NULL;

-- Front-of-site children listing and keyset pagination.
CREATE INDEX IF NOT EXISTS idx_commerce_product_category_tree
    ON commerce_product_category (tenant_id, organization_id, parent_id, sort_order, id)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_commerce_product_category_back_tree
    ON commerce_product_category (tenant_id, organization_id, back_parent_id, sort_order, id)
    WHERE deleted_at IS NULL;

-- Subtree scans: path LIKE '/1/12/%'.
CREATE INDEX IF NOT EXISTS idx_commerce_product_category_path
    ON commerce_product_category (tenant_id, path)
    WHERE deleted_at IS NULL;

-- Foreign key columns are indexed (DATABASE_SPEC section 11).
CREATE INDEX IF NOT EXISTS idx_commerce_product_category_parent
    ON commerce_product_category (parent_id);
CREATE INDEX IF NOT EXISTS idx_commerce_product_category_back_parent
    ON commerce_product_category (back_parent_id);

CREATE TABLE IF NOT EXISTS commerce_product_category_translation (
    id BIGINT NOT NULL,
    tenant_id BIGINT NOT NULL,
    organization_id BIGINT NOT NULL DEFAULT 0,
    category_id BIGINT NOT NULL,
    locale TEXT NOT NULL,
    field_name TEXT NOT NULL,
    value TEXT NOT NULL,
    version BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (id),
    CONSTRAINT fk_commerce_product_category_translation_category
        FOREIGN KEY (category_id) REFERENCES commerce_product_category (id) ON DELETE CASCADE,
    CONSTRAINT ck_commerce_product_category_translation_locale
        CHECK (locale ~ '^[a-z]{2}(-[A-Z][a-z]{3})?(-[A-Z]{2})?$'),
    CONSTRAINT ck_commerce_product_category_translation_field
        CHECK (field_name IN ('name', 'description', 'meta_title', 'meta_description')),
    CONSTRAINT ck_commerce_product_category_translation_value
        CHECK (char_length(value) BETWEEN 1 AND 2000)
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_commerce_product_category_translation
    ON commerce_product_category_translation (category_id, locale, field_name);

CREATE INDEX IF NOT EXISTS idx_commerce_product_category_translation_lookup
    ON commerce_product_category_translation (tenant_id, locale, category_id);

-- ---------------------------------------------------------------------------
-- commerce_product_attribute (+ values)
-- ---------------------------------------------------------------------------
-- The attribute dictionary holds only machine properties. Its ROLE in a given
-- category (key / sales / parameter) lives in commerce_product_category_attribute.
CREATE TABLE IF NOT EXISTS commerce_product_attribute (
    id BIGINT NOT NULL,
    uuid UUID NOT NULL DEFAULT gen_random_uuid(),
    tenant_id BIGINT NOT NULL,
    organization_id BIGINT NOT NULL DEFAULT 0,
    attribute_no TEXT NOT NULL,
    name TEXT NOT NULL,
    value_type TEXT NOT NULL DEFAULT 'enum',
    input_hint TEXT NOT NULL DEFAULT 'select',
    is_multi_value BOOLEAN NOT NULL DEFAULT FALSE,
    unit_symbol TEXT,
    sort_order BIGINT NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'active',
    version BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by BIGINT,
    updated_by BIGINT,
    deleted_at TIMESTAMPTZ,
    deleted_by BIGINT,
    PRIMARY KEY (id),
    CONSTRAINT uk_commerce_product_attribute_uuid UNIQUE (uuid),
    CONSTRAINT ck_commerce_product_attribute_name_length
        CHECK (char_length(name) BETWEEN 1 AND 100),
    CONSTRAINT ck_commerce_product_attribute_value_type
        CHECK (value_type IN ('enum', 'text', 'number', 'bool', 'date')),
    CONSTRAINT ck_commerce_product_attribute_input_hint
        CHECK (input_hint IN ('select', 'multi_select', 'text', 'number', 'toggle', 'date')),
    CONSTRAINT ck_commerce_product_attribute_status
        CHECK (status IN ('active', 'inactive')),
    -- Only enumerations can be bound as a SKU sales axis with discrete values.
    CONSTRAINT ck_commerce_product_attribute_multi_value_needs_enum
        CHECK (NOT is_multi_value OR value_type = 'enum')
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_commerce_product_attribute_tenant_no
    ON commerce_product_attribute (tenant_id, attribute_no)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_commerce_product_attribute_tenant_status_sort
    ON commerce_product_attribute (tenant_id, organization_id, status, sort_order, id)
    WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS commerce_product_attribute_translation (
    id BIGINT NOT NULL,
    tenant_id BIGINT NOT NULL,
    organization_id BIGINT NOT NULL DEFAULT 0,
    attribute_id BIGINT NOT NULL,
    locale TEXT NOT NULL,
    field_name TEXT NOT NULL,
    value TEXT NOT NULL,
    version BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (id),
    CONSTRAINT fk_commerce_product_attribute_translation_attribute
        FOREIGN KEY (attribute_id) REFERENCES commerce_product_attribute (id) ON DELETE CASCADE,
    CONSTRAINT ck_commerce_product_attribute_translation_locale
        CHECK (locale ~ '^[a-z]{2}(-[A-Z][a-z]{3})?(-[A-Z]{2})?$'),
    CONSTRAINT ck_commerce_product_attribute_translation_field
        CHECK (field_name IN ('name', 'unit_symbol', 'help_text')),
    CONSTRAINT ck_commerce_product_attribute_translation_value
        CHECK (char_length(value) BETWEEN 1 AND 500)
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_commerce_product_attribute_translation
    ON commerce_product_attribute_translation (attribute_id, locale, field_name);

CREATE TABLE IF NOT EXISTS commerce_product_attribute_value (
    id BIGINT NOT NULL,
    uuid UUID NOT NULL DEFAULT gen_random_uuid(),
    tenant_id BIGINT NOT NULL,
    organization_id BIGINT NOT NULL DEFAULT 0,
    attribute_id BIGINT NOT NULL,
    value_code TEXT NOT NULL,
    display_value TEXT NOT NULL,
    color_hex TEXT,
    media_resource_id BIGINT,
    sort_order BIGINT NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'active',
    version BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ,
    deleted_by BIGINT,
    PRIMARY KEY (id),
    CONSTRAINT uk_commerce_product_attribute_value_uuid UNIQUE (uuid),
    CONSTRAINT fk_commerce_product_attribute_value_attribute
        FOREIGN KEY (attribute_id) REFERENCES commerce_product_attribute (id),
    CONSTRAINT ck_commerce_product_attribute_value_display_length
        CHECK (char_length(display_value) BETWEEN 1 AND 200),
    CONSTRAINT ck_commerce_product_attribute_value_color_hex
        CHECK (color_hex IS NULL OR color_hex ~ '^#[0-9A-Fa-f]{6}$'),
    CONSTRAINT ck_commerce_product_attribute_value_status
        CHECK (status IN ('active', 'inactive'))
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_commerce_product_attribute_value_tenant_attr_code
    ON commerce_product_attribute_value (tenant_id, attribute_id, value_code)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_commerce_product_attribute_value_attribute_sort
    ON commerce_product_attribute_value (tenant_id, attribute_id, status, sort_order, id)
    WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS commerce_product_attribute_value_translation (
    id BIGINT NOT NULL,
    tenant_id BIGINT NOT NULL,
    organization_id BIGINT NOT NULL DEFAULT 0,
    attribute_value_id BIGINT NOT NULL,
    locale TEXT NOT NULL,
    field_name TEXT NOT NULL,
    value TEXT NOT NULL,
    version BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (id),
    CONSTRAINT fk_commerce_product_attribute_value_translation_value
        FOREIGN KEY (attribute_value_id) REFERENCES commerce_product_attribute_value (id) ON DELETE CASCADE,
    CONSTRAINT ck_commerce_product_attribute_value_translation_locale
        CHECK (locale ~ '^[a-z]{2}(-[A-Z][a-z]{3})?(-[A-Z]{2})?$'),
    CONSTRAINT ck_commerce_product_attribute_value_translation_field
        CHECK (field_name IN ('display_value', 'help_text')),
    CONSTRAINT ck_commerce_product_attribute_value_translation_value
        CHECK (char_length(value) BETWEEN 1 AND 200)
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_commerce_product_attribute_value_translation
    ON commerce_product_attribute_value_translation (attribute_value_id, locale, field_name);

-- ---------------------------------------------------------------------------
-- commerce_product_category_attribute  (the category attribute template)
-- ---------------------------------------------------------------------------
-- This table is where an attribute's ROLE in a category is decided. The same
-- attribute can be `sales` in one category and `parameter` in another, which is
-- why `attribute_role` is not stored on commerce_product_attribute.
--
-- Templates are materialized: creating a child category copies the parent's
-- bindings and records `source_category_id`. Editing a copied row is how a child
-- overrides the inherited role. Reads therefore never walk the tree.
CREATE TABLE IF NOT EXISTS commerce_product_category_attribute (
    id BIGINT NOT NULL,
    uuid UUID NOT NULL DEFAULT gen_random_uuid(),
    tenant_id BIGINT NOT NULL,
    organization_id BIGINT NOT NULL DEFAULT 0,
    category_id BIGINT NOT NULL,
    attribute_id BIGINT NOT NULL,
    attribute_role TEXT NOT NULL,
    source_category_id BIGINT,
    is_required BOOLEAN NOT NULL DEFAULT FALSE,
    is_searchable BOOLEAN NOT NULL DEFAULT FALSE,
    is_filterable BOOLEAN NOT NULL DEFAULT FALSE,
    is_comparable BOOLEAN NOT NULL DEFAULT FALSE,
    sort_order BIGINT NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'active',
    version BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by BIGINT,
    updated_by BIGINT,
    deleted_at TIMESTAMPTZ,
    deleted_by BIGINT,
    PRIMARY KEY (id),
    CONSTRAINT uk_commerce_product_category_attribute_uuid UNIQUE (uuid),
    CONSTRAINT fk_commerce_product_category_attribute_category
        FOREIGN KEY (category_id) REFERENCES commerce_product_category (id) ON DELETE CASCADE,
    CONSTRAINT fk_commerce_product_category_attribute_attribute
        FOREIGN KEY (attribute_id) REFERENCES commerce_product_attribute (id),
    CONSTRAINT fk_commerce_product_category_attribute_source
        FOREIGN KEY (source_category_id) REFERENCES commerce_product_category (id),
    CONSTRAINT ck_commerce_product_category_attribute_role
        CHECK (attribute_role IN ('key', 'sales', 'parameter')),
    CONSTRAINT ck_commerce_product_category_attribute_status
        CHECK (status IN ('active', 'inactive')),
    CONSTRAINT ck_commerce_product_category_attribute_source_not_self
        CHECK (source_category_id IS NULL OR source_category_id <> category_id)
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_commerce_product_category_attribute_binding
    ON commerce_product_category_attribute (tenant_id, category_id, attribute_id)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_commerce_product_category_attribute_role
    ON commerce_product_category_attribute (tenant_id, category_id, attribute_role, sort_order, id)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_commerce_product_category_attribute_attribute
    ON commerce_product_category_attribute (attribute_id);

-- ---------------------------------------------------------------------------
-- commerce_product_spu
-- ---------------------------------------------------------------------------
-- Fixed-semantics fields (model_no, barcode, weight, tax class) are columns,
-- not dynamic attributes, so they are queryable and constrainable
-- (DATABASE_SPEC section 13: JSON must not carry core/filter fields).
CREATE TABLE IF NOT EXISTS commerce_product_spu (
    id BIGINT NOT NULL,
    uuid UUID NOT NULL DEFAULT gen_random_uuid(),
    tenant_id BIGINT NOT NULL,
    organization_id BIGINT NOT NULL DEFAULT 0,
    spu_no TEXT NOT NULL,
    category_id BIGINT NOT NULL,
    brand_id BIGINT,
    name TEXT NOT NULL,
    title TEXT,
    subtitle TEXT,
    description TEXT,
    product_type TEXT NOT NULL DEFAULT 'physical',
    model_no TEXT,
    barcode TEXT,
    weight_gram BIGINT,
    volume_ml BIGINT,
    tax_class_code TEXT,
    status TEXT NOT NULL DEFAULT 'draft',
    sales_status TEXT NOT NULL DEFAULT 'inactive',
    version BIGINT NOT NULL DEFAULT 0,
    published_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by BIGINT,
    updated_by BIGINT,
    deleted_at TIMESTAMPTZ,
    deleted_by BIGINT,
    PRIMARY KEY (id),
    CONSTRAINT uk_commerce_product_spu_uuid UNIQUE (uuid),
    CONSTRAINT fk_commerce_product_spu_category
        FOREIGN KEY (category_id) REFERENCES commerce_product_category (id),
    CONSTRAINT ck_commerce_product_spu_name_length
        CHECK (char_length(name) BETWEEN 1 AND 300),
    CONSTRAINT ck_commerce_product_spu_product_type
        CHECK (product_type IN ('physical', 'digital', 'service', 'membership', 'points')),
    CONSTRAINT ck_commerce_product_spu_weight
        CHECK (weight_gram IS NULL OR weight_gram >= 0),
    CONSTRAINT ck_commerce_product_spu_volume
        CHECK (volume_ml IS NULL OR volume_ml >= 0),
    CONSTRAINT ck_commerce_product_spu_status
        CHECK (status IN ('draft', 'active', 'inactive', 'archived')),
    CONSTRAINT ck_commerce_product_spu_sales_status
        CHECK (sales_status IN ('active', 'inactive')),
    -- A SPU may only be sellable once it has left draft.
    CONSTRAINT ck_commerce_product_spu_sales_requires_published
        CHECK (sales_status <> 'active' OR status <> 'draft'),
    -- Publishing is a one-way transition with a recorded instant.
    CONSTRAINT ck_commerce_product_spu_published_at
        CHECK (published_at IS NULL OR status <> 'draft')
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_commerce_product_spu_tenant_no
    ON commerce_product_spu (tenant_id, spu_no)
    WHERE deleted_at IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS uk_commerce_product_spu_tenant_barcode
    ON commerce_product_spu (tenant_id, barcode)
    WHERE deleted_at IS NULL AND barcode IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_commerce_product_spu_tenant_status_updated
    ON commerce_product_spu (tenant_id, organization_id, status, updated_at DESC, id)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_commerce_product_spu_tenant_category
    ON commerce_product_spu (tenant_id, category_id, status, id)
    WHERE deleted_at IS NULL;

-- Sellable listings: the predicate other services (for example order's
-- recharge flow) should use instead of reaching into SKU internals.
CREATE INDEX IF NOT EXISTS idx_commerce_product_spu_tenant_sellable
    ON commerce_product_spu (tenant_id, organization_id, product_type, id)
    WHERE deleted_at IS NULL AND sales_status = 'active';

CREATE INDEX IF NOT EXISTS idx_commerce_product_spu_category
    ON commerce_product_spu (category_id);

CREATE TABLE IF NOT EXISTS commerce_product_spu_translation (
    id BIGINT NOT NULL,
    tenant_id BIGINT NOT NULL,
    organization_id BIGINT NOT NULL DEFAULT 0,
    spu_id BIGINT NOT NULL,
    locale TEXT NOT NULL,
    field_name TEXT NOT NULL,
    value TEXT NOT NULL,
    version BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (id),
    CONSTRAINT fk_commerce_product_spu_translation_spu
        FOREIGN KEY (spu_id) REFERENCES commerce_product_spu (id) ON DELETE CASCADE,
    CONSTRAINT ck_commerce_product_spu_translation_locale
        CHECK (locale ~ '^[a-z]{2}(-[A-Z][a-z]{3})?(-[A-Z]{2})?$'),
    CONSTRAINT ck_commerce_product_spu_translation_field
        CHECK (field_name IN ('name', 'title', 'subtitle', 'description', 'meta_title', 'meta_description')),
    CONSTRAINT ck_commerce_product_spu_translation_value
        CHECK (char_length(value) BETWEEN 1 AND 8000)
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_commerce_product_spu_translation
    ON commerce_product_spu_translation (spu_id, locale, field_name);

CREATE INDEX IF NOT EXISTS idx_commerce_product_spu_translation_lookup
    ON commerce_product_spu_translation (tenant_id, locale, spu_id);

-- ---------------------------------------------------------------------------
-- commerce_product_spu_attribute  (key + parameter values on the SPU)
-- ---------------------------------------------------------------------------
-- Replaces the old free-form `spec_json`. Only `key` and `parameter` roles are
-- allowed here: `sales` values belong to a SKU, not to the SPU.
CREATE TABLE IF NOT EXISTS commerce_product_spu_attribute (
    id BIGINT NOT NULL,
    uuid UUID NOT NULL DEFAULT gen_random_uuid(),
    tenant_id BIGINT NOT NULL,
    organization_id BIGINT NOT NULL DEFAULT 0,
    spu_id BIGINT NOT NULL,
    attribute_id BIGINT NOT NULL,
    attribute_value_id BIGINT,
    raw_value TEXT,
    attribute_role TEXT NOT NULL,
    sort_order BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ,
    PRIMARY KEY (id),
    CONSTRAINT uk_commerce_product_spu_attribute_uuid UNIQUE (uuid),
    CONSTRAINT fk_commerce_product_spu_attribute_spu
        FOREIGN KEY (spu_id) REFERENCES commerce_product_spu (id) ON DELETE CASCADE,
    CONSTRAINT fk_commerce_product_spu_attribute_attribute
        FOREIGN KEY (attribute_id) REFERENCES commerce_product_attribute (id),
    CONSTRAINT fk_commerce_product_spu_attribute_value
        FOREIGN KEY (attribute_value_id) REFERENCES commerce_product_attribute_value (id),
    CONSTRAINT ck_commerce_product_spu_attribute_role
        CHECK (attribute_role IN ('key', 'parameter')),
    -- Exactly one carrier: a dictionary value, or a raw literal.
    CONSTRAINT ck_commerce_product_spu_attribute_carrier
        CHECK ((attribute_value_id IS NULL) <> (raw_value IS NULL))
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_commerce_product_spu_attribute_value
    ON commerce_product_spu_attribute (tenant_id, spu_id, attribute_id)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_commerce_product_spu_attribute_role
    ON commerce_product_spu_attribute (tenant_id, spu_id, attribute_role, sort_order, id)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_commerce_product_spu_attribute_attribute
    ON commerce_product_spu_attribute (attribute_id);

CREATE INDEX IF NOT EXISTS idx_commerce_product_spu_attribute_value
    ON commerce_product_spu_attribute (attribute_value_id);

-- ---------------------------------------------------------------------------
-- commerce_product_sku
-- ---------------------------------------------------------------------------
-- `variant_signature` is the deterministic signature of this SKU's sales-axis
-- value combination (built from commerce_product_sku_attribute, sorted by
-- attribute id). It is what makes "one sellable unit per axis combination"
-- enforceable instead of aspirational.
--
-- Money: `*_price_minor` is the exact integer minor-unit amount and
-- `price_scale` is the exponent snapshotted from commerce_currency at write
-- time. 640 CNY is stored as 64000 with price_scale = 2 - never as the
-- major-unit string '640'. `price_scale` is redundant with the currency row by
-- design: it keeps historical rows readable if a currency's exponent is ever
-- redefined, and it makes the divisor explicit at the row level (DB095).
--
-- `sale_price_minor` is mandatory and `list_price_minor` is optional: NULL
-- means "no reference price was declared", which is different from "the
-- reference price is zero". Neither currency nor scale is defaulted, because a
-- default would conceal a missing business value (DATABASE_SPEC section 11).
--
-- Inventory is DECLARED here and OWNED by sdkwork-inventory. No quantity
-- column may be added to this table.
CREATE TABLE IF NOT EXISTS commerce_product_sku (
    id BIGINT NOT NULL,
    uuid UUID NOT NULL DEFAULT gen_random_uuid(),
    tenant_id BIGINT NOT NULL,
    organization_id BIGINT NOT NULL DEFAULT 0,
    spu_id BIGINT NOT NULL,
    sku_no TEXT NOT NULL,
    variant_signature TEXT NOT NULL,
    name TEXT,
    title TEXT,
    currency_code TEXT NOT NULL,
    price_scale SMALLINT NOT NULL,
    list_price_minor BIGINT,
    sale_price_minor BIGINT NOT NULL,
    cost_price_minor BIGINT,
    fulfillment_type TEXT NOT NULL DEFAULT 'physical',
    inventory_tracking TEXT NOT NULL DEFAULT 'none',
    inventory_policy TEXT NOT NULL DEFAULT 'deny',
    weight_gram BIGINT,
    barcode TEXT,
    status TEXT NOT NULL DEFAULT 'draft',
    sales_status TEXT NOT NULL DEFAULT 'inactive',
    version BIGINT NOT NULL DEFAULT 0,
    published_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by BIGINT,
    updated_by BIGINT,
    deleted_at TIMESTAMPTZ,
    deleted_by BIGINT,
    PRIMARY KEY (id),
    CONSTRAINT uk_commerce_product_sku_uuid UNIQUE (uuid),
    CONSTRAINT fk_commerce_product_sku_spu
        FOREIGN KEY (spu_id) REFERENCES commerce_product_spu (id) ON DELETE CASCADE,
    CONSTRAINT fk_commerce_product_sku_currency
        FOREIGN KEY (currency_code) REFERENCES commerce_currency (code),
    CONSTRAINT ck_commerce_product_sku_variant_signature
        CHECK (char_length(variant_signature) BETWEEN 1 AND 500),
    CONSTRAINT ck_commerce_product_sku_price_scale
        CHECK (price_scale BETWEEN 0 AND 8),
    CONSTRAINT ck_commerce_product_sku_list_price
        CHECK (list_price_minor IS NULL OR list_price_minor >= 0),
    CONSTRAINT ck_commerce_product_sku_sale_price
        CHECK (sale_price_minor >= 0),
    CONSTRAINT ck_commerce_product_sku_cost_price
        CHECK (cost_price_minor IS NULL OR cost_price_minor >= 0),
    -- A declared reference price is never below the price being charged. A NULL
    -- reference price means "not declared" and is always acceptable.
    CONSTRAINT ck_commerce_product_sku_sale_not_above_list
        CHECK (list_price_minor IS NULL OR sale_price_minor <= list_price_minor),
    CONSTRAINT ck_commerce_product_sku_weight
        CHECK (weight_gram IS NULL OR weight_gram >= 0),
    CONSTRAINT ck_commerce_product_sku_fulfillment_type
        CHECK (fulfillment_type IN ('physical', 'digital', 'service', 'membership_activation', 'points_topup')),
    CONSTRAINT ck_commerce_product_sku_inventory_tracking
        CHECK (inventory_tracking IN ('none', 'quantity')),
    CONSTRAINT ck_commerce_product_sku_inventory_policy
        CHECK (inventory_policy IN ('deny', 'backorder')),
    -- A non-tracked SKU has no stock to run out of, so backorder is nonsense.
    CONSTRAINT ck_commerce_product_sku_policy_requires_tracking
        CHECK (inventory_tracking = 'quantity' OR inventory_policy = 'deny'),
    CONSTRAINT ck_commerce_product_sku_status
        CHECK (status IN ('draft', 'active', 'inactive', 'archived')),
    CONSTRAINT ck_commerce_product_sku_sales_status
        CHECK (sales_status IN ('active', 'inactive')),
    CONSTRAINT ck_commerce_product_sku_sales_requires_active
        CHECK (sales_status <> 'active' OR status = 'active'),
    CONSTRAINT ck_commerce_product_sku_published_at
        CHECK (published_at IS NULL OR status <> 'draft')
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_commerce_product_sku_tenant_no
    ON commerce_product_sku (tenant_id, sku_no)
    WHERE deleted_at IS NULL;

-- The SPU/SKU invariant: one live SKU per sales-axis combination per SPU.
CREATE UNIQUE INDEX IF NOT EXISTS uk_commerce_product_sku_variant
    ON commerce_product_sku (tenant_id, spu_id, variant_signature)
    WHERE deleted_at IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS uk_commerce_product_sku_tenant_barcode
    ON commerce_product_sku (tenant_id, barcode)
    WHERE deleted_at IS NULL AND barcode IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_commerce_product_sku_spu_sales
    ON commerce_product_sku (tenant_id, spu_id, sales_status, id)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_commerce_product_sku_tenant_status
    ON commerce_product_sku (tenant_id, organization_id, status, sales_status, id)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_commerce_product_sku_spu
    ON commerce_product_sku (spu_id);

CREATE TABLE IF NOT EXISTS commerce_product_sku_attribute (
    id BIGINT NOT NULL,
    uuid UUID NOT NULL DEFAULT gen_random_uuid(),
    tenant_id BIGINT NOT NULL,
    organization_id BIGINT NOT NULL DEFAULT 0,
    sku_id BIGINT NOT NULL,
    attribute_id BIGINT NOT NULL,
    attribute_value_id BIGINT NOT NULL,
    sort_order BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ,
    PRIMARY KEY (id),
    CONSTRAINT uk_commerce_product_sku_attribute_uuid UNIQUE (uuid),
    CONSTRAINT fk_commerce_product_sku_attribute_sku
        FOREIGN KEY (sku_id) REFERENCES commerce_product_sku (id) ON DELETE CASCADE,
    CONSTRAINT fk_commerce_product_sku_attribute_attribute
        FOREIGN KEY (attribute_id) REFERENCES commerce_product_attribute (id),
    CONSTRAINT fk_commerce_product_sku_attribute_value
        FOREIGN KEY (attribute_value_id) REFERENCES commerce_product_attribute_value (id)
);

-- One value per sales axis per SKU.
CREATE UNIQUE INDEX IF NOT EXISTS uk_commerce_product_sku_attribute_axis
    ON commerce_product_sku_attribute (tenant_id, sku_id, attribute_id)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_commerce_product_sku_attribute_attribute
    ON commerce_product_sku_attribute (attribute_id);

CREATE INDEX IF NOT EXISTS idx_commerce_product_sku_attribute_value
    ON commerce_product_sku_attribute (attribute_value_id);

-- Reverse lookup: "which SKUs carry this axis value" (facet filtering).
CREATE INDEX IF NOT EXISTS idx_commerce_product_sku_attribute_tenant_value
    ON commerce_product_sku_attribute (tenant_id, attribute_value_id, sku_id)
    WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS commerce_product_sku_translation (
    id BIGINT NOT NULL,
    tenant_id BIGINT NOT NULL,
    organization_id BIGINT NOT NULL DEFAULT 0,
    sku_id BIGINT NOT NULL,
    locale TEXT NOT NULL,
    field_name TEXT NOT NULL,
    value TEXT NOT NULL,
    version BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (id),
    CONSTRAINT fk_commerce_product_sku_translation_sku
        FOREIGN KEY (sku_id) REFERENCES commerce_product_sku (id) ON DELETE CASCADE,
    CONSTRAINT ck_commerce_product_sku_translation_locale
        CHECK (locale ~ '^[a-z]{2}(-[A-Z][a-z]{3})?(-[A-Z]{2})?$'),
    CONSTRAINT ck_commerce_product_sku_translation_field
        CHECK (field_name IN ('name', 'title', 'description', 'meta_title', 'meta_description')),
    CONSTRAINT ck_commerce_product_sku_translation_value
        CHECK (char_length(value) BETWEEN 1 AND 4000)
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_commerce_product_sku_translation
    ON commerce_product_sku_translation (sku_id, locale, field_name);

CREATE INDEX IF NOT EXISTS idx_commerce_product_sku_translation_lookup
    ON commerce_product_sku_translation (tenant_id, locale, sku_id);

-- ---------------------------------------------------------------------------
-- commerce_product_media
-- ---------------------------------------------------------------------------
-- MEDIA_RESOURCE_SPEC section 6: media is referenced by stable identity
-- (`media_resource_id`, owned by Drive), never by a bare `url` column.
CREATE TABLE IF NOT EXISTS commerce_product_media (
    id BIGINT NOT NULL,
    uuid UUID NOT NULL DEFAULT gen_random_uuid(),
    tenant_id BIGINT NOT NULL,
    organization_id BIGINT NOT NULL DEFAULT 0,
    owner_type TEXT NOT NULL,
    owner_id BIGINT NOT NULL,
    media_role TEXT NOT NULL,
    media_resource_id BIGINT NOT NULL,
    resource_snapshot JSONB NOT NULL DEFAULT '{}'::jsonb,
    alt_text TEXT,
    sort_order BIGINT NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'active',
    version BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ,
    deleted_by BIGINT,
    PRIMARY KEY (id),
    CONSTRAINT uk_commerce_product_media_uuid UNIQUE (uuid),
    CONSTRAINT ck_commerce_product_media_owner_type
        CHECK (owner_type IN ('spu', 'sku', 'category', 'attribute_value')),
    CONSTRAINT ck_commerce_product_media_role
        CHECK (media_role IN ('main_image', 'gallery_image', 'detail_image', 'sku_image', 'video', 'manual', 'certificate')),
    -- An attribute-value swatch is an image; a SKU axis carries a sku_image.
    CONSTRAINT ck_commerce_product_media_owner_role
        CHECK (owner_type <> 'attribute_value' OR media_role IN ('main_image', 'sku_image', 'gallery_image')),
    CONSTRAINT ck_commerce_product_media_status
        CHECK (status IN ('active', 'inactive')),
    CONSTRAINT ck_commerce_product_media_sort_order
        CHECK (sort_order >= 0)
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_commerce_product_media_slot
    ON commerce_product_media (tenant_id, owner_type, owner_id, media_role, sort_order)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_commerce_product_media_owner
    ON commerce_product_media (tenant_id, owner_type, owner_id, status, sort_order, id)
    WHERE deleted_at IS NULL;

-- At most one primary image per owner.
CREATE UNIQUE INDEX IF NOT EXISTS uk_commerce_product_media_main_image
    ON commerce_product_media (tenant_id, owner_type, owner_id)
    WHERE deleted_at IS NULL AND media_role = 'main_image';

CREATE INDEX IF NOT EXISTS idx_commerce_product_media_tenant_resource
    ON commerce_product_media (tenant_id, media_resource_id)
    WHERE deleted_at IS NULL;

-- ---------------------------------------------------------------------------
-- commerce_price_list (+ items)
-- ---------------------------------------------------------------------------
-- Two-layer pricing: the SKU carries the base price; a price list item
-- overrides it for a market x customer segment x minimum quantity x window.
-- Member prices and campaign prices are price lists, never extra SKU columns.
CREATE TABLE IF NOT EXISTS commerce_price_list (
    id BIGINT NOT NULL,
    uuid UUID NOT NULL DEFAULT gen_random_uuid(),
    tenant_id BIGINT NOT NULL,
    organization_id BIGINT NOT NULL DEFAULT 0,
    price_list_no TEXT NOT NULL,
    name TEXT NOT NULL,
    currency_code TEXT NOT NULL,
    market_code TEXT,
    customer_segment TEXT,
    priority INTEGER NOT NULL DEFAULT 100,
    status TEXT NOT NULL DEFAULT 'active',
    starts_at TIMESTAMPTZ,
    ends_at TIMESTAMPTZ,
    version BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by BIGINT,
    updated_by BIGINT,
    deleted_at TIMESTAMPTZ,
    deleted_by BIGINT,
    PRIMARY KEY (id),
    CONSTRAINT uk_commerce_price_list_uuid UNIQUE (uuid),
    CONSTRAINT uk_commerce_price_list_id_currency UNIQUE (id, currency_code),
    CONSTRAINT fk_commerce_price_list_currency
        FOREIGN KEY (currency_code) REFERENCES commerce_currency (code),
    CONSTRAINT ck_commerce_price_list_name_length
        CHECK (char_length(name) BETWEEN 1 AND 200),
    CONSTRAINT ck_commerce_price_list_priority
        CHECK (priority BETWEEN 0 AND 100000),
    CONSTRAINT ck_commerce_price_list_window
        CHECK (ends_at IS NULL OR starts_at IS NULL OR ends_at > starts_at),
    CONSTRAINT ck_commerce_price_list_status
        CHECK (status IN ('active', 'inactive'))
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_commerce_price_list_tenant_no
    ON commerce_price_list (tenant_id, price_list_no)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_commerce_price_list_resolution
    ON commerce_price_list (tenant_id, organization_id, status, priority DESC, id)
    WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS commerce_price_list_item (
    id BIGINT NOT NULL,
    uuid UUID NOT NULL DEFAULT gen_random_uuid(),
    tenant_id BIGINT NOT NULL,
    organization_id BIGINT NOT NULL DEFAULT 0,
    price_list_id BIGINT NOT NULL,
    sku_id BIGINT NOT NULL,
    currency_code TEXT NOT NULL,
    price_scale SMALLINT NOT NULL,
    price_minor BIGINT NOT NULL,
    min_quantity BIGINT NOT NULL DEFAULT 1,
    status TEXT NOT NULL DEFAULT 'active',
    version BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by BIGINT,
    updated_by BIGINT,
    deleted_at TIMESTAMPTZ,
    deleted_by BIGINT,
    PRIMARY KEY (id),
    CONSTRAINT uk_commerce_price_list_item_uuid UNIQUE (uuid),
    CONSTRAINT fk_commerce_price_list_item_list
        FOREIGN KEY (price_list_id) REFERENCES commerce_price_list (id) ON DELETE CASCADE,
    CONSTRAINT fk_commerce_price_list_item_sku
        FOREIGN KEY (sku_id) REFERENCES commerce_product_sku (id) ON DELETE CASCADE,
    -- Composite FK: an item can never disagree with its list's currency.
    CONSTRAINT fk_commerce_price_list_item_list_currency
        FOREIGN KEY (price_list_id, currency_code)
        REFERENCES commerce_price_list (id, currency_code),
    CONSTRAINT fk_commerce_price_list_item_currency
        FOREIGN KEY (currency_code) REFERENCES commerce_currency (code),
    CONSTRAINT ck_commerce_price_list_item_price_scale
        CHECK (price_scale BETWEEN 0 AND 8),
    CONSTRAINT ck_commerce_price_list_item_price
        CHECK (price_minor >= 0),
    CONSTRAINT ck_commerce_price_list_item_min_quantity
        CHECK (min_quantity >= 1),
    CONSTRAINT ck_commerce_price_list_item_status
        CHECK (status IN ('active', 'inactive'))
);

-- One live price per list x SKU x quantity tier.
CREATE UNIQUE INDEX IF NOT EXISTS uk_commerce_price_list_item_tier
    ON commerce_price_list_item (tenant_id, price_list_id, sku_id, min_quantity)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_commerce_price_list_item_resolution
    ON commerce_price_list_item (tenant_id, price_list_id, sku_id, min_quantity DESC, id)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_commerce_price_list_item_sku
    ON commerce_price_list_item (sku_id);
