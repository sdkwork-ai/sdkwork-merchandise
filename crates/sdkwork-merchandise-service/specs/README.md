# sdkwork-merchandise-service component specs

This directory is the local contract index for the merchandise service crate.
The machine-readable authority is [component.spec.json](./component.spec.json);
global SDKWork standards remain authoritative.

## Component

| Field | Value |
| --- | --- |
| Name | `sdkwork-merchandise-service` |
| Type | `rust-crate` |
| Root | `crates/sdkwork-merchandise-service` |
| Domain | `commerce` |
| Capability | `merchandise` |
| Layer | `backend-service` |
| Status | `stable` |

## Public Contract

- Service contract: `catalog_service_contract()` — capability tokens are the
  registered backend route operation ids, verbatim, so the declared set and the
  route manifest are checkable as a plain set equality.
- Repository port: `CatalogRepositoryPort`, owned here as a trait and
  implemented by the repository owner; the required-port set is pinned by
  `CatalogPortRequirement::standard_commands`.
- Domain axes: the typed `ProductType`, `ProductStatus`, `FulfillmentType`,
  `InventoryTrackingMode`, and `LifecycleStatus` enums. Each owns one baseline
  CHECK vocabulary and is pinned to it by the
  `storage_vocabulary_is_accepted_by_the_baseline_check_constraints` test.
- Typed commands and queries for the category, attribute, price-list, SPU, and
  SKU surfaces. The command set is the write model the repository port consumes;
  there is no parallel draft family. Identifier fields cross this boundary as
  decimal strings, never as `number`.
- Package export: `.`.
- No HTTP route or generated SDK is owned by this crate.

## Canonical Specs

- [COMPONENT_SPEC.md](../../../../sdkwork-specs/COMPONENT_SPEC.md)
- [MODULE_SPEC.md](../../../../sdkwork-specs/MODULE_SPEC.md)
- [DOMAIN_SPEC.md](../../../../sdkwork-specs/DOMAIN_SPEC.md)
- [API_SPEC.md](../../../../sdkwork-specs/API_SPEC.md)
- [PAGINATION_SPEC.md](../../../../sdkwork-specs/PAGINATION_SPEC.md)
- [CODE_STYLE_SPEC.md](../../../../sdkwork-specs/CODE_STYLE_SPEC.md)
- [NAMING_SPEC.md](../../../../sdkwork-specs/NAMING_SPEC.md)
- [RUST_CODE_SPEC.md](../../../../sdkwork-specs/RUST_CODE_SPEC.md)
- [SECURITY_SPEC.md](../../../../sdkwork-specs/SECURITY_SPEC.md)
- [TEST_SPEC.md](../../../../sdkwork-specs/TEST_SPEC.md)

## Verification

```bash
cargo test -p sdkwork-merchandise-service
```
