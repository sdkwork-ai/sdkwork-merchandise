-- ============================================================================
-- Merchandise course seed (tenant 100001): digital course product line.
-- ============================================================================
-- Second product line after the membership catalog in 001: a different
-- category (1001), a different product type (`digital`), and its own sales
-- axis (`offering`, 1102/1214 — declared in 001's attribute dictionary).
--
-- RESERVED SEED ID BLOCKS (see 001_merchandise_bootstrap.sql):
--     1001         commerce_product_category (course)
--     1501         commerce_product_spu (course catalog)
--     1620         commerce_product_sku (AI efficiency column)
--     1732         commerce_product_sku_attribute (offering axis)
--     1302         commerce_product_category_attribute (course template)
--
-- Idempotent: every statement upserts on the reserved primary key.
-- ============================================================================

INSERT INTO commerce_product_category
    (id, tenant_id, organization_id, category_no, parent_id, path, depth, name,
     is_leaf, sort_order, status)
VALUES
    (1001, 100001, 0, 'course', NULL, '/1001/', 0, '课程', TRUE, 20, 'active')
ON CONFLICT (id) DO UPDATE SET
    category_no = EXCLUDED.category_no,
    name = EXCLUDED.name,
    is_leaf = EXCLUDED.is_leaf,
    sort_order = EXCLUDED.sort_order,
    status = EXCLUDED.status,
    updated_at = NOW();

INSERT INTO commerce_product_spu
    (id, tenant_id, organization_id, spu_no, category_id, brand_id, name, title, product_type,
     status, sales_status, published_at)
VALUES
    (1501, 100001, 0, 'course-catalog', 1001, 2200, '课程目录', '课程目录', 'digital',
     'active', 'active', NOW())
ON CONFLICT (id) DO UPDATE SET
    spu_no = EXCLUDED.spu_no,
    category_id = EXCLUDED.category_id,
    brand_id = EXCLUDED.brand_id,
    name = EXCLUDED.name,
    title = EXCLUDED.title,
    product_type = EXCLUDED.product_type,
    status = EXCLUDED.status,
    sales_status = EXCLUDED.sales_status,
    updated_at = NOW();

INSERT INTO commerce_product_category_attribute
    (id, tenant_id, organization_id, category_id, attribute_id, attribute_role,
     source_category_id, is_required, is_searchable, is_filterable, is_comparable,
     sort_order, status)
VALUES
    (1302, 100001, 0, 1001, 1102, 'sales', NULL, TRUE, TRUE, TRUE, FALSE, 10, 'active')
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

INSERT INTO commerce_product_sku
    (id, tenant_id, organization_id, spu_id, sku_no, variant_signature, name, title,
     currency_code, price_scale, list_price_minor, sale_price_minor,
     fulfillment_type, inventory_tracking, inventory_policy,
     status, sales_status, published_at)
VALUES
    (1620, 100001, 0, 1501, 'course-column-ai-productivity',
     'offering=offering-column-ai-productivity', 'AI 效率工作法',
     'AI 效率工作法', 'CNY', 2, 19900, 12900, 'digital', 'none', 'deny',
     'active', 'active', NOW())
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

-- Course ladder item on the shared retail list: the SKU lives in this file, so
-- its price-list item does too (001's items reference membership SKUs only).
INSERT INTO commerce_price_list_item
    (id, uuid, tenant_id, organization_id, price_list_id, sku_id, currency_code,
     price_scale, price_minor, min_quantity, status)
VALUES
    (2117, gen_random_uuid(), 100001, 0, 2100, 1620, 'CNY', 2, 12900, 1, 'active')
ON CONFLICT (id) DO UPDATE SET
    price_list_id = EXCLUDED.price_list_id,
    sku_id = EXCLUDED.sku_id,
    currency_code = EXCLUDED.currency_code,
    price_scale = EXCLUDED.price_scale,
    price_minor = EXCLUDED.price_minor,
    min_quantity = EXCLUDED.min_quantity,
    status = EXCLUDED.status,
    updated_at = NOW();

-- The sales axis backing `variant_signature = offering=offering-column-ai-productivity`.
INSERT INTO commerce_product_sku_attribute
    (id, tenant_id, organization_id, sku_id, attribute_id, attribute_value_id, sort_order)
VALUES
    (1732, 100001, 0, 1620, 1102, 1214, 10)
ON CONFLICT (id) DO UPDATE SET
    sku_id = EXCLUDED.sku_id,
    attribute_id = EXCLUDED.attribute_id,
    attribute_value_id = EXCLUDED.attribute_value_id,
    sort_order = EXCLUDED.sort_order,
    updated_at = NOW();
