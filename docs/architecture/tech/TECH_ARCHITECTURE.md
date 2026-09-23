# Merchandise Technical Architecture

Status: active development  
Owner: SDKWork maintainers  
Updated: 2026-09-23  
Specs: `ARCHITECTURE_DECISION_SPEC.md`, `RUST_CODE_SPEC.md`, `API_SPEC.md`,
`WEB_FRAMEWORK_SPEC.md`, `DATABASE_SPEC.md`, `DATABASE_FRAMEWORK_SPEC.md`

## 1. Architecture Overview

`sdkwork-merchandise` is the commerce merchandise capability owner. It implements
the catalog route module that the Shop backend authority mounts, backed by a
contract-first Rust service, a PostgreSQL repository adapter, and runtime
composition crates.

```text
Shop backend authority: sdkwork-shop-backend-api
        |  mounts sdkwork-api-merchandise-assembly in the same origin
        v
sdkwork-routes-merchandise-backend-api
        |  27 routes under /backend/v3/api/catalog/*
        v
sdkwork-merchandise-web-support
        |  HTTP DTO, response envelope, catalog store port and its adapters
        v
CatalogRepositoryPort        <-- sdkwork-merchandise-service
        |                        drafts, commands, queries, validation
        v
sdkwork-merchandise-repository-sqlx
        |  PostgresCommerceCatalogStore
        v
commerce_* baseline tables (17)
        database/ddl/baseline/postgres/0001_merchandise_baseline.sql
```

There is exactly one product model: SPU master data owned by
`commerce_product_spu`, sellable units by `commerce_product_sku`, the category
tree by `commerce_product_category`, the attribute dictionary by
`commerce_product_attribute` / `commerce_product_attribute_value`, category
templates by `commerce_product_category_attribute`, and pricing by
`commerce_price_list` / `commerce_price_list_item`. The former single-SKU
service/repository family and its `spec_json` / `_sdkwork` metadata envelope have
been deleted; product classification is a first-class `product_type` column, not
a JSON side channel.

## 2. Technology Choices

- Rust domain and service contracts (`RUST_CODE_SPEC.md`)
- SQLx against PostgreSQL for authoritative-server persistence
- `sdkwork-database` pool and lifecycle integration (`DATABASE_FRAMEWORK_SPEC.md`)
- Axum route crates integrated through `sdkwork-web-framework`
- `sdkwork-commerce-money` for multi-precision money: integer minor units, an
  explicit currency code and scale, and rejection (not silent rounding) of
  over-precision input
- Generated SDKs and SDKWork response envelopes for HTTP consumers

## 3. System Boundaries And Modules

| Layer | Owner | Responsibility |
| --- | --- | --- |
| Money kernel | `sdkwork-commerce-money` | Currency scale registry, minor-unit arithmetic, allocation, rounding |
| Service contract | `sdkwork-merchandise-service` | Validation, typed commands/queries, domain drafts, `CatalogRepositoryPort`, capability tokens |
| Persistence adapter | `sdkwork-merchandise-repository-sqlx` | Tenant-scoped SQL and transactions over the merchandise baseline |
| HTTP support | `sdkwork-merchandise-web-support` | Shared DTO, response envelope, and catalog store port without API ownership |
| Backend routes | `sdkwork-routes-merchandise-backend-api` | Catalog operator routes contributed to the Shop backend authority |
| Service host | `sdkwork-merchandise-service-host` | Process-shared pool acquisition and service hosting |
| Database host | `sdkwork-merchandise-database-host` | Module loading, lifecycle options, `bootstrap_merchandise_database` |
| Runtime composition | `sdkwork-api-merchandise-assembly` | Route assembly and web module context for federated hosts |
| Runtime gateway | `sdkwork-api-merchandise-standalone-gateway` | Pool, IAM, route, and readiness wiring for standalone runs |
| Browser client | `apps/sdkwork-merchandise-pc` | PC console consuming the generated backend SDK |

Inventory quantities, carts, and buyer addresses are owned by other
capabilities (`sdkwork-inventory`, `sdkwork-catalog`) and are deliberately absent
from every crate here.

## 4. Directory And Package Layout

- `crates/sdkwork-commerce-money/`
- `crates/sdkwork-merchandise-service/`
- `crates/sdkwork-merchandise-repository-sqlx/`
- `crates/sdkwork-merchandise-web-support/`
- `crates/sdkwork-routes-merchandise-backend-api/`
- `crates/sdkwork-merchandise-database-host/`
- `crates/sdkwork-merchandise-service-host/`
- `crates/sdkwork-api-merchandise-assembly/`
- `crates/sdkwork-api-merchandise-standalone-gateway/`
- `apps/sdkwork-merchandise-pc/`

Each authored crate owns its local `specs/component.spec.json` contract.

## 5. API, SDK, And Data Ownership

- Backend API prefix: `/backend/v3/api/catalog`, 27 registered routes.
- Canonical API authority: `sdkwork-shop-backend-api`; the Shop assembly mounts
  `sdkwork-api-merchandise-assembly` in the same origin and generates the
  combined owner-only backend SDK.
- Canonical generated SDK: `sdkwork-shop-backend-sdk`. No
  Merchandise-owned app-api, backend-api, open-api, or standalone SDK family
  exists.
- Capability tokens are the registered route operation ids, verbatim. The
  declared set and the route manifest are pinned as a set equality by a test, so
  a token with no route and a route with no token both fail.
- Database tables: the 17-table `commerce_*` baseline in
  `database/ddl/baseline/postgres/0001_merchandise_baseline.sql`, with contract
  artifacts regenerated from that baseline.
- Identifier fields are `BIGINT` in storage and decimal strings on the wire, per
  `API_SPEC.md` §13.6. Frontend code never converts ids to `number`.

HTTP handlers use `sdkwork-web-framework` response mapping.
Backend-admin consumers use `sdkwork-shop-backend-sdk`; the Shop assembly mounts
this repository's backend route module in the same origin.

## 6. Security, Privacy, And Observability

- Tenant and organization predicates are applied before merchandise reads or
  writes.
- List queries are bounded to at most 200 records per page.
- SQL uses bound parameters; no user input is concatenated into statements.
- Errors map to typed service errors and must not expose raw SQL details at HTTP
  boundaries.
- Runtime hosts provide pool health, readiness, tracing, and audit integration.

## 7. Deployment And Runtime Topology

The repository supports embedded and standalone composition. The service host
first tries the process-shared pool and otherwise bootstraps the merchandise
database module through `bootstrap_merchandise_database`, so a federated host can
register this module and run init, migrate, and seed through the shared
orchestrator. Authoritative-server persistence is PostgreSQL only. The capability
is still in pre-release implementation and makes no production deployment claim.

## 8. Architecture Decision Index

- One product model, SPU/SKU: the single-SKU service and repository family was
  deleted rather than kept as a second path.
- `spec_json` was removed as a storage carrier; product classification,
  specification, and localization each got a typed home.
- Money is integer minor units with explicit currency and scale; there are no
  `member_price` or `activity_price` columns.
- Pricing is two layers: the SKU base price plus `commerce_price_list_item`
  coverage by market, customer segment, quantity break, and time window.

## 9. Open Work And Known Gaps

These are known and deliberately not disguised as complete:

- The authored OpenAPI request bodies reference the opaque
  `CommerceOperationCommand` schema. Eleven of the seventeen write operations are
  affected, so the document names none of their request properties. The generated
  SDK therefore types every `create`/`update` body as an unbounded bag, and
  `API_SPEC` section 13.6 cannot be declared on the request side at all. The HTTP
  field set is currently owned by the `sdkwork-merchandise-web-support` DTOs alone,
  with nothing machine-checkable tying a documented property to a handler field.
  The five operations with a body are the four `create`/`update` pairs plus
  `attributes.create`; the four `DELETE`s and the two product transitions correctly
  declare no body at all.
- `CatalogRepositoryPort` is declared in `sdkwork-merchandise-service`, advertised as
  a provided port by `crates/sdkwork-merchandise-service/specs/component.spec.json`,
  materialised into `generated/composition.resolved.json`, and implemented nowhere in
  the workspace. The port the router actually depends on is `CommerceCatalogStore`,
  which is declared **and** implemented inside `sdkwork-merchandise-web-support` for
  the concrete `PostgresCommerceCatalogStore`. Two consequences follow: the
  composition registry advertises a port that cannot be satisfied, and the HTTP
  support crate names a `sqlx` type rather than depending on a port owned by the
  service crate. Aligning on a single port is an architecture change with
  composition-spec impact.
- Attribute `value_type`, `input_hint`, `is_multi_value`, and `sort_order` are
  hardcoded as `'enum'`, `'select'`, `FALSE`, `0` by the create statement, so no
  API can declare a numeric, date, boolean, range, or multi-select attribute.
- `created_by`, `updated_by`, and `deleted_by` are written as `NULL`. No write
  command carries an actor, although `IamAppContext.user_id` is available at the
  router. The columns exist and the baseline constrains them, so this is a
  missing input rather than a missing design.
- `variant_signature` falls back to `sku_no` until sales axes are an API input.
  The one-live-SKU-per-signature unique index still holds, but two colourways of
  one product cannot yet be distinguished by signature.
- The `commerce_` prefix is registered at domain granularity by eight
  repositories, so table-family ownership inside the family is not expressible
  in the workspace registry yet.
- `sdkwork-order` still reads the deleted `price_amount` column and divides by a
  literal `100`, so it cannot consume the `*_minor` + `price_scale` contract until
  that repository is aligned.
- The operationIds `categories.management.list`, `products.management.list`,
  `products.management.retrieve`, and `attributes.management.list` carry a
  `management` segment that maps to no URL segment. `API_SPEC` section 7.3 allows
  exactly one trailing action segment and derives the middle segments from the
  static path, so the segment is not sanctioned there; it is however the
  established shape in `sdkwork-community`, `sdkwork-company`, `sdkwork-feeds`,
  `sdkwork-music`, and `sdkwork-notary`, and section 7.3 also makes an operationId
  change breaking with explicit version governance required. Raising it belongs in
  `sdkwork-specs`, not in a single consuming repository.

## 9.1 Closed Since The v1 Baseline

Recorded so the entries above are not read as a full list of what the v1
repository adapter did:

- `products` and `spus` were two collection paths for one aggregate
  (`commerce_product_spu`): same table, same commands, and list handlers that
  differed only in their error string. The `/catalog/spus` collection was retired
  and the two lifecycle transitions moved to
  `/catalog/products/{productId}/publish` and `/archive`, so
  `products.publish` and `products.archive` are now the only names for them. The
  surviving vocabulary is `products`, which is what the rest of SDKWork uses: the
  app surface is `/app/v3/api/shops/current/products` and the PC consumer calls
  `products.retrieve`. The operation count went from 27 to 24. The
  `spus.*` capability tokens are now in the service crate's banned-fragment list,
  so the retired surface cannot be declared again.
- The `/products` and `/spus` list handlers shared one `Query<SpuListQueryParams>`
  that matched neither contract. The DTO is now `ProductListQueryParams`, carrying
  exactly the six parameters `/products` declares, and `q` is implemented: the
  store binds it and PostgreSQL builds the `ILIKE` pattern over `spu_no`, `title`,
  and `name` from the bound parameter, so the needle never becomes SQL text. The
  `cursor` parameter disappeared with the collection that was its only user.
- The four `category_attributes` operations are served end to end. The store gained
  `list_category_attributes`, `count_category_attributes`,
  `create_category_attribute`, `update_category_attribute`, and
  `delete_category_attribute` over static `concat!` statements: the `BIGINT`
  primary key is minted before the `INSERT`, a duplicate live binding on
  `uk_commerce_product_category_attribute_binding` becomes a `409`, a missing
  category or attribute becomes a `422` through the `23503` mapping rather than a
  pre-check race, and deletion is the `deleted_at` pair. The `RETURNING` clause
  carries the projection, so no separate retrieve statement exists.
- Every list route answers through `success_offset_page`. The `category_attributes`
  and `price_lists` collections previously returned a bare JSON array, which is not
  the `data.items` / `data.pageInfo` envelope this repository's `AGENTS.md`
  mandates; both now page in SQL and report a total.
- The `organization_id` query parameter was removed from all seven list handlers.
  It was never declared in the OpenAPI, and it was consumed only as
  `subject.organization_id.or(params.organization_id)`, which let a caller whose
  token carried no organization select any organization inside the tenant. Scope
  now comes from `IamAppContext` alone, matching the create handlers, which already
  refuse to run without a context organization.
- `scope` was removed from the `/attributes` parameter list: the attribute model
  has no scope column, so the parameter could not have been honoured.
- The `category_attributes` query type gained `attribute_id`, `status`, `page`, and
  `page_size`, and `price_lists` gained `currency_code`, `market_code`, `page`, and
  `page_size`, so every parameter the document declares for those two collections is
  now read. The SKU collection's parent filter is `product_id`, the name the
  document declares; the handler had accepted the undocumented `spu_id` instead, so
  the documented parameter was silently ignored.
- The PostgreSQL adapter now writes `BIGINT` primary keys minted by an injected
  `Arc<dyn IdGenerator>` before the `INSERT`, binds `TIMESTAMPTZ` from `NOW()`,
  and stores money as `*_minor` plus `price_scale`, replacing the previous
  `uuid_v7()` text ids, ISO-8601 text timestamps, and text amounts.
- Every statement is a `&'static str` built from `concat!` and literal-only column
  macros, so there is no dynamic-SQL surface for `sqlx` to guard against.
- `ProductStatus` asserts that no `deleted` value exists, and deletion is the
  `deleted_at` / `deleted_by` pair with `deleted_at IS NULL` on every read.
- Domain storage vocabulary is pinned to the baseline CHECK sets, so a value the
  schema would reject fails in the service test rather than at `INSERT` time.

## 10. Verification

```bash
cargo metadata --no-deps --format-version 1
cargo fmt -- --check
cargo clippy --workspace --tests -- -D warnings
cargo test --workspace
pnpm test:node
node ../sdkwork-specs/tools/check-api-operation-patterns.mjs --workspace .
node ../sdkwork-specs/tools/check-api-response-envelope.mjs --workspace .
node ../sdkwork-specs/tools/check-pagination.mjs --workspace .
node ../sdkwork-specs/tools/check-rust-crate-naming-standard.mjs --root .
node ../sdkwork-specs/tools/check-rust-manifest-standard.mjs --root .
node ../sdkwork-specs/tools/check-database-framework-standard.mjs --root .
pnpm api:check
pnpm api:check:route-manifest
pnpm api:assembly:validate
pnpm check
pnpm verify
```

`pnpm test:node` covers the closure gates that no compiler can express:
`tests/contract/catalog-sql-baseline-closure.test.mjs` asserts every column the
catalog SQL touches exists on a table the statement touches and in the baseline,
and `tests/contract/seed-manifest-closure.test.mjs` asserts every seed path the
manifest names resolves to a file, that each locale set's checksum is the real
one, and that no shipped seed script is left unrun.
