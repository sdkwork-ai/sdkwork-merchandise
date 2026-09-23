import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

// The authored OpenAPI document is the contract; the axum handlers are the implementation. Nothing
// in the toolchain compares them: `api:check` compares the document against its generated
// materialisation, and the route-manifest check compares operation ids. Both can pass while a
// declared query parameter is silently dropped by the handler, which is worse than a missing route —
// the caller sends `?page=2`, receives `200 OK`, and gets page 1 back with no signal.
//
// This test closes that gap by reading the two sides and requiring the sets to match:
//
//   * every query parameter the document declares for a route must exist as a field of the
//     `Query<T>` struct that route's handler extracts, and
//   * every field of that struct must be a declared query parameter (an undeclared field is an
//     undocumented input the SDK cannot generate).
//
// Routes whose handler takes no `Query<T>` are skipped, and so are operations whose documented
// inputs are entirely path parameters.

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const OPENAPI = "apis/backend-api/merchandise/shop-backend-api.merchandise.openapi.json";
const ROUTER = "crates/sdkwork-merchandise-web-support/src/backend_catalog_router.rs";
const DTO_SOURCES = [
  "crates/sdkwork-merchandise-web-support/src/catalog_store.rs",
  "crates/sdkwork-merchandise-web-support/src/backend_catalog_router.rs",
];

const document = JSON.parse(readFileSync(path.join(root, OPENAPI), "utf8"));
const router = readFileSync(path.join(root, ROUTER), "utf8");
const dtoSources = DTO_SOURCES.map((file) => readFileSync(path.join(root, file), "utf8")).join("\n");

/** Every `struct <Name> { ... }` in the DTO modules, keyed by name, as field-name lists. */
function collectStructFields(sources) {
  const fields = new Map();
  const structPattern = /struct\s+(\w+)\s*\{([\s\S]*?)\n\}/g;
  for (const match of sources.matchAll(structPattern)) {
    const [, name, body] = match;
    fields.set(
      name,
      [...body.matchAll(/^\s*(?:pub\s+)?(\w+)\s*:/gm)].map((field) => field[1]),
    );
  }
  return fields;
}

/** The distinct `Query<T>` extractor types each `async fn <handler>` uses, if any. */
function collectHandlerQueryTypes(source) {
  const byHandler = new Map();
  const fnPattern = /async\s+fn\s+(\w+)\s*\(([\s\S]*?)\)\s*->\s*Response/g;
  for (const match of source.matchAll(fnPattern)) {
    const [, handler, parameters] = match;
    const queryTypes = [...parameters.matchAll(/Query<(\w+)>/g)].map((query) => query[1]);
    if (queryTypes.length) byHandler.set(handler, queryTypes);
  }
  return byHandler;
}

/**
 * Route path → the handler each method dispatches to, from the builder chain.
 *
 * Only the chained form `get(a).post(b)` is read; a route registered by a bare `get(a)` is handled
 * by the same code path because the chain regex still matches `get(a)` followed by `)`.
 */
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

const structFields = collectStructFields(dtoSources);
const handlerQueryTypes = collectHandlerQueryTypes(router);
const routeHandlers = collectRouteHandlers(router);

function declaredQueryParams(operation) {
  return (operation.parameters ?? [])
    .filter((parameter) => parameter.in === "query" && typeof parameter.name === "string")
    .map((parameter) => parameter.name);
}

test("the router, the DTO modules, and the OpenAPI document were all parsed", () => {
  assert.ok(routeHandlers.size > 0, `no routes parsed from ${ROUTER}`);
  assert.ok(structFields.size > 0, "no query DTO structs parsed");
  assert.ok(handlerQueryTypes.size > 0, "no handler query extractors parsed");
  assert.ok(Object.keys(document.paths).length > 0, `no paths parsed from ${OPENAPI}`);
});

test("every query parameter the OpenAPI declares is reachable by the handler", () => {
  const problems = [];
  let compared = 0;

  for (const [routePath, methods] of routeHandlers) {
    const documented = document.paths[routePath];
    if (!documented) continue;

    for (const [method, handler] of methods) {
      const operation = documented[method];
      if (!operation) continue;
      const declared = declaredQueryParams(operation);
      if (!declared.length) continue;
      if (operation.requestBody) continue; // query+body routes are compared on the body elsewhere

      const queryTypes = handlerQueryTypes.get(handler);
      if (!queryTypes?.length) {
        problems.push(
          `${method.toUpperCase()} ${routePath} -> ${handler} declares [${declared.join(", ")}] but the handler extracts no Query<T>`,
        );
        continue;
      }

      const implemented = new Set(queryTypes.flatMap((type) => structFields.get(type) ?? []));
      compared += 1;

      for (const parameter of declared) {
        if (!implemented.has(parameter)) {
          problems.push(
            `${method.toUpperCase()} ${routePath} -> ${handler} (${queryTypes.join(", ")}): documented query parameter \`${parameter}\` is not read by the handler`,
          );
        }
      }
      for (const field of implemented) {
        if (!declared.includes(field)) {
          problems.push(
            `${method.toUpperCase()} ${routePath} -> ${handler} (${queryTypes.join(", ")}): field \`${field}\` is accepted but not declared in ${OPENAPI}`,
          );
        }
      }
    }
  }

  assert.ok(compared > 0, "no route with query parameters was compared");
  assert.deepEqual(problems, [], `query parameter closure failed:\n${problems.join("\n")}`);
});

test("every list operation returns the pagination envelope, not a bare array", () => {
  const problems = [];

  for (const [routePath, methods] of routeHandlers) {
    const documented = document.paths[routePath];
    const get = methods.get("get");
    if (!documented?.get || !get) continue;

    const declaresPaging = declaredQueryParams(documented.get).some((name) =>
      ["page", "page_size"].includes(name),
    );
    if (!declaresPaging) continue;

    // A paginated collection read must be served through the offset-page helper, which emits
    // `data.items` + `data.pageInfo`. A bare-array helper would present the same 200 with no way for
    // a caller to know more pages exist.
    const body = router.slice(router.indexOf(`async fn ${get}(`));
    const untilNext = body.slice(0, body.indexOf("\nasync fn ", 1));
    if (!untilNext.includes("success_offset_page")) {
      problems.push(
        `GET ${routePath} -> ${get} documents page/page_size but does not respond through success_offset_page`,
      );
    }
  }

  assert.deepEqual(problems, [], problems.join("\n"));
});
