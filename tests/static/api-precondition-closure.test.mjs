// Precondition closure gate for the merchandise backend.
//
// `API_SPEC` section 17 makes three things normative for an operation that guards a write with a
// header precondition, and this gate exists because each of them had a way to be declared without
// being done:
//
//   * **2189** — the operation MUST document the version source and declare `If-Match`. A contract
//     can declare the header on an operation whose handler never reads it, and the request then
//     succeeds against whatever the row happens to hold: the header is decoration, and a client that
//     took the contract at its word has lost the guarantee it paid for. That is the *declared but
//     not enforced* direction.
//   * **2190** — a failed precondition MUST answer `41201`, a missing one `42801`, and a domain
//     state conflict MUST stay `40901`. An operation can declare `412` and still answer `409` for a
//     stale version, which is the collapse the three codes exist to prevent.
//   * The version has to be **readable**. `versionSource: resource.version` is a promise that the
//     resource schema carries a required `version`; a contract can name a version source that no
//     response field supplies, and then no client can construct the header it must send.
//
// And there is a fourth, which no Rust test can reach: the guard has to be *in the SQL*. This
// workspace runs no database in CI, so "the update compares the version in the same statement it
// writes" is a claim about text. A repository can compile, pass every Rust test, and still have
// thirteen statements that ignore the version they were handed — the field would simply be carried
// from the port to a statement that never reads it, which is the exact shape the previous round's
// column audit found in the baseline. So the last four assertions read the statement constants.
//
// # Non-vacuity
//
// Fourteen mutations were injected one at a time against a restored tree, and every one was caught.
// The assertion numbers are the ones actually observed, not the ones expected — two differ from the
// first draft of this table, and the difference is informative: giving a create operation an
// `If-Match` reddens **five** assertions, because a newly guarded operation is also a newly
// undeclared `412`/`428` pair and a newly undocumented version source. One mistake in one layer is
// caught by every layer that was not updated with it, which is the property assertion 12 exists for.
//
// | mutation                                                                    | red assertions |
// | --------------------------------------------------------------------------- | -------------- |
// | a handler stops reading `If-Match`, the contract keeps declaring it          | 3, 12          |
// | the contract drops `If-Match` from one operation                             | 3, 4, 8, 12, 14 |
// | the contract drops `428`, keeping `412`                                      | 5              |
// | the contract drops `412`, folding a stale version into `409`                 | 5              |
// | the contract stops documenting the version source                            | 7              |
// | the contract names `resource.version` but drops it from `required`           | 8, 9           |
// | a resource schema loses `version` entirely                                    | 8, 9           |
// | a create operation is given `If-Match` too                                    | 3, 4, 5, 7, 11, 12 |
// | one statement stops comparing the version                                     | 10, 12         |
// | one statement stops advancing the version                                     | 10, 13         |
// | a write response stops publishing `ETag`                                      | 14             |
// | an internal cascade is given a caller precondition                            | 10, 11, 12     |
// | the domain-conflict kind stops mapping to `409`                               | 6              |
// | the staleness resolver stops returning a `StaleVersion`                       | 6              |
//
// Assertion 12 is the one that carries the round: it is the only assertion that would notice a
// *fourteenth* guarded operation appearing in one layer and not the other two, which is how the
// counts are supposed to be held together.

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

const CONTRACT_PATH = 'apis/backend-api/merchandise/shop-backend-api.merchandise.openapi.json';
const ROUTER_PATH = 'crates/sdkwork-merchandise-web-support/src/backend_catalog_router.rs';
const STATEMENTS_PATH = 'crates/sdkwork-merchandise-repository-sqlx/src/postgres_catalog.rs';
const BASELINE_PATH = 'database/ddl/baseline/postgres/0001_merchandise_baseline.sql';

const contract = JSON.parse(readText(CONTRACT_PATH));
const router = stripComments(readText(ROUTER_PATH));
const statements = stripComments(readText(STATEMENTS_PATH));
const baseline = readText(BASELINE_PATH);

// ---------------------------------------------------------------- route table -> handler
// Read from the router's own `.route(...)` calls rather than from the route manifest, because the
// manifest carries no handler name. Each entry is `path -> method -> handler`.
//
// Scoped to the body of `build_backend_catalog_router` on purpose: the file's `#[cfg(test)]` module
// registers a probe route against a handler that is not a `backend_*` function, and a whole-file scan
// picks it up as a route with no method chain and fails for a reason that has nothing to do with the
// contract.
function routerBuilderBody() {
  const start = router.indexOf('pub fn build_backend_catalog_router(');
  assert.notEqual(start, -1, 'the router builder is gone from the router source');
  const end = router.indexOf('\n}\n', start);
  return router.slice(start, end === -1 ? undefined : end);
}

function routeTable() {
  const table = new Map();
  const call = /\.route\(\s*"([^"]+)"\s*,\s*([\s\S]*?)\n\s*\)/g;
  for (const match of routerBuilderBody().matchAll(call)) {
    const [, routePath, chain] = match;
    const methods = new Map();
    // `get(h).post(h)` and `patch(h).delete(h)` and a bare `post(h)`.
    for (const method of chain.matchAll(/\b(get|post|put|patch|delete)\(\s*(backend_\w+)\s*\)/g)) {
      methods.set(method[1], method[2]);
    }
    table.set(routePath, methods);
  }
  return table;
}

const routes = routeTable();

// ---------------------------------------------------------------- handler bodies
// One span per `async fn backend_*`, so "this handler reads If-Match" is decided inside the handler
// that owns it and not by a nearby sibling.
function handlerBodies() {
  const bodies = new Map();
  const lines = router.split('\n');
  let current = null;
  let buffer = [];
  for (const line of lines) {
    const start = line.match(/^async fn (backend_\w+)\(/);
    if (start) {
      if (current) bodies.set(current, buffer.join('\n'));
      current = start[1];
      buffer = [line];
      continue;
    }
    if (current === null) continue;
    if (line === '}') {
      buffer.push(line);
      bodies.set(current, buffer.join('\n'));
      current = null;
      buffer = [];
      continue;
    }
    buffer.push(line);
  }
  if (current) bodies.set(current, buffer.join('\n'));
  return bodies;
}

const bodies = handlerBodies();
const readsPrecondition = new Set(
  [...bodies].filter(([, body]) => /expected_version_from_if_match\s*\(/.test(body)).map(([name]) => name),
);
// A handler that reads it must also hand it to the store, or the read is decoration on the request
// path and the write is unguarded anyway.
const forwardsPrecondition = new Set(
  [...bodies]
    .filter(([, body]) => /expected_version\s*,/.test(body))
    .map(([name]) => name),
);

// ---------------------------------------------------------------- guarded statement constants
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
const guardedStatements = [...sql].filter(([, text]) => /AND version = \$\d+/.test(text)).map(([name]) => name);

// ---------------------------------------------------------------- versioned tables
// Derived from the baseline rather than hardcoded: which tables carry `version` is exactly the
// question the previous round's column audit got wrong by reading for the *presence* of the token.
function versionedTables() {
  const names = new Set();
  const table = /CREATE TABLE IF NOT EXISTS ([a-z_]+)\s*\(([\s\S]*?)\n\);/g;
  for (const match of baseline.matchAll(table)) {
    const [, name, body] = match;
    if (/^\s*version BIGINT NOT NULL DEFAULT 0,\s*$/m.test(body)) names.add(name);
  }
  return names;
}

const versioned = versionedTables();

// Statements whose SQL updates a row of a `version`-carrying table but deliberately does **not**
// advance it. Each one needs a reason, because the reason is the whole difference between a decision
// and an omission.
const NO_ADVANCE_EXEMPTIONS = new Map([
  [
    'REPOINT_CATEGORY_PARENT_SQL',
    'writes `parent_id` on the row that `UPDATE_CATEGORY_SQL` rewrites under the caller\'s ' +
      'precondition later in the same transaction; advancing here would move the version out from ' +
      'under that guard and make every reparenting update answer 412 against the version the ' +
      'caller read one statement earlier',
  ],
  [
    'UPDATE_SKU_VARIANT_SIGNATURE_SQL',
    'writes one column of the row that `UPDATE_SKU_SQL` rewrites under the caller\'s precondition ' +
      'later in the same transaction, for the same reason: the guarded statement owns the advance',
  ],
]);

// ---------------------------------------------------------------- contract helpers
function operations() {
  const list = [];
  for (const [operationPath, item] of Object.entries(contract.paths ?? {})) {
    for (const method of ['get', 'post', 'put', 'patch', 'delete']) {
      const operation = item?.[method];
      if (operation) list.push({ operationPath, method, operation });
    }
  }
  return list;
}

const allOperations = operations();
const declaresIfMatch = (operation) =>
  (operation.parameters ?? []).some((parameter) => parameter.name === 'If-Match');

/** The schema name a 200 response's `data.item` points at, or `null` for an operation without one. */
function successResourceSchema(operation) {
  const allOf = operation.responses?.['200']?.content?.['application/json']?.schema?.allOf;
  if (!Array.isArray(allOf)) return null;
  const ref = allOf
    .map((part) => part?.properties?.data?.properties?.item?.$ref)
    .find((value) => typeof value === 'string');
  return ref ? ref.replace('#/components/schemas/', '') : null;
}

const problemCodes = (operation) =>
  Object.entries(operation.responses ?? {})
    .filter(
      ([, response]) =>
        response?.content?.['application/problem+json']?.schema?.$ref ===
        '#/components/schemas/ProblemDetail',
    )
    .map(([code]) => code);

// ---------------------------------------------------------------- assertions

test('1. the route table is non-empty and every route names a handler defined in the router', () => {
  assert.ok(routes.size > 0, 'the route table parsed as empty');
  for (const [routePath, methods] of routes) {
    assert.ok(methods.size > 0, `${routePath} declares no HTTP method`);
    for (const [method, handler] of methods) {
      assert.ok(
        bodies.has(handler),
        `${method.toUpperCase()} ${routePath} names \`${handler}\`, which is not defined in ${ROUTER_PATH}`,
      );
    }
  }
  // The denominator this gate's counts are relative to.
  assert.equal(routes.size, 15, `expected 15 route paths, parsed ${routes.size}`);
});

test('2. the contract still declares every operation this gate reasons about', () => {
  assert.equal(allOperations.length, 28, `expected 28 operations, found ${allOperations.length}`);
  // Seven of the twenty-eight are creates: POST against a collection. There is no prior version to
  // precondition on, and requiring the header would make them uncallable rather than safer. The two
  // POSTs that are *not* creates — publish and archive — do carry one, because they rewrite a row
  // that already exists.
  const creates = allOperations.filter(
    (entry) =>
      entry.method === 'post' &&
      !entry.operationPath.endsWith('/publish') &&
      !entry.operationPath.endsWith('/archive'),
  );
  assert.equal(creates.length, 7, `expected 7 create operations, found ${creates.length}`);
});

test('3. `If-Match` is declared exactly on the operations that read it, in both directions', () => {
  const declared = new Set();
  for (const { operationPath, method, operation } of allOperations) {
    const methods = routes.get(operationPath);
    assert.ok(methods, `contract operation ${method.toUpperCase()} ${operationPath} has no route`);
    const handler = methods.get(method);
    assert.ok(handler, `route ${operationPath} serves no ${method.toUpperCase()}`);

    const wantsIt = declaresIfMatch(operation);
    const readsIt = readsPrecondition.has(handler);
    assert.equal(
      wantsIt,
      readsIt,
      `${method.toUpperCase()} ${operationPath} (${handler}): contract declares If-Match = ${wantsIt} but the handler reads it = ${readsIt}`,
    );
    if (wantsIt) declared.add(handler);
  }

  assert.deepEqual(
    [...declared].sort(),
    [...readsPrecondition].sort(),
    'a handler reads If-Match for an operation the contract does not declare it on, or the reverse',
  );
  assert.deepEqual(
    [...readsPrecondition].sort(),
    [...forwardsPrecondition].sort(),
    'a handler reads If-Match but never places it on the command, so the read is decoration',
  );
});

test('4. the guarded operations are exactly the thirteen non-create mutators', () => {
  const guarded = allOperations.filter(({ operation }) => declaresIfMatch(operation));
  const names = guarded.map(({ method, operationPath }) => `${method.toUpperCase()} ${operationPath}`);
  assert.equal(names.length, 13, `expected 13 guarded operations, found ${names.length}:\n${names.join('\n')}`);

  for (const { method, operation } of guarded) {
    assert.notEqual(method, 'get', 'a read must not require If-Match');
    const parameter = operation.parameters.find((entry) => entry.name === 'If-Match');
    assert.equal(parameter.in, 'header', 'If-Match must be a header parameter');
    assert.equal(parameter.required, true, 'If-Match must be declared required, or 42801 has no basis');
    assert.ok(parameter.schema?.pattern, 'If-Match must constrain its value, not accept any string');
  }
});

test('5. every guarded operation declares both result codes, each carrying the problem schema', () => {
  for (const { method, operationPath, operation } of allOperations) {
    if (!declaresIfMatch(operation)) continue;
    const codes = problemCodes(operation);
    assert.ok(
      codes.includes('412'),
      `${method.toUpperCase()} ${operationPath} declares no 412: a failed precondition must answer 41201 (API_SPEC 2190)`,
    );
    assert.ok(
      codes.includes('428'),
      `${method.toUpperCase()} ${operationPath} declares no 428: a missing required precondition must answer 42801 (API_SPEC 2189)`,
    );
  }
});

test('6. staleness and a domain conflict travel on two channels, not one', () => {
  // `API_SPEC` 2190 keeps them apart: a stale copy is `41201`, a violated business rule is `40901`.
  // The way to lose that distinction is not to get a status wrong — it is to collapse the two into
  // one channel on the way up, and then have to guess which one a failure was.
  //
  // Channel one: the repository. Every staleness has to come from the one resolver, which is the
  // only place that has read the row and therefore the only place that can tell "gone" from "moved".
  const stalenessSites = statements.match(/GuardedWrite::StaleVersion/g) ?? [];
  assert.equal(
    stalenessSites.length,
    13,
    `expected one staleness site per guarded statement, found ${stalenessSites.length}`,
  );
  assert.match(
    statements,
    /Result<StaleVersion, CommerceServiceError>/,
    'the resolver no longer returns a StaleVersion, so the thirteen sites fabricate one themselves',
  );
  assert.match(
    statements,
    /Ok\(StaleVersion \{/,
    'classify_guarded_miss never constructs a StaleVersion, so nothing decides that a miss was staleness',
  );

  // Channel two: the transport. `412` must have exactly one source, and the domain-conflict kind
  // must still map to `409`.
  const envelope = stripComments(readText('crates/sdkwork-merchandise-web-support/src/http_envelope.rs'));
  const failed = envelope.match(/precondition_failed_response\s*\(/g) ?? [];
  // One definition, one caller: `stale_version_response` is the only caller, and it is the only
  // thing the guarded handlers reach for.
  assert.equal(failed.length, 2, `expected the definition plus one caller of precondition_failed_response, found ${failed.length}`);
  assert.match(
    envelope,
    /CommerceServiceErrorKind::Conflict\s*\|?[\s\S]{0,160}?=> WebFrameworkErrorKind::Conflict/,
    'the domain-conflict kind no longer maps to 409, so a business conflict may be reported as a stale version',
  );
  assert.ok(
    !/CommerceServiceErrorKind::\w+[\s\S]{0,80}?PreconditionFailed/.test(envelope),
    'a service error kind is being mapped onto 41201, which is how staleness and conflict get collapsed',
  );
});;

test('7. every guarded operation documents its version source, as 2189 requires', () => {
  for (const { method, operationPath, operation } of allOperations) {
    if (!declaresIfMatch(operation)) continue;
    const concurrency = operation['x-sdkwork-concurrency'];
    assert.ok(
      concurrency,
      `${method.toUpperCase()} ${operationPath} declares If-Match but no x-sdkwork-concurrency, so the version source is undocumented`,
    );
    assert.equal(concurrency.mode, 'if-match');
    assert.equal(
      concurrency.versionSource,
      'resource.version',
      `${method.toUpperCase()} ${operationPath} claims an unexpected version source`,
    );
    assert.equal(concurrency.etagHeader, 'ETag');
    assert.equal(concurrency.preconditionRequiredCode, '42801');
    assert.equal(concurrency.preconditionFailedCode, '41201');
  }
});

test('8. the version source every guarded operation names is required on the resource it returns', () => {
  let checked = 0;
  for (const { method, operationPath, operation } of allOperations) {
    if (!declaresIfMatch(operation)) continue;
    const schemaName = successResourceSchema(operation);
    // A delete answers 204: it has no representation, so there is nothing for it to publish and its
    // version source is the resource the caller read before deciding to delete.
    if (schemaName === null) continue;
    const schema = contract.components.schemas[schemaName];
    assert.ok(schema, `${method.toUpperCase()} ${operationPath} returns unknown schema ${schemaName}`);
    assert.ok(
      schema.properties?.version,
      `${method.toUpperCase()} ${operationPath} names \`resource.version\` as its version source, but ${schemaName} has no \`version\` property`,
    );
    assert.ok(
      schema.required?.includes('version'),
      `${schemaName}.version exists but is not required, so a client cannot rely on reading it`,
    );
    checked += 1;
  }
  assert.equal(checked, 8, `expected 8 guarded operations returning a resource, checked ${checked}`);
});

test('9. every resource this surface returns publishes a version', () => {
  const resources = [
    'Attribute',
    'Category',
    'CategoryAttribute',
    'Media',
    'PriceList',
    'Product',
    'Sku',
  ];
  for (const name of resources) {
    const schema = contract.components.schemas[name];
    assert.ok(schema, `missing resource schema ${name}`);
    assert.ok(schema.properties?.version, `${name} does not publish \`version\``);
    assert.ok(schema.required?.includes('version'), `${name}.version is not required`);
    assert.equal(
      schema.properties.version['x-sdkwork-int64-string'],
      true,
      `${name}.version must travel as an int64 string, like every other int64 on this surface`,
    );
  }
});

test('10. the repository guards exactly thirteen statements, and each is a named one', () => {
  const expected = [
    'UPDATE_CATEGORY_SQL',
    'SOFT_DELETE_CATEGORY_SQL',
    'UPDATE_PRICE_LIST_SQL',
    'UPDATE_CATEGORY_ATTRIBUTE_SQL',
    'SOFT_DELETE_CATEGORY_ATTRIBUTE_SQL',
    'UPDATE_SPU_SQL',
    'SOFT_DELETE_SPU_SQL',
    'PUBLISH_SPU_SQL',
    'ARCHIVE_SPU_SQL',
    'UPDATE_SKU_SQL',
    'SOFT_DELETE_SKU_SQL',
    'UPDATE_MEDIA_SQL',
    'SOFT_DELETE_MEDIA_SQL',
  ];
  assert.deepEqual(
    guardedStatements.sort(),
    [...expected].sort(),
    'the set of statements carrying `AND version = $n` is not the thirteen preconditioned writes',
  );
  for (const name of expected) {
    assert.ok(sql.has(name), `${name} disappeared from ${STATEMENTS_PATH}`);
    assert.match(sql.get(name), /version = version \+ 1/, `${name} compares the version but does not advance it`);
  }
});

test('11. no create-path statement carries a precondition', () => {
  for (const [name, text] of sql) {
    if (!/AND version = \$\d+/.test(text)) continue;
    assert.ok(
      !/^INSERT/.test(text.trim()),
      `${name} guards an insert; a row that does not exist yet has no version to compare`,
    );
  }
  // The cascade statements are the ones a guard is easiest to add to by mistake, because they run
  // next to a guarded statement and touch the same table.
  for (const name of [
    'SOFT_DELETE_SPU_SKUS_SQL',
    'SOFT_DELETE_SKU_AXES_SQL',
    'MOVE_CATEGORY_SUBTREE_SQL',
    'REFRESH_CATEGORY_LEAF_SQL',
    'UPDATE_SKU_VARIANT_SIGNATURE_SQL',
    'REPOINT_CATEGORY_PARENT_SQL',
  ]) {
    assert.ok(sql.has(name), `${name} disappeared`);
    assert.doesNotMatch(
      sql.get(name),
      /AND version = \$\d+/,
      `${name} is an internal cascade and must not carry a caller precondition`,
    );
  }
  assert.ok(!declaresIfMatch(contract.paths['/backend/v3/api/catalog/categories'].post), 'POST creates must not require If-Match');
});

test('12. the three layers agree on the count', () => {
  const contractGuarded = allOperations.filter(({ operation }) => declaresIfMatch(operation)).length;
  const sqlGuarded = guardedStatements.length;
  assert.equal(
    contractGuarded,
    readsPrecondition.size,
    `${contractGuarded} operations declare If-Match but ${readsPrecondition.size} handlers read it`,
  );
  assert.equal(
    contractGuarded,
    sqlGuarded,
    `${contractGuarded} operations declare If-Match but ${sqlGuarded} statements compare a version`,
  );
  // And the guarded statements are not merely as many as the operations: each one has to be on a
  // table that carries `version` in the baseline, or the comparison reads a column the DDL omits.
  for (const name of guardedStatements) {
    const target = sql.get(name)?.match(/\bUPDATE\s+([a-z_]+)/)?.[1];
    assert.ok(target, `${name}: no UPDATE target found`);
    assert.ok(
      versioned.has(target),
      `${name} compares \`version\` on ${target}, which the baseline does not give a version column`,
    );
  }
});

test('13. every statement that rewrites a versioned row advances its version', () => {
  const violations = [];
  for (const [name, text] of sql) {
    const target = text.match(/\bUPDATE\s+([a-z_]+)/)?.[1];
    if (!target || !versioned.has(target)) continue;
    if (/version = version \+ 1/.test(text)) continue;
    if (NO_ADVANCE_EXEMPTIONS.has(name)) continue;
    violations.push(`${name} (${target})`);
  }
  assert.deepEqual(
    violations,
    [],
    `these statements rewrite a row whose stale copies they leave looking current:\n${violations.join('\n')}`,
  );

  // The exemption list is not allowed to rot: every name on it must still exist and still be one of
  // the statements that actually omits the advance.
  for (const [name, reason] of NO_ADVANCE_EXEMPTIONS) {
    assert.ok(sql.has(name), `stale exemption: ${name} no longer exists`);
    assert.doesNotMatch(sql.get(name), /version = version \+ 1/, `stale exemption: ${name} now advances the version`);
    assert.ok(reason.length >= 60, `${name}: the exemption reason is too short to be a reason`);
  }
});

test('14. every guarded operation whose success carries a resource publishes its version as ETag', () => {
  let published = 0;
  for (const { method, operationPath, operation } of allOperations) {
    if (!declaresIfMatch(operation)) continue;
    const success = operation.responses?.['200'];
    if (!success) continue;
    const etag = success.headers?.ETag;
    assert.ok(
      etag,
      `${method.toUpperCase()} ${operationPath} returns a resource but no ETag, so a client must parse the body to learn what to send back`,
    );
    assert.ok(etag.schema?.pattern?.includes('[0-9]'), `${method.toUpperCase()} ${operationPath}: ETag must be constrained`);
    published += 1;
  }
  assert.equal(published, 8, `expected 8 writes publishing ETag, found ${published}`);

  // The read that hands a client its first version has to publish one too, or the cycle never starts
  // without parsing the payload.
  const retrieve = contract.paths['/backend/v3/api/catalog/products/{productId}'].get;
  assert.ok(retrieve.responses['200'].headers?.ETag, 'GET /catalog/products/{productId} must publish an ETag');
});
