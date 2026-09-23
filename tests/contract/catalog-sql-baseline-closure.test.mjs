// Catalog SQL must close over the merchandise database baseline.
//
// The workspace-level `check-database-framework-standard.mjs` gate only checks
// the organization-id sentinel, prefix registration, and drift policy. It does
// NOT check contract-to-implementation closure, so a repository can read or
// write a table it does not own, or a table that does not exist at all, and the
// workspace gate still reports success. That is how `commerce_cart`,
// `commerce_cart_item`, and `commerce_user_address` writes survived inside a
// capability whose route manifest contained zero cart or address routes.
//
// This test closes the table-level and column-level halves of that gap for this
// repository.
//
// The column assertion is expected to be RED until the repository adapter is
// migrated to the baseline's `*_minor` money, `depth`, and `variant_signature`
// columns. That failure list IS the migration work order, which is why the gate
// lands before the fix rather than after it.

import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const baselinePath = path.join(
  root,
  "database/ddl/baseline/postgres/0001_merchandise_baseline.sql",
);

/** Table families this capability is the physical owner of. */
const OWNED_TABLE_PATTERNS = [
  /^commerce_product_/,
  /^commerce_price_list/,
  /^commerce_currency$/,
];

/** Table families explicitly owned by sibling capabilities. */
const FOREIGN_TABLE_PATTERNS = [
  /^commerce_cart/,
  /^commerce_user_address/,
  /^commerce_inventory_/,
  /^commerce_shop/,
  /^commerce_order/,
  /^commerce_payment/,
];

function baselineTables(ddl) {
  return [...ddl.matchAll(/CREATE TABLE (?:IF NOT EXISTS )?([a-z_]+)\s*\(/g)].map((m) => m[1]);
}

const COLUMN_TYPES =
  "BIGINT|TEXT|TIMESTAMPTZ|SMALLINT|BOOLEAN|INTEGER|INT4|JSONB|NUMERIC|UUID|DATE";

/** table -> Set<column>, parsed from the baseline DDL. */
function baselineColumns(ddl) {
  const tables = new Map();
  for (const match of ddl.matchAll(
    /CREATE TABLE (?:IF NOT EXISTS )?([a-z_]+)\s*\(([\s\S]*?)\n\);/g,
  )) {
    const columns = new Set(
      [...match[2].matchAll(new RegExp(`^\\s+([a-z_]+)\\s+(?:${COLUMN_TYPES})\\b`, "gm"))].map(
        (column) => column[1],
      ),
    );
    tables.set(match[1], columns);
  }
  return tables;
}

/**
 * Words that are never a column reference in catalog SQL.
 *
 * Everything not listed here that appears in a statement is treated as a
 * column candidate. A newly introduced keyword or function therefore fails
 * loudly and gets added here, which is the safe direction for a gate.
 */
const SQL_NON_COLUMN_WORDS = new Set([
  // clauses, operators, predicates
  "select", "from", "where", "and", "or", "not", "null", "is", "in", "insert",
  "into", "values", "update", "set", "delete", "returning", "on", "conflict",
  "do", "nothing", "group", "by", "order", "limit", "offset", "as", "left",
  "right", "inner", "outer", "join", "having", "distinct", "case", "when",
  "then", "else", "end", "asc", "desc", "exists", "between", "like", "ilike",
  "any", "all", "union", "with", "using", "default", "true", "false",
  // functions
  "count", "sum", "min", "max", "coalesce", "cast", "now", "char_length",
  "lower", "upper", "trim", "concat", "string_agg", "array_agg",
  "gen_random_uuid",
  // types
  "text", "bigint", "smallint", "integer", "numeric", "jsonb", "uuid", "date",
  "timestamptz", "int4", "varchar", "boolean",
  "interval", "current_timestamp", "timezone",
]);

/** Words that may follow a table name without being an alias. */
const ALIAS_STOP_WORDS = new Set([
  "where", "on", "set", "left", "right", "inner", "outer", "join", "select",
  "group", "order", "limit", "offset", "values", "returning", "and", "or",
  "using", "as", "cross", "full", "natural",
]);

/** String literals and `$n` placeholders are not identifiers. */
function stripLiterals(statement) {
  return statement.replace(/'(?:[^']|'')*'/g, " ").replace(/\$\d+/g, " ");
}

/** Table aliases declared by the statement, so `sku.status` reads as `status`. */
function statementAliases(statement) {
  const aliases = new Set();
  for (const match of statement.matchAll(
    /\b(?:from|join|update|insert\s+into)\s+([a-z_][a-z0-9_]*)(?:\s+(?:as\s+)?([a-z_][a-z0-9_]*))?/gi,
  )) {
    const alias = match[2]?.toLowerCase();
    if (alias && !ALIAS_STOP_WORDS.has(alias)) aliases.add(alias);
  }
  return aliases;
}

/** Column candidates in a statement: identifiers, minus keywords and aliases. */
function referencedColumns(statement) {
  const body = stripLiterals(statement);
  const aliases = statementAliases(body);
  const names = new Set();
  for (const match of body.matchAll(/\b([a-z][a-z0-9_]*)\b/g)) {
    const word = match[1];
    if (word.length < 2) continue;
    if (SQL_NON_COLUMN_WORDS.has(word)) continue;
    if (aliases.has(word)) continue;
    names.add(word);
  }
  return names;
}

/**
 * Pull SQL out of Rust source.
 *
 * Catalog SQL lives in raw string literals (`r#"..."#`), so a naive
 * single-quote scan silently finds nothing. Both forms are collected here.
 */
function sqlStatements(source) {
  const blocks = [];
  for (const match of source.matchAll(/r#*"([\s\S]*?)"#*/g)) blocks.push(match[1]);
  for (const match of source.matchAll(/"(?:\\.|[^"\\])*"/g)) blocks.push(match[0].slice(1, -1));
  return blocks.filter((block) => /\b(SELECT|INSERT INTO|UPDATE|DELETE FROM)\b/.test(block));
}

function rustSources(directory) {
  const files = [];
  const visit = (dir) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const absolute = path.join(dir, entry.name);
      if (entry.isDirectory()) visit(absolute);
      else if (entry.name.endsWith(".rs")) files.push(absolute);
    }
  };
  visit(directory);
  return files;
}

function referencedTables(statement) {
  const names = new Set();
  for (const match of statement.matchAll(
    /\b(?:from|join|insert into|update|delete from)\s+([a-z][a-z0-9_]*)/gi,
  )) {
    names.add(match[1]);
  }
  return names;
}

test("baseline declares only table families this capability owns", () => {
  const tables = baselineTables(readFileSync(baselinePath, "utf8"));
  assert.equal(tables.length, 17, "baseline table count changed; update the expectation deliberately");

  const foreign = tables.filter((t) => FOREIGN_TABLE_PATTERNS.some((p) => p.test(t)));
  assert.deepEqual(foreign, [], "baseline must not declare tables owned by sibling capabilities");

  const unowned = tables.filter((t) => !OWNED_TABLE_PATTERNS.some((p) => p.test(t)));
  assert.deepEqual(unowned, [], "baseline tables must belong to an owned family");
});

test("every table referenced by catalog SQL exists in the baseline", () => {
  const declared = new Set(baselineTables(readFileSync(baselinePath, "utf8")));
  const crateSources = path.join(root, "crates");
  const violations = [];

  for (const file of rustSources(crateSources)) {
    const source = readFileSync(file, "utf8");
    for (const statement of sqlStatements(source)) {
      for (const table of referencedTables(statement)) {
        if (!table.startsWith("commerce_")) continue;
        if (!declared.has(table)) {
          violations.push(`${path.relative(root, file)}: references undeclared table \`${table}\``);
        }
      }
    }
  }

  assert.deepEqual(
    [...new Set(violations)].sort(),
    [],
    "catalog SQL must only touch tables declared in the merchandise baseline",
  );
});

test("catalog SQL never touches foreign-capability tables", () => {
  const crateSources = path.join(root, "crates");
  const violations = [];

  for (const file of rustSources(crateSources)) {
    const source = readFileSync(file, "utf8");
    for (const statement of sqlStatements(source)) {
      for (const table of referencedTables(statement)) {
        if (FOREIGN_TABLE_PATTERNS.some((p) => p.test(table))) {
          violations.push(`${path.relative(root, file)}: touches foreign table \`${table}\``);
        }
      }
    }
  }

  assert.deepEqual(
    [...new Set(violations)].sort(),
    [],
    "carts, addresses, inventory, orders, and payments are owned by sibling capabilities",
  );
});

test("every column referenced by catalog SQL exists on a table the statement touches", () => {
  const columns = baselineColumns(readFileSync(baselinePath, "utf8"));
  const allColumns = new Set([...columns.values()].flatMap((set) => [...set]));
  const declared = new Set(columns.keys());
  const violations = [];

  for (const file of rustSources(path.join(root, "crates"))) {
    const source = readFileSync(file, "utf8");
    for (const statement of sqlStatements(source)) {
      const touched = [...referencedTables(statement)].filter((table) => declared.has(table));
      // A statement that touches known tables is held to those tables' columns.
      // A fragment with no recognizable table falls back to the whole baseline
      // rather than being silently skipped.
      const scope =
        touched.length > 0
          ? new Set(touched.flatMap((table) => [...columns.get(table)]))
          : allColumns;

      for (const name of referencedColumns(statement)) {
        if (name.startsWith("commerce_")) continue;
        if (scope.has(name)) continue;
        const existsElsewhere = allColumns.has(name);
        violations.push(
          `${path.relative(root, file)}: \`${name}\` ${
            existsElsewhere
              ? "is not a column of the table(s) this statement touches"
              : "is not a column in the merchandise baseline"
          }`,
        );
      }
    }
  }

  assert.deepEqual(
    [...new Set(violations)].sort(),
    [],
    "catalog SQL must only reference columns declared in the merchandise baseline",
  );
});
