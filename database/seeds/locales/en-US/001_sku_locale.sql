-- ============================================================================
-- en-US locale seed: translated display values for the membership catalog.
-- ============================================================================
-- This file writes ONLY translation rows. Base tables hold machine fields plus
-- the default-locale (zh-CN) display value, which doubles as the fallback, so
-- an en-US row is a genuine override rather than a copy of the base value.
--
-- The previous version of this seed wrote `name` and `spec_json` back into
-- `commerce_product_sku`. That made zh-CN and en-US fight over one base column
-- (whichever locale ran last won) and buried localized marketing copy inside a
-- JSON blob where it could not be queried. Translation tables let every locale
-- coexist, and the resolution rule is: translation for the requested locale,
-- otherwise the base value.
--
-- Idempotent: upserts on the reserved translation ids declared in
-- database/contract/schema.yaml (block 3000..3999).
-- ============================================================================

INSERT INTO commerce_product_spu_translation
    (id, tenant_id, organization_id, spu_id, locale, field_name, value)
VALUES
    (3000, 100001, 0, 1500, 'en-US', 'name', 'Membership Catalog'),
    (3001, 100001, 0, 1500, 'en-US', 'title', 'Membership Catalog')
ON CONFLICT (id) DO UPDATE SET
    spu_id = EXCLUDED.spu_id,
    locale = EXCLUDED.locale,
    field_name = EXCLUDED.field_name,
    value = EXCLUDED.value,
    updated_at = NOW();

INSERT INTO commerce_product_sku_translation
    (id, tenant_id, organization_id, sku_id, locale, field_name, value)
VALUES
    (3100, 100001, 0, 1600, 'en-US', 'name', 'Basic Annual'),       (3101, 100001, 0, 1600, 'en-US', 'title', 'Basic Annual'),
    (3102, 100001, 0, 1601, 'en-US', 'name', 'Standard Annual'),    (3103, 100001, 0, 1601, 'en-US', 'title', 'Standard Annual'),
    (3104, 100001, 0, 1602, 'en-US', 'name', 'Premium Annual'),     (3105, 100001, 0, 1602, 'en-US', 'title', 'Premium Annual'),
    (3106, 100001, 0, 1603, 'en-US', 'name', 'Super Annual'),       (3107, 100001, 0, 1603, 'en-US', 'title', 'Super Annual'),
    (3108, 100001, 0, 1604, 'en-US', 'name', 'Basic Monthly'),      (3109, 100001, 0, 1604, 'en-US', 'title', 'Basic Monthly'),
    (3110, 100001, 0, 1605, 'en-US', 'name', 'Standard Monthly'),   (3111, 100001, 0, 1605, 'en-US', 'title', 'Standard Monthly'),
    (3112, 100001, 0, 1606, 'en-US', 'name', 'Premium Monthly'),    (3113, 100001, 0, 1606, 'en-US', 'title', 'Premium Monthly'),
    (3114, 100001, 0, 1607, 'en-US', 'name', 'Super Monthly'),      (3115, 100001, 0, 1607, 'en-US', 'title', 'Super Monthly'),
    (3116, 100001, 0, 1608, 'en-US', 'name', 'Basic Quarterly'),    (3117, 100001, 0, 1608, 'en-US', 'title', 'Basic Quarterly'),
    (3118, 100001, 0, 1609, 'en-US', 'name', 'Standard Quarterly'), (3119, 100001, 0, 1609, 'en-US', 'title', 'Standard Quarterly'),
    (3120, 100001, 0, 1610, 'en-US', 'name', 'Premium Quarterly'),  (3121, 100001, 0, 1610, 'en-US', 'title', 'Premium Quarterly'),
    (3122, 100001, 0, 1611, 'en-US', 'name', 'Super Quarterly'),    (3123, 100001, 0, 1611, 'en-US', 'title', 'Super Quarterly'),
    (3124, 100001, 0, 1612, 'en-US', 'name', 'Basic Single'),       (3125, 100001, 0, 1612, 'en-US', 'title', 'Basic Single'),
    (3126, 100001, 0, 1613, 'en-US', 'name', 'Standard Single'),    (3127, 100001, 0, 1613, 'en-US', 'title', 'Standard Single'),
    (3128, 100001, 0, 1614, 'en-US', 'name', 'Premium Single'),     (3129, 100001, 0, 1614, 'en-US', 'title', 'Premium Single'),
    (3130, 100001, 0, 1615, 'en-US', 'name', 'Super Single'),       (3131, 100001, 0, 1615, 'en-US', 'title', 'Super Single')
ON CONFLICT (id) DO UPDATE SET
    sku_id = EXCLUDED.sku_id,
    locale = EXCLUDED.locale,
    field_name = EXCLUDED.field_name,
    value = EXCLUDED.value,
    updated_at = NOW();

INSERT INTO commerce_product_attribute_translation
    (id, tenant_id, organization_id, attribute_id, locale, field_name, value)
VALUES
    (3200, 100001, 0, 1100, 'en-US', 'name', 'Tier'),
    (3201, 100001, 0, 1101, 'en-US', 'name', 'Billing Period')
ON CONFLICT (id) DO UPDATE SET
    attribute_id = EXCLUDED.attribute_id,
    locale = EXCLUDED.locale,
    field_name = EXCLUDED.field_name,
    value = EXCLUDED.value,
    updated_at = NOW();

INSERT INTO commerce_product_attribute_value_translation
    (id, tenant_id, organization_id, attribute_value_id, locale, field_name, value)
VALUES
    (3300, 100001, 0, 1200, 'en-US', 'display_value', 'Basic'),
    (3301, 100001, 0, 1201, 'en-US', 'display_value', 'Standard'),
    (3302, 100001, 0, 1202, 'en-US', 'display_value', 'Premium'),
    (3303, 100001, 0, 1203, 'en-US', 'display_value', 'Super'),
    (3304, 100001, 0, 1210, 'en-US', 'display_value', 'Annual'),
    (3305, 100001, 0, 1211, 'en-US', 'display_value', 'Quarterly'),
    (3306, 100001, 0, 1212, 'en-US', 'display_value', 'Monthly'),
    (3307, 100001, 0, 1213, 'en-US', 'display_value', 'Single Month')
ON CONFLICT (id) DO UPDATE SET
    attribute_value_id = EXCLUDED.attribute_value_id,
    locale = EXCLUDED.locale,
    field_name = EXCLUDED.field_name,
    value = EXCLUDED.value,
    updated_at = NOW();
