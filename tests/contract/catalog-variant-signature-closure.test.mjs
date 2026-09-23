// The variant signature is a convention shared by three places that cannot see each other: the Rust
// builder in the repository crate, the `variant_signature` values the baseline seed stores, and the
// `uk_commerce_product_sku_variant` unique index that compares them. The index only means "one
// sellable unit per axis combination" if all three agree, and nothing checked that they did.
//
// The convention, from `postgres_catalog.rs::build_variant_signature`: one `attribute_no=value_code`
// term per axis, ordered by `attribute_no`, joined with `;`; a SKU on no axis falls back to its own
// `sku_no`, because the empty combination has no storage form (`char_length(variant_signature)
// BETWEEN 1 AND 500`).
//
// This gate recomputes that convention from the seed's own dictionary and axis rows and compares the
// result with the `variant_signature` each seed row stores. It is deliberately a *recomputation*
// rather than a re-listing: a stored signature that was hand-edited, or a dictionary row renamed
// without updating the SKUs that reference it, produces a different string here. It also reads the
// baseline DDL for the index that makes the ordering key unique, because "ordered by `attribute_no`"
// is only a total order while that index holds.
//
// The companion Rust unit tests cover the two properties the seed cannot demonstrate — that the
// builder sorts when handed an unordered set, and that it refuses an over-long combination — because
// the seed's rows are all single-valued per axis and all far inside the bound.
//
// # Non-vacuity
//
// Recomputing is only worth anything if the recomputation can disagree, and the seed is the input
// that decides. Each row below was applied and the gate run; the four "must redden" rows went red
// naming the SKU or the index, and the one row that must stay green did, which is itself the
// assertion that the convention is keyed on `attribute_no` and not on `sort_order`.
//
// | mutation                                                       | expected | observed |
// | -------------------------------------------------------------- | -------- | -------- |
// | one value's `value_code` in the dictionary changed              | red      | red      |
// | one SKU's stored `variant_signature` edited by hand             | red      | red      |
// | the two axes exchange their `attribute_no` (only the order moves)| red     | red      |
// | the two axes exchange their `sort_order`                        | green    | green    |
// | the ordering key's unique index narrowed to `(tenant_id)`       | red      | red      |
//
// The third row is the one that separates the two candidate ordering keys. The seed's axes are
// `tier`/`period` by `attribute_no` but 10/20 by `sort_order`, so the two keys disagree about which
// term comes first; the seed's signatures are `period=…;tier=…`, i.e. `attribute_no` order. Swapping
// the `attribute_no` values leaves the set of business keys untouched and only moves the order, so a
// gate that ordered by `sort_order` (or by insertion order) would have stayed green there. The
// fourth row is its mirror: moving `sort_order` must not change an answer.
//
// The first row's failure report reads `stored \`period=annual;tier=basic\` but its 2 axis row(s)
// recompute to \`period=annual;tier=basicx\``, which names the SKU so the red rows are
// distinguishable in a report even though three of them redden the same assertion.
//
// Every probe wrote back the file it touched; the seed and the DDL were both confirmed byte-identical
// afterwards, because a battery that leaves the tree mutated is worse than no battery.

import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';
import { test } from 'node:test';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const SEED = 'database/seeds/common/001_merchandise_bootstrap.sql';
const DDL = 'database/ddl/baseline/postgres/0001_merchandise_baseline.sql';

const seed = readFileSync(path.join(root, SEED), 'utf8').replace(/\r\n/g, '\n');

/**
 * Splits a `VALUES` tuple list into tuples of raw value text, respecting single quotes.
 *
 * Parenthesis depth is tracked rather than "the next `)` ends the tuple", because the seed's values
 * include function calls: `NOW()` in every `created_at`/`updated_at` column, and a shallow parser
 * treats the `(` of `NOW(` as the start of a new tuple, discarding the row's columns so far and
 * handing back a one-value tuple. Quoting is respected for the same reason, so a literal containing
 * `,` or `(` does not split a row.
 */
function tuplesOf(valuesText) {
  const tuples = [];
  let current = null;
  let inQuote = false;
  let depth = 0;
  for (let index = 0; index < valuesText.length; index += 1) {
    const char = valuesText[index];
    if (char === "'") {
      // `''` is an escaped quote inside a literal, not the end of one.
      if (inQuote && valuesText[index + 1] === "'") {
        if (current) current.push("''");
        index += 1;
        continue;
      }
      inQuote = !inQuote;
      if (current) current.push(char);
      continue;
    }
    if (!inQuote && (char === '(' || char === ')')) {
      if (char === '(') {
        depth += 1;
        // Only the outermost `(` opens a tuple; a nested one belongs to the value (`NOW()`).
        if (depth === 1) {
          current = [];
          continue;
        }
      } else {
        depth -= 1;
        if (depth === 0) {
          tuples.push(current.join(''));
          current = null;
          continue;
        }
      }
    }
    if (current) current.push(char);
  }
  return tuples.map((tuple) => splitValues(tuple));
}

/** Splits one tuple body on top-level commas, respecting single quotes. */
function splitValues(tuple) {
  const values = [];
  let current = '';
  let inQuote = false;
  for (let index = 0; index < tuple.length; index += 1) {
    const char = tuple[index];
    if (char === "'") {
      if (inQuote && tuple[index + 1] === "'") {
        current += "''";
        index += 1;
        continue;
      }
      inQuote = !inQuote;
      current += char;
      continue;
    }
    if (char === ',' && !inQuote) {
      values.push(current.trim());
      current = '';
      continue;
    }
    current += char;
  }
  values.push(current.trim());
  return values;
}

/** Unwraps a SQL literal: `'text'` becomes `text`, anything else is kept as written. */
function literal(value) {
  const trimmed = value.trim();
  if (trimmed.startsWith("'") && trimmed.endsWith("'")) {
    return trimmed.slice(1, -1).replaceAll("''", "'");
  }
  return trimmed;
}

/**
 * Rows of one `INSERT INTO <table> (...columns...) VALUES ...` statement, keyed by column name.
 *
 * The column list is read rather than assumed, because the statements here write every column and a
 * positional parser would silently map the wrong value the first time a column is inserted in the
 * middle — which is exactly the kind of change a baseline edit makes.
 */
function rowsOf(table) {
  const start = seed.indexOf(`INSERT INTO ${table}\n`);
  assert.notEqual(start, -1, `${SEED} must insert into ${table}`);
  const statement = seed.slice(start, seed.indexOf('ON CONFLICT', start) === -1 ? undefined : seed.indexOf('ON CONFLICT', start));
  const columns = /\(([^)]*)\)\s*\nVALUES/.exec(statement);
  assert.ok(columns, `${table}: the insert must name its columns`);
  const names = columns[1].split(',').map((name) => name.trim());
  const valuesStart = statement.indexOf('VALUES');
  return tuplesOf(statement.slice(valuesStart + 'VALUES'.length)).map((values) => {
    assert.equal(
      values.length,
      names.length,
      `${table}: a tuple has ${values.length} values for ${names.length} columns`,
    );
    return Object.fromEntries(names.map((name, position) => [name, literal(values[position])]));
  });
}

const attributes = rowsOf('commerce_product_attribute');
const attributeValues = rowsOf('commerce_product_attribute_value');
const skus = rowsOf('commerce_product_sku');
const skuAxes = rowsOf('commerce_product_sku_attribute');

const attributeNoById = new Map(attributes.map((row) => [row.id, row.attribute_no]));
const valueById = new Map(attributeValues.map((row) => [row.id, row]));

/** Recomputes one SKU's signature from the seed's dictionary and axis rows. */
function recomputeSignature(sku) {
  const axes = skuAxes
    .filter((row) => row.sku_id === sku.id)
    .map((row) => {
      const attributeNo = attributeNoById.get(row.attribute_id);
      assert.ok(
        attributeNo,
        `sku ${sku.id} references attribute ${row.attribute_id}, which the seed does not declare`,
      );
      const value = valueById.get(row.attribute_value_id);
      assert.ok(
        value,
        `sku ${sku.id} references value ${row.attribute_value_id}, which the seed does not declare`,
      );
      assert.equal(
        value.attribute_id,
        row.attribute_id,
        `sku ${sku.id} pairs attribute ${row.attribute_id} with value ${row.attribute_value_id}, whose attribute is ${value.attribute_id}`,
      );
      return { attributeNo, valueCode: value.value_code };
    });

  if (axes.length === 0) return sku.sku_no;

  return axes
    .sort((left, right) =>
      left.attributeNo < right.attributeNo ? -1 : left.attributeNo > right.attributeNo ? 1 : 0,
    )
    .map((axis) => `${axis.attributeNo}=${axis.valueCode}`)
    .join(';');
}

// --------------------------------------------------------------- assertions

test('the seed declares the dictionary, the SKUs, and their axes', () => {
  assert.ok(attributes.length >= 2, `expected the seeded attributes, saw ${attributes.length}`);
  assert.ok(attributeValues.length >= 4, `expected the seeded values, saw ${attributeValues.length}`);
  assert.ok(skus.length >= 8, `expected the seeded SKUs, saw ${skus.length}`);
  assert.ok(skuAxes.length >= 8, `expected the seeded SKU axes, saw ${skuAxes.length}`);
});

// Sorting by `attribute_no` alone only yields one answer if `attribute_no` is unique per tenant.
// `build_variant_signature` breaks ties on `attribute_id`, but that tie-break is unreachable while
// this index holds — so the index, not the tie-break, is what makes the order total. Asserting the
// index means a future narrowing of it is reported here rather than silently moving the signature's
// determinism onto the Rust fallback.
test('the ordering key is unique per tenant, so the axis order is total', () => {
  const ddl = readFileSync(path.join(root, DDL), 'utf8').replace(/\r\n/g, '\n');
  const index =
    /CREATE UNIQUE INDEX IF NOT EXISTS uk_commerce_product_attribute_tenant_no\s*\n\s*ON commerce_product_attribute \(([^)]*)\)\s*\n\s*WHERE deleted_at IS NULL;/.exec(
      ddl,
    );
  assert.ok(
    index,
    `${DDL} must keep a partial unique index over the axis ordering key, or the signature order stops being total`,
  );
  assert.deepEqual(
    index[1].split(',').map((column) => column.trim()),
    ['tenant_id', 'attribute_no'],
    'the ordering key must stay unique per tenant among live rows',
  );
});

test('every seeded variant signature is the one its axes recompute to', () => {
  const mismatches = [];
  let withAxes = 0;
  let withoutAxes = 0;

  for (const sku of skus) {
    const axisCount = skuAxes.filter((row) => row.sku_id === sku.id).length;
    if (axisCount === 0) withoutAxes += 1;
    else withAxes += 1;

    const expected = recomputeSignature(sku);
    if (sku.variant_signature !== expected) {
      mismatches.push(
        `sku ${sku.id} (${sku.sku_no}): the seed stores \`${sku.variant_signature}\` but its ${axisCount || 'zero'} axis row(s) recompute to \`${expected}\``,
      );
    }
  }

  assert.deepEqual(
    mismatches,
    [],
    'the stored signature and the recomputed convention must agree, or a hand-edited row or a renamed dictionary entry would let two SKUs share one logical variant',
  );
  assert.ok(
    withAxes > 0,
    'no seeded SKU carries an axis, so the recomputation above never exercised the term format',
  );
  // The fallback branch is the Rust unit tests\' subject; if the seed ever gains a SKU on no axis
  // this becomes reachable here too, and asserting the count is what notices that.
  assert.equal(
    withoutAxes,
    0,
    'a seeded SKU with no axis rows would take the `sku_no` fallback, which this gate does not currently exercise',
  );
});
