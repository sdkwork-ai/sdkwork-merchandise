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
