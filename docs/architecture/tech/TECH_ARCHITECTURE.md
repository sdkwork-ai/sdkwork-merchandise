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
| Service contract | `sdkwork-merchandise-service` | Validation, typed commands/queries, domain drafts, and the strictly-typed, asynchronous `CatalogRepositoryPort` it owns |
| Persistence adapter | `sdkwork-merchandise-repository-sqlx` | Tenant-scoped SQL and transactions over the merchandise baseline, and the `impl CatalogRepositoryPort` binding |
| HTTP support | `sdkwork-merchandise-web-support` | Shared DTO, response envelope, and route mounting over an injected store port; owns no API surface and no persistence contract |
| Backend routes | `sdkwork-routes-merchandise-backend-api` | Catalog operator routes contributed to the Shop backend authority; mounts HTTP and constructs nothing |
| Service host | `sdkwork-merchandise-service-host` | Process-shared pool acquisition, the Snowflake identity, and construction of the catalog repository behind the port |
| Database host | `sdkwork-merchandise-database-host` | Module loading, lifecycle options, `bootstrap_merchandise_database` |
| Runtime composition | `sdkwork-api-merchandise-assembly` | Route assembly and web module context for federated hosts |
| Runtime gateway | `sdkwork-api-merchandise-standalone-gateway` | Pool, IAM, route, and readiness wiring for standalone runs |
| Browser client | `apps/sdkwork-merchandise-pc` | PC console consuming the generated backend SDK |

Inventory quantities, carts, and buyer addresses are owned by other
capabilities (`sdkwork-inventory`, `sdkwork-catalog`) and are deliberately absent
from every crate here.

Dependencies point one way across these rows. `sdkwork-merchandise-service` owns the port and
depends on nothing but the shared contract types; `sdkwork-merchandise-repository-sqlx` depends on
the service crate to implement it; `sdkwork-merchandise-web-support` and the route crate depend on
the port and never on a repository; and `sdkwork-merchandise-service-host` is the single crate that
names both the port and its implementation, because constructing a dependency requires knowing what
it is.

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

- **Identifier strings are parsed in the repository, not at the request boundary.** `API_SPEC`
  section 13.6 asks the HTTP adapter to turn an inbound int64 string back into a native integer;
  today the adapter carries `*_id` as `String` into the command and
  `parse_id` in `sdkwork-merchandise-repository-sqlx` does the conversion. The behaviour is
  equivalent (a non-decimal id is a `400` either way) but the layers are not, which is why the
  contract's **request** id properties deliberately omit `x-sdkwork-rust-type`: the hint would claim
  an `i64` binding that does not exist yet. The **response** id properties do carry
  `x-sdkwork-rust-type: i64`, because that binding is real — a response id is produced from a Rust
  `i64` field, not parsed into one. The asymmetry is intentional and mirrors the two directions'
  actual bindings; it is not an oversight in either.
- **The shared untyped envelopes are now unreferenced by this document.** No operation answers with
  `SdkWorkResourceResponse` or `SdkWorkListResponse` any more, because every 2xx body pins its
  resource. Both stay declared: they are the specs-owned shared set (`sdkwork-specs`
  `tools/lib/openapi-envelope-schemas.mjs`), other repositories still `$ref` them, and the migration
  tool emits them, so deleting them locally would fight the library rather than remove dead code.
- **`commerce_price_list_item` has no code path at all.** Removing the dead reader left the table
  declared in the baseline with its CHECK constraints and foreign keys, and nothing in
  `sdkwork-merchandise` reads or writes it. Either a price-list-item operation is owed, or the table
  should be retired; the two are different decisions and neither is made here.
- **`tenantId` and `organizationId` are published unevenly.** `PriceListResource` and
  `CategoryAttributeResource` expose them; `Category`, `Product`, and `Sku` do not. Both are always
  the caller's own scope, so the values are not a disclosure, but a client cannot tell from the
  contract whether the omission is deliberate.
- **Handler-level rejection is pinned by a static gate, not by an end-to-end test.** The closure
  gate fixes the wire pattern of every int64 field and the vocabulary of every enum, and
  `parse_int64` is unit-tested, but no test drives a full write route into a store, because
  `CatalogRepositoryPort` has no test double. Adding one is now cheap — the router holds
  `Arc<dyn CatalogRepositoryPort>` and the composition root decides what arrives, so a double needs
  no database — but until it exists, a mutation of a handler's parse path is caught by review rather
  than by a test.
- Attribute `value_type`, `input_hint`, `is_multi_value`, and `sort_order` are
  hardcoded as `'enum'`, `'select'`, `FALSE`, `0` by the create statement, so no
  API can declare a numeric, date, boolean, range, or multi-select attribute.
  `valueType` and `sortOrder` are at least readable back; `input_hint` and
  `is_multi_value` are written on `INSERT` and never selected, so they appear in
  neither the record nor the response and would need the read model widened before
  they could be published.
- `created_by`, `updated_by`, and `deleted_by` are written as `NULL`. No write
  command carries an actor, although `IamAppContext.user_id` is available at the
  router. The columns exist and the baseline constrains them, so this is a
  missing input rather than a missing design.
- `variant_signature` is built from the SKU's sales axes, which are now an API input
  (`attributeValueIds` on the SKU create and update bodies). The signature is the resolved
  `attributeNo=valueCode` terms ordered by `attributeNo` and joined with `;`, so it is stable across a
  tenant re-creating an attribute — an id-based signature would let the same logical variant exist
  twice and would silently defeat `uk_commerce_product_sku_variant`. The convention is shared by three
  places that cannot see each other — the repository builder, the seed's reference rows, and the
  unique index that compares them — so `tests/contract/catalog-variant-signature-closure.test.mjs`
  recomputes it from the seed and the DDL rather than trusting any one of them. Two limits remain: a
  SKU on no axis still falls back to `sku_no`, because the empty combination has no terms to join, and
  the repository derives the attribute behind each value rather than accepting it, so a caller cannot
  yet declare an axis that the category template does not carry.
- **Seven of the seventeen baseline tables have no code path, and `tests/contract/
  catalog-table-coverage.test.mjs` now names them.** The gate partitions the baseline into tables a
  non-comment source file reaches and tables recorded in its `DECLARED_WITHOUT_A_CODE_PATH` list, and
  asserts that every recorded entry also appears in this section. The seven, and the decision each is
  waiting on:
  - `commerce_product_category_translation`, `commerce_product_attribute_translation`,
    `commerce_product_attribute_value_translation`, `commerce_product_spu_translation`, and
    `commerce_product_sku_translation` — **localization is an unimplemented feature**, not a schema
    question. The baseline models per-locale names for all five entities, the seed manifest carries a
    `localeSet`, and no operation, read model, or command reads a locale: every write stores the
    default locale on the base row. Wiring this needs a locale input on the write bodies, a
    locale filter on the reads, and a decision about whether a missing translation falls back to the
    base row or is an error.
  - `commerce_product_spu_attribute` — **a missing write path for a feature the schema already
    models.** `commerce_product_category_attribute` reserves an attribute as a `parameter` or a
    `sales` axis; only the sales axes are reachable today, through the SKU variant bindings. A
    product's parameter values (material, warranty term, processor) therefore cannot be recorded at
    all, so the product detail page has no specification data. The gap is the operation, not the
    table.
  - `commerce_price_list_item` — **a design decision, not a build task.** Four questions have to be
    answered together: whether per-SKU list prices exist alongside
    `commerce_product_sku.sale_price_minor`, which of the two a checkout reads, whether an item
    carries its own currency scale, and what happens to item rows when a price list is deactivated.
    Until they are, the table is either owed an operation or is redundant, and retiring it is a
    different change from wiring it.
- **The table-level coverage gate has a column-level blind spot, and it currently hides 47
  columns.** Measured 2026-09-23: the baseline declares 271 columns across the 17 tables, and 47 of
  them appear in neither a non-comment source file nor the authored contract. A table either has a
  code path or is recorded, so a live table can carry an entire dead column set invisibly — this is
  the same failure the table gate was written to end, one level down. The 47, grouped by the decision
  each is waiting on:
  - **23 are the audit columns** `created_by`/`updated_by`/`deleted_by` on `category`, `attribute`,
    `category_attribute`, `spu`, `sku`, `price_list`, and `price_list_item`, plus
    `attribute_value.deleted_by` and `media.deleted_by`. `IamAppContext.user_id` is available at the
    router and the baseline constrains these columns; the gap is that no write command carries an
    actor, so every row records no one.
  - **5 are `locale` on the translation tables** — the localization item above, counted again here
    because a column census sees what a table census cannot.
  - **10 are the commercial attributes** `spu.brand_id`, `spu.model_no`, `spu.barcode`,
    `spu.weight_gram`, `spu.volume_ml`, `spu.tax_class_code`, `sku.barcode`, `sku.weight_gram`,
    `sku.cost_price_minor`, and `sku.inventory_policy`. The baseline's own comment calls these
    "fixed-semantics fields … columns, not dynamic attributes, so they are queryable and
    constrainable", and it backs two of them with unique partial indexes
    (`uk_commerce_product_spu_tenant_barcode`, `uk_commerce_product_sku_tenant_barcode`). Nothing
    reads or writes any of them, so the schema promises barcode uniqueness, ship weight, volume, a
    tax class, and a cost price that the service cannot receive — and a cost price is what a margin
    needs.
  - **2 are `commerce_currency.display_symbol` and `display_name`**, so a price cannot be formatted
    with the currency's own symbol without a second source of truth.
  - **2 are presentation metadata for the variant axis**: `attribute.unit_symbol` and
    `attribute_value.color_hex`. Without the colour a swatch cannot be rendered, which is the
    standard presentation of a colourway.
  - **2 are `commerce_price_list_item.price_minor` and `min_quantity`** — the quantity-break and
    per-SKU-price item of the price-list decision above.
  - **3 remain single columns**: `category.back_parent_id` (provenance for a copied category tree),
    `spu_attribute.raw_value` (the unparsed parameter value, part of the `spu_attribute` item
    above), and `price_list.customer_segment` (the B2B segment a list applies to).
- 🔴 **Two of those columns show why a column *presence* check is not enough, and both are the
  reason this section states the count rather than claiming the census closes the class.**
  `commerce_product_sku.version` would be judged *wired* by any presence test, because the
  repository writes `SET version = version + 1`; but nothing reads it, nothing compares it, and it
  is absent from every read model and from the contract, so it enforces nothing — two concurrent
  editors both succeed and the second silently overwrites the first. The column exists, is bumped,
  and protects no one. Its mirror is `commerce_product_sku.inventory_policy`: it appears in exactly
  one place, a doc comment on `InventoryTrackingMode` stating that "the policy is derived from it,
  never set independently" — and no code derives it, so the value is always the column default
  `deny`, `ck_commerce_product_sku_policy_requires_tracking` is satisfied trivially, and
  `backorder` is unreachable. **A census finds columns nothing mentions; it cannot find columns that
  are mentioned for the wrong reason**, so `version` and `inventory_policy` are recorded here by
  hand, with the evidence, rather than left to the gate. The `version` half of that pair is now
  closed — see section 9.1, "Optimistic concurrency is enforced, not declared" — while
  `inventory_policy` remains open.
- **The media surface stores a caller-supplied projection.** `commerce_product_media` holds the Drive
  reference (`media_resource_id`, derived from the snapshot's `id`) plus a `resource_snapshot`
  read-model projection, exactly as `MEDIA_RESOURCE_SPEC` section 5 requires. What is not wired is a
  Drive read port: nothing in this capability verifies that the referenced node exists, is readable by
  the tenant, or still matches the snapshot, because there is no Drive client injected to ask. The
  consequence is bounded — an attachment can reference a resource that is not there, and a caller can
  write a snapshot that has drifted from the file — and closing it means a required drive port on the
  service crate plus a call inside the create/update transaction, not a schema change.
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
- **`sdkwork-merchandise-web-support` declares a layer role the architecture
  specification does not define.** Its `specs/component.spec.json` says
  `backend-provider`, a string that appears in neither the crate-family table nor the
  rules of `APPLICATION_LAYERED_ARCHITECTURE_SPEC.md` section 4.1; the crate is also the
  only `*-web-support` carrier in the workspace, and it holds two layers at once — the
  route table and query binding (L1) and the DTOs, mappers, and error envelope (the
  L1/L2 boundary). This is no longer a dependency-direction violation, since the
  repository edge is gone and its responsibilities are honest, but the declaration
  still names a role no shared gate can reason about, so nothing outside this
  repository's own gates can check it.

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
  schema would reject fails in the service test rather than at `INSERT` time. The
  comparison is scoped per table, not per column name: `attribute_role` carries two
  different CHECK sets (`commerce_product_category_attribute` accepts
  `key`/`sales`/`parameter`; `commerce_product_spu_attribute` accepts only
  `key`/`parameter`), so binding `AttributeRole` to the wrong table would compare
  against the wrong list and pass.
- The eleven write request bodies are typed. Each operation references a named
  component schema (`CreateCategoryRequest`, `UpdateSkuRequest`, …) with
  `additionalProperties: false`, camelCase properties, an explicit `required` list,
  `int64` properties declared as JSON strings, enums equal to the baseline CHECK
  set of the column they feed, `maxLength` equal to the Rust bound constant and to
  the baseline `char_length` bound, and monetary amounts declared in the currency's
  minor unit with `x-sdkwork-money-unit: minor`. `CommerceOperationCommand` is
  retired. The request bodies are closed on the Rust side too: every DTO carries
  `deny_unknown_fields`, so an undeclared field is a `400` rather than a silently
  ignored value.
- Money left the major-denomination representation on the write path. The SKU price
  fields are `salePriceMinor` and `listPriceMinor`, the commands carry `i64` minor
  units, and the repository's major-to-minor conversion (`to_minor`) is gone. The
  currency's `minor_unit_exponent` is still read, but only to snapshot the row's
  `price_scale`; nothing on this path multiplies or divides by a literal.
- A rejected body is answered through the platform problem envelope. Bare
  `axum::Json` had produced an unenveloped, uncorrelated `422`; `CatalogJson` maps
  every extraction rejection onto the same `400` `ProblemDetail` the operations
  already declare, which is also what `deny_unknown_fields` needed in order not to
  publish a status code the contract does not list.
- The catalog's bounded text fields are validated at the command boundary.
  `CATEGORY_NAME_MAX_CHARS`, `ATTRIBUTE_NAME_MAX_CHARS`, `SPU_TITLE_MAX_CHARS`, and
  `PRICE_LIST_NO_MAX_CHARS` carry the baseline's `char_length` bounds, so an
  over-long name is a `422` naming the field instead of a `23514` the caller cannot
  act on.
- The response bodies are typed. Six resource schemas (`Category`, `Product`, `Sku`,
  `Attribute`, `CategoryAttribute`, `PriceList`) declare every field the adapter serializes, the
  fourteen single-resource operations publish `data.item` through the `sdkwork-specs`
  `typedSdkWorkResourceResponse()` builder, and the six list operations answer with a named
  `<Resource>ListResponse` wrapping the shared `PageInfo`. `API_SPEC` section 12 is the authority
  for the naming, and `sdkwork-order`'s `ShipmentListResponse` is the shape it is modelled on. Six
  operations that declared `201` with no body now declare the created resource, which is what the
  handlers had been returning all along.
- The response side stopped disagreeing with the request side about one resource. `spuId`/`spuNo`
  became `productId`/`productNo` on the wire, so the bodies, the `/products/{productId}` path, and
  the list filters now spell the resource the same way. `map_spu` became `map_product`; the domain
  and the storage column keep the `spu` name, and the translation lives in the single mapper.
- Two narrow integer fields stopped reaching the wire as JSON numbers. `depth` and `price_scale`
  are declared `format: int64`, so section 13.6 requires a decimal string regardless of the column's
  width; both now serialize through `serde_int64`. The SKU row's field is published as
  `minorUnitExponent`, because a field named `priceScale` is classified as money by the section 13.2
  validator and this value is an exponent, not an amount.
- A response type that no operation could reach is gone. `PriceListItemResponse`,
  `map_price_list_item`, the `CommerceCatalogStore::retrieve_sku_prices` declaration and its
  implementation, the repository read behind it, `PriceListItemRecord`, and `SkuPriceRetrieveQuery`
  formed a chain with no handler at its head; all six are deleted rather than kept as an unexposed
  capability.
- There is one catalog store port, it is owned by the service crate, and it is implemented. The
  port was declared twice over: `CatalogRepositoryPort` lived in `sdkwork-merchandise-service` with
  twenty-five **synchronous** methods and no implementation anywhere — a signature no I/O adapter
  can satisfy — while `CommerceCatalogStore` lived in `sdkwork-merchandise-web-support` with the
  thirty-two asynchronous methods the router actually called, implemented for
  `PostgresCommerceCatalogStore` in a third crate. The consequences were visible in both
  directions: `generated/composition.resolved.json` advertised a provided port that composition
  could resolve and no code could honour, and the HTTP adapter held a `sdkwork-database-id`
  `IdGenerator`, a `sqlx::PgPool`, and the concrete store, so a transport crate owned the
  persistence contract. The port is now `sdkwork_merchandise_service::CatalogRepositoryPort`,
  asynchronous, and bound by
  `crates/sdkwork-merchandise-repository-sqlx/src/postgres_catalog_port.rs`; construction moved to
  `MerchandiseServiceHost::catalog_repository()`, which is the single crate permitted to name both
  the port and its implementation. The HTTP adapter's dependency on
  `sdkwork-merchandise-repository-sqlx`, `sdkwork-database-id`, and `sqlx` is gone, and so is the
  route crate's on `sdkwork-database-sqlx`. One invariant also moved earlier in the lifecycle: the
  host now fails in its constructor when handed a non-PostgreSQL pool, where the route crate used to
  panic per request.
- `commerce_product_media` had no code path, so a product had no images at all. The media surface
  now exists end to end — port, transaction-scoped SQL, DTOs, four operations, `MediaResource` in the
  authored contract, and the capability tokens — and the two design questions that were open are
  settled the way the specifications require rather than the way that was quickest: the stored
  reference is a Drive identity plus a read-model snapshot instead of a URL, and the owner is a
  `(ownerType, ownerId)` pair verified per kind inside the write transaction instead of four nullable
  foreign keys, which would let one row claim two owners.
- The SKU variant axis was inert. `commerce_product_sku_attribute` had no writer,
  `variant_signature` fell back to `sku_no`, and two colourways of one product could not be
  distinguished by the unique index that exists for exactly that. `attributeValueIds` on the SKU
  create and update bodies now carries the axis, the attribute behind each value is derived
  server-side, the resolved set must cover the category's active `sales` axes exactly once, and the
  signature is recomputed from business keys so it survives a tenant re-creating an attribute.
- Typed commerce failures were all answered as `503 DependencyUnavailable`, including the validation
  and not-found failures the contract declares as `400` and `404`. The envelope helper now maps each
  error kind to the status its operation declares.
- **Optimistic concurrency is enforced, not declared.** `commerce_product_sku.version` was bumped by
  every write and compared by nothing, so two editors of one row both succeeded and the second
  silently overwrote the first; the same held for the other eleven `version` columns, and nine of the
  thirteen writers did not even bump. The row version is now the precondition: the resource schemas
  publish `version`, every guarded write requires `If-Match`, a single `SELECT … FOR UPDATE`-free
  guard compares it **inside the statement that writes**, and a `0`-row result is classified by one
  follow-up read into `404` (the row is gone) or `412` (it moved).
  - *Why the comparison is in the statement and not beside it.* Two designs were available:
    compare in Rust against a lock read from the same transaction, or fold the comparison into the
    writing statement's `WHERE`. Both are race-free, but only the second is checkable without a
    database — `AND version = $n` is a property of the statement text, so
    `tests/static/api-precondition-closure.test.mjs` can assert it on all thirteen statements in CI,
    where this workspace runs no PostgreSQL. The trusted-read design would have needed call-graph
    analysis to verify, which is the kind of check that quietly stops being true.
  - *Why `412` is not `409`.* `API_SPEC` 2190 separates a failed precondition (`41201`) from a
    domain state conflict (`40901`), and these operations answer both — a duplicate business key on
    update is a `409`. The repository therefore reports staleness on the **success** channel, as
    `GuardedWrite::StaleVersion`, instead of as `CommerceServiceError::conflict`. Recovering the
    distinction by matching the message text would be exactly the classification-by-string the
    shared contract type forbids, so the two outcomes are kept apart by type from the statement that
    knows which one happened.
  - *The version the caller gets back is the version the row is at.* Every guarded statement
    advances `version` and returns it through `RETURNING`, and the response publishes that number
    both as the body's `version` and as the `ETag` header; `GET /catalog/products/{productId}`
    publishes one too, so a client can hold the header and hand it straight back without parsing a
    payload. A row's version advances exactly once per transaction, by the statement that owns the
    rewrite: `REPOINT_CATEGORY_PARENT_SQL` and `UPDATE_SKU_VARIANT_SIGNATURE_SQL` write other
    columns of the row their guarded statement is about to rewrite, and advancing there would move
    the version out from under the guard one statement later. Cascades and materialised-path
    rewrites — `SOFT_DELETE_SPU_SKUS_SQL`, `MOVE_CATEGORY_SUBTREE_SQL`,
    `REFRESH_CATEGORY_LEAF_SQL` — advance the rows they touch but carry no guard, because none of
    those rows is a caller's `If-Match` target and a retired SKU or a moved descendant still has to
    look different to whoever read it.
  - *What is deliberately not version-visible.* A category move rewrites its descendants' `path`
    and its parents' `is_leaf`, and all of those advance. The moved row itself advances once. A
    client editing a **descendant** after its ancestor moved therefore still passes
    `If-Match` — the descendant's own version did advance, so the client is refused only if it read
    before the move and the move bumped that descendant, which it does. What no `version` expresses
    is "the subtree was re-rooted", because the guarded update sets `name`/`sortOrder`/`status` and
    cannot clobber a derived `path`; a subtree revision counter is a different mechanism and is not
    claimed here.
  - *The two protocol codes are bridged locally.* `SdkWorkResultCode` already defines
    `PreconditionFailed = 41201` and `PreconditionRequired = 42801` with the right HTTP statuses, but
    `WebFrameworkErrorKind` has no variant for either, and `problem_response` derives both the status
    and the code from that kind alone — so neither code is expressible through it. Rather than widen
    a shared crate outside this module's ownership, the two responses are built here from
    `SdkWorkProblemDetail` directly, in `http_envelope.rs`. The bridge is four lines and it is a
    bridge: when the shared kind set grows those two variants, this is the code to delete.

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
`tests/contract/catalog-variant-signature-closure.test.mjs` recomputes the
`variant_signature` convention — `attributeNo=valueCode` terms ordered by
`attributeNo`, joined with `;`, falling back to `sku_no` — from the seed's own
attribute dictionary, SKU rows, and axis rows, and compares the result with the
signature each seeded SKU stores. It recomputes rather than re-lists on purpose: a
hand-edited signature, or a dictionary entry renamed without updating the SKUs that
reference it, produces a different string. It also reads the baseline DDL for the
partial unique index over `(tenant_id, attribute_no)`, because "ordered by
`attribute_no`" is only a total order while that index holds, and a narrowing of it
would move the signature's determinism onto the repository's tie-break,
`tests/contract/catalog-table-coverage.test.mjs` closes the other direction — every
table the baseline declares is either reached by a non-comment source file or
recorded, with the decision it is waiting on, in a list this section also has to
name — so a table that carries CHECK constraints, foreign keys, and indexes while no
operation can populate or expose it is a recorded decision rather than an invisible
one, `tests/contract/seed-manifest-closure.test.mjs` asserts every seed path the
manifest names resolves to a file, that each locale set's checksum is the real
one, and that no shipped seed script is left unrun,
`tests/static/api-request-body-closure.test.mjs` compares the authored request
schemas against the extracting DTOs, the domain commands, and the baseline,
`tests/static/api-response-body-closure.test.mjs` compares the authored response
schemas against the response structs, the resource each handler actually maps, and
the baseline, so a documented property, a `required` entry, an enum value, an
`Option` that is not nullable, an `i64` without a string serializer, a monetary unit,
or a `201` that carries no body fails here rather than in a generated SDK, and
`tests/static/catalog-port-implementation-closure.test.mjs` compares the provided
ports against the crates: every advertised `target` must be an item the declaring
crate really declares, every trait port must have a real `impl` somewhere in
`crates/`, every `[dependencies]` entry on a `*-repository-*` crate must belong to a
composition root, the HTTP adapter must hold the port rather than a driver type, and
`generated/composition.resolved.json` must advertise exactly the ports the specs
declare. That last gate is the local answer to two blind spots in the shared
validators: `check-component-port-bindings` validates the *shape* of a port
declaration and never looks for an implementation, and the shared Rust composition
validator applies the route-to-repository prohibition only to service crates.
`tests/static/api-precondition-closure.test.mjs` holds the three layers of a guarded
write together: it reads the router's own `.route(...)` calls to learn which handler
serves which operation, reads each handler's span to learn whether that handler reads
`If-Match`, compares both against the contract's declared parameter in **both**
directions, and then reads the thirteen SQL statement constants to assert each one
compares the version in its `WHERE` and advances it in its `SET`. It also asserts that
every resource schema reached by a guarded operation publishes a required `version`
(naming a version source that no field supplies is otherwise a promise a generated SDK
cannot keep), that the internal cascades carry no caller precondition, and that the
counts in all three layers agree. Assertion 12 is the one that matters most: a
*fourteenth* guarded operation appearing in one layer and not the others is the failure
this gate exists to catch, and nothing else in the workspace would see it.
`tests/static/catalog-status-derivation-closure.test.mjs` holds the SPU and the SKU to one
rule about what being on sale means. The two tables carry the same `status` and
`sales_status` columns under the same three constraints, and the repository's statements
now derive `sales_status` from the status they were handed — so two statements that agree
today can disagree after one edit, in the one direction that matters: a filter on
sales_status returning a set no caller expects. The gate reads the statements and asserts
that `sales_status` is never bound from the caller, that the derivation in `UPDATE_SPU_SQL`
and `UPDATE_SKU_SQL` is the same expression once placeholders are normalized, that a
statement deriving it from `$n` also assigns `status` from that same `$n`, that a literal
status is one the baseline CHECK admits, that both product inserts start a row
`(draft, inactive)`, and that no statement ever clears `published_at` — which is what keeps
"publishing is a one-way transition" a property of the code and not of a comment.

Each of the seven closure gates that carries a mutation battery was shown non-vacuous by
it — fifty-three probes in total, every verdict as specified, and every recorded file
restored byte-identically afterwards. The probes and the assertion each one reddens are
listed in its own header: eight for the coverage gate, eight for the port gate, five for
the variant-signature gate, seven for the request-body gate, four for the response-body
gate, fourteen for the precondition gate, and seven for the status-derivation gate. The two
API gates rewrite the parsed
document rather than its text, because the export tool re-serialises the file and string
anchors stop matching; the port gate mutates the components, the manifests, and the
committed resolution in turn.

Four of those batteries changed the gate they were testing, which is the point of running
them rather than asserting non-vacuity. The coverage gate's battery is the reason two of
its rules are shaped the way they are: a control row proved that a comment naming a table
must not count as a code path, and a whole-document search for a gap table was satisfied by
an aside in this file's own section 10, so the check now reads section 9 and stops at 9.1.
The port gate's second row exists because its first draft read raw source: this crate's
documentation quotes the very `impl` the gate looks for, so the gate stayed green with the
real binding commented out — it now strips comments before searching. The status-derivation
gate's seventh row is the reason its first assertion does not skip a statement it finds no
status placeholder in: deleting `status = COALESCE($n::TEXT, status)` leaves the derivation
behind, and the "if a status is bound, the derivation uses it" form of that assertion called
the result consistent while the write ignored every status the caller sent. The
variant-signature gate's fourth row is a probe whose verdict is *green* on purpose. The
seed's two candidate ordering keys disagree about which axis comes first (`attribute_no` is
`tier`/`period`, `sort_order` is 10/20), and the seed's stored signatures follow
`attribute_no`; so moving `sort_order` must change nothing, and a gate that had keyed on it
would have gone red there. Its third row is the mirror image, moving only the order while
leaving the set of business keys intact. The precondition gate's header records the
*observed* reddening, and two rows differ from the first draft of that table — the draft
said the mutation that gives a create operation an `If-Match` reddens three assertions, and
it reddens six. That is the gate behaving correctly rather than a correction to the gate: a
newly guarded operation is also a newly undeclared `412`/`428` pair and a newly undocumented
version source, so one mistake in one layer is caught by every layer that was not updated
with it.
