// Status-derivation closure gate for the merchandise catalog.
//
// `commerce_product_spu` and `commerce_product_sku` carry the same two status columns under the same
// three constraints: a four-value `status` CHECK, a two-value `sales_status` CHECK, and a
// there-is-no-selling-what-is-not-active CHECK. Two tables that agree today can stop agreeing in one
// edit, and the repository's own doc comments claim they cannot: `UPDATE_SPU_SQL` says it derives
// `sales_status` "the same three lines `UPDATE_SKU_SQL` uses, so the two tables cannot drift apart
// in what `active` means". That claim is worth a gate for the reason this workspace already keeps a
// gate over the SQL text at all: no database runs in CI, so every such claim is a claim about text.
//
// The specific things that can go wrong, and why each is invisible without this gate:
//
//   * **`sales_status` becomes a parameter.** It is a *derived* column — `active` if and only if
//     `status = 'active'` — and the moment it is bound from the caller a request can assert a
//     sellable row that is not active, which the CHECK rejects with a `23514` that names nothing the
//     caller did. The SKU once had this shape; the parameter is what this assertion forbids.
//   * **The two derivations drift.** If the SPU derives `sales_status` from a different expression
//     than the SKU, a product and its variant can disagree about being on sale, and the
//     `sales_status = 'active'` filters in `LIST_SKUS_SQL` start returning a set no caller expects.
//     Textual equality after placeholder normalization is the cheapest honest way to hold two
//     statements to one rule.
//   * **`published_at` is cleared.** `ck_...(published_at) CHECK (published_at IS NULL OR status <>
//     'draft')` makes publishing a one-way transition, and nothing in this repository clears the
//     column, which is what makes `status = 'draft'` reachable only before the first publish. An
//     edit that "resets" the column to make a return to `draft` possible would silently rewrite the
//     documented lifecycle of every product that has one.
//   * **A literal status is misspelled.** `'archive'` instead of `'archived'` compiles, is bound by
//     no type, and fails at run time against the CHECK.
//   * **A create stops starting at `draft`.** The catalog's callers read a freshly created product
//     as unpublished — that is what the `draft` insert is for — and a create that started a row
//     `active` would publish a product nobody published.
//
// # Non-vacuity
//
// Seven mutations were injected one at a time against an otherwise restored tree, and the assertion
// numbers below are the ones actually observed. Two of them shaped the assertions rather than merely
// confirming them, which is why they are listed:
//
//   * M2 is why assertion 1 exists separately. Normalizing placeholders erases *which* column an
//     expression was derived from, so a statement that derives `sales_status` from the title instead
//     of from its own status passes assertion 2 unmodified. Only assertion 1 sees it, because only
//     assertion 1 compares the placeholder rather than the shape around it.
//   * M7 is why assertion 1 does not simply skip a statement it cannot find a status placeholder in.
//     Deleting `status = COALESCE($n::TEXT, status)` leaves the `CASE` behind, and a version of this
//     assertion that only checked "if a status is bound, the derivation uses it" would have called
//     that consistent — while the write ignored every status the caller sent.
//
// | mutation                                                                     | red assertions |
// | ---------------------------------------------------------------------------- | -------------- |
// | M1 `UPDATE_SPU_SQL` binds `sales_status = $7` instead of deriving it           | 1, 2           |
// | M2 `UPDATE_SPU_SQL` derives `sales_status` from `$1`, the title                | 1              |
// | M3 the SKU's derivation switches its `'active'` and `'inactive'` branches       | 2              |
// | M4 `SOFT_DELETE_SPU_SQL` also clears `published_at`, enabling a return to draft | 3              |
// | M5 `PUBLISH_SPU_SQL` becomes `status = 'publish'`                               | 4              |
// | M6 `INSERT_SKU_SQL` starts a row `active`                                       | 5              |
// | M7 `UPDATE_SPU_SQL` stops accepting a status at all                             | 1              |
//
// Assertion 2 is the one that carries the round's premise: it is the only assertion that would
// notice the two statements answering "is this on sale" differently while each remains internally
// consistent.

import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';
import { test } from 'node:test';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');

/** The working tree checks files out as CRLF; every pattern below is written for LF. */
function readText(relativePath) {
  return readFileSync(path.join(root, relativePath), 'utf8').replace(/\r\n/g, '\n');
}

function stripComments(source) {
  return source.replace(/\/\*[\s\S]*?\*\//g, ' ').replace(/\/\/[^\n]*/g, ' ');
}

const STATEMENTS_PATH = 'crates/sdkwork-merchandise-repository-sqlx/src/postgres_catalog.rs';
const BASELINE_PATH = 'database/ddl/baseline/postgres/0001_merchandise_baseline.sql';

const statements = stripComments(readText(STATEMENTS_PATH));
const baseline = readText(BASELINE_PATH);

/** The literal SQL of a `const NAME_SQL: &str` item, with `concat!` arms joined. */
function statementConstants() {
  const found = new Map();
  const item = /^const ([A-Z_]+_SQL): &str = ([\s\S]*?);$/gm;
  for (const match of statements.matchAll(item)) {
    const [, name, body] = match;
    const literals = [...body.matchAll(/"((?:[^"\\]|\\.)*)"/g)].map((part) => part[1]);
    found.set(name, literals.join(''));
  }
  return found;
}

const sql = statementConstants();

/** Placeholders are positional, so two statements that agree can still spell them differently. */
function normalizePlaceholders(text) {
  return text.replace(/\$\d+/g, '$n').replace(/\s+/g, ' ').trim();
}

/**
 * The baseline's own vocabulary for a table's `status` column.
 *
 * Read from the CHECK rather than hardcoded, because the CHECK is the model: a fifth status added to
 * the DDL should widen this gate without anyone remembering to come back to it.
 */
function statusVocabulary(table) {
  const constraint = new RegExp(`CONSTRAINT ck_${table}_status\\s+CHECK \\(status IN \\(([^)]*)\\)\\)`);
  const match = baseline.match(constraint);
  assert.ok(match, `the baseline no longer declares ck_${table}_status`);
  return match[1]
    .split(',')
    .map((token) => token.trim())
    .filter((token) => token.length > 0)
    .map((token) => token.replace(/^'|'$/g, ''));
}

/** The `CASE WHEN ... END` expression a statement assigns to `sales_status`, if it assigns one. */
function salesStatusExpression(text) {
  const match = text.match(/sales_status = (CASE WHEN[\s\S]*?END)/);
  return match ? match[1] : null;
}

test('1. `sales_status` is derived, never bound, and always from the statement\'s own status', () => {
  const writing = [...sql].filter(([, text]) => /sales_status\s*=/.test(text));
  assert.ok(writing.length >= 5, `only ${writing.length} statements assign sales_status`);

  for (const [name, text] of writing) {
    assert.ok(
      !/sales_status = \$/.test(text),
      `${name} binds sales_status from the caller; it is derived from status, and a caller that can ` +
        'set it independently can assert a sellable row the CHECK then refuses',
    );

    // The three statements that write a literal pair (`publish`, `archive`, soft delete) state the
    // derived column outright and have no status expression to agree with. The rest accept a status
    // from the caller, and those are the ones this assertion is about.
    const expression = salesStatusExpression(text);
    if (!expression) continue;

    const derivedFrom = expression.match(/COALESCE\((\$\d+)::TEXT, status\)/);
    assert.ok(
      derivedFrom,
      `${name} derives sales_status from something other than a status it accepted from the caller`,
    );

    const placeholder = derivedFrom[1];
    assert.ok(
      text.includes(`status = COALESCE(${placeholder}::TEXT, status)`),
      `${name} derives sales_status from ${placeholder} but never assigns status from ${placeholder}, ` +
        'so the derived column can contradict a status the statement refuses to accept — and the ' +
        'caller asking for that status is told the write succeeded',
    );
  }
});

test('2. the SPU and the SKU derive `sales_status` with one rule, not two', () => {
  const spu = salesStatusExpression(sql.get('UPDATE_SPU_SQL') ?? '');
  const sku = salesStatusExpression(sql.get('UPDATE_SKU_SQL') ?? '');

  assert.ok(spu, 'UPDATE_SPU_SQL no longer derives sales_status');
  assert.ok(sku, 'UPDATE_SKU_SQL no longer derives sales_status');
  assert.equal(
    normalizePlaceholders(spu),
    normalizePlaceholders(sku),
    'the product and its variants now answer "is this on sale" differently, so `sales_status` ' +
      'filters in the list statements no longer mean one thing',
  );
});

test('3. `published_at` is never cleared, because publishing is a one-way transition', () => {
  // `NOW()` is itself a call, so the assignment matcher has to survive one level of nesting.
  const assignment = /published_at = (CASE WHEN[\s\S]*?END|COALESCE\([^()]*(?:\([^()]*\)[^()]*)*\))/g;
  for (const [name, text] of sql) {
    assert.ok(
      !/published_at = NULL/.test(text),
      `${name} clears published_at; ck_..._published_at is ` +
        '`published_at IS NULL OR status <> \'draft\'`, so clearing it is how a published row would ' +
        'become a draft again — and the baseline says publishing is one-way',
    );
    assert.ok(
      !/published_at = \$/.test(text),
      `${name} binds published_at from the caller, which puts a recorded instant back in play`,
    );
    // `NOW()` is itself a call, so the assignment matcher has to survive one level of nesting.
    for (const [, assigned] of text.matchAll(assignment)) {
      assert.ok(
        /COALESCE\(published_at, NOW\(\)\)/.test(assigned),
        `${name} assigns published_at ${assigned}, which is neither a first recording nor a ` +
          'leave-it-alone arm',
      );
    }
  }
});

test('4. every literal status a statement writes is one the baseline admits', () => {
  const admitted = new Set([...statusVocabulary('commerce_product_spu'), ...statusVocabulary('commerce_product_sku')]);
  assert.deepEqual(
    [...admitted].sort(),
    ['active', 'archived', 'draft', 'inactive'],
    'the baseline status vocabulary moved; this gate is written against the four-value set',
  );

  let seen = 0;
  for (const [name, text] of sql) {
    for (const [, literal] of text.matchAll(/status = '([^']*)'/g)) {
      seen += 1;
      assert.ok(
        admitted.has(literal),
        `${name} writes status = '${literal}', which ck_..._status does not admit`,
      );
    }
  }
  assert.ok(seen >= 3, `only ${seen} literal status assignments were checked`);
});

test('5. both product inserts start a row unpublished, which is what `draft` means here', () => {
  for (const name of ['INSERT_SPU_SQL', 'INSERT_SKU_SQL']) {
    const text = sql.get(name);
    assert.ok(text, `${name} disappeared from ${STATEMENTS_PATH}`);
    assert.match(
      text,
      /'draft', 'inactive'\)/,
      `${name} no longer starts the row as (draft, inactive); a create that starts a row active ` +
        'publishes a product nobody published',
    );
  }
});
