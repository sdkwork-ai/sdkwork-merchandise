-- ============================================================================
-- Merchandise catalog seed (tenant 100001): membership products.
-- ============================================================================
-- This seed is the baseline reference data for the membership catalog and is
-- idempotent: every statement upserts on the reserved primary key.
--
-- RESERVED SEED ID BLOCKS (declared in database/contract/schema.yaml, required
-- by DATABASE_SPEC section 6.4 / SUBJECT_ID_SPEC section 111). All values sit
-- far below any snowflake id, so runtime allocation can never collide:
--     1..99        commerce_currency
--     1000..1099   commerce_product_category
--     1100..1199   commerce_product_attribute
--     1200..1299   commerce_product_attribute_value
--     1300..1399   commerce_product_category_attribute
--     1500..1599   commerce_product_spu
--     1600..1699   commerce_product_sku
--     1700..1799   commerce_product_sku_attribute
--     2000..2099   commerce_price_list
--     2100..2199   commerce_price_list_item
--     3000..3999   *_translation
--
-- MONEY. `sale_price_minor` / `list_price_minor` are exact integer minor units
-- of the SKU's currency. The membership SKUs are CNY with exponent 2 (see
-- 001_currency.sql), so a business price of 640 yuan is stored as 64000 and
-- 660 yuan as 66000. A NULL `list_price_minor` means no reference price was
-- declared (the single-purchase SKUs), which is different from a reference
-- price of zero. Nothing here stores a major-unit string.
--
-- LOCALES. zh-CN is the module default locale (seeds/seed.manifest.json
-- `defaultLocale`), so the base tables carry the zh-CN display values and
-- translation tables add every other locale. Base-column localization used to
-- make the zh-CN and en-US seeds overwrite each other; translation rows let all
-- locales coexist.
--
-- Deliberately NOT seeded here:
--   * promotional marketing badges/tags. That copy belonged in `spec_json`,
--     where it could be neither localized nor queried. Badges are
--     promotion-capability display data rather than catalog master data; see
--     the open items in docs/engineering/reviews.
--   * price lists and their items. Price lists are tenant configuration, not
--     install reference data.
--   * any stock quantity. Inventory is owned by sdkwork-inventory; this module
--     only declares `inventory_tracking` / `inventory_policy`.
-- ============================================================================

-- ---------------------------------------------------------------------------
-- Category
-- ---------------------------------------------------------------------------
INSERT INTO commerce_product_category
    (id, tenant_id, organization_id, category_no, parent_id, path, depth, name,
     is_leaf, sort_order, status)
VALUES
    (1000, 100001, 0, 'membership', NULL, '/1000/', 0, '会员服务', TRUE, 10, 'active')
ON CONFLICT (id) DO UPDATE SET
    category_no = EXCLUDED.category_no,
    parent_id = EXCLUDED.parent_id,
    path = EXCLUDED.path,
    depth = EXCLUDED.depth,
    name = EXCLUDED.name,
    is_leaf = EXCLUDED.is_leaf,
    sort_order = EXCLUDED.sort_order,
    status = EXCLUDED.status,
    updated_at = NOW();

-- ---------------------------------------------------------------------------
-- Attributes: the two sales axes that distinguish membership SKUs
-- ---------------------------------------------------------------------------
INSERT INTO commerce_product_attribute
    (id, tenant_id, organization_id, attribute_no, name, value_type, input_hint,
     is_multi_value, sort_order, status)
VALUES
    (1100, 100001, 0, 'tier',   '会员档位', 'enum', 'select', FALSE, 10, 'active'),
    (1101, 100001, 0, 'period', '订阅周期', 'enum', 'select', FALSE, 20, 'active')
ON CONFLICT (id) DO UPDATE SET
    attribute_no = EXCLUDED.attribute_no,
    name = EXCLUDED.name,
    value_type = EXCLUDED.value_type,
    input_hint = EXCLUDED.input_hint,
    is_multi_value = EXCLUDED.is_multi_value,
    sort_order = EXCLUDED.sort_order,
    status = EXCLUDED.status,
    updated_at = NOW();

INSERT INTO commerce_product_attribute_value
    (id, tenant_id, organization_id, attribute_id, value_code, display_value, sort_order, status)
VALUES
    (1200, 100001, 0, 1100, 'basic',    '基础版', 10, 'active'),
    (1201, 100001, 0, 1100, 'standard', '标准版', 20, 'active'),
    (1202, 100001, 0, 1100, 'premium',  '巅峰版', 30, 'active'),
    (1203, 100001, 0, 1100, 'super',    '超级版', 40, 'active'),
    (1210, 100001, 0, 1101, 'annual',    '连续包年', 10, 'active'),
    (1211, 100001, 0, 1101, 'quarterly', '连续包季', 20, 'active'),
    (1212, 100001, 0, 1101, 'monthly',   '连续包月', 30, 'active'),
    (1213, 100001, 0, 1101, 'single',    '单月购买', 40, 'active')
ON CONFLICT (id) DO UPDATE SET
    attribute_id = EXCLUDED.attribute_id,
    value_code = EXCLUDED.value_code,
    display_value = EXCLUDED.display_value,
    sort_order = EXCLUDED.sort_order,
    status = EXCLUDED.status,
    updated_at = NOW();

-- Both axes are `sales` for this category: their value combinations identify
-- the SKU. The attribute dictionary carries no role of its own, so the same
-- entry can be a plain `parameter` in another category.
INSERT INTO commerce_product_category_attribute
    (id, tenant_id, organization_id, category_id, attribute_id, attribute_role,
     source_category_id, is_required, is_searchable, is_filterable, is_comparable,
     sort_order, status)
VALUES
    (1300, 100001, 0, 1000, 1100, 'sales', NULL, TRUE, TRUE, TRUE, TRUE, 10, 'active'),
    (1301, 100001, 0, 1000, 1101, 'sales', NULL, TRUE, TRUE, TRUE, TRUE, 20, 'active')
ON CONFLICT (id) DO UPDATE SET
    category_id = EXCLUDED.category_id,
    attribute_id = EXCLUDED.attribute_id,
    attribute_role = EXCLUDED.attribute_role,
    is_required = EXCLUDED.is_required,
    is_searchable = EXCLUDED.is_searchable,
    is_filterable = EXCLUDED.is_filterable,
    is_comparable = EXCLUDED.is_comparable,
    sort_order = EXCLUDED.sort_order,
    status = EXCLUDED.status,
    updated_at = NOW();

-- ---------------------------------------------------------------------------
-- SPU
-- ---------------------------------------------------------------------------
INSERT INTO commerce_product_spu
    (id, tenant_id, organization_id, spu_no, category_id, name, title, product_type,
     status, sales_status, published_at)
VALUES
    (1500, 100001, 0, 'membership-catalog', 1000, '会员目录', '会员目录', 'membership',
     'active', 'active', NOW())
ON CONFLICT (id) DO UPDATE SET
    spu_no = EXCLUDED.spu_no,
    category_id = EXCLUDED.category_id,
    name = EXCLUDED.name,
    title = EXCLUDED.title,
    product_type = EXCLUDED.product_type,
    status = EXCLUDED.status,
    sales_status = EXCLUDED.sales_status,
    updated_at = NOW();

-- ---------------------------------------------------------------------------
-- SKUs
-- ---------------------------------------------------------------------------
-- `variant_signature` is the deterministic signature of the sales-axis value
-- combination: one `attribute_no=value_code` term per axis, ordered by
-- `attribute_no` ascending, joined with `;`. The owning service builds it and
-- uk_commerce_product_sku_variant enforces it, which is what makes "one live
-- SKU per axis combination" a real invariant instead of documentation.
INSERT INTO commerce_product_sku
    (id, tenant_id, organization_id, spu_id, sku_no, variant_signature, name, title,
     currency_code, price_scale, list_price_minor, sale_price_minor,
     fulfillment_type, inventory_tracking, inventory_policy,
     status, sales_status, published_at)
VALUES
    (1600, 100001, 0, 1500, 'basic-annual',       'period=annual;tier=basic',       '基础版-连续包年', '基础版-连续包年', 'CNY', 2, 66000,   64000,   'membership_activation', 'none', 'deny', 'active', 'active', NOW()),
    (1601, 100001, 0, 1500, 'standard-annual',    'period=annual;tier=standard',    '标准版-连续包年', '标准版-连续包年', 'CNY', 2, 189600,  183900,  'membership_activation', 'none', 'deny', 'active', 'active', NOW()),
    (1602, 100001, 0, 1500, 'premium-annual',     'period=annual;tier=premium',     '巅峰版-连续包年', '巅峰版-连续包年', 'CNY', 2, 519600,  504000,  'membership_activation', 'none', 'deny', 'active', 'active', NOW()),
    (1603, 100001, 0, 1500, 'super-annual',       'period=annual;tier=super',       '超级版-连续包年', '超级版-连续包年', 'CNY', 2, 1299600, 1260600, 'membership_activation', 'none', 'deny', 'active', 'active', NOW()),
    (1604, 100001, 0, 1500, 'basic-monthly',      'period=monthly;tier=basic',      '基础版-连续包月', '基础版-连续包月', 'CNY', 2, 5500,    5400,    'membership_activation', 'none', 'deny', 'active', 'active', NOW()),
    (1605, 100001, 0, 1500, 'standard-monthly',   'period=monthly;tier=standard',   '标准版-连续包月', '标准版-连续包月', 'CNY', 2, 15800,   15600,   'membership_activation', 'none', 'deny', 'active', 'active', NOW()),
    (1606, 100001, 0, 1500, 'premium-monthly',    'period=monthly;tier=premium',    '巅峰版-连续包月', '巅峰版-连续包月', 'CNY', 2, 43300,   42900,   'membership_activation', 'none', 'deny', 'active', 'active', NOW()),
    (1607, 100001, 0, 1500, 'super-monthly',      'period=monthly;tier=super',      '超级版-连续包月', '超级版-连续包月', 'CNY', 2, 108300,  107200,  'membership_activation', 'none', 'deny', 'active', 'active', NOW()),
    (1608, 100001, 0, 1500, 'basic-quarterly',    'period=quarterly;tier=basic',    '基础版-连续包季', '基础版-连续包季', 'CNY', 2, 16500,   16200,   'membership_activation', 'none', 'deny', 'active', 'active', NOW()),
    (1609, 100001, 0, 1500, 'standard-quarterly', 'period=quarterly;tier=standard', '标准版-连续包季', '标准版-连续包季', 'CNY', 2, 47400,   46500,   'membership_activation', 'none', 'deny', 'active', 'active', NOW()),
    (1610, 100001, 0, 1500, 'premium-quarterly',  'period=quarterly;tier=premium',  '巅峰版-连续包季', '巅峰版-连续包季', 'CNY', 2, 129900,  127300,  'membership_activation', 'none', 'deny', 'active', 'active', NOW()),
    (1611, 100001, 0, 1500, 'super-quarterly',    'period=quarterly;tier=super',    '超级版-连续包季', '超级版-连续包季', 'CNY', 2, 324900,  318400,  'membership_activation', 'none', 'deny', 'active', 'active', NOW()),
    (1612, 100001, 0, 1500, 'basic-single',       'period=single;tier=basic',       '基础版-单月购买', '基础版-单月购买', 'CNY', 2, NULL,     5500,    'membership_activation', 'none', 'deny', 'active', 'active', NOW()),
    (1613, 100001, 0, 1500, 'standard-single',    'period=single;tier=standard',    '标准版-单月购买', '标准版-单月购买', 'CNY', 2, NULL,     15800,   'membership_activation', 'none', 'deny', 'active', 'active', NOW()),
    (1614, 100001, 0, 1500, 'premium-single',     'period=single;tier=premium',     '巅峰版-单月购买', '巅峰版-单月购买', 'CNY', 2, NULL,     43300,   'membership_activation', 'none', 'deny', 'active', 'active', NOW()),
    (1615, 100001, 0, 1500, 'super-single',       'period=single;tier=super',       '超级版-单月购买', '超级版-单月购买', 'CNY', 2, NULL,     108300,  'membership_activation', 'none', 'deny', 'active', 'active', NOW())
ON CONFLICT (id) DO UPDATE SET
    spu_id = EXCLUDED.spu_id,
    sku_no = EXCLUDED.sku_no,
    variant_signature = EXCLUDED.variant_signature,
    name = EXCLUDED.name,
    title = EXCLUDED.title,
    currency_code = EXCLUDED.currency_code,
    price_scale = EXCLUDED.price_scale,
    list_price_minor = EXCLUDED.list_price_minor,
    sale_price_minor = EXCLUDED.sale_price_minor,
    fulfillment_type = EXCLUDED.fulfillment_type,
    inventory_tracking = EXCLUDED.inventory_tracking,
    inventory_policy = EXCLUDED.inventory_policy,
    status = EXCLUDED.status,
    sales_status = EXCLUDED.sales_status,
    updated_at = NOW();

-- ---------------------------------------------------------------------------
-- SKU sales-axis assignments: two rows per SKU (period, then tier)
-- ---------------------------------------------------------------------------
INSERT INTO commerce_product_sku_attribute
    (id, tenant_id, organization_id, sku_id, attribute_id, attribute_value_id, sort_order)
VALUES
    (1700, 100001, 0, 1600, 1101, 1210, 10), (1701, 100001, 0, 1600, 1100, 1200, 20),
    (1702, 100001, 0, 1601, 1101, 1210, 10), (1703, 100001, 0, 1601, 1100, 1201, 20),
    (1704, 100001, 0, 1602, 1101, 1210, 10), (1705, 100001, 0, 1602, 1100, 1202, 20),
    (1706, 100001, 0, 1603, 1101, 1210, 10), (1707, 100001, 0, 1603, 1100, 1203, 20),
    (1708, 100001, 0, 1604, 1101, 1212, 10), (1709, 100001, 0, 1604, 1100, 1200, 20),
    (1710, 100001, 0, 1605, 1101, 1212, 10), (1711, 100001, 0, 1605, 1100, 1201, 20),
    (1712, 100001, 0, 1606, 1101, 1212, 10), (1713, 100001, 0, 1606, 1100, 1202, 20),
    (1714, 100001, 0, 1607, 1101, 1212, 10), (1715, 100001, 0, 1607, 1100, 1203, 20),
    (1716, 100001, 0, 1608, 1101, 1211, 10), (1717, 100001, 0, 1608, 1100, 1200, 20),
    (1718, 100001, 0, 1609, 1101, 1211, 10), (1719, 100001, 0, 1609, 1100, 1201, 20),
    (1720, 100001, 0, 1610, 1101, 1211, 10), (1721, 100001, 0, 1610, 1100, 1202, 20),
    (1722, 100001, 0, 1611, 1101, 1211, 10), (1723, 100001, 0, 1611, 1100, 1203, 20),
    (1724, 100001, 0, 1612, 1101, 1213, 10), (1725, 100001, 0, 1612, 1100, 1200, 20),
    (1726, 100001, 0, 1613, 1101, 1213, 10), (1727, 100001, 0, 1613, 1100, 1201, 20),
    (1728, 100001, 0, 1614, 1101, 1213, 10), (1729, 100001, 0, 1614, 1100, 1202, 20),
    (1730, 100001, 0, 1615, 1101, 1213, 10), (1731, 100001, 0, 1615, 1100, 1203, 20)
ON CONFLICT (id) DO UPDATE SET
    sku_id = EXCLUDED.sku_id,
    attribute_id = EXCLUDED.attribute_id,
    attribute_value_id = EXCLUDED.attribute_value_id,
    sort_order = EXCLUDED.sort_order,
    updated_at = NOW();

-- ---------------------------------------------------------------------------
-- Localized display values are NOT stored here.
-- ---------------------------------------------------------------------------
-- zh-CN is the module default locale, so the base tables above already carry the
-- zh-CN display values and act as the fallback for every other locale. Every
-- other locale lives in seeds/locales/<locale>/ so that locales can coexist
-- instead of overwriting one another in a base column.

-- See seeds/locales/en-US/001_sku_locale.sql for the translation pattern.