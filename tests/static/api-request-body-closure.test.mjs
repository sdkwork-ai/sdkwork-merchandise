import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

// The authored OpenAPI document is the contract; the axum DTOs and the domain commands are the
// implementation. Until this test existed nothing compared the two on the request side: the
// document declared every write body as the opaque `CommerceOperationCommand`
// (`{"type":"object","additionalProperties":true}`), so the only authoritative statement of which
// fields a write accepts was the Rust struct — invisible to the SDK generator, to reviewers, and to
// `API_SPEC` section 13's int64 and monetary-unit rules.
//
// This test closes that gap. For every operation that declares a request body it requires:
//
//   1. the body to reference one named component schema, with `additionalProperties: false` and the
//      shell schema retired — otherwise a caller has no typed body to build against;
//   2. the document's property set to equal the extracting DTO's field set (compared in the wire
//      camelCase spelling the `rename_all` attribute produces) and its `required` list to equal the
//      DTO's non-`Option` fields;
//   3. each property's JSON type to match the Rust field type, with every `format: int64` property
//      carrying `x-sdkwork-int64-string` and a decimal digit pattern;
//   4. every `enum` to equal the baseline CHECK set of the column that column feeds;
//   5. every `maxLength` to equal the Rust bound constant and that constant to equal the baseline's
//      `char_length` bound for the mapped column;
//   6. every monetary field to declare the minor unit (`x-sdkwork-money-unit: minor`) and to be
//      bound by a command field that really is an `i64`.
//
// Anything the document declares that the implementation cannot honour, and anything the
// implementation accepts that the document does not declare, is reported as a failure naming the
// operation.
//
// # An opaque JSON body field
//
// `CreateMediaRequest.resource` is a `MediaResource` and its Rust type is a `serde_json::Value`, so no
// field list exists to compare against: the keys live in the validator, not in the type. Two rules
// cover the gap rather than skipping it. The type check requires the property to `$ref` a declared
// component, so the shape is reviewable and generatable instead of an inline blob; and
// `the shared MediaResource schema declares exactly the key set the domain validates` compares the
// component's properties, `required`, and closedness against `MEDIA_RESOURCE_REQUIRED_KEYS` and
// `MEDIA_RESOURCE_OPTIONAL_KEYS` in both directions — the validator is read back out of
// `validation/mod.rs`, not paraphrased here, so neither side can move alone.
//
// # A nullable wire field
//
// A body field the document declares as `type: ["string", "null"]` is carried in Rust as a **nested**
// `Option`. One `Option` cannot express three states, and a field that can be cleared needs all
// three: absent (leave it alone), `null` (clear it), and a value (replace it). `UpdateSkuRequest.
// listPriceMinor` is the field this exists for. The gate unwraps both layers and requires the inner
// type to match the document's non-`null` branch, so the nullability is compared rather than
// tolerated — dropping `null` from the document, or flattening the command field back to one
// `Option`, is a failure either way.
//
// # Non-vacuity
//
// | mutation                                                       | caught by |
// | -------------------------------------------------------------- | --------- |
// | `CreateMediaRequest.mediaRole` loses `gallery_image`            | `every documented enum is exactly the baseline CHECK set of the column it feeds` |
// | `CreateMediaRequest.ownerType` narrowed to `["spu"]`            | `every documented enum is exactly the baseline CHECK set of the column it feeds` |
// | `CreateMediaRequest.resource` inlined instead of `$ref`d        | `each documented property type matches the Rust field type, int64 included` |
// | `metadata` removed from `MEDIA_RESOURCE_OPTIONAL_KEYS`          | `the shared MediaResource schema declares exactly the key set the domain validates` |
// | `metadata` added to `MediaResource.required`                    | `the shared MediaResource schema declares exactly the key set the domain validates` |
// | `UpdateSkuRequest.listPriceMinor` drops `null` from its type    | `each documented property type matches the Rust field type, int64 included` |
// | `list_price_minor` flattened back to `Option<i64>`              | `every monetary field declares the minor unit and reaches an i64 command field` |

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const OPENAPI = "apis/backend-api/merchandise/shop-backend-api.merchandise.openapi.json";
const ROUTER = "crates/sdkwork-merchandise-web-support/src/backend_catalog_router.rs";
const DTO_SOURCES = [
  "crates/sdkwork-merchandise-web-support/src/catalog_store.rs",
  ROUTER,
];
const COMMAND_SOURCE = "crates/sdkwork-merchandise-service/src/commands/mod.rs";
const VALIDATION_SOURCE = "crates/sdkwork-merchandise-service/src/validation/mod.rs";
const BASELINE = "database/ddl/baseline/postgres/0001_merchandise_baseline.sql";

/** The schema the retired shell occupied. It must not come back. */
const SHELL_SCHEMA = "CommerceOperationCommand";

const document = JSON.parse(readFileSync(path.join(root, OPENAPI), "utf8"));
/**
 * Reads a source file with line endings normalised to LF.
 *
 * This repository carries no `.gitattributes` while `core.autocrlf` is on, so a file on disk is
 * CRLF or LF depending on which tool wrote it last. The attribute and field patterns below are
 * line-anchored, and a stray `\r` between `]` and `\n` would make an enclosing pattern miss
 * without any of the inputs being wrong.
 */
const readText = (file) => readFileSync(path.join(root, file), "utf8").replace(/\r\n/g, "\n");

const router = readText(ROUTER);
const dtoSources = DTO_SOURCES.map(readText).join("\n");
const commandSource = readText(COMMAND_SOURCE);
const validationSource = readText(VALIDATION_SOURCE);
const baseline = readText(BASELINE);

// ------------------------------------------------------------------ parsers

/**
 * Every `struct <Name> { ... }`, keyed by name, with its serde attribute block and field types.
 *
 * The attribute block is captured because two of its entries are part of the closure: the
 * document declares camelCase names and a closed body, and only `rename_all = "camelCase"` plus
 * `deny_unknown_fields` make those claims true of the deserializer.
 */
function collectStructs(sources) {
  const structs = new Map();
  const pattern = /((?:^[ \t]*#\[[^\]]*\][ \t]*\n)*)(?:pub\s+)?struct\s+(\w+)\s*\{([\s\S]*?)\n\}/gm;
  for (const match of sources.matchAll(pattern)) {
    const [, attributes, name, body] = match;
    const fields = [];
    for (const field of body.matchAll(/^[ \t]*(?:pub\s+)?(\w+)\s*:\s*([^,]+),/gm)) {
      fields.push({ name: field[1], type: field[2].trim() });
    }
    structs.set(name, { attributes, fields });
  }
  return structs;
}

/** The `CatalogJson<T>` body type each `async fn <handler>` extracts, if any. */
function collectHandlerBodyTypes(source) {
  const byHandler = new Map();
  const fnPattern = /async\s+fn\s+(\w+)\s*\(([\s\S]*?)\)\s*->\s*Response/g;
  for (const match of source.matchAll(fnPattern)) {
    const [, handler, parameters] = match;
    const bodyTypes = [...parameters.matchAll(/CatalogJson<(\w+)>/g)].map((body) => body[1]);
    if (bodyTypes.length) byHandler.set(handler, bodyTypes);
  }
  return byHandler;
}

/** Route path → the handler each method dispatches to, from the builder chain. */
function collectRouteHandlers(source) {
  const routes = new Map();
  const routePattern = /\.route\(\s*"([^"]+)"\s*,\s*([\s\S]*?)\n\s*\)/g;
  for (const match of source.matchAll(routePattern)) {
    const [, routePath, chain] = match;
    const methods = new Map();
    for (const method of chain.matchAll(/\b(get|post|patch|put|delete)\((\w+)\)/g)) {
      methods.set(method[1], method[2]);
    }
    if (methods.size) routes.set(routePath, methods);
  }
  return routes;
}

/**
 * Per table: the allowed values of every `<column> IN (...)` CHECK, and the `char_length` bound of
 * every `char_length(<column>) BETWEEN a AND b` CHECK.
 *
 * Scoping by table matters: `attribute_role` is constrained on both
 * `commerce_product_category_attribute` (key/sales/parameter) and `commerce_product_spu_attribute`
 * (key/parameter), and only the first one is the vocabulary this contract publishes.
 */
function collectTableConstraints(source) {
  const tables = new Map();
  const blocks = source.matchAll(/CREATE TABLE IF NOT EXISTS (\w+)\s*\(([\s\S]*?)\n\);/g);
  for (const [, table, body] of blocks) {
    const enums = new Map();
    for (const check of body.matchAll(/CHECK\s*\(\s*(\w+)\s+IN\s*\(([^)]*)\)\s*\)/g)) {
      const values = [...check[2].matchAll(/'([^']*)'/g)].map((value) => value[1]);
      if (values.length) enums.set(check[1], values);
    }
    const lengths = new Map();
    for (const check of body.matchAll(
      /CHECK\s*\(\s*char_length\((\w+)\)\s+BETWEEN\s+(\d+)\s+AND\s+(\d+)\s*\)/g,
    )) {
      lengths.set(check[1], { min: Number(check[2]), max: Number(check[3]) });
    }
    tables.set(table, { enums, lengths });
  }
  return tables;
}

/** `pub const <NAME>: usize = <n>;`, the bounds the commands validate against. */
function collectCharBounds(source) {
  const bounds = new Map();
  for (const match of source.matchAll(/pub const (\w+): usize = (\d+);/g)) {
    bounds.set(match[1], Number(match[2]));
  }
  return bounds;
}

/**
 * `pub const <NAME>: [&str; n] = ["a", "b", ...];`, the closed key sets a validator enforces.
 *
 * A `MediaResource` is carried as an opaque JSON document, so its field list does not appear in any
 * struct this gate can compare against. The Rust key-set constants are where that list does live, and
 * reading them back is what lets the document be compared with the validator instead of with prose.
 */
function collectKeyLists(source) {
  const lists = new Map();
  for (const match of source.matchAll(/pub const (\w+): \[&str; \d+\] = \[([\s\S]*?)\];/g)) {
    lists.set(
      match[1],
      [...match[2].matchAll(/"([^"]*)"/g)].map((entry) => entry[1]),
    );
  }
  return lists;
}

/** `camelCase` wire spelling of a snake_case Rust field name. */
function toWireName(field) {
  return field.replace(/_([a-z0-9])/g, (_, letter) => letter.toUpperCase());
}

const structs = collectStructs(dtoSources);
const commandStructs = collectStructs(commandSource);
const handlerBodyTypes = collectHandlerBodyTypes(router);
const routeHandlers = collectRouteHandlers(router);
const tableConstraints = collectTableConstraints(baseline);
const charBounds = collectCharBounds(validationSource);
const keyLists = collectKeyLists(validationSource);

/**
 * Every write operation, located on all three sides at once.
 *
 * `handler` and `dto` come from the router, `schema` from the document, so a schema that is not
 * actually extracted by the route's handler cannot be silently substituted for the right one.
 */
function collectWriteOperations() {
  const operations = [];
  for (const [routePath, pathItem] of Object.entries(document.paths ?? {})) {
    for (const [method, operation] of Object.entries(pathItem ?? {})) {
      if (!operation || typeof operation !== "object" || !operation.requestBody) continue;
      const handlers = routeHandlers.get(routePath);
      const handler = handlers?.get(method);
      const dto = handler ? handlerBodyTypes.get(handler) : undefined;
      operations.push({ routePath, method, operation, handler, dto: dto?.[0] });
    }
  }
  return operations;
}

const writeOperations = collectWriteOperations();

/**
 * Contract property → the domain command field it is carried into.
 *
 * Only the monetary fields need this: the amount's unit is a claim the contract makes about the
 * Rust type, so the type that leaves the adapter has to be inspectable.
 */
const MONEY_BINDINGS = [
  {
    schema: "CreateSkuRequest",
    property: "salePriceMinor",
    command: "CreateProductSkuCommand",
    field: "sale_price_minor",
    type: "i64",
  },
  {
    schema: "CreateSkuRequest",
    property: "listPriceMinor",
    command: "CreateProductSkuCommand",
    field: "list_price_minor",
    type: "Option<i64>",
  },
  {
    schema: "UpdateSkuRequest",
    property: "salePriceMinor",
    command: "UpdateProductSkuCommand",
    field: "sale_price_minor",
    type: "Option<i64>",
  },
  {
    schema: "UpdateSkuRequest",
    property: "listPriceMinor",
    command: "UpdateProductSkuCommand",
    field: "list_price_minor",
    // Nullable on the wire, so the command carries a nested `Option`: the outer one is serde's
    // "the key was absent" and the inner one is "the value was null". A plain `Option<i64>` would
    // make "leave the reference price alone" and "clear it" the same instruction.
    type: "Option<Option<i64>>",
  },
];

/** Contract enum property → the baseline column whose CHECK is its vocabulary. */
const ENUM_BINDINGS = [
  {
    schema: "UpdateCategoryRequest",
    property: "status",
    table: "commerce_product_category",
    column: "status",
  },
  {
    schema: "CreateProductRequest",
    property: "productType",
    table: "commerce_product_spu",
    column: "product_type",
  },
  {
    schema: "CreateSkuRequest",
    property: "fulfillmentType",
    table: "commerce_product_sku",
    column: "fulfillment_type",
  },
  {
    schema: "CreateSkuRequest",
    property: "inventoryTracking",
    table: "commerce_product_sku",
    column: "inventory_tracking",
  },
  {
    schema: "UpdateSkuRequest",
    property: "fulfillmentType",
    table: "commerce_product_sku",
    column: "fulfillment_type",
  },
  {
    schema: "UpdateSkuRequest",
    property: "inventoryTracking",
    table: "commerce_product_sku",
    column: "inventory_tracking",
  },
  {
    schema: "UpdateSkuRequest",
    property: "status",
    table: "commerce_product_sku",
    column: "status",
  },
  {
    schema: "UpdateProductRequest",
    property: "status",
    table: "commerce_product_spu",
    column: "status",
  },
  {
    schema: "CreateCategoryAttributeRequest",
    property: "role",
    table: "commerce_product_category_attribute",
    column: "attribute_role",
  },
  {
    schema: "UpdateCategoryAttributeRequest",
    property: "role",
    table: "commerce_product_category_attribute",
    column: "attribute_role",
  },
  {
    schema: "UpdateCategoryAttributeRequest",
    property: "status",
    table: "commerce_product_category_attribute",
    column: "status",
  },
  {
    schema: "UpdatePriceListRequest",
    property: "status",
    table: "commerce_price_list",
    column: "status",
  },
  {
    schema: "CreateMediaRequest",
    property: "ownerType",
    table: "commerce_product_media",
    column: "owner_type",
  },
  {
    schema: "CreateMediaRequest",
    property: "mediaRole",
    table: "commerce_product_media",
    column: "media_role",
  },
  {
    schema: "UpdateMediaRequest",
    property: "mediaRole",
    table: "commerce_product_media",
    column: "media_role",
  },
  {
    schema: "UpdateMediaRequest",
    property: "status",
    table: "commerce_product_media",
    column: "status",
  },
];

/**
 * Contract bounded-text property → the Rust constant and the baseline column behind it.
 *
 * `property` names a `maxLength` on the property itself; `items` names one on its `items` schema.
 */
const LENGTH_BINDINGS = [
  {
    schema: "CreateCategoryRequest",
    property: "name",
    constant: "CATEGORY_NAME_MAX_CHARS",
    table: "commerce_product_category",
    column: "name",
  },
  {
    schema: "UpdateCategoryRequest",
    property: "name",
    constant: "CATEGORY_NAME_MAX_CHARS",
    table: "commerce_product_category",
    column: "name",
  },
  {
    schema: "CreateProductRequest",
    property: "title",
    constant: "SPU_TITLE_MAX_CHARS",
    table: "commerce_product_spu",
    column: "name",
  },
  {
    schema: "UpdateProductRequest",
    property: "title",
    constant: "SPU_TITLE_MAX_CHARS",
    table: "commerce_product_spu",
    column: "name",
  },
  {
    schema: "CreateAttributeRequest",
    property: "name",
    constant: "ATTRIBUTE_NAME_MAX_CHARS",
    table: "commerce_product_attribute",
    column: "name",
  },
  {
    schema: "CreateAttributeRequest",
    property: "values",
    items: true,
    constant: "ATTRIBUTE_VALUE_MAX_CHARS",
    table: "commerce_product_attribute_value",
    column: "display_value",
  },
  {
    schema: "CreatePriceListRequest",
    property: "priceListNo",
    constant: "PRICE_LIST_NO_MAX_CHARS",
    table: "commerce_price_list",
    column: "name",
  },
];

const requestSchemaNames = () =>
  Object.keys(document.components?.schemas ?? {}).filter((name) => /Request$/.test(name));

// ------------------------------------------------------------------ tests

test("the document, the router, the DTOs, the commands, and the baseline were all parsed", () => {
  assert.ok(Object.keys(document.paths ?? {}).length > 0, `no paths parsed from ${OPENAPI}`);
  assert.ok(routeHandlers.size > 0, `no routes parsed from ${ROUTER}`);
  assert.ok(structs.size > 0, "no DTO structs parsed");
  assert.ok(commandStructs.size > 0, `no command structs parsed from ${COMMAND_SOURCE}`);
  assert.ok(handlerBodyTypes.size > 0, "no handler body extractors parsed");
  assert.ok(tableConstraints.size > 0, `no tables parsed from ${BASELINE}`);
  assert.ok(charBounds.size > 0, `no char bounds parsed from ${VALIDATION_SOURCE}`);
  assert.ok(writeOperations.length > 0, "no operation with a request body was found");
});

test("every write body is a named, closed schema and the opaque shell is retired", () => {
  const problems = [];
  const schemas = document.components?.schemas ?? {};

  if (SHELL_SCHEMA in schemas) {
    problems.push(
      `${SHELL_SCHEMA} is still declared; an opaque body schema lets the document claim a typed operation while naming no field the SDK can generate`,
    );
  }

  for (const { routePath, method, operation, handler, dto } of writeOperations) {
    const label = `${method.toUpperCase()} ${routePath}`;
    const content = operation.requestBody?.content;
    const mediaTypes = Object.keys(content ?? {});
    if (mediaTypes.length !== 1 || mediaTypes[0] !== "application/json") {
      problems.push(`${label} declares request media types [${mediaTypes.join(", ")}]`);
      continue;
    }
    const ref = content["application/json"]?.schema?.$ref;
    if (!ref) {
      problems.push(`${label} declares an inline request schema; it must reference a component`);
      continue;
    }
    const schemaName = ref.split("/").pop();
    const schema = schemas[schemaName];
    if (!schema) {
      problems.push(`${label} references the missing schema ${schemaName}`);
      continue;
    }
    if (schema.additionalProperties !== false) {
      problems.push(
        `${schemaName} must declare additionalProperties: false (API_SPEC section 12); the duplicate-property class this closes is exactly the one the shell schema reopened`,
      );
    }
    if (!schema.properties || !Object.keys(schema.properties).length) {
      problems.push(`${schemaName} declares no properties`);
    }
    if (!handler || !dto) {
      problems.push(`${label} has a request body but its handler extracts no CatalogJson<T>`);
    }
  }

  assert.deepEqual(problems, [], `write body naming failed:\n${problems.join("\n")}`);
});

test("each request schema's fields and required list are exactly the extracting DTO's", () => {
  const problems = [];
  let compared = 0;

  for (const { routePath, method, operation, dto } of writeOperations) {
    const label = `${method.toUpperCase()} ${routePath}`;
    if (!dto) continue;
    const schemaName = operation.requestBody.content["application/json"].schema.$ref.split("/").pop();
    const schema = document.components.schemas[schemaName];
    const struct = structs.get(dto);
    if (!struct) {
      problems.push(`${label} extracts ${dto}, which is not a struct in the DTO modules`);
      continue;
    }
    compared += 1;

    if (!/rename_all\s*=\s*"camelCase"/.test(struct.attributes)) {
      problems.push(
        `${dto} does not declare serde(rename_all = "camelCase"); the document's property names would be wrong for every multi-word field`,
      );
    }
    if (!/deny_unknown_fields/.test(struct.attributes)) {
      problems.push(
        `${dto} does not declare serde(deny_unknown_fields); ${schemaName} promises a closed body the deserializer would not enforce`,
      );
    }
    if (/serde\s*\([^)]*\bdefault\b/.test(struct.attributes)) {
      problems.push(
        `${dto} declares a serde default; a defaulted field is optional on the wire while the command validates it as required`,
      );
    }

    const declared = Object.keys(schema.properties ?? {});
    const implemented = struct.fields.map((field) => toWireName(field.name));
    for (const property of declared) {
      if (!implemented.includes(property)) {
        problems.push(
          `${label}: ${schemaName}.${property} is documented but ${dto} has no matching field, so the caller's value is silently dropped`,
        );
      }
    }
    for (const property of implemented) {
      if (!declared.includes(property)) {
        problems.push(
          `${label}: ${dto} accepts \`${property}\`, which ${schemaName} does not declare, so the SDK cannot generate it`,
        );
      }
    }

    const required = [...(schema.required ?? [])].sort();
    const nonOptional = struct.fields
      .filter((field) => !field.type.startsWith("Option<"))
      .map((field) => toWireName(field.name))
      .sort();
    if (required.join(",") !== nonOptional.join(",")) {
      problems.push(
        `${label}: ${schemaName}.required is [${required.join(", ")}] but ${dto} treats [${nonOptional.join(", ")}] as required`,
      );
    }
  }

  assert.ok(compared > 0, "no request body was compared against its DTO");
  assert.deepEqual(problems, [], `request field closure failed:\n${problems.join("\n")}`);
});

test("each documented property type matches the Rust field type, int64 included", () => {
  const problems = [];
  const schemas = document.components.schemas;

  for (const { routePath, method, operation, dto } of writeOperations) {
    if (!dto) continue;
    const label = `${method.toUpperCase()} ${routePath}`;
    const schemaName = operation.requestBody.content["application/json"].schema.$ref.split("/").pop();
    const schema = schemas[schemaName];
    const struct = structs.get(dto);
    if (!struct) continue;

    for (const field of struct.fields) {
      const property = toWireName(field.name);
      const declared = schema.properties?.[property];
      if (!declared) continue;

      const optional = field.type.startsWith("Option<");
      const inner = optional ? field.type.slice("Option<".length, -1) : field.type;

      // A nested `Option` is not decoration. `UpdateProductSkuCommand::list_price_minor` has to tell
      // three states apart — the key was absent, the value was `null`, the value was a number — and
      // one `Option` can only carry two. The extra layer is exactly the difference between "leave
      // the stored reference price alone" and "clear it", and the document makes the same claim with
      // `type: ["string", "null"]`. So the *inner* type is what has to match the JSON type, and the
      // nullability is checked alongside it rather than ignored.
      const nullable = inner.startsWith("Option<");
      const carried = nullable ? inner.slice("Option<".length, -1) : inner;

      if (carried === "Vec<String>") {
        if (declared.type !== "array" || declared.items?.type !== "string") {
          problems.push(
            `${schemaName}.${property} must be an array of strings to match ${dto}.${field.name}: ${field.type}`,
          );
        }
        continue;
      }

      // An opaque JSON document. The gate cannot compare a `serde_json::Value` with a field list —
      // the keys live in a validator, not in the type — so it demands the one thing it can still
      // check: that the document points at a declared shared schema rather than at an inline blob,
      // which is what makes the shape reviewable and the SDK generatable. The shape itself is
      // compared with the validator by the MediaResource test below.
      if (carried === "serde_json::Value") {
        const ref = declared.$ref?.split("/").pop();
        if (!ref || !schemas[ref]) {
          problems.push(
            `${schemaName}.${property} must $ref a declared shared schema to match ${dto}.${field.name}: ${field.type}`,
          );
        }
        continue;
      }

      const expected = { String: "string", bool: "boolean", i32: "integer", i64: "integer" }[carried];
      if (!expected) {
        problems.push(
          `${dto}.${field.name} has type ${field.type}, which this gate does not map to a JSON type`,
        );
        continue;
      }
      const declaredTypes = Array.isArray(declared.type) ? declared.type : [declared.type];
      const expectedTypes = nullable ? [expected, "null"] : [expected];
      if (
        declaredTypes.length !== expectedTypes.length
        || !expectedTypes.every((one) => declaredTypes.includes(one))
      ) {
        problems.push(
          `${schemaName}.${property} declares type ${JSON.stringify(declared.type)}, but ${dto}.${field.name} is ${field.type}`,
        );
      }

      // API_SPEC section 13.6: an int64 on the wire is a string, so an int64-valued Rust field must
      // be carried as one. `format: int64` is the document's own claim that the value is that wide.
      if (declared.format === "int64") {
        if (!declaredTypes.includes("string")) {
          problems.push(
            `${schemaName}.${property} declares format int64 but type ${JSON.stringify(declared.type)}; API_SPEC section 13.6 requires a string`,
          );
        }
        if (declared["x-sdkwork-int64-string"] !== true) {
          problems.push(
            `${schemaName}.${property} declares format int64 without x-sdkwork-int64-string: true`,
          );
        }
        if (typeof declared.pattern !== "string" || !/^\^.*\$$/.test(declared.pattern)) {
          problems.push(
            `${schemaName}.${property} declares format int64 without a digit pattern`,
          );
        }
        if (carried !== "String") {
          problems.push(
            `${schemaName}.${property} is documented as an int64 string but ${dto}.${field.name} is ${field.type}; the adapter must carry the wire string`,
          );
        }
      }

      if (declared.minLength !== undefined && declared.minLength !== 1) {
        problems.push(
          `${schemaName}.${property} declares minLength ${declared.minLength}; the only non-emptiness bound the commands enforce is 1`,
        );
      }
    }
  }

  assert.deepEqual(problems, [], `request type closure failed:\n${problems.join("\n")}`);
});

test("a property whose wire name ends in Id is carried as an int64 string", () => {
  const problems = [];
  for (const schemaName of requestSchemaNames()) {
    for (const [property, declared] of Object.entries(
      document.components.schemas[schemaName].properties ?? {},
    )) {
      if (!/Id$/.test(property)) continue;
      if (declared.type !== "string" || declared.format !== "int64") {
        problems.push(
          `${schemaName}.${property} is an identifier and must be a type-string/format-int64 property`,
        );
      }
    }
  }
  assert.deepEqual(problems, [], problems.join("\n"));
});

test("every documented enum is exactly the baseline CHECK set of the column it feeds", () => {
  const problems = [];
  const schemas = document.components.schemas;
  const bound = new Set(ENUM_BINDINGS.map((entry) => `${entry.schema}.${entry.property}`));

  for (const schemaName of requestSchemaNames()) {
    for (const [property, declared] of Object.entries(schemas[schemaName].properties ?? {})) {
      if (!declared.enum) continue;
      if (!bound.has(`${schemaName}.${property}`)) {
        problems.push(
          `${schemaName}.${property} declares an enum that is not bound to a baseline CHECK by this gate`,
        );
      }
      if (declared.type !== "string") {
        problems.push(`${schemaName}.${property} declares an enum whose type is ${declared.type}`);
      }
    }
  }

  for (const { schema, property, table, column } of ENUM_BINDINGS) {
    const declared = schemas[schema]?.properties?.[property];
    if (!declared?.enum) {
      problems.push(`${schema}.${property} is bound to ${table}.${column} but declares no enum`);
      continue;
    }
    const allowed = tableConstraints.get(table)?.enums.get(column);
    if (!allowed) {
      problems.push(`${table}.${column} has no \`IN (...)\` CHECK in the baseline`);
      continue;
    }
    const documented = [...declared.enum].sort().join(",");
    const storage = [...allowed].sort().join(",");
    if (documented !== storage) {
      problems.push(
        `${schema}.${property} documents [${documented}] but ${table}.${column} accepts [${storage}]`,
      );
    }
  }

  assert.deepEqual(problems, [], `request enum closure failed:\n${problems.join("\n")}`);
});

test("every documented maxLength is the Rust bound and the baseline char_length bound", () => {
  const problems = [];
  const schemas = document.components.schemas;
  const bound = new Set(LENGTH_BINDINGS.map((entry) => `${entry.schema}.${entry.property}`));

  for (const schemaName of requestSchemaNames()) {
    for (const [property, declared] of Object.entries(schemas[schemaName].properties ?? {})) {
      if (declared.maxLength !== undefined && !bound.has(`${schemaName}.${property}`)) {
        problems.push(
          `${schemaName}.${property} declares maxLength ${declared.maxLength} without a bound Rust constant and baseline CHECK behind it`,
        );
      }
      const itemMaxLength = declared.items?.maxLength;
      if (itemMaxLength !== undefined && !bound.has(`${schemaName}.${property}`)) {
        problems.push(
          `${schemaName}.${property} items declare maxLength ${itemMaxLength} without a bound constant`,
        );
      }
    }
  }

  for (const { schema, property, items, constant, table, column } of LENGTH_BINDINGS) {
    const declared = schemas[schema]?.properties?.[property];
    if (!declared) {
      problems.push(`${schema}.${property} is bound to ${table}.${column} but is not documented`);
      continue;
    }
    const target = items ? declared.items : declared;
    const maxLength = target?.maxLength;
    const minLength = target?.minLength;
    const rustBound = charBounds.get(constant);
    const sqlBound = tableConstraints.get(table)?.lengths.get(column);
    if (rustBound === undefined) {
      problems.push(`${constant} is not declared in ${VALIDATION_SOURCE}`);
      continue;
    }
    if (!sqlBound) {
      problems.push(`${table}.${column} has no char_length CHECK in the baseline`);
      continue;
    }
    if (maxLength !== rustBound) {
      problems.push(
        `${schema}.${property} declares maxLength ${maxLength} but ${constant} is ${rustBound}`,
      );
    }
    if (rustBound !== sqlBound.max) {
      problems.push(
        `${constant} is ${rustBound} but ${table}.${column} admits at most ${sqlBound.max} characters`,
      );
    }
    if (minLength !== sqlBound.min) {
      problems.push(
        `${schema}.${property} declares minLength ${minLength} but ${table}.${column} requires at least ${sqlBound.min} character`,
      );
    }
  }

  assert.deepEqual(problems, [], `request length closure failed:\n${problems.join("\n")}`);
});

test("the shared MediaResource schema declares exactly the key set the domain validates", () => {
  // `resource` is a `serde_json::Value` on the DTO, so the field-set comparison above cannot see
  // inside it, and neither could an inline object schema be trusted to stay right. The validator's
  // two key-set constants are the authority for which keys a `MediaResource` may carry on this
  // surface, so both directions are compared: a key the validator accepts but the document omits is
  // unreachable for a generated SDK, and a key the document publishes but the validator refuses is a
  // promise the server breaks. `MEDIA_RESOURCE_SPEC` section 3 also requires the standard set to be
  // closed, which is why `bucketId`-style object keys are refused rather than dropped.
  const schema = document.components.schemas.MediaResource;
  assert.ok(
    schema,
    "MediaResource must be declared: the media request bodies and Media.resourceSnapshot both reference it",
  );

  const required = keyLists.get("MEDIA_RESOURCE_REQUIRED_KEYS");
  const optional = keyLists.get("MEDIA_RESOURCE_OPTIONAL_KEYS");
  assert.ok(required?.length, `MEDIA_RESOURCE_REQUIRED_KEYS must be declared in ${VALIDATION_SOURCE}`);
  assert.ok(optional?.length, `MEDIA_RESOURCE_OPTIONAL_KEYS must be declared in ${VALIDATION_SOURCE}`);

  assert.equal(
    schema.additionalProperties,
    false,
    "MediaResource must be closed, because the validator refuses a key outside the declared set",
  );
  assert.deepEqual(
    [...(schema.required ?? [])].sort(),
    [...required].sort(),
    "MediaResource.required must be exactly the keys the validator demands: id is required on this surface because commerce_product_media.media_resource_id is NOT NULL",
  );
  assert.deepEqual(
    Object.keys(schema.properties).sort(),
    [...required, ...optional].sort(),
    "MediaResource.properties must be exactly the keys the validator accepts",
  );
});

test("every monetary field declares the minor unit and reaches an i64 command field", () => {
  const problems = [];
  const schemas = document.components.schemas;
  const bound = new Set(MONEY_BINDINGS.map((entry) => `${entry.schema}.${entry.property}`));

  for (const schemaName of requestSchemaNames()) {
    for (const [property, declared] of Object.entries(schemas[schemaName].properties ?? {})) {
      if (declared["x-sdkwork-money-unit"] === undefined) continue;
      if (!bound.has(`${schemaName}.${property}`)) {
        problems.push(
          `${schemaName}.${property} declares a monetary unit that is not bound to a command field by this gate`,
        );
      }
    }
  }

  for (const { schema, property, command, field, type } of MONEY_BINDINGS) {
    const declared = schemas[schema]?.properties?.[property];
    if (!declared) {
      problems.push(`${schema}.${property} is not documented`);
      continue;
    }
    if (declared["x-sdkwork-money-unit"] !== "minor") {
      problems.push(
        `${schema}.${property} must declare x-sdkwork-money-unit: minor (API_SPEC section 13.2.1)`,
      );
    }
    if (declared["x-sdkwork-rust-type"] !== "i64") {
      problems.push(`${schema}.${property} must declare x-sdkwork-rust-type: i64`);
    }
    if (declared.pattern !== "^[0-9]+$") {
      problems.push(
        `${schema}.${property} must refuse a negative amount: the baseline CHECK is \`>= 0\` and the sign carries no direction here`,
      );
    }
    const commandField = commandStructs.get(command)?.fields.find((entry) => entry.name === field);
    if (!commandField) {
      problems.push(`${command}.${field} does not exist`);
      continue;
    }
    if (commandField.type !== type) {
      problems.push(
        `${command}.${field} is ${commandField.type} but ${schema}.${property} is documented as ${type}`,
      );
    }
  }

  assert.deepEqual(problems, [], `monetary closure failed:\n${problems.join("\n")}`);
});
