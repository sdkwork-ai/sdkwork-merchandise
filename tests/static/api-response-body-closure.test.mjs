// Response-body closure gate for the merchandise backend contract.
//
// The authored OpenAPI document names every response field, the Rust response structs decide what
// is actually serialized, and the baseline DDL owns the enumerations. Nothing else compares them:
// `api:check` is a synchronisation check, `check-api-response-envelope` only asserts the envelope
// shape, and `check-api-operation-patterns` only inspects schemas the contract declares. A response
// schema could name zero fields, or name a different set from the struct that produces it, with
// every gate green.
//
// The contract surface here is the mirror of `api-request-body-closure.test.mjs`, and the two share
// their conventions on purpose: `required` is the non-`Option` fields, an `Option<T>` is declared
// `["<t>", "null"]` and left out of `required`, every `i64` is a decimal string, and every `enum`
// must equal the CHECK set of the column behind it, scoped by table.
//
// # Two allowances, both of them narrower than they look
//
// The media surface adds the first response field whose Rust type is neither a scalar nor an `Option`
// of one: `resource_snapshot` is a `serde_json::Value`, so the field-set comparison cannot see inside
// it and the type map has no entry for it. Neither gap is papered over.
//
// `SHARED_COMPONENT_SCHEMAS` says which component schemas are models rather than published resources,
// so the one-to-one mapping stays exact instead of being loosened to "at least one `*Response` struct
// each"; every entry has to state a reason about the model, and an unlisted name still fails.
// `an opaque JSON read model points at a declared, closed shared schema` then requires the one thing
// that is checkable — the document must name a declared, closed schema rather than shrug — and the
// shape of that schema is compared with the Rust validator by the request gate, where the key-set
// constants live.
//
// # Non-vacuity
//
// This gate's four rules that the media surface touched were each mutated and each went red:
//
// | mutation                                                    | caught by |
// | ----------------------------------------------------------- | --------- |
// | `map_media` deleted from `MAPPER_TO_RESOURCE`               | `each operation publishes the resource its handler actually maps` |
// | `attributeValues` removed from `Sku.required`               | `resource properties equal the struct fields…` |
// | `Media.resourceSnapshot` renamed                            | `resource properties equal the struct fields…` |
// | `MediaResource.additionalProperties` set to `true`          | `an opaque JSON read model points at a declared, closed shared schema` |

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

const contract = JSON.parse(readText('apis/backend-api/merchandise/shop-backend-api.merchandise.openapi.json'));
const routerSource = readText('crates/sdkwork-merchandise-web-support/src/backend_catalog_router.rs');
const storeSource = readText('crates/sdkwork-merchandise-web-support/src/catalog_store.rs');
const baseline = readText('database/ddl/baseline/postgres/0001_merchandise_baseline.sql');

const HTTP_METHODS = ['get', 'post', 'put', 'patch', 'delete'];

// ---------------------------------------------------------------- contract side

const operations = [];
for (const [operationPath, pathItem] of Object.entries(contract.paths)) {
  for (const method of HTTP_METHODS) {
    const operation = pathItem[method];
    if (operation) operations.push({ operationPath, method, operation });
  }
}

/** Resolves a local `$ref`, or returns the node unchanged. */
function deref(node) {
  if (!node || typeof node !== 'object') return node;
  const target = node.$ref;
  if (typeof target !== 'string') return node;
  const name = target.split('/').pop();
  const resolved = contract.components.schemas[name];
  assert.ok(resolved, `unresolvable $ref ${target}`);
  return resolved;
}

/** Pulls the `application/json` schema of a success response, or null when the operation has none. */
function successBodySchema(operation) {
  for (const status of ['200', '201', '202']) {
    const schema = operation.responses?.[status]?.content?.['application/json']?.schema;
    if (schema) return { status, schema };
  }
  return null;
}

/**
 * The `allOf` branch of a typed envelope that pins `data`, resolved one level.
 *
 * Both the item responses (which the specs `typedSdkWorkResourceResponse()` helper builds) and the
 * named list responses put the typed payload in the second `allOf` branch.
 *
 * Do not simply take the first branch that has a `data` property: `SdkWorkApiResponse` declares its
 * own `data` slot as a bare description with no fields, so that branch always matches first and
 * would make every response look untyped. The typed branch is the one whose `data` has properties.
 */
function typedDataBranch(schema) {
  const resolved = deref(schema);
  const branches = Array.isArray(resolved.allOf) ? resolved.allOf.map(deref) : [];
  const pinned = branches.filter((branch) => branch?.properties?.data?.properties);
  return (
    pinned.find((branch) => {
      const payload = branch.properties.data.properties;
      return payload.item || payload.items;
    }) ??
    pinned[0] ??
    null
  );
}

/**
 * The resource a response body publishes: the `$ref` behind `data.item`, or behind
 * `data.items[]`.
 */
function publishedResource(schema) {
  const branch = typedDataBranch(schema);
  if (!branch) return null;
  const data = branch.properties.data;
  const item = data.properties?.item;
  // Read the `$ref` off the node itself: dereferencing first would hand back the resource schema,
  // which has no `$ref` of its own.
  if (item) return { shape: 'item', resource: item.$ref?.split('/').pop() ?? null, data };
  const items = data.properties?.items;
  if (items) return { shape: 'list', resource: items.items?.$ref?.split('/').pop() ?? null, data };
  return null;
}

// ------------------------------------------------------------------- rust side

/** Splits `catalog_store.rs` into `pub struct <Name> { <fields> }` blocks. */
function rustResponseStructs(source) {
  const structs = {};
  const pattern = /pub struct (\w+Response) \{([\s\S]*?)\n\}/g;
  for (let match = pattern.exec(source); match; match = pattern.exec(source)) {
    const [, name, body] = match;
    const fields = [];
    let pendingSerde = null;
    for (const rawLine of body.split('\n')) {
      const line = rawLine.trim();
      if (line.startsWith('#[serde(')) {
        pendingSerde = line;
        continue;
      }
      if (line.startsWith('//') || line === '') continue;
      // The type charset admits `.` so a field typed with a path (`serde_json::Value`) parses rather
      // than being silently skipped: a skipped field would drop out of the property-set comparison
      // and the schema would then be reported as declaring a property the struct does not have.
      const field = /^(\w+):\s*([A-Za-z0-9_:<>,. ]+),$/.exec(line);
      if (!field) {
        pendingSerde = null;
        continue;
      }
      const [, rustName, rustType] = field;
      const rename = pendingSerde ? /rename\s*=\s*"([^"]+)"/.exec(pendingSerde) : null;
      const wireName = rename
        ? rename[1]
        : rustName.replace(/_([a-z0-9])/g, (_all, char) => char.toUpperCase());
      const optional = /^Option</.test(rustType);
      const inner = optional ? rustType.slice('Option<'.length, -1) : rustType;
      fields.push({
        rustName,
        wireName,
        rustType,
        optional,
        inner,
        int64: inner === 'i64',
        int64String: Boolean(pendingSerde && /serde_int64/.test(pendingSerde)),
      });
      pendingSerde = null;
    }
    structs[name] = fields;
  }
  return structs;
}

const rustStructs = rustResponseStructs(storeSource);
const resourceOf = (structName) => structName.replace(/Response$/, '');

// --------------------------------------------------------------- router side

/**
 * `path + method -> handler function name`, parsed from the `.route(...)` table.
 *
 * The terminator is the closing paren at the block's own indentation rather than "the first `)` on
 * a line": a chain such as `get(a).patch(b).delete(c)` ends an inner `get(a)` with a newline, so a
 * lazily matched `\n\s*\)\n` would cut the chain in half and silently drop the route.
 */
function routedHandlers(source) {
  const table = {};
  const routePattern = /\.route\(\s*"([^"]+)",([\s\S]*?)\n {8}\)/g;
  for (let match = routePattern.exec(source); match; match = routePattern.exec(source)) {
    const [, routePath, chain] = match;
    const handlerPattern = /\b(get|post|put|patch|delete)\((backend_\w+)\)/g;
    for (let handler = handlerPattern.exec(chain); handler; handler = handlerPattern.exec(chain)) {
      table[`${handler[1].toUpperCase()} ${routePath}`] = handler[2];
    }
  }
  return table;
}

/** Splits the router into `async fn <name>` bodies so `map_*` calls are attributed to their handler. */
function handlerBodies(source) {
  const bodies = {};
  const chunks = source.split(/\nasync fn /).slice(1);
  for (const chunk of chunks) {
    const name = /^(\w+)/.exec(chunk)?.[1];
    if (name) bodies[name] = chunk;
  }
  return bodies;
}

/** `map_* -> resource schema name`. Longest names first so `map_category_attribute` wins. */
const MAPPER_TO_RESOURCE = {
  map_category_attribute: 'CategoryAttribute',
  map_price_list: 'PriceList',
  map_category: 'Category',
  map_product: 'Product',
  map_sku: 'Sku',
  map_media: 'Media',
  map_attribute: 'Attribute',
};

/**
 * Component schemas that are shared models rather than published resources.
 *
 * `components.schemas` mixes two kinds of definition: the resources an operation returns, and the
 * models those resources and their request bodies are built from. Only the first kind maps onto a
 * `*Response` struct, so the one-to-one assertion below has to be told which names are the second
 * kind — otherwise a shared model reads as a resource nobody publishes.
 *
 * Every entry needs a reason, and the reason has to be a statement about the model rather than about
 * this file: `MediaResource` is owned by `MEDIA_RESOURCE_SPEC` and is published as a *field* of
 * `Media.resourceSnapshot` and of the media request bodies, never as an operation's payload. An
 * unlisted name is a failure, which is what keeps this list from growing into a silencer.
 */
const SHARED_COMPONENT_SCHEMAS = new Map([
  [
    'MediaResource',
    'MEDIA_RESOURCE_SPEC section 3 model, reused by Media.resourceSnapshot and the media request bodies',
  ],
]);

/** Resolves a local `$ref` chain, so a property that points at a shared model can be inspected. */
function resolveSchema(node) {
  let current = node;
  for (let hop = 0; hop < 8 && current?.$ref; hop += 1) {
    const name = current.$ref.split('/').pop();
    const next = contract.components.schemas[name];
    assert.ok(next, `unresolvable $ref ${current.$ref}`);
    current = next;
  }
  return current;
}

const routed = routedHandlers(routerSource);
const bodies = handlerBodies(routerSource);

// ------------------------------------------------------------------- baseline

const DOCUMENTED_TABLES = {
  Category: 'commerce_product_category',
  Attribute: 'commerce_product_attribute',
  Product: 'commerce_product_spu',
  Sku: 'commerce_product_sku',
  CategoryAttribute: 'commerce_product_category_attribute',
  PriceList: 'commerce_price_list',
  Media: 'commerce_product_media',
};

/** Returns the `CREATE TABLE ... (...)` body of one table. */
function tableBlock(table) {
  const start = baseline.indexOf(`CREATE TABLE IF NOT EXISTS ${table} (`);
  assert.notEqual(start, -1, `baseline must declare ${table}`);
  const rest = baseline.slice(start);
  const end = rest.indexOf('\n);');
  assert.notEqual(end, -1, `baseline block for ${table} must terminate`);
  return rest.slice(0, end);
}

/** `CHECK (<column> IN (...))` set for one column of one table. */
function checkSet(table, column) {
  const block = tableBlock(table);
  const needle = `${column} IN (`;
  const position = block.indexOf(needle);
  assert.notEqual(position, -1, `${table}.${column} must carry a CHECK (<column> IN (...)) set`);
  const rest = block.slice(position + needle.length);
  const close = rest.indexOf(')');
  return rest
    .slice(0, close)
    .split(',')
    .map((value) => value.trim().replace(/^'|'$/g, ''))
    .filter(Boolean);
}

/** Contract property name -> the column whose CHECK set constrains it. Scoped by table. */
const ENUM_BINDINGS = {
  Category: { status: 'status' },
  Attribute: { valueType: 'value_type', status: 'status' },
  Product: { productType: 'product_type', status: 'status', salesStatus: 'sales_status' },
  Sku: {
    fulfillmentType: 'fulfillment_type',
    inventoryTracking: 'inventory_tracking',
    status: 'status',
    salesStatus: 'sales_status',
  },
  CategoryAttribute: { attributeRole: 'attribute_role', status: 'status' },
  PriceList: { status: 'status' },
  // The media vocabulary is paired rather than free-standing: `ownerType` and `mediaRole` are two
  // CHECKs, and `ck_commerce_product_media_owner_role` narrows the combination. Each column is
  // compared against its own set here; the combination is a domain rule, pinned by the service
  // crate's own tests against the same constraint.
  Media: { ownerType: 'owner_type', mediaRole: 'media_role', status: 'status' },
};

/** Contract property name -> the money column it publishes. */
const MONEY_BINDINGS = {
  Sku: { salePriceMinor: 'sale_price_minor', listPriceMinor: 'list_price_minor' },
};

// ================================================================ assertions

test('the contract, the router, the response structs, and the baseline all parse', () => {
  assert.equal(operations.length, 28, 'contract must declare 28 operations');
  assert.equal(Object.keys(rustStructs).length, 7, 'seven response structs are published');
  assert.ok(routed['GET /backend/v3/api/catalog/products'], 'route table must yield handlers');
  assert.ok(bodies.backend_list_products, 'handler bodies must be splittable');
  for (const resource of Object.values(DOCUMENTED_TABLES)) tableBlock(resource);
});

test('every resource schema has a response struct and vice versa', () => {
  const declared = Object.keys(contract.components.schemas).filter(
    (name) =>
      !/^(SdkWork|PageInfo|ProblemDetail|FieldError)/.test(name) &&
      !/(Request|ListResponse)$/.test(name) &&
      !SHARED_COMPONENT_SCHEMAS.has(name),
  );
  const expected = Object.keys(rustStructs).map(resourceOf).sort();
  assert.deepEqual(declared.sort(), expected, 'each resource schema must have exactly one `<Name>Response` struct');
});

test('no operation body is the bare shared envelope; every one pins its resource', () => {
  const untyped = [];
  for (const { method, operationPath, operation } of operations) {
    const body = successBodySchema(operation);
    if (!body) continue;
    const published = publishedResource(body.schema);
    if (!published?.resource) {
      untyped.push(`${operation.operationId}: response is not typed to a resource`);
    }
  }
  assert.deepEqual(untyped, [], 'every 2xx JSON body must resolve to a resource schema');
});

test('each operation publishes the resource its handler actually maps', () => {
  const mismatches = [];
  for (const { method, operationPath, operation } of operations) {
    const handler = routed[`${method.toUpperCase()} ${operationPath}`];
    assert.ok(handler, `${operation.operationId}: no handler routed for ${method.toUpperCase()} ${operationPath}`);
    const body = successBodySchema(operation);
    const handlerSource = bodies[handler];
    assert.ok(handlerSource, `${operation.operationId}: handler ${handler} body not found`);
    const mappers = [...handlerSource.matchAll(/\bmap_(category_attribute|price_list|category|product|sku|media|attribute)\b/g)]
      .map((match) => match[0]);
    const distinct = [...new Set(mappers)];
    if (!body) {
      assert.deepEqual(distinct, [], `${operation.operationId}: a bodyless handler must not map a resource`);
      continue;
    }
    assert.equal(distinct.length, 1, `${operation.operationId}: handler must map exactly one resource, saw ${distinct}`);
    const expected = MAPPER_TO_RESOURCE[distinct[0]];
    const published = publishedResource(body.schema);
    assert.equal(
      published.resource,
      expected,
      `${operation.operationId}: contract publishes ${published.resource} but ${handler} maps ${expected}`,
    );
  }
});

test('an opaque JSON read model points at a declared, closed shared schema', () => {
  // `resource_snapshot` is a `serde_json::Value`: the repository hands back whatever was written, so
  // the struct cannot pin its fields and the property comparison above cannot see them. Left at that,
  // it would be the one response field a caller has no schema for — and `MEDIA_RESOURCE_SPEC`
  // section 5 requires the stored snapshot to be a `MediaResource`-shaped projection, so the field is
  // documented after all, just not by the struct. This assertion is what keeps the document from
  // answering "some object" to a question the spec answers.
  const problems = [];
  let checked = 0;
  for (const [structName, fields] of Object.entries(rustStructs)) {
    const resource = resourceOf(structName);
    const schema = contract.components.schemas[resource];
    for (const field of fields) {
      if (field.inner !== 'serde_json::Value') continue;
      checked += 1;
      const property = schema.properties[field.wireName];
      const ref = property?.$ref?.split('/').pop();
      if (!ref || !contract.components.schemas[ref]) {
        problems.push(
          `${resource}.${field.wireName}: ${structName} publishes an opaque JSON document that declares no shared schema`,
        );
        continue;
      }
      const target = resolveSchema(property);
      if (target.type !== 'object') {
        problems.push(`${resource}.${field.wireName}: ${ref} must be declared type object`);
      }
      if (target.additionalProperties !== false) {
        problems.push(
          `${resource}.${field.wireName}: ${ref} must be closed, or the document would admit keys the validator refuses`,
        );
      }
    }
  }
  assert.ok(checked > 0, 'no opaque JSON read model was checked, so this rule passed vacuously');
  assert.deepEqual(problems, []);
});

test('the shared untyped envelopes are no longer reachable from any operation', () => {
  const referenced = operations.filter(({ operation }) => {
    const body = successBodySchema(operation);
    return body && /\$ref":\s*"#\/components\/schemas\/SdkWork(Resource|List)Response/.test(JSON.stringify(body.schema));
  });
  assert.deepEqual(referenced.map(({ operation }) => operation.operationId), []);
  for (const shared of ['SdkWorkApiResponse', 'SdkWorkResourceData', 'SdkWorkPageData', 'PageInfo']) {
    assert.ok(contract.components.schemas[shared], `${shared} must stay declared as a shared component`);
  }
});

test('resource properties equal the struct fields, with required marking the non-Option fields', () => {
  const problems = [];
  for (const [structName, fields] of Object.entries(rustStructs)) {
    const resource = resourceOf(structName);
    const schema = contract.components.schemas[resource];
    assert.ok(schema, `${resource} schema must exist`);
    assert.deepEqual(
      Object.keys(schema.properties).sort(),
      fields.map((field) => field.wireName).sort(),
      `${resource}: schema properties must equal ${structName} fields in camelCase`,
    );
    const expectedRequired = fields.filter((field) => !field.optional).map((field) => field.wireName).sort();
    assert.deepEqual(
      [...(schema.required ?? [])].sort(),
      expectedRequired,
      `${resource}: required must be exactly the non-Option fields`,
    );
    assert.equal(schema.additionalProperties, false, `${resource}: additionalProperties must be false`);
    for (const field of fields.filter((field) => field.optional)) {
      const property = schema.properties[field.wireName];
      assert.deepEqual(
        property.type,
        [field.inner === 'i64' ? 'string' : field.inner === 'String' ? 'string' : 'boolean', 'null'],
        `${resource}.${field.wireName}: an Option must be declared as a nullable union`,
      );
    }
  }
  assert.deepEqual(problems, []);
});

test('every i64 field is a decimal int64 string, and no i64 field is serialized as a number', () => {
  const problems = [];
  for (const [structName, fields] of Object.entries(rustStructs)) {
    const resource = resourceOf(structName);
    const schema = contract.components.schemas[resource];
    for (const field of fields) {
      const property = schema.properties[field.wireName];
      if (!property) {
        problems.push(`${resource}.${field.wireName}: ${structName} publishes a field the contract does not declare`);
        continue;
      }
      const type = field.optional ? property.type[0] : property.type;
      if (!field.int64) {
        if (type === 'string' && property.format === 'int64') {
          problems.push(`${resource}.${field.wireName}: ${field.rustType} is not an i64 but is declared format int64`);
        }
        continue;
      }
      if (!field.int64String) {
        problems.push(
          `${resource}.${field.wireName}: Rust i64 field ${field.rustName} has no serde_int64 serializer, so it reaches the wire as a JSON number`,
        );
      }
      if (type !== 'string' || property.format !== 'int64') {
        problems.push(`${resource}.${field.wireName}: an i64 must be declared type string, format int64`);
      }
      if (property['x-sdkwork-int64-string'] !== true) {
        problems.push(`${resource}.${field.wireName}: missing x-sdkwork-int64-string`);
      }
      if (property['x-sdkwork-rust-type'] !== 'i64') {
        problems.push(`${resource}.${field.wireName}: missing x-sdkwork-rust-type: i64`);
      }
      if (!isDecimalDigitPattern(property.pattern)) {
        problems.push(`${resource}.${field.wireName}: missing a decimal digit pattern`);
      }
    }
  }
  assert.deepEqual(problems, []);
});

/** A digit pattern per API_SPEC section 13, e.g. `^-?[0-9]+$` or `^[0-9]+$`. */
function isDecimalDigitPattern(pattern) {
  return typeof pattern === 'string' && pattern.startsWith('^') && pattern.endsWith('$') && pattern.includes('[0-9]');
}

test('every enum equals the CHECK set of its column', () => {
  const problems = [];
  for (const [resource, bindings] of Object.entries(ENUM_BINDINGS)) {
    const table = DOCUMENTED_TABLES[resource];
    const schema = contract.components.schemas[resource];
    for (const [property, column] of Object.entries(bindings)) {
      const declared = schema.properties[property].enum;
      assert.ok(Array.isArray(declared), `${resource}.${property} must declare an enum`);
      const allowed = checkSet(table, column).sort();
      if (JSON.stringify([...declared].sort()) !== JSON.stringify(allowed)) {
        problems.push(
          `${resource}.${property}: contract says [${[...declared].sort()}] but ${table}.${column} allows [${allowed}]`,
        );
      }
    }
  }
  assert.deepEqual(problems, []);
});

test('every monetary field declares the minor unit', () => {
  for (const [resource, bindings] of Object.entries(MONEY_BINDINGS)) {
    const schema = contract.components.schemas[resource];
    for (const [property, column] of Object.entries(bindings)) {
      assert.equal(
        schema.properties[property]['x-sdkwork-money-unit'],
        'minor',
        `${resource}.${property} must declare x-sdkwork-money-unit: minor (${column})`,
      );
    }
  }
});

test('creation responses carry a body and delete responses do not', () => {
  const problems = [];
  for (const { operation } of operations) {
    const isCreate = operation.operationId.endsWith('.create');
    const isDelete = operation.operationId.endsWith('.delete');
    if (isCreate) {
      const created = operation.responses['201'];
      assert.ok(created, `${operation.operationId}: a create must declare 201`);
      const schema = created.content?.['application/json']?.schema;
      if (!schema) {
        problems.push(`${operation.operationId}: declares 201 with no body while the handler returns the created resource`);
        continue;
      }
      assert.equal(publishedResource(schema)?.shape, 'item', `${operation.operationId}: 201 must publish data.item`);
      continue;
    }
    if (isDelete) {
      assert.ok(operation.responses['204'], `${operation.operationId}: a delete must declare 204`);
      if (operation.responses['204'].content) {
        problems.push(`${operation.operationId}: 204 must not declare a body`);
      }
      continue;
    }
    assert.equal(successBodySchema(operation)?.status, '200', `${operation.operationId}: a non-create write must return 200`);
  }
  assert.deepEqual(problems, []);
});

test('list responses publish a typed array and the standard page info', () => {
  const lists = operations.filter(({ operation }) => operation.operationId.endsWith('.list'));
  assert.equal(lists.length, 7, 'seven list operations are expected');
  for (const { operation } of lists) {
    const schema = deref(successBodySchema(operation).schema);
    assert.ok(schema.allOf, `${operation.operationId}: a list response must be an allOf extension`);
    const published = publishedResource(schema);
    assert.equal(published.shape, 'list', `${operation.operationId}: must publish data.items`);
    assert.deepEqual(
      [...published.data.required].sort(),
      ['items', 'pageInfo'],
      `${operation.operationId}: data must require items and pageInfo`,
    );
    assert.equal(
      JSON.stringify(published.data.properties.pageInfo),
      JSON.stringify({ $ref: '#/components/schemas/PageInfo' }),
      `${operation.operationId}: pageInfo must $ref the shared PageInfo`,
    );
  }
});
