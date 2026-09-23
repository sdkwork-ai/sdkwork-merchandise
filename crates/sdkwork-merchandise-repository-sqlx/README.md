# sdkwork-merchandise-repository-sqlx

- Domain: `commerce`
- Capability: `merchandise`
- Package type: Rust SQLx repository crate
- Status: active development

This crate is the SQL persistence adapter for merchandise-owned catalog data.
It serves the catalog repository surface against PostgreSQL and does not own
pool construction, schema lifecycle, or HTTP transport.

## Public API

- `PostgresCommerceCatalogStore::new(PgPool)` is the single exported adapter. It
  serves category, attribute, category-attribute binding, price-list, SPU, and
  SKU operations for the catalog repository port.
- No other module is part of the public surface; consumers integrate through the
  package export only.

## Required SDK Surface

None. The repository receives a database pool through native Rust composition
and does not call HTTP APIs.

## Configuration

The composition root constructs the `sqlx::PgPool` and injects it. This crate
does not read environment variables and does not create production pools
directly. Schema registration, migrations, seeds, and drift checks remain owned
by the database lifecycle layer.

## Deployment Profile And Runtime Target Behavior

Authoritative-server persistence for this capability is PostgreSQL only; there
is no SQLite deployment target. SQL is parameter-bound, listing uses store-level
`LIMIT`/`OFFSET`, and multi-row writes run inside a transaction.

## Security

Every query and write carries tenant scope, and organization scope is applied
where the operation requires it. Cross-domain tables are out of bounds for this
crate: carts and buyer addresses belong to `sdkwork-catalog`, inventory belongs
to `sdkwork-inventory`.

## Known Open Work

The adapter still emits legacy `uuid_v7()` text identifiers and ISO-8601 text
timestamps, and still binds prices as text. The merchandise baseline defines
`BIGINT` identifiers, `TIMESTAMPTZ` timestamps, and `*_minor BIGINT` money with
an explicit currency and scale. Aligning this adapter to the baseline is a
tracked work item and has not landed yet.

## Extension Points

Add new persistence behavior by implementing an existing merchandise service
port. Do not expose table-level CRUD to consumer domains, construct ad hoc
database pools, or add schema definitions to this crate.

## Verification

```bash
cargo test -p sdkwork-merchandise-repository-sqlx
```

## Owner And Status

The machine-readable component contract is
`specs/component.spec.json`. SDKWork global standards remain authoritative.
