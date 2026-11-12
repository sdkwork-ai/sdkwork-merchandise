# Merchandise database module

Managed by the `sdkwork-database` lifecycle SPI. Table prefix: `commerce_`
(commerce domain / merchandise capability). Strategy:
`baseline-plus-migrations`.

## What a fresh deployment gets

One boot of the application (or one `pnpm db:bootstrap`) runs three phases in
order:

1. **Baseline** — `database/ddl/baseline/postgres/0001_merchandise_baseline.sql`
   creates all **19 `commerce_*` tables** (SPU/SKU catalog, category dual tree,
   typed attribute dictionary + value dictionary, category-attribute template,
   brands + translations, price lists + ladder items, media, per-entity
   translation tables, currency registry) with CHECK constraints, foreign keys,
   partial unique indexes, and the sellable-projection indexes.
2. **Migrations** — `database/migrations/postgres/0002_*.up.sql` upgrades
   deployments bootstrapped from the pre-brand baseline. Fresh deployments
   already get everything from the baseline, so the migration is a no-op there.
3. **Seeds** — `database/seeds/` populates the full demo-ready sellable graph
   (see below). All seed statements are upserts on **reserved id blocks**
   (declared in `database/contract/schema.yaml → reservedSeedIdBlocks`), so
   re-running or re-booting never collides with runtime snowflake ids.

## Seed data (demo-ready)

| File | Seeds |
|---|---|
| `seeds/common/001_currency.sql` | 9 currencies (CNY/USD/EUR/…/POINTS) with minor-unit exponents |
| `seeds/common/001_merchandise_bootstrap.sql` | Brands (2200), membership category + tier/period attributes + 8 values + sales bindings, SPU 1500 (brand 2200) + 16 SKUs with CNY prices + 32 axes, media (3 demo images), price list 2100 + 18 items incl. ladder tiers |
| `seeds/common/002_course_products.sql` | Course category (1001), SPU 1501 (brand 2200), course template binding, SKU 1620 + offering axis, course price item |
| `seeds/locales/en-US/001_sku_locale.sql` | en-US translations: categories, SPUs, SKUs, attributes, values, brands |
| `seeds/locales/zh-CN/001_sku_locale.sql` | No-op — zh-CN is the default locale carried in base tables |

Everything is upsert-only: deploying into a database with real tenant data
touches only the reserved id blocks, never tenant rows.

## Boot behavior and environments

`database.manifest.json` declares module defaults; `etc/topology/base.env`
layered env files make the deployment decision; environment variables win:

| Key | Manifest default | Topology (all envs) |
|---|---|---|
| `SDKWORK_DATABASE_AUTO_MIGRATE` | `false` | **`true`** — pending migrations apply on boot |
| `SDKWORK_DATABASE_SEED_ON_BOOT` | `true` | **`true`** — demo data seeds on boot |
| `SDKWORK_DATABASE_SEED_LOCALE` | `zh-CN` | `zh-CN` |
| `SDKWORK_DATABASE_SEED_PROFILE` | `standard` | `standard` |

**Multi-locale demos**: boot seeds the default locale (zh-CN) only. Each
additional active locale applies with one explicit command, e.g.
`pnpm db:seed -- --locale en-US --profile standard` (or the ops seed
endpoint). Missing translations fall back to the base row at read time.

**Shared-database note**: the dev database `sdkwork_ai_dev` hosts many
SDKWork modules, and sibling modules (shop/membership/course) create
overlapping `commerce_product_*` tables. The merchandise init skips its
baseline when the anchor table already exists — on a shared schema where a
sibling created it first, reset the merchandise-owned tables and history
rows (see REVIEW-20261112 section 12) or point the deployment at a
dedicated database. Standalone deployments with their own database are
unaffected.

Production that wants ops-controlled init flips `AUTO_MIGRATE`/`SEED_ON_BOOT`
to `false` and runs `pnpm db:bootstrap` out of band — the seeds are upserts, so
nothing else changes.

## Connecting the CLI

`db:seed` / `db:bootstrap` / `db:init` run `sdkwork-database-cli` against the
process env (`SDKWORK_DATABASE_*`; `bootstrap_merchandise_database_from_env`
also reads a `.env` in the working directory). For the `db:postgres:*` scripts,
copy `config/host/postgres.env.example` to `config/host/postgres.env`
(same `SDKWORK_DATABASE_*` keys; never committed).

## Commands

```bash
pnpm run db:validate          # offline: framework standard (schema/registry/seed security)
pnpm run db:materialize:contract
pnpm run db:plan              # needs Postgres
pnpm run db:bootstrap         # needs Postgres: init + migrate + seed in one step
pnpm run db:init / db:migrate / db:seed / db:status / db:drift:check
pnpm run db:postgres:plan / db:postgres:init   # via config/host/postgres.env
pnpm run db:verify:baseline   # real-Postgres integration verifier (baseline + seeds, twice)
```

## Verification

`pnpm db:verify:baseline` spins up the baseline, applies every seed twice, and
asserts table count (19), per-table row counts, seed idempotency,
`variant_signature` recomputation, price round-trips through the currency
registry, and zh-CN/en-US coexistence — 94 assertions, all green.
