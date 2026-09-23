// Table-to-code-path coverage gate for the merchandise baseline.
//
// `catalog-sql-baseline-closure.test.mjs` closes one direction of the baseline contract: every table
// a statement names exists, and every column it reads exists on the table it reads it from. The other
// direction is invisible to it. A table the baseline declares that no statement ever names is not an
// error there — and cannot be, because the baseline is authored ahead of the code on purpose, so
// "declared but not yet reached" is the normal state of a capability under construction rather than
// an anomaly.
//
// That is exactly why it needs a gate. A declared table with no reader and no writer is a promise
// nothing keeps: it carries CHECK constraints, foreign keys, and index definitions that look like
// agreed design, while no operation can populate or expose it. Seven of the seventeen tables here
// were in that state, and nothing in the repository said so.
//
// This gate makes the state explicit and keeps it explicit. Every table in the baseline must be
// either:
//
//   * **wired** — named by a non-comment source file under `crates/*/src/`, so some code path reads
//     or writes it; or
//   * **recorded** — named in `DECLARED_WITHOUT_A_CODE_PATH` below, together with the decision it is
//     waiting on.
//
// An entry in that list is a recorded gap, not a passing grade. Two rules keep it from decaying into
// a silencer: the list is compared against the wired set as an exact partition, so wiring a table
// without deleting its entry fails *and* deleting an entry without wiring the table fails; and every
// entry must also appear in `docs/architecture/tech/TECH_ARCHITECTURE.md` section 9, so a gap cannot
// be dropped from the code and the prose separately. The list is expected to shrink.
//
// Two deliberate exclusions, both of which exist to keep the gate honest:
//
//   * **Seeds are not a code path.** `database/seeds/**` inserts rows into several of these tables,
//     and a seed is data rather than an implementation: it proves a table can be populated, not that
//     any operation can. Counting seeds would have hidden `commerce_price_list_item`, which is
//     written by the bootstrap seed and by nothing else.
//   * **Comments are not a code path.** `postgres_catalog.rs` documents the tables it maps and
//     `TECH_ARCHITECTURE.md` names the unwired ones, so a raw text search reads prose as evidence.
//     `stripComments` is not a nicety here; it is the difference between a gate and a spelling check.
//     The mutation battery below uses exactly that as its control.
//
// # Non-vacuity
//
// Nineteen mutations were injected one at a time across this gate and the two closure gates the same
// change extended; every verdict matched the expectation, and the control that must *not* flip stayed
// green. Listed here are this file's eight, by the test that caught each:
//
// | mutation                                                              | verdict          | caught by |
// | --------------------------------------------------------------------- | ---------------- | --------- |
// | a *comment* naming a gap table added to the repository adapter          | green — control  | —         |
// | the same comment, with `stripComments` made a no-op                     | red              | wiring    |
// | one `DECLARED_WITHOUT_A_CODE_PATH` entry commented out                  | red              | wiring    |
// | the declared-gap count left at 7 while the list changed                 | red              | list size |
// | one entry's reason replaced with `TODO`                                 | red              | reasons   |
// | one gap table removed from `TECH_ARCHITECTURE.md` section 9             | red              | notes     |
// | `commerce_price_list_item` renamed out of the baseline DDL               | red              | wiring    |
// | a gap table named by a *non-comment* constant outside the adapter        | red              | adapter   |
//
// "wiring" is `every baseline table is either wired or recorded as a declared gap`; "notes" is
// `every declared gap is also recorded in the architecture notes`; "adapter" is `no recorded gap is
// wired from outside the repository adapter`.
//
// The control row is the one that matters: without it, the second row's red could be attributed to
// the comment being misread rather than to the stripping, which is the difference between a gate and
// a spelling check. The last row was added after the first battery run, where its weaker form — a
// comment, which the stripper removes — passed and proved nothing.
//
// The architecture-notes rule was also strengthened by the same run: a whole-document
// `includes(table)` was satisfied by an aside in section 10, so the check is now scoped to section 9,
// the section that records open work, and stops at 9.1, which lists what was closed.

import { readFileSync, readdirSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';
import { test } from 'node:test';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');

/** The working tree checks files out as CRLF; every pattern below is written for LF. */
function readText(relativePath) {
  return readFileSync(path.join(root, relativePath), 'utf8').replace(/\r\n/g, '\n');
}

/**
 * Removes Rust comments before a table name is searched for.
 *
 * This module's own documentation quotes table names, `postgres_catalog.rs` explains the tables it
 * maps in its module header, and `TECH_ARCHITECTURE.md` lists the ones nothing reaches. Prose is not
 * evidence of a code path, so every comment is removed before the search — and the mutation battery
 * above proves the removal is load-bearing rather than cosmetic.
 */
function stripComments(source) {
  return source.replace(/\/\*[\s\S]*?\*\//g, ' ').replace(/\/\/[^\n]*/g, ' ');
}

/** Every `.rs` file under `crates/<crate>/src/`, with comments removed, keyed by crate directory. */
function workspaceSources() {
  const byCrate = new Map();
  for (const crate of readdirSync(path.join(root, 'crates')).sort()) {
    const srcRoot = path.join(root, 'crates', crate, 'src');
    let star;
    try {
      star = statSync(srcRoot);
    } catch {
      continue;
    }
    if (!star.isDirectory()) continue;
    const chunks = [];
    const walk = (absolute) => {
      for (const entry of readdirSync(absolute).sort()) {
        const child = path.join(absolute, entry);
        if (statSync(child).isDirectory()) walk(child);
        else if (entry.endsWith('.rs')) {
          chunks.push(stripComments(readFileSync(child, 'utf8').replace(/\r\n/g, '\n')));
        }
      }
    };
    walk(srcRoot);
    byCrate.set(crate, chunks.join('\n'));
  }
  return byCrate;
}

const baseline = readText('database/ddl/baseline/postgres/0001_merchandise_baseline.sql');
const architecture = readText('docs/architecture/tech/TECH_ARCHITECTURE.md');

/**
 * Section 9 of the architecture notes, and only section 9.
 *
 * The check is scoped rather than run over the whole document because a table name appears in several
 * places for several reasons — section 10 discusses the vocabulary test that compares two tables
 * column by column — and a mention in an aside is not a record of a gap. Section 9.1 is excluded too:
 * it lists what was *closed*, so a name there says the opposite of what this gate needs to read.
 */
const openWorkSection = (() => {
  const start = architecture.indexOf('## 9. Open Work And Known Gaps');
  assert.notEqual(start, -1, 'TECH_ARCHITECTURE.md must keep a section 9 to record the gaps');
  const closed = architecture.indexOf('## 9.1', start);
  return architecture.slice(start, closed === -1 ? architecture.length : closed);
})();

const sourcesByCrate = workspaceSources();
const source = [...sourcesByCrate.values()].join('\n');

const baselineTables = [...baseline.matchAll(/CREATE TABLE IF NOT EXISTS ([a-z_]+)\s*\(/g)].map(
  (match) => match[1],
);

/**
 * Tables the baseline declares that no code path reaches, and the decision each is waiting on.
 *
 * Each reason has to say which decision is owed, because "not yet" is the state this gate exists to
 * record and a restatement of it would record nothing. The three groups below are genuinely
 * different kinds of work, which is why they are not collapsed into one note:
 *
 *   * the translation family is a missing feature — localization is in scope for a commercial
 *     catalog and simply has no write path, read model, or operation yet;
 *   * `commerce_product_spu_attribute` is a missing write path for a feature the template tables
 *     already model, so the gap is the operation rather than the schema;
 *   * `commerce_price_list_item` is a *design* question: a price-list item may be owed, or the table
 *     may be redundant next to `commerce_product_sku.sale_price_minor`, and the two answers have
 *     different consequences for the SKU write model.
 */
const DECLARED_WITHOUT_A_CODE_PATH = new Map([
  [
    'commerce_product_category_translation',
    'Localized category names have no write path, read model, or operation; the locale set is recorded in the seed manifest and nothing consumes it.',
  ],
  [
    'commerce_product_attribute_translation',
    'Localized attribute names have no write path, read model, or operation; the create command writes the default locale onto the base row only.',
  ],
  [
    'commerce_product_attribute_value_translation',
    'Localized dictionary values have no write path, read model, or operation; display_value on the base row is the only locale published.',
  ],
  [
    'commerce_product_spu_translation',
    'Localized product titles and descriptions have no write path, read model, or operation; the SPU create body accepts one title for the default locale.',
  ],
  [
    'commerce_product_sku_translation',
    'Localized SKU names have no write path, read model, or operation; the SKU create body accepts one name and one title for the default locale.',
  ],
  [
    'commerce_product_spu_attribute',
    'A product declares no attribute values: the category template reserves axes and parameters, and only the sales axes reach a write path today, through the SKU variant bindings. A parameter-value operation is owed.',
  ],
  [
    'commerce_price_list_item',
    'Four price decisions are needed before this can be wired or retired: whether per-SKU list prices exist alongside commerce_product_sku.sale_price_minor, which one a checkout reads, whether price_list_item carries its own currency scale, and what happens to item rows when a price list is deactivated.',
  ],
]);

const wired = baselineTables.filter((table) => source.includes(table));
const recorded = [...DECLARED_WITHOUT_A_CODE_PATH.keys()];
const unwired = baselineTables.filter((table) => !source.includes(table));

// --------------------------------------------------------------- assertions

test('the baseline, the architecture notes, and the workspace sources were all read', () => {
  assert.ok(baselineTables.length > 0, 'no table was parsed from the baseline');
  assert.ok(source.length > 0, 'no Rust source was read from crates/*/src');
  assert.ok(
    architecture.includes('## 9. Open Work And Known Gaps'),
    'TECH_ARCHITECTURE.md must keep a section 9 to record the gaps this gate recognises',
  );
  // Positive control: the search has to be capable of finding a table, or the partition below would
  // hold for every table trivially.
  assert.ok(
    wired.length >= 10,
    `expected at least the ten wired tables, saw ${wired.length}; a search that finds nothing would partition the baseline vacuously`,
  );
});

test('every baseline table is either wired or recorded as a declared gap', () => {
  const unrecorded = unwired.filter((table) => !recorded.includes(table));
  assert.deepEqual(
    unrecorded,
    [],
    'these tables exist in the baseline and no code path reaches them, so each needs an entry in DECLARED_WITHOUT_A_CODE_PATH naming the decision it waits on',
  );

  const wiredButRecorded = wired.filter((table) => recorded.includes(table));
  assert.deepEqual(
    wiredButRecorded,
    [],
    'these tables have a code path and are still recorded as gaps, so the record is stale; delete the entry rather than leaving the list overstating the gap',
  );

  assert.deepEqual(
    [...wired, ...unwired].sort(),
    [...baselineTables].sort(),
    'the two sets must partition the baseline exactly',
  );
});

test('the declared-gap list is exactly the size it says it is', () => {
  // The size is asserted rather than derived: a new declared table that nothing reaches must make
  // this list longer deliberately, with a reason, rather than arriving as an extra entry.
  assert.equal(
    DECLARED_WITHOUT_A_CODE_PATH.size,
    7,
    'the declared-gap list changed size; re-confirm each entry is still a gap, then update this count deliberately',
  );
});

test('every declared gap states the decision it is waiting on', () => {
  const problems = [];
  for (const [table, reason] of DECLARED_WITHOUT_A_CODE_PATH) {
    if (typeof reason !== 'string' || reason.trim().length < 60) {
      problems.push(`${table}: the reason names no decision (got \`${reason}\`)`);
      continue;
    }
    if (/^(todo|tbd|n\/a|pending)\b/i.test(reason.trim())) {
      problems.push(`${table}: \`${reason}\` records the gap without naming the work`);
    }
  }
  assert.deepEqual(problems, []);
});

test('every declared gap is also recorded in the architecture notes', () => {
  const undocumented = recorded.filter((table) => !openWorkSection.includes(table));
  assert.deepEqual(
    undocumented,
    [],
    'a gap kept only in this file is invisible to a reader of the architecture notes, and a gap kept only in the notes is invisible to a reviewer of the code; both have to say it, and the code-level list has to be answered in section 9 — the section that records open work — rather than anywhere the name happens to appear',
  );
});

test('no recorded gap is wired from outside the repository adapter', () => {
  // SQL belongs to the repository crate and to nothing else: that is the whole reason the port
  // exists. A table reached from any other crate would mean the rule broke, so this asserts the
  // converse of the gap list — every table that *is* reached is reached from the adapter. The check
  // matters more than it looks: without it, a table named in a comment-stripped but unrelated source
  // file would count as wired and its recorded gap would read as stale.
  const adapter = sourcesByCrate.get('sdkwork-merchandise-repository-sqlx');
  assert.ok(adapter, 'the repository adapter crate must exist under crates/');
  const wiredOutsideAdapter = wired.filter((table) => !adapter.includes(table));
  assert.deepEqual(
    wiredOutsideAdapter,
    [],
    'these tables have a code path outside the repository adapter, so either the path belongs in the adapter or the search is reading a name that is not SQL',
  );
});
