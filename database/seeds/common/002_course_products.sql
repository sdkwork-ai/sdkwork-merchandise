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
    (id, tenant_id, organization_id, spu_no, category_id, name, title, product_type,
     status, sales_status, published_at)
VALUES
    (1501, 100001, 0, 'course-catalog', 1001, '课程目录', '课程目录', 'digital',
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
