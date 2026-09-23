# Merchandise PRD

Status: active development  
Owner: SDKWork maintainers  
Application: `sdkwork-merchandise`  
Updated: 2026-09-23  
Specs: `REQUIREMENTS_SPEC.md`, `DOCUMENTATION_SPEC.md`, `DOMAIN_SPEC.md`

## 1. Background And Problem

Commerce applications need one authoritative owner for sellable SPU/SKU master
data, its category and attribute structure, and its pricing layers. Domain
features such as notary services must be able to model a service as merchandise
and reuse the common order and payment systems without creating parallel product
tables or bespoke persistence.

## 2. Target Users

Commerce administrators, vertical-domain operators, backend integrators, and
client applications that publish or select sellable merchandise.

## 3. Goals And Non-Goals

### Goals

- Own SPU/SKU validation, lifecycle status, product-type classification,
  fulfillment classification, and tenant/organization isolation.
- Own the category tree, the attribute dictionary, and category attribute
  templates, with the attribute role (key / sales / parameter) decided per
  category rather than per attribute.
- Own pricing as two layers: the SKU base price, and price-list items covering
  market, customer segment, quantity break, and time window.
- Store money as integer minor units with an explicit currency code and scale,
  and reject over-precision input rather than rounding it silently.
- Let vertical capabilities reuse existing commerce order and payment
  ownership without duplicating merchandise or database definitions.
- Keep HTTP routes, SDK generation, and persistence aligned with SDKWork
  contracts.

### Non-Goals

- Owning order, payment, IAM, Drive, or inventory lifecycle. Inventory
  quantities and reservations belong to `sdkwork-inventory`; carts and buyer
  addresses belong to `sdkwork-catalog`.
- Maintaining two product models. There is one model: SPU plus SKU.
- Free-form JSON as the storage home for classification, specification, or
  filterable attributes.
- Per-marketing-concept price columns such as `member_price` or
  `activity_price`; those are price-list rows.
- Hand-written HTTP clients or direct consumer writes to commerce tables.

## 4. Scope

- SPU/SKU catalog master data and backend administration.
- Category, attribute, category-attribute template, and price-list management.
- Backend catalog route surface: `/backend/v3/api/catalog/*`, 27 routes,
  contributed to `sdkwork-shop-backend-api` and generated in
  `sdkwork-shop-backend-sdk`.
- A PostgreSQL persistence adapter over the merchandise database baseline.
- Money handling through the `sdkwork-commerce-money` kernel.

Localization is split so that base tables carry the default language and
`*_translation` tables carry the rest; two locales can therefore coexist.
Promotional display data (badges, tags) is explicitly deferred rather than being
stored in a JSON side channel.

## 5. User Scenarios

- An operator creates a category tree, registers attributes, and binds them to a
  category as key, sales, or parameter roles.
- An operator creates an SPU of a given product type, then one or more SKUs
  beneath it, each with its own price and inventory-tracking mode.
- An operator searches one tenant and organization one page at a time, filtered
  by category, product type, status, or text.
- A vertical domain reads merchandise through the merchandise owner and then
  creates an order through the order owner.
- An operator manages a price list that overrides the SKU base price for a given
  market, customer segment, quantity break, and time window.

## 6. Success Metrics

- All owner-boundary tests pass on the service and repository packages.
- The declared capability set is exactly the registered route operation set.
- No consumer domain performs direct SPU/SKU writes.
- List paths enforce store-level page bounds and tenant predicates.
- Every money value round-trips between wire, service, and storage without
  precision loss, including zero-decimal and three-decimal currencies.

## 7. Delivery Phase

- Current phase: model v2 implementation and contract alignment.
- Production release: not yet declared, and the application has not been
  deployed.
- Completion gate: route, Shop backend SDK authority, security, database,
  documentation, and topology checks pass in the owning repositories.

## 8. Linked Requirements

- Component contracts: `crates/*/specs/component.spec.json`
- Target model plan: [PLAN-20260923-merchandise-catalog-model-v2.md](../../engineering/plans/PLAN-20260923-merchandise-catalog-model-v2.md)
- Technical architecture: [TECH_ARCHITECTURE.md](../../architecture/tech/TECH_ARCHITECTURE.md)
- Change evidence: [CHANGELOG.md](../../changelogs/CHANGELOG.md)
- Canonical standards: `../../../../sdkwork-specs/`

## 9. Open Questions

Shop owns the backend API authority and SDK family. Merchandise owns the catalog
capability implementation. `DOMAIN_SPEC.md` treats `catalog` and `merchandise`
as sibling capabilities, while this repository's service contract is named
`commerce.catalog` and its routes live under `/backend/v3/api/catalog/*`, so the
naming boundary between the two still needs an owner decision.
