# sdkwork-merchandise-service

- Domain: `commerce`
- Capability: `merchandise`
- Package type: Rust service crate
- Status: active development

This crate owns merchandise-domain validation, typed commands and queries, and
the catalog service contract for SPU/SKU master data and its category,
attribute, and price-list surfaces. It does not create database pools or perform
HTTP transport. Persistence is supplied through the repository port by the
composition root.

## Public API

- `catalog_service_contract()` — the declared capability set, held identical to
  the registered backend route operation ids.
- `CatalogRepositoryPort` with `CatalogPortRequirement::standard_commands()`,
  which pins the write set a host must be able to serve.
- Domain axes: `ProductType`, `ProductStatus`, `FulfillmentType`,
  `InventoryTrackingMode`, and `LifecycleStatus`, each of which is the single
  owner of one baseline CHECK vocabulary and round-trips through
  `as_storage_str`/`from_storage_str`.
- Typed commands for categories, attributes, category-attribute bindings, price
  lists, SPU (create/update/delete/publish/archive), and SKU
  (create/update/delete). The commands are the write model: the repository port
  consumes them directly, so there is no second draft type per table.
- Typed list and retrieve queries for categories, attributes, category
  attributes, price lists, SPUs, and SKUs, with `page`/`page_size` bounded to
  1..=200.

The package-level export is the supported integration entrypoint. Consumers must
not depend on internal module paths.

## Required SDK Surface

None. This is a domain/service crate and does not own an HTTP SDK.

## Configuration

No environment access is performed here. Tenants, organizations, page bounds,
and fulfillment type are supplied through typed commands and queries. Database
pool construction belongs to the approved `sdkwork-database` composition layer.

## Deployment Profile And Runtime Target Behavior

The crate runs embedded in a host or behind the merchandise gateways. It is
runtime-neutral: the host supplies the repository implementation through
`CatalogRepositoryPort`.

## Security

All repository operations require tenant scope, and organization scope is
required where the operation writes or where tenant-wide admin listing is not
declared. List queries are bounded to at most 200 records per page, and writes
validate required text, money, status, and enum axes before persistence.

## Identifier Wire Contract

Identifier fields (`tenant_id`, `organization_id`, `category_id`, `attribute_id`,
`spu_id`, `sku_id`, `price_list_id`, and their `*_no` business numbers) cross
this boundary as decimal strings and are never converted to a numeric type.
Money crosses as a validated `CommerceMoney` value carrying its own scale.

## Extension Points

Implement `CatalogRepositoryPort` in an approved owner repository. Keep SPU/SKU
persistence and transaction handling in the repository owner; do not duplicate
table writes in consumer domains.

## Verification

```bash
cargo test -p sdkwork-merchandise-service
```

## Owner And Status

The machine-readable component contract is
`specs/component.spec.json`. SDKWork global standards remain authoritative.
