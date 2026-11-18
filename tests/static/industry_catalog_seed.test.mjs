import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');

test('industry catalog seed is registered in the standard profile', () => {
  const manifest = JSON.parse(readFileSync(join(root, 'database/seeds/seed.manifest.json'), 'utf8'));
  const common = manifest.profiles?.standard?.common ?? [];
  assert.ok(
    common.includes('common/004_industry_catalog.sql'),
    'the standard seed profile must apply the industry catalog seed',
  );
  assert.equal(manifest.defaultLocale, 'zh-CN');
});

test('industry catalog seed generates at least 1000 products', () => {
  const sql = readFileSync(join(root, 'database/seeds/common/004_industry_catalog.sql'), 'utf8');
  const generation = /generate_series\(1,\s*(\d+)\)\s+AS g/.exec(sql);
  assert.ok(generation, 'the seed must generate products with generate_series');
  assert.ok(
    Number(generation[1]) >= 1000,
    `the seed must generate at least 1000 products, saw ${generation[1]}`,
  );
  for (const table of [
    'commerce_product_category',
    'commerce_product_brand',
    'commerce_product_spu',
    'commerce_product_sku',
    'commerce_product_sku_attribute',
    'commerce_product_spu_attribute',
    'commerce_product_media',
    'commerce_price_list_item',
  ]) {
    assert.match(sql, new RegExp(`INSERT INTO ${table}\\b`), `${table} is missing from the seed`);
  }
  assert.ok(
    sql.includes("'active'") && sql.includes("'draft'") && sql.includes("'archived'"),
    'the generated catalog must include the full status spread',
  );
});
