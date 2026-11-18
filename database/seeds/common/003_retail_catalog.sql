-- ============================================================================
-- Merchandise retail catalog seed (tenant 100001): electronics, office and
-- home product lines.
-- ============================================================================
-- The membership catalog (001) and the course catalog (002) demonstrate the
-- two subscription-shaped product types. This seed adds the retail graph an
-- operator expects to see on a fresh deployment: a two-level category tree,
-- variant attributes with dictionaries, several brands, physical and service
-- SPUs with SKUs, media rows, SPU specifications, and retail price-list
-- overrides.
--
-- Idempotent: every statement upserts on the reserved primary key, so startup
-- seeding and `db:seed` re-runs never duplicate or rewrite operator edits.
--
-- RESERVED SEED ID BLOCKS used here (see 001_merchandise_bootstrap.sql):
--     1010..1022    commerce_product_category
--     1104..1109    commerce_product_attribute
--     1215..1224    commerce_product_attribute_value
--     1304..1312    commerce_product_category_attribute
--     1510..1518    commerce_product_spu
--     1630..1643    commerce_product_sku
--     1740..1780    commerce_product_sku_attribute
--     1850..1860    commerce_product_spu_attribute
--     2110..2129    commerce_price_list_item
--     2202..2204    commerce_product_brand
--     2430..2444    commerce_product_media
--
-- MONEY: all retail SKUs are CNY with exponent 2 (see 001_currency.sql), so a
-- business price of 399 yuan is stored as 39900. Price-list items override the
-- SKU base price and inherit currency/scale from their list (2100).
--
-- INVENTORY: no stock quantity is seeded. Inventory is owned by
-- sdkwork-inventory and this module only declares `inventory_tracking` /
-- `inventory_policy`. A physical SKU stays sellable only after the inventory
-- module has a stock row for it.
-- ============================================================================

-- ---------------------------------------------------------------------------
-- Brands
-- ---------------------------------------------------------------------------
INSERT INTO commerce_product_brand
    (id, uuid, tenant_id, organization_id, brand_no, name, description, status, sort_order)
VALUES
    (2202, gen_random_uuid(), 100001, 0, 'brand-nebula', '星云 Nebula', '高端网络与存储设备品牌', 'active', 10),
    (2203, gen_random_uuid(), 100001, 0, 'brand-aurora', '极光 Aurora', '智能办公与配件品牌', 'active', 20),
    (2204, gen_random_uuid(), 100001, 0, 'brand-lumen', '流明 Lumen', '智能家居与生活电器品牌', 'active', 30)
ON CONFLICT (id) DO UPDATE SET
    brand_no = EXCLUDED.brand_no,
    name = EXCLUDED.name,
    description = EXCLUDED.description,
    status = EXCLUDED.status,
    sort_order = EXCLUDED.sort_order,
    updated_at = NOW();

-- ---------------------------------------------------------------------------
-- Category tree (two roots, five leaf categories)
-- ---------------------------------------------------------------------------
INSERT INTO commerce_product_category
    (id, tenant_id, organization_id, category_no, parent_id, path, depth, name,
     is_leaf, sort_order, status)
VALUES
    (1010, 100001, 0, 'electronics', NULL, '/1010/', 0, '消费电子', FALSE, 20, 'active'),
    (1011, 100001, 0, 'network',     1010, '/1010/1011/', 1, '网络设备', TRUE, 10, 'active'),
    (1012, 100001, 0, 'smart-home',  1010, '/1010/1012/', 1, '智能家居', TRUE, 20, 'active'),
    (1013, 100001, 0, 'accessories', 1010, '/1010/1013/', 1, '数码配件', TRUE, 30, 'active'),
    (1020, 100001, 0, 'office-home', NULL, '/1020/', 0, '办公与生活', FALSE, 30, 'active'),
    (1021, 100001, 0, 'office-peripherals', 1020, '/1020/1021/', 1, '办公外设', TRUE, 10, 'active'),
    (1022, 100001, 0, 'home-appliance',     1020, '/1020/1022/', 1, '生活电器', TRUE, 20, 'active')
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
-- Attribute dictionary: colour and connectivity are sales axes, the number
-- attributes are category parameters.
-- ---------------------------------------------------------------------------
INSERT INTO commerce_product_attribute
    (id, tenant_id, organization_id, attribute_no, name, value_type, input_hint,
     is_multi_value, sort_order, status)
VALUES
    (1104, 100001, 0, 'color',         '颜色',   'enum',   'select', FALSE, 50, 'active'),
    (1105, 100001, 0, 'connectivity',  '连接方式', 'enum',   'select', FALSE, 60, 'active'),
    (1106, 100001, 0, 'capacity',      '容量',   'enum',   'select', FALSE, 70, 'active'),
    (1107, 100001, 0, 'material',      '材质',   'enum',   'select', FALSE, 80, 'active'),
    (1108, 100001, 0, 'warranty_months', '保修月数', 'number', 'number', FALSE, 90, 'active'),
    (1109, 100001, 0, 'power_watt',    '功率（瓦）', 'number', 'number', FALSE, 100, 'active')
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
    (1215, 100001, 0, 1104, 'obsidian',  '曜石黑', 10, 'active'),
    (1216, 100001, 0, 1104, 'moonwhite', '月光白', 20, 'active'),
    (1217, 100001, 0, 1104, 'starryblue','星空蓝', 30, 'active'),
    (1218, 100001, 0, 1105, 'wifi7',     'Wi-Fi 7', 10, 'active'),
    (1219, 100001, 0, 1105, 'dual2_5g',  '双 2.5G 网口', 20, 'active'),
    (1220, 100001, 0, 1105, 'bt54',      '蓝牙 5.4', 30, 'active'),
    (1221, 100001, 0, 1106, '512gb',     '512GB', 10, 'active'),
    (1222, 100001, 0, 1106, '1tb',       '1TB', 20, 'active'),
    (1223, 100001, 0, 1107, 'aluminum',  '铝合金', 10, 'active'),
    (1224, 100001, 0, 1107, 'abs',       '环保 ABS', 20, 'active')
ON CONFLICT (id) DO UPDATE SET
    attribute_id = EXCLUDED.attribute_id,
    value_code = EXCLUDED.value_code,
    display_value = EXCLUDED.display_value,
    sort_order = EXCLUDED.sort_order,
    status = EXCLUDED.status,
    updated_at = NOW();

-- Category templates: sales axes identify the SKU, parameters describe it.
INSERT INTO commerce_product_category_attribute
    (id, tenant_id, organization_id, category_id, attribute_id, attribute_role,
     source_category_id, is_required, is_searchable, is_filterable, is_comparable,
     sort_order, status)
VALUES
    (1304, 100001, 0, 1011, 1104, 'sales',     NULL, TRUE,  TRUE,  TRUE,  TRUE,  10, 'active'),
    (1305, 100001, 0, 1011, 1105, 'sales',     NULL, TRUE,  TRUE,  TRUE,  TRUE,  20, 'active'),
    (1306, 100001, 0, 1011, 1108, 'parameter', NULL, FALSE, FALSE, TRUE,  FALSE, 30, 'active'),
    (1307, 100001, 0, 1012, 1104, 'sales',     NULL, TRUE,  TRUE,  TRUE,  TRUE,  10, 'active'),
    (1308, 100001, 0, 1012, 1109, 'parameter', NULL, FALSE, FALSE, TRUE,  FALSE, 20, 'active'),
    (1309, 100001, 0, 1013, 1104, 'sales',     NULL, TRUE,  TRUE,  TRUE,  TRUE,  10, 'active'),
    (1310, 100001, 0, 1013, 1106, 'sales',     NULL, FALSE, TRUE,  TRUE,  TRUE,  20, 'active'),
    (1311, 100001, 0, 1021, 1104, 'sales',     NULL, TRUE,  TRUE,  TRUE,  TRUE,  10, 'active'),
    (1312, 100001, 0, 1021, 1107, 'parameter', NULL, FALSE, FALSE, TRUE,  FALSE, 20, 'active')
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
-- SPUs: network devices, smart home, accessories, office peripherals, home
-- appliance, and one installation service.
-- ---------------------------------------------------------------------------
INSERT INTO commerce_product_spu
    (id, tenant_id, organization_id, spu_no, category_id, brand_id, name, title,
     product_type, status, sales_status, published_at)
VALUES
    (1510, 100001, 0, 'router-nebula-x7', 1011, 2202, '星云 X7 路由器', '星云 X7 双频 Wi-Fi 7 路由器', 'physical', 'active', 'active', NOW()),
    (1511, 100001, 0, 'router-aurora-ax6', 1011, 2203, '极光 AX6 路由器', '极光 AX6 双 2.5G 口路由器', 'physical', 'active', 'active', NOW()),
    (1512, 100001, 0, 'smart-hub-lumen', 1012, 2204, '流明智能中控', '流明智能家居中控屏', 'physical', 'active', 'active', NOW()),
    (1513, 100001, 0, 'smart-sensor-kit', 1012, 2204, '流明传感器套装', '流明温湿度与门窗传感器套装', 'physical', 'active', 'active', NOW()),
    (1514, 100001, 0, 'nvme-ssd-nebula', 1013, 2202, '星云 NVMe 固态硬盘', '星云 PCIe 4.0 NVMe 固态硬盘', 'physical', 'active', 'active', NOW()),
    (1515, 100001, 0, 'usb-c-dock-aurora', 1013, 2203, '极光 USB-C 扩展坞', '极光 12 合 1 USB-C 扩展坞', 'physical', 'draft', 'inactive', NULL),
    (1516, 100001, 0, 'keyboard-aurora-k87', 1021, 2203, '极光 K87 机械键盘', '极光 K87 三模机械键盘', 'physical', 'active', 'active', NOW()),
    (1517, 100001, 0, 'purifier-lumen-p2', 1022, 2204, '流明 P2 空气净化器', '流明 P2 除醛空气净化器', 'physical', 'active', 'active', NOW()),
    (1518, 100001, 0, 'inst-svc-network', 1011, 2202, '网络设备安装服务', '星云网络设备上门安装服务', 'service', 'active', 'active', NOW())
ON CONFLICT (id) DO UPDATE SET
    spu_no = EXCLUDED.spu_no,
    category_id = EXCLUDED.category_id,
    brand_id = EXCLUDED.brand_id,
    name = EXCLUDED.name,
    title = EXCLUDED.title,
    product_type = EXCLUDED.product_type,
    status = EXCLUDED.status,
    sales_status = EXCLUDED.sales_status,
    published_at = EXCLUDED.published_at,
    updated_at = NOW();

-- ---------------------------------------------------------------------------
-- SKUs: one row per sales-axis combination.
-- ---------------------------------------------------------------------------
INSERT INTO commerce_product_sku
    (id, tenant_id, organization_id, spu_id, sku_no, variant_signature, name, title,
     currency_code, price_scale, list_price_minor, sale_price_minor,
     fulfillment_type, inventory_tracking, inventory_policy,
     status, sales_status, published_at)
VALUES
    (1630, 100001, 0, 1510, 'x7-obsidian', 'color=obsidian;connectivity=wifi7', 'X7 曜石黑', '星云 X7 路由器 曜石黑', 'CNY', 2, 49900, 44900, 'physical', 'quantity', 'deny', 'active', 'active', NOW()),
    (1631, 100001, 0, 1510, 'x7-moonwhite', 'color=moonwhite;connectivity=wifi7', 'X7 月光白', '星云 X7 路由器 月光白', 'CNY', 2, 49900, 44900, 'physical', 'quantity', 'deny', 'active', 'active', NOW()),
    (1632, 100001, 0, 1511, 'ax6-obsidian', 'color=obsidian;connectivity=dual2_5g', 'AX6 曜石黑', '极光 AX6 路由器 曜石黑', 'CNY', 2, 39900, 35900, 'physical', 'quantity', 'deny', 'active', 'active', NOW()),
    (1633, 100001, 0, 1511, 'ax6-starryblue', 'color=starryblue;connectivity=dual2_5g', 'AX6 星空蓝', '极光 AX6 路由器 星空蓝', 'CNY', 2, 39900, 35900, 'physical', 'quantity', 'deny', 'active', 'active', NOW()),
    (1634, 100001, 0, 1512, 'hub-moonwhite', 'color=moonwhite', '中控屏 月光白', '流明智能中控屏 月光白', 'CNY', 2, 129900, 119900, 'physical', 'quantity', 'deny', 'active', 'active', NOW()),
    (1635, 100001, 0, 1512, 'hub-obsidian', 'color=obsidian', '中控屏 曜石黑', '流明智能中控屏 曜石黑', 'CNY', 2, 129900, 119900, 'physical', 'quantity', 'deny', 'active', 'active', NOW()),
    (1636, 100001, 0, 1513, 'sensor-kit-moonwhite', 'color=moonwhite', '传感器套装 月光白', '流明传感器套装 月光白', 'CNY', 2, 29900, 25900, 'physical', 'quantity', 'deny', 'active', 'active', NOW()),
    (1637, 100001, 0, 1514, 'ssd-1tb', 'capacity=1tb', 'NVMe 1TB', '星云 NVMe 固态硬盘 1TB', 'CNY', 2, 79900, 69900, 'physical', 'quantity', 'deny', 'active', 'active', NOW()),
    (1638, 100001, 0, 1514, 'ssd-512gb', 'capacity=512gb', 'NVMe 512GB', '星云 NVMe 固态硬盘 512GB', 'CNY', 2, 49900, 42900, 'physical', 'quantity', 'deny', 'active', 'active', NOW()),
    (1639, 100001, 0, 1515, 'dock-obsidian', 'color=obsidian', '扩展坞 曜石黑', '极光 USB-C 扩展坞 曜石黑', 'CNY', 2, 69900, 59900, 'physical', 'quantity', 'deny', 'draft', 'inactive', NULL),
    (1640, 100001, 0, 1516, 'k87-obsidian', 'color=obsidian', 'K87 曜石黑', '极光 K87 机械键盘 曜石黑', 'CNY', 2, 49900, 45900, 'physical', 'quantity', 'deny', 'active', 'active', NOW()),
    (1641, 100001, 0, 1516, 'k87-moonwhite', 'color=moonwhite', 'K87 月光白', '极光 K87 机械键盘 月光白', 'CNY', 2, 49900, 45900, 'physical', 'quantity', 'deny', 'active', 'active', NOW()),
    (1642, 100001, 0, 1517, 'p2-moonwhite', 'color=moonwhite', '净化器 P2 月光白', '流明 P2 空气净化器 月光白', 'CNY', 2, 189900, 169900, 'physical', 'quantity', 'deny', 'active', 'active', NOW()),
    (1643, 100001, 0, 1518, 'inst-network-standard', 'channel=onsite', '标准上门安装', '星云网络设备标准上门安装', 'CNY', 2, 19900, 19900, 'service', 'none', 'deny', 'active', 'active', NOW())
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
    published_at = EXCLUDED.published_at,
    updated_at = NOW();

-- ---------------------------------------------------------------------------
-- SKU axes (sales attributes) for the variant matrix.
-- ---------------------------------------------------------------------------
INSERT INTO commerce_product_sku_attribute
    (id, tenant_id, organization_id, sku_id, attribute_id, attribute_value_id, sort_order)
VALUES
    (1740, 100001, 0, 1630, 1104, 1215, 10), (1741, 100001, 0, 1630, 1105, 1218, 20),
    (1742, 100001, 0, 1631, 1104, 1216, 10), (1743, 100001, 0, 1631, 1105, 1218, 20),
    (1744, 100001, 0, 1632, 1104, 1215, 10), (1745, 100001, 0, 1632, 1105, 1219, 20),
    (1746, 100001, 0, 1633, 1104, 1217, 10), (1747, 100001, 0, 1633, 1105, 1219, 20),
    (1748, 100001, 0, 1634, 1104, 1216, 10),
    (1749, 100001, 0, 1635, 1104, 1215, 10),
    (1750, 100001, 0, 1636, 1104, 1216, 10),
    (1751, 100001, 0, 1637, 1106, 1222, 10),
    (1752, 100001, 0, 1638, 1106, 1221, 10),
    (1753, 100001, 0, 1639, 1104, 1215, 10),
    (1754, 100001, 0, 1640, 1104, 1215, 10),
    (1755, 100001, 0, 1641, 1104, 1216, 10),
    (1756, 100001, 0, 1642, 1104, 1216, 10)
ON CONFLICT (id) DO UPDATE SET
    sku_id = EXCLUDED.sku_id,
    attribute_id = EXCLUDED.attribute_id,
    attribute_value_id = EXCLUDED.attribute_value_id,
    sort_order = EXCLUDED.sort_order,
    updated_at = NOW();

-- ---------------------------------------------------------------------------
-- SPU specifications (category parameters, not SKU axes).
-- ---------------------------------------------------------------------------
INSERT INTO commerce_product_spu_attribute
    (id, uuid, tenant_id, organization_id, spu_id, attribute_id, attribute_value_id,
     raw_value, attribute_role, sort_order)
VALUES
    (1850, gen_random_uuid(), 100001, 0, 1510, 1108, NULL, '24', 'parameter', 10),
    (1851, gen_random_uuid(), 100001, 0, 1511, 1108, NULL, '24', 'parameter', 10),
    (1852, gen_random_uuid(), 100001, 0, 1512, 1109, NULL, '15', 'parameter', 10),
    (1853, gen_random_uuid(), 100001, 0, 1516, 1107, 1223, NULL, 'parameter', 10),
    (1854, gen_random_uuid(), 100001, 0, 1517, 1109, NULL, '65', 'parameter', 10),
    (1855, gen_random_uuid(), 100001, 0, 1514, 1108, NULL, '60', 'parameter', 10)
ON CONFLICT (id) DO UPDATE SET
    spu_id = EXCLUDED.spu_id,
    attribute_id = EXCLUDED.attribute_id,
    attribute_value_id = EXCLUDED.attribute_value_id,
    raw_value = EXCLUDED.raw_value,
    attribute_role = EXCLUDED.attribute_role,
    sort_order = EXCLUDED.sort_order,
    updated_at = NOW();

-- ---------------------------------------------------------------------------
-- Demo media: Drive-backed snapshots pointing at the demo space (same pattern
-- as 001). Production tenants replace these with real Drive uploads.
-- ---------------------------------------------------------------------------
INSERT INTO commerce_product_media
    (id, uuid, tenant_id, organization_id, owner_type, owner_id, media_role,
     media_resource_id, resource_snapshot, alt_text, sort_order, status)
VALUES
    (2430, gen_random_uuid(), 100001, 0, 'spu', 1510, 'main_image', 90010,
     '{"id":"90010","kind":"image","source":"external_url","url":"https://picsum.photos/seed/router-x7-main/640/480","fileName":"router-x7-main.png","mimeType":"image/png"}'::jsonb,
     '星云 X7 路由器主图', 0, 'active'),
    (2431, gen_random_uuid(), 100001, 0, 'spu', 1511, 'main_image', 90011,
     '{"id":"90011","kind":"image","source":"external_url","url":"https://picsum.photos/seed/router-ax6-main/640/480","fileName":"router-ax6-main.png","mimeType":"image/png"}'::jsonb,
     '极光 AX6 路由器主图', 0, 'active'),
    (2432, gen_random_uuid(), 100001, 0, 'spu', 1512, 'main_image', 90012,
     '{"id":"90012","kind":"image","source":"external_url","url":"https://picsum.photos/seed/smart-hub-main/640/480","fileName":"smart-hub-main.png","mimeType":"image/png"}'::jsonb,
     '流明智能中控主图', 0, 'active'),
    (2433, gen_random_uuid(), 100001, 0, 'spu', 1513, 'main_image', 90013,
     '{"id":"90013","kind":"image","source":"external_url","url":"https://picsum.photos/seed/sensor-kit-main/640/480","fileName":"sensor-kit-main.png","mimeType":"image/png"}'::jsonb,
     '流明传感器套装主图', 0, 'active'),
    (2434, gen_random_uuid(), 100001, 0, 'spu', 1514, 'main_image', 90014,
     '{"id":"90014","kind":"image","source":"external_url","url":"https://picsum.photos/seed/nvme-ssd-main/640/480","fileName":"nvme-ssd-main.png","mimeType":"image/png"}'::jsonb,
     '星云 NVMe 固态硬盘主图', 0, 'active'),
    (2435, gen_random_uuid(), 100001, 0, 'spu', 1515, 'main_image', 90015,
     '{"id":"90015","kind":"image","source":"external_url","url":"https://picsum.photos/seed/usb-c-dock-main/640/480","fileName":"usb-c-dock-main.png","mimeType":"image/png"}'::jsonb,
     '极光 USB-C 扩展坞主图', 0, 'active'),
    (2436, gen_random_uuid(), 100001, 0, 'spu', 1516, 'main_image', 90016,
     '{"id":"90016","kind":"image","source":"external_url","url":"https://picsum.photos/seed/keyboard-k87-main/640/480","fileName":"keyboard-k87-main.png","mimeType":"image/png"}'::jsonb,
     '极光 K87 机械键盘主图', 0, 'active'),
    (2437, gen_random_uuid(), 100001, 0, 'spu', 1517, 'main_image', 90017,
     '{"id":"90017","kind":"image","source":"external_url","url":"https://picsum.photos/seed/purifier-p2-main/640/480","fileName":"purifier-p2-main.png","mimeType":"image/png"}'::jsonb,
     '流明 P2 空气净化器主图', 0, 'active'),
    (2438, gen_random_uuid(), 100001, 0, 'spu', 1518, 'main_image', 90018,
     '{"id":"90018","kind":"image","source":"external_url","url":"https://picsum.photos/seed/inst-network-main/640/480","fileName":"inst-network-main.png","mimeType":"image/png"}'::jsonb,
     '网络设备安装服务主图', 0, 'active'),
    (2440, gen_random_uuid(), 100001, 0, 'spu', 1510, 'gallery_image', 90020,
     '{"id":"90020","kind":"image","source":"external_url","url":"https://picsum.photos/seed/router-x7-gallery/640/480","fileName":"router-x7-gallery.png","mimeType":"image/png"}'::jsonb,
     '星云 X7 路由器场景图', 10, 'active')
ON CONFLICT (id) DO UPDATE SET
    owner_type = EXCLUDED.owner_type,
    owner_id = EXCLUDED.owner_id,
    media_role = EXCLUDED.media_role,
    media_resource_id = EXCLUDED.media_resource_id,
    resource_snapshot = EXCLUDED.resource_snapshot,
    alt_text = EXCLUDED.alt_text,
    sort_order = EXCLUDED.sort_order,
    status = EXCLUDED.status,
    updated_at = NOW();

-- ---------------------------------------------------------------------------
-- Retail price-list overrides on the existing CNY retail list (2100).
-- ---------------------------------------------------------------------------
INSERT INTO commerce_price_list_item
    (id, uuid, tenant_id, organization_id, price_list_id, sku_id, currency_code,
     price_scale, price_minor, min_quantity, status)
VALUES
    (2110, gen_random_uuid(), 100001, 0, 2100, 1630, 'CNY', 2, 44900, 1, 'active'),
    (2111, gen_random_uuid(), 100001, 0, 2100, 1631, 'CNY', 2, 44900, 1, 'active'),
    (2112, gen_random_uuid(), 100001, 0, 2100, 1632, 'CNY', 2, 35900, 1, 'active'),
    (2113, gen_random_uuid(), 100001, 0, 2100, 1633, 'CNY', 2, 35900, 1, 'active'),
    (2114, gen_random_uuid(), 100001, 0, 2100, 1634, 'CNY', 2, 119900, 1, 'active'),
    (2115, gen_random_uuid(), 100001, 0, 2100, 1635, 'CNY', 2, 119900, 1, 'active'),
    (2116, gen_random_uuid(), 100001, 0, 2100, 1636, 'CNY', 2, 25900, 1, 'active'),
    (2117, gen_random_uuid(), 100001, 0, 2100, 1637, 'CNY', 2, 69900, 1, 'active'),
    (2118, gen_random_uuid(), 100001, 0, 2100, 1638, 'CNY', 2, 42900, 1, 'active'),
    (2119, gen_random_uuid(), 100001, 0, 2100, 1640, 'CNY', 2, 45900, 1, 'active'),
    (2120, gen_random_uuid(), 100001, 0, 2100, 1640, 'CNY', 2, 43900, 10, 'active'),
    (2121, gen_random_uuid(), 100001, 0, 2100, 1641, 'CNY', 2, 45900, 1, 'active'),
    (2122, gen_random_uuid(), 100001, 0, 2100, 1642, 'CNY', 2, 169900, 1, 'active')
ON CONFLICT (id) DO UPDATE SET
    price_list_id = EXCLUDED.price_list_id,
    sku_id = EXCLUDED.sku_id,
    currency_code = EXCLUDED.currency_code,
    price_scale = EXCLUDED.price_scale,
    price_minor = EXCLUDED.price_minor,
    min_quantity = EXCLUDED.min_quantity,
    status = EXCLUDED.status,
    updated_at = NOW();
