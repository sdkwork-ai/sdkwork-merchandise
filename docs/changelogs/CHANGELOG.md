# Merchandise Changelog

## Unreleased - 2026-09-23

- Removed the `spec_json` / `_sdkwork` metadata envelope. Product
  classification is now the `commerce_product_spu.product_type` column, and
  specification values move to typed attribute tables.
- Removed the second product model. The single-SKU service, repository port,
  and SQLx family (`service/ports/single_sku_merchandise.rs`,
  `service/service/single_sku_merchandise.rs`,
  `repository-sqlx/single_sku_merchandise*`) were deleted rather than kept
  alongside the SPU/SKU path.
- Removed out-of-bounds code that read and wrote `commerce_cart`,
  `commerce_cart_item`, and `commerce_user_address`; those tables belong to
  `sdkwork-catalog`.
- Removed the HTTP contract's `catalog.categorySeeds`, `cart.*`, and
  `addresses.*` capability tokens. Declared capability tokens are now the
  registered route operation ids, verbatim, asserted by a test.
- Rewrote the PostgreSQL baseline to 17 tables with 74 CHECK constraints, 25
  foreign keys, 13 unique constraints, and 81 indexes; ids became `BIGINT`,
  timestamps `TIMESTAMPTZ`, and money `*_minor BIGINT` plus currency and scale.
- Added the `sdkwork-commerce-money` crate for multi-precision money with an ISO
  4217-derived scale registry and explicit over-precision rejection.
- Moved localization to base-table default language plus `*_translation` tables,
  so zh-CN and en-US can coexist instead of overwriting each other.
- Added `tests/verify_merchandise_baseline.py` (`pnpm db:verify:baseline`) and
  `database/seeds/common/001_currency.sql`.
- Deleted the domain draft family (`ProductSpuDraft`, `ProductSkuDraft`,
  `ProductCategoryDraft`, `ProductAttributeDraft`) from
  `sdkwork-merchandise-service`. It was a second write model for the same rows,
  reachable only from tests: the repository port consumes the `*Command` types, so
  every draft field was a duplicate name for a command field. The
  one rule the drafts enforced that the commands did not — the baseline
  `char_length(display_value) BETWEEN 1 AND 200` bound on attribute values — moved
  into `CreateAttributeCommand::validate`, so it is still a boundary `422` instead
  of a mid-transaction `23514`.
- Rewrote `sdkwork-merchandise-repository-sqlx` against the v2 baseline: primary
  keys are `BIGINT` minted by an injected `Arc<dyn IdGenerator>` before the
  `INSERT`, timestamps bind from `NOW()`, money is stored as `*_minor` plus the
  `price_scale` read from `commerce_currency.minor_unit_exponent`, and deletion is
  `deleted_at`/`deleted_by` with `deleted_at IS NULL` on every read. Every statement
  is a `&'static str` assembled from `concat!` and literal-only column macros, so
  no statement is built with `format!` and `sqlx`'s `SqlSafeStr` guard is satisfied
  without an escape hatch.
- Pinned the service crate's storage vocabulary to the baseline CHECK sets
  (`product_type` `digital`/`points`, `fulfillment_type` `physical`/`digital`/
  `points_topup`/`service`, `inventory_tracking` `quantity`/`none`), and deleted the
  SPU `visible_surfaces` and attribute `scope` fields, which the baseline does not
  declare.
- Injected the process Snowflake identity into the composition root. New
  `identity` and `runtime_env` modules in `sdkwork-merchandise-service-host` acquire
  the node id once per process — leased from the platform node registry in
  production-like environments, a static `SDKWORK_MERCHANDISE_SNOWFLAKE_NODE_ID`
  in development — and `MerchandiseServiceHost::id_generator()` hands the same
  generator to every repository. A static node id in a production-like environment
  is a startup failure, because it cannot be collision-free once more than one
  instance runs.
- Added `tests/contract/seed-manifest-closure.test.mjs`, which closes three holes
  `seed.manifest.json` left open: it resolves every path the manifest names with
  the same rules `sdkwork-database-spi` applies at seed time (so a typo fails in
  `pnpm test:node` instead of during a live `db:seed`), recomputes each locale
  set's checksum as `sha256` over the LF-normalised UTF-8 text of its files, and
  fails on a seed script that no profile runs. The existing manifest values were
  already correct under that rule; the gate is what keeps them so, because the
  framework itself only checks the field is non-empty.
- Documented `database/seeds/common/001_bootstrap.sql` as an intentional no-op and
  named the two seeds that actually carry the module's required reference data.
- Wired `@sdkwork/utils` into `tsconfig.base.json` and corrected the depth of the
  `@sdkwork/sdk-common` path mapping in `apps/sdkwork-merchandise-pc/tsconfig.json`,
  which resolved one directory above `sdkwork-space`.
- Implemented the four `category_attributes` operations, which were published in the
  OpenAPI document, the route manifest, and the SDK while the PostgreSQL adapter
  answered each one with a not-yet-implemented error. The store gained
  `list_category_attributes`, `count_category_attributes`,
  `create_category_attribute`, `update_category_attribute`, and
  `delete_category_attribute` over static `concat!` statements: the id is minted
  before the `INSERT`, a duplicate live binding on
  `uk_commerce_product_category_attribute_binding` becomes a `409`, a missing
  category or attribute becomes a `422` through the existing `23503` mapping rather
  than a pre-check race, and deletion is the `deleted_at` pair. The `UPDATE` carries
  a `RETURNING` projection, so no separate retrieve statement is needed.
- Completed the category attribute binding model in the write path:
  `CreateCategoryAttributeCommand` carries `role`, `comparable`, and
  `source_category_id`, and `UpdateCategoryAttributeCommand` carries `role`,
  `comparable`, and `status`. `AttributeRole` (`key` / `sales` / `parameter`) is now
  a domain type whose accepted set is pinned to
  `ck_commerce_product_category_attribute_role`, and the create body defaults to
  `parameter` so an unclassified binding cannot accidentally declare a sales axis.
  The self-inheritance CHECK is enforced at the boundary as a `422` naming the field.
- Switched the `category_attributes` and `price_lists` collections to the offset
  pagination envelope. Both previously answered with a bare JSON array, which is not
  the `data.items` / `data.pageInfo` shape this repository's `AGENTS.md` mandates and
  contradicts every other list route in the same document.
- Removed the `organization_id` query parameter from all seven list handlers. It was
  never declared in the authored OpenAPI, and it was read only as
  `subject.organization_id.or(params.organization_id)`, so a caller whose token
  carried no organization could select any organization inside the tenant.
  Organization scope now comes from `IamAppContext` alone, which is what the create
  handlers already required.
- Removed `scope` from the `/backend/v3/api/catalog/attributes` parameter list. The
  attribute model has no scope column, so the documented parameter could not have
  been honoured.
- Added the query parameters the document already declared but no handler read:
  `attribute_id`, `status`, `page`, and `page_size` on `category_attributes`, and
  `currency_code`, `market_code`, `page`, and `page_size` on `price_lists`. Added
  `count_price_lists` and `count_category_attributes` so the totals are computed in
  SQL rather than by loading the collection.
- Aligned the SKU collection's parent filter to the documented name `product_id`; the
  handler had accepted the undocumented `spu_id`, so the parameter the OpenAPI
  declared was silently ignored.
- Added `tests/static/api-query-param-closure.test.mjs`, which parses the router, the
  query DTO modules, and the OpenAPI document and fails when a documented query
  parameter is not read by its handler, when a handler accepts a field the document
  does not declare, or when a list operation documents `page`/`page_size` without
  responding through the offset envelope. Written RED first: it reported 24 findings,
  all of which are now closed. Verified non-vacuous by injecting a documented-but-unread
  `bogus_filter` parameter, observing the exact failure line, then removing it.
- Retired the `/backend/v3/api/catalog/spus` collection. It was a second name for
  `commerce_product_spu`: same table, same commands, and list handlers that differed
  only in their error string. The two lifecycle transitions moved to
  `/backend/v3/api/catalog/products/{productId}/publish` and `/archive` as
  `products.publish` and `products.archive`, the `spus.*` capability tokens were
  replaced, and the retired tokens joined the service crate's banned-fragment list so
  the surface cannot be declared again. The operation count went from 27 to 24.
  `products` is the surviving vocabulary across SDKWork: the app surface is
  `/app/v3/api/shops/current/products` and the PC consumer calls `products.retrieve`.
- Split the list DTO that `/products` and `/spus` shared. One
  `Query<SpuListQueryParams>` could satisfy neither contract, because `/products`
  declares `q`, `category_id`, `product_type`, and `sort` while `/spus` declared `q` and
  `cursor`. `ProductListQueryParams` now carries exactly the six parameters
  `/products` declares.
- Implemented the `q` product search. The store binds it as a parameter and
  PostgreSQL concatenates the `ILIKE` pattern over `spu_no`, `title`, and `name`, so
  the needle never becomes SQL text; a `NULL` needle disables the predicate instead of
  matching nothing.
- Removed the phantom request bodies from `products.publish` and `products.archive`.
  Both declared the opaque `CommerceOperationCommand` while the handler takes only the
  path id, so the documented body could never have had an effect.
- Typed all eleven write request bodies. Each operation now references a named
  component schema — `CreateCategoryRequest`, `UpdateCategoryRequest`,
  `CreateProductRequest`, `UpdateProductRequest`, `CreateSkuRequest`,
  `UpdateSkuRequest`, `CreateAttributeRequest`, `CreateCategoryAttributeRequest`,
  `UpdateCategoryAttributeRequest`, `CreatePriceListRequest`,
  `UpdatePriceListRequest` — with `additionalProperties: false`, camelCase property
  names, an explicit `required` list, enums equal to the baseline CHECK set of the
  column they feed, `maxLength` equal to both the Rust bound constant and the
  baseline `char_length` bound, and `int64` properties declared as JSON strings with
  `x-sdkwork-int64-string`. `CommerceOperationCommand`
  (`{"type":"object","additionalProperties":true}`) is deleted: it named no request
  field, so the generated SDK typed every write body as an unbounded bag and
  `API_SPEC` section 13.6 could not be declared on the request side at all.
- Moved money to minor units on the write path. `CreateSkuRequest` and
  `UpdateSkuRequest` carry `salePriceMinor` and `listPriceMinor` as
  `format: int64` strings with `x-sdkwork-money-unit: minor`, matching what the
  response already emitted (`salePriceMinor`). `CreateProductSkuCommand` and
  `UpdateProductSkuCommand` now carry `i64` minor units, so the repository's
  major-to-minor conversion (`to_minor`) is gone; the currency's
  `minor_unit_exponent` is still read, but only to snapshot the row's `price_scale`.
  Registering a currency that the money kernel cannot use still fails there, because
  the registry row is validated before its scale is trusted.
- Closed the request bodies on the Rust side. Every write DTO carries
  `serde(deny_unknown_fields)`, so a field the document does not declare is a `400`
  instead of a value the adapter silently ignored — the exact failure mode an
  `additionalProperties: false` contract is supposed to exclude.
- Answered a rejected body with the platform problem envelope. Bare `axum::Json`
  produced an unenveloped, uncorrelated `422` (plain text, no `traceId`) before the
  handler ran. `CatalogJson` maps every extraction rejection onto the `400`
  `ProblemDetail` the write operations already declare, so adding
  `deny_unknown_fields` did not also publish a status code the contract does not list.
- Bound the catalog's text fields at the command boundary:
  `CATEGORY_NAME_MAX_CHARS` (200), `ATTRIBUTE_NAME_MAX_CHARS` (100),
  `SPU_TITLE_MAX_CHARS` (300, the bound on the name derived from it), and
  `PRICE_LIST_NO_MAX_CHARS` (200, likewise). An over-long value is now a `422` naming
  the field instead of a `23514` raised mid-transaction inside PostgreSQL.
- Renamed the SKU create body's parent field to `productId` and the product create
  body's business key to `productNo`. The documented parameter, the URL path
  (`/products/{productId}`), and the query filter were already `product`-spelled, so
  the bodies were the last request-side place publishing the internal `spu` name.
- Added `tests/static/api-request-body-closure.test.mjs`. It compares the authored
  request schemas against the DTOs each route extracts, the domain commands those
  DTOs feed, and the baseline: contract properties equal the DTO's fields in their
  wire camelCase spelling, `required` equals the non-`Option` fields, each property's
  JSON type matches the Rust field, every `enum` equals its column's CHECK set, every
  `maxLength` equals its constant and that constant equals the SQL bound, and every
  monetary field declares the minor unit and reaches an `i64` command field. Verified
  non-vacuous by a nine-mutation battery — renaming a property, dropping a `required`
  entry, widening an enum, loosening a length, switching a price to major units,
  opening a closed body, resurrecting the shell schema, removing
  `deny_unknown_fields`, and declaring an optional price as `Option<String>` instead
  of `Option<i64>` — each of which turned exactly its own assertion red. The
  mutations rewrite the parsed JSON object rather than the raw text, because the
  export tool re-serialises the file and string anchors silently stop matching.
- Added request-boundary unit tests in `sdkwork-merchandise-web-support`: a
  documented body reaches the handler, an undeclared field is refused with
  `application/problem+json` carrying `code: 40001` and a `traceId`, a JSON number
  where the contract declares a string is refused, a missing required field is
  refused, and the int64 string parser rejects a non-decimal value.
- Extended `storage_vocabulary_is_accepted_by_the_baseline_check_constraints` to
  `AttributeRole` against `commerce_product_category_attribute.attribute_role`. The
  comparison is now scoped per table rather than per column name, because
  `attribute_role` carries two different CHECK sets: the category attribute table
  accepts `key`/`sales`/`parameter` while `commerce_product_spu_attribute` accepts
  only `key`/`parameter`. Verified non-vacuous by loading the SPU-scoped list for the
  category case, which fails with ``domain emits `sales`, baseline allows [key,
  parameter]``, and by naming an unloaded table, which trips the guard.
- Typed the response bodies. Added six resource schemas (`Category`, `Product`,
  `Sku`, `Attribute`, `CategoryAttribute`, `PriceList`) and six named
  `<Resource>ListResponse` schemas, and rewired all twenty operations that return a
  body: the fourteen single-resource responses now publish `data.item` through the
  `sdkwork-specs` `typedSdkWorkResourceResponse()` builder, and the six lists answer
  with a `<Resource>ListResponse` wrapping the shared `PageInfo`. The naming follows
  `API_SPEC` section 12 and the list shape follows `sdkwork-order`'s
  `ShipmentListResponse`. No operation answers with the untyped
  `SdkWorkResourceResponse` or `SdkWorkListResponse` any more; those components stay
  declared because they are the specs-owned shared set.
- Fixed six operations whose `201` declared no body while their handlers returned
  `success_created_resource(...)`. The created resource is now declared, which is what
  the wire had been carrying.
- Renamed the response side's product vocabulary: `spuId` and `spuNo` became
  `productId` and `productNo`, matching the request bodies, the
  `/products/{productId}` path, and the list filters. `SpuResponse` and `map_spu`
  became `ProductResponse` and `map_product`; the domain and the `spu_no` column keep
  their names, and the single mapper does the translation.
- Stopped sending two narrow integer fields as JSON numbers. `depth` and
  `price_scale` are declared `format: int64`, and section 13.6 requires a decimal
  string regardless of the column's width, so both now serialize through
  `serde_int64`. The SKU row's field is published as `minorUnitExponent` because
  `priceScale` is classified as money by the section 13.2 validator while the value is
  a unit exponent.
- Deleted an unreachable response family: `PriceListItemResponse`,
  `map_price_list_item`, `CommerceCatalogStore::retrieve_sku_prices` and its
  implementation, `RETRIEVE_SKU_PRICES_SQL`, `map_price_list_item_row`,
  `PriceListItemRecord`, and `SkuPriceRetrieveQuery`. No handler ever called the
  method, so nothing on the wire changes. `commerce_price_list_item` now has no code
  path at all, which is recorded in the architecture notes.
- Added `tests/static/api-response-body-closure.test.mjs`. It asserts that every 2xx
  JSON body pins a resource, that each operation publishes the resource its own
  handler maps, that resource properties equal the response struct's fields in
  camelCase, that `required` is exactly the non-`Option` fields, that an `Option` is a
  nullable union, that every Rust `i64` field carries a `serde_int64` serializer and is
  declared a decimal int64 string with `x-sdkwork-int64-string` and
  `x-sdkwork-rust-type`, that every enum equals its column's CHECK set, that monetary
  fields declare the minor unit, that creates declare a body and deletes do not, and
  that lists publish a typed array plus the standard page info. Verified non-vacuous by
  an eleven-mutation battery.
- Unified the catalog store port on the one the composition specification already
  advertised. `CatalogRepositoryPort` is now asynchronous and is the only store port:
  its twenty-five synchronous, never-implemented methods are replaced by the
  thirty-two asynchronous ones the router actually called, `CommerceCatalogFuture` and
  `CatalogOffsetPage` moved to `sdkwork-merchandise-service` with it, and
  `CatalogOffsetPage::new` applies the pagination defaults so no adapter picks its own
  page size. A synchronous port was an unsatisfiable contract rather than a strict one,
  which is why nothing had ever implemented it.
- Bound the port where the dependency direction allows it. The thirty-two forwarding
  methods left `sdkwork-merchandise-web-support` and became
  `impl CatalogRepositoryPort for PostgresCommerceCatalogStore` in a new
  `crates/sdkwork-merchandise-repository-sqlx/src/postgres_catalog_port.rs`; the HTTP
  adapter would have had to depend on the repository crate to name the type. The
  adapter's dependency on `sdkwork-merchandise-repository-sqlx`, `sdkwork-database-id`
  and `sqlx` is removed, and `CommerceCatalogStore`,
  `backend_catalog_router_with_postgres_pool`, and `CatalogState`'s concrete store are
  gone with it.
- Moved repository construction to the composition root.
  `MerchandiseServiceHost::catalog_repository()` builds the store from the process's
  single pool and single id generator and hands it out as `Arc<dyn
  CatalogRepositoryPort>`; the route crate now reads
  `build_backend_catalog_router(host.catalog_repository())` and no longer names a pool,
  a driver, or `sdkwork-database-sqlx`. The PostgreSQL-only invariant is enforced in
  the host constructor, so a non-PostgreSQL pool fails startup instead of panicking
  inside a request path.
- Added `tests/static/catalog-port-implementation-closure.test.mjs` (eight assertions).
  It asserts that no Rust crate provides a port without naming a target, that every
  advertised target is an item the declaring crate really declares, that every trait
  port has at least one real `impl` in `crates/`, that the catalog port is implemented
  by the repository crate and nothing else, that no crate outside a composition root
  depends on a concrete repository crate, that the HTTP adapter holds the port rather
  than a driver type, and that `generated/composition.resolved.json` advertises exactly
  the ports the specs declare. Verified non-vacuous by an eight-mutation battery; the
  gate strips Rust comments before searching because the service crate's own
  documentation quotes the `impl` it looks for, and a raw-text scan stayed green with
  the real binding commented out.
- Regenerated `generated/composition.resolved.json`, which still listed the deleted
  `backend_catalog_router_with_postgres_pool` entry point.
- Added the product media surface, which is the first code path onto
  `commerce_product_media`. Four operations — `media.list`, `media.create`,
  `media.update`, `media.delete` — are served from a flat `/catalog/media` collection
  addressed by an `(ownerType, ownerId)` pair rather than four nested
  `/products/{id}/media`-shaped routes, because the baseline lets one attachment hang off
  `spu`, `sku`, `category`, or `attribute_value` and the pair is what
  `uk_commerce_product_media_slot` is already keyed on. Storage is a reference plus a
  read-model projection (`media_resource_id` + `resource_snapshot`), never a URL:
  `MEDIA_RESOURCE_SPEC` section 5 forbids a presigned URL becoming the system of record,
  so there is no `url` column and no `imageUrl` field. `media_resource_id` is *derived*
  from the snapshot's `id` by `validation::media_resource_identity`, which the repository
  calls inside the write transaction, so the reference and the projection cannot describe
  different files. The polymorphic owner has no foreign key — PostgreSQL cannot express
  "this BIGINT points at one of four tables" — so the owner is verified per kind inside
  the same transaction, and the role/owner-kind pair is re-checked against the row's
  *stored* owner under `FOR UPDATE` on update.
- Added the SKU variant axis as a request input. `attributeValueIds` on the SKU create
  and update bodies carries dictionary value ids; the attribute behind each value is
  derived server-side, the resolved set must cover the product category's active `sales`
  axes exactly once, and the `variant_signature` is recomputed as `attributeNo=valueCode`
  terms ordered by `attributeNo` and joined with `;`. The signature uses business keys
  rather than snowflakes deliberately: an id-based signature changes when a tenant
  re-creates an attribute, which would let the same logical variant exist twice and
  silently defeat `uk_commerce_product_sku_variant`. This closes the open item that said
  two colourways could not be distinguished by signature.
- Added `MediaResource` to the authored contract as a single shared component with all 22
  standard keys, `required: [id, kind, source]`, and `additionalProperties: false`. `id` is
  required on this surface because `commerce_product_media.media_resource_id` is
  `BIGINT NOT NULL`; `bucketId`-style object keys stay refused rather than dropped, per
  `MEDIA_RESOURCE_SPEC` section 3.
- Corrected the HTTP status mapping for typed commerce failures.
  `catalog_system_response` answered every service error as `503 DependencyUnavailable`,
  including the validation and not-found failures the contract declares as `400` and `404`.
  It is now `catalog_error_response` and maps `Validation` to `400`, `NotFound` to `404`,
  `Conflict`/`Locked`/`InvalidState`/`InsufficientBalance` to `409`, and the rest to `503`.
- Added `tests/contract/catalog-table-coverage.test.mjs`. It partitions the seventeen
  baseline tables into those a non-comment source file reaches and those listed with the
  decision they are waiting on, asserts the partition is exact in both directions,
  asserts the list's size, requires each reason to name the work rather than restate the
  gap, and requires every recorded gap to also appear in `TECH_ARCHITECTURE.md` section 9.
  Seven tables are recorded: the five `*_translation` tables (localization is an
  unimplemented feature, not a schema question), `commerce_product_spu_attribute` (a
  missing write path for a feature the schema already models, so product specification
  data cannot be recorded), and `commerce_price_list_item` (a design decision — per-SKU
  list prices may be owed, or the table may be redundant next to
  `commerce_product_sku.sale_price_minor`). Seeds are deliberately not counted as a code
  path, which is what keeps `commerce_price_list_item` visible: a bootstrap seed writes it
  and nothing else does. Verified non-vacuous by an eight-mutation battery, including the
  control that a comment naming a table is not evidence of a code path.
- Extended the two body-closure gates for the media surface rather than exempting it. The
  response gate gained `SHARED_COMPONENT_SCHEMAS` — an explicit, reasoned list of
  component schemas that are models rather than published resources, so the one-to-one
  resource-to-`*Response` mapping stays exact instead of being loosened — and a rule that
  an opaque `serde_json::Value` field must point at a declared, closed schema. The request
  gate gained the reverse rule: `MediaResource`'s properties, `required`, and closedness
  are compared, in both directions, against `MEDIA_RESOURCE_REQUIRED_KEYS` and
  `MEDIA_RESOURCE_OPTIONAL_KEYS` read back out of `validation/mod.rs`. The nested SKU axis
  read model is named `SkuAxisView` rather than `SkuAxisResponse` because the `Response`
  suffix means "the one struct per published resource" to the response gate, and this type
  is published inside `Sku`, not as a resource of its own. Nineteen mutations across the
  three gates and the service tests were injected; every verdict matched, and the results
  are recorded in each gate's header.
- Added `tests/contract/catalog-variant-signature-closure.test.mjs`. The `variant_signature`
  convention is shared by three things that cannot see each other — the repository builder,
  the baseline seed's reference rows, and the `uk_commerce_product_sku_variant` unique index
  that compares them — and only the first of those was checked, so a hand-edited seed row or
  a dictionary entry renamed without updating the SKUs that reference it would have gone
  unnoticed. The gate parses the seed's own `INSERT`s and recomputes each SKU's signature
  from its axis rows, comparing the result with what the row stores; it deliberately
  recomputes rather than re-lists, so the assertion can disagree with the seed. It also
  asserts the partial unique index over `(tenant_id, attribute_no)`, because ordering by
  `attribute_no` is only a total order while that index holds — otherwise the signature's
  determinism would silently depend on the repository's `attribute_id` tie-break. Two
  corrections came out of writing it. The signature parser's first draft treated any `)` as
  the end of a `VALUES` tuple, which the `NOW()` in every timestamp column turned into a
  one-value row; it now tracks parenthesis depth. And `build_variant_signature` was sorting
  its axes but relying on `attribute_no` ordering being handed to it, which the Rust unit
  tests and this gate now both hold it to. Verified non-vacuous by five probes: three that
  redden (a changed `value_code`, a hand-edited stored signature, the two axes exchanging
  their `attribute_no`, which moves only the order) and one that must stay green (the axes
  exchanging their `sort_order`, an ordering key the convention has to ignore), plus the
  narrowed-index probe.
- Made the row `version` the precondition for every write to an existing row, closing the
  gap the column audit left open: `version` was bumped by nine of thirteen writers and
  compared by none, so two concurrent editors both succeeded and the second silently
  overwrote the first. All thirteen UPDATE/DELETE statements now carry `AND version = $n` in
  their `WHERE` and `version = version + 1` in their `SET`; the seven resource schemas and
  their Rust response structs publish `version` as a required, int64-string field; `If-Match`
  is required on the thirteen non-create mutators (`42801` when absent, `41201` when stale);
  and every success that carries a resource — the eight writes and
  `GET /catalog/products/{productId}` — publishes the version as `ETag` as well as in the
  body. The comparison lives **inside** the statement that writes rather than beside it, and
  that choice is what makes the property checkable without a database: `AND version = $n` is
  a fact about statement text, so a static gate can hold all thirteen to it in CI, where the
  workspace runs no PostgreSQL. A `0`-row guarded result is classified by one follow-up read
  against the row's own state, which turns the old single `404` for "no row matched" into the
  right one of `404` (the row is gone) and `412` (it moved). Staleness travels on the port's
  **success** channel, as `GuardedWrite::StaleVersion`, instead of as
  `CommerceServiceError::conflict`: `API_SPEC` 2190 keeps a stale copy (`41201`) apart from a
  domain conflict (`40901`), and these operations answer both, so recovering the distinction
  by matching the error message would have been exactly the classification-by-string the
  shared contract type forbids. `SdkWorkResultCode` already defines both protocol codes with
  the right statuses, but `WebFrameworkErrorKind` has no variant for either and
  `problem_response` derives both the status and the code from that kind alone — so rather
  than widen a shared crate outside this module's ownership, the two responses are built from
  `SdkWorkProblemDetail` directly in `http_envelope.rs`. The bridge is four lines and is
  marked as one. Two statements deliberately advance no version, with reasons recorded at the
  statements: `REPOINT_CATEGORY_PARENT_SQL` and `UPDATE_SKU_VARIANT_SIGNATURE_SQL` write other
  columns of the row their guarded statement rewrites later in the same transaction, so
  advancing there would move the version out from under the guard and make every reparenting
  update answer `412` against the version the caller read one statement earlier. Cascades and
  materialised-path rewrites (`SOFT_DELETE_SPU_SKUS_SQL`, `MOVE_CATEGORY_SUBTREE_SQL`,
  `REFRESH_CATEGORY_LEAF_SQL`) advance the rows they touch but carry no guard — none of those
  rows is a caller's `If-Match` target, and a retired SKU or a moved descendant still has to
  look different to whoever read it. Verified by
  `tests/static/api-precondition-closure.test.mjs`, which reads the router's own `.route(...)`
  calls to tie each operation to its handler, reads each handler's span to learn whether it
  reads the header, compares both against the contract's declared parameter in both
  directions, and then reads the thirteen SQL statements. Its fourteenth mutation row is the
  one that carries the round: it is the only check that would notice a fourteenth guarded
  operation appearing in one layer and not the other two. Fourteen probes, every verdict as
  specified, every file restored byte-identically.

- Made capability-owned SKU metadata a first-class carrier instead of a substitution into
  `spec_json`. `commerce_product_sku.metadata` is `JSONB NOT NULL DEFAULT '{}'`, and
  `CreateProductSkuCommand.metadata` / `UpdateProductSkuCommand.metadata` read and write it
  verbatim, so a capability whose SKU carries fields the catalog has no column for — a notary
  matter's `spec`, for instance — round-trips them without the catalog interpreting them. The
  update side is three-state (`None` preserves, `Some({})` clears, `Some(value)` replaces), so a
  price-only edit cannot erase another capability's fields. `spec_json` was a second name for the
  same fact and it leaked: the old path stripped an `_sdkwork` envelope out of the value on every
  read, which meant the catalog and the capability disagreed about what was stored.
- Made two fields removable, because a two-state `Option` cannot say "clear this" and a
  nullable column has no in-band empty value to say it with. `UpdateSkuRequest.listPriceMinor`
  and `UpdateProductRequest.description` are now `["string", "null"]` on the wire and
  `Option<Option<_>>` in Rust: an absent key preserves, an explicit `null` clears, a value
  replaces. `UPDATE_SKU_SQL` was already close — it resolved the two collapsed states to "keep
  the stored price" — and `UPDATE_SPU_SQL` now binds a boolean presence flag alongside the
  stated text, because when both states arrive as `NULL`, `COALESCE` cannot tell them apart. The
  request bodies decode through one `deserialize_present_option` rather than two hand-rolled
  shims. Until this, a caller could restate a strike-through price or a description but never take
  one away.
- Made `commerce_product_spu.status` writable, which is what the SPU had been missing next to the
  SKU. The reachable set was `draft` (only at insert), `active`, and `archived`, with `inactive`
  reachable only from `delete` — which reaches it by also hiding the row. So no caller could state
  "this product is inactive" without retiring it. `UpdateProductSpuCommand.status` accepts the same
  four values the SKU already accepted, and `UPDATE_SPU_SQL` derives `sales_status` (and
  `published_at`) from it in the same three lines `UPDATE_SKU_SQL` uses, so the two tables cannot
  drift apart in what `active` means. `draft` is deliberately still unreachable after publication:
  `ck_commerce_product_spu_published_at` is `published_at IS NULL OR status <> 'draft'` and nothing
  clears `published_at`, which is the baseline's own note that publishing is a one-way transition.
  The `23514` mapping in `store_error` now names the constraint it violated instead of reporting a
  generic integrity failure, because the baseline states the model in CHECKs and the name of the
  rule is the one thing a caller needs.
- Added `tests/static/catalog-status-derivation-closure.test.mjs`, holding the two product tables
  to one rule about being on sale: `sales_status` is never bound from the caller, the derivation in
  `UPDATE_SPU_SQL` and `UPDATE_SKU_SQL` is the same expression once placeholders are normalized, a
  statement that derives it from `$n` also assigns `status` from that same `$n`, a literal status
  is one the baseline admits, both product inserts start a row `(draft, inactive)`, and no
  statement clears `published_at`. Seven probes, every verdict as specified, every file restored
  byte-identically. Its seventh row is why the first assertion does not skip a statement whose
  status placeholder it cannot find: deleting `status = COALESCE($n::TEXT, status)` leaves the
  derivation intact, and the narrower form of the assertion called that consistent while the write
  ignored every status the caller sent. Corrected the gate counts in
  `docs/architecture/tech/TECH_ARCHITECTURE.md` while adding this one — the request-body gate's
  battery had grown from five probes to seven in the previous change and the tally was not updated
  with it.
## 2026-07-11

- Added the reusable `SingleSkuMerchandiseRepositoryPort` and service facade
  for one-SPU/one-SKU merchandise operations. (Deleted on 2026-09-23.)
- Added SQLite/PostgreSQL SQLx persistence with tenant/org/fulfillment scoping,
  bounded store-level pagination, atomic writes, injected Snowflake primary
  ids, deterministic idempotency business numbers, and SKU `spec_json`/status
  updates. (SQLite and `spec_json` removed on 2026-09-23.)
- Documented the owner boundary and explicitly kept database schema lifecycle
  outside the new capability; no DDL or migration was added. (Superseded on
  2026-09-23 by the 17-table baseline rewrite.)
- Corrected merchandise module contracts and the catalog test fixtures to use
  the current smallest-unit `CommerceMoney` format.
