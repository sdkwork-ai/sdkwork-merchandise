//! PostgreSQL catalog store for `sdkwork-merchandise`.
//!
//! # Contract with the baseline
//!
//! Every statement here is written against
//! `database/ddl/baseline/postgres/0001_merchandise_baseline.sql` and nothing else. Four rules
//! follow from that baseline and are applied uniformly:
//!
//! 1. **Identity is a BIGINT snowflake assigned before the INSERT.** `SUBJECT_ID_SPEC` requires the
//!    primary key to exist before the row does, because a category's `path` contains its own id.
//!    The generator is injected (`Arc<dyn IdGenerator>`), never read from a process global.
//! 2. **Money is exact integer minor units plus a scale.** The wire already carries minor units
//!    (`API_SPEC` section 13.2.1), so no statement here converts a major amount: the scale is
//!    *read* from `commerce_currency.minor_unit_exponent` and snapshotted onto the row as
//!    `price_scale`, which is what keeps a historical amount readable if a currency's exponent is
//!    ever redefined. No layer multiplies or divides by a literal such as `/ 100`.
//! 3. **Deletion is `deleted_at`/`deleted_by`, never a status.** No baseline CHECK admits a
//!    `'deleted'` status, so a soft-delete column pair is the only way to retire a row; every read
//!    therefore filters `deleted_at IS NULL`.
//! 4. **Timestamps come from the database.** `created_at`/`updated_at` are `TIMESTAMPTZ` filled by
//!    `NOW()`, so there is one clock and no hand-rolled calendar arithmetic.
//!
//! # SQL is static by construction
//!
//! Every statement is a `&'static str` assembled from `concat!` and literal-only column macros.
//! Nothing here builds SQL with `format!`, so `sqlx`'s `SqlSafeStr` guard is satisfied without an
//! `AssertSqlSafe` escape hatch and there is no dynamic-SQL surface to audit. The only whitelist that
//! varies a statement is the SPU sort key, which selects between three compiled constants.
//!
//! # Known open items
//!
//! * `created_by`/`updated_by`/`deleted_by` are left NULL: no write command carries an actor yet,
//!   although `IamAppContext.user_id` is available at the router. Threading it is the next
//!   increment; see `docs/architecture/tech/TECH_ARCHITECTURE.md` section 9.
//! * The media snapshot is written from the caller's `MediaResource`. Resolving it from Drive at
//!   write time would need a Drive read port, which this composition does not wire yet; the
//!   snapshot is therefore a projection the caller supplies, exactly as `MEDIA_RESOURCE_SPEC`
//!   section 5 permits, and the reference stays authoritative.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, SecondsFormat, Utc};
use sdkwork_commerce_money::MoneyUnit;
use sdkwork_contract_service::CommerceServiceError;
use sdkwork_database_id::IdGenerator;
use sdkwork_merchandise_service::{
    ArchiveSpuCommand, AttributeListQuery, AttributeRecord, AttributeRole,
    CategoryAttributeListQuery, CategoryAttributeRecord, CategoryListQuery, CategoryRecord,
    CategoryRetrieveQuery, CreateAttributeCommand, CreateCategoryAttributeCommand,
    CreateCategoryCommand, CreateMediaCommand, CreatePriceListCommand, CreateProductSkuCommand,
    CreateProductSpuCommand, DeleteCategoryAttributeCommand, DeleteCategoryCommand,
    DeleteMediaCommand, DeleteProductSkuCommand, DeleteProductSpuCommand, GuardedWrite,
    LifecycleStatus, MediaListQuery, MediaOwnerType, MediaRecord, PriceListListQuery,
    PriceListRecord, ProductSkuListQuery, ProductSkuRetrieveQuery, ProductSpuListQuery,
    ProductSpuRetrieveQuery, PublishSpuCommand, SkuAxisRecord, SkuRecord, SpuRecord, StaleVersion,
    UpdateCategoryAttributeCommand, UpdateCategoryCommand, UpdateMediaCommand,
    UpdatePriceListCommand, UpdateProductSkuCommand, UpdateProductSpuCommand,
};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Row, Transaction};

use sdkwork_merchandise_service::validation::SKU_VARIANT_SIGNATURE_MAX_CHARS;

// ------------------------------------------------------------------ statement fragments
//
// Macros rather than `const` items so `concat!` can splice them into literals. A `const` identifier
// would force `format!`, which `sqlx` rejects precisely because it is the shape SQL injection takes.

macro_rules! category_columns {
    () => {
        "id, tenant_id, organization_id, category_no, parent_id, path, depth, is_leaf, name, \
         sort_order, status, version, created_at, updated_at"
    };
}

macro_rules! attribute_columns {
    () => {
        "id, tenant_id, organization_id, attribute_no, name, value_type, status, sort_order, \
         version, created_at, updated_at"
    };
}

macro_rules! category_attribute_columns {
    () => {
        "id, tenant_id, organization_id, category_id, attribute_id, attribute_role, \
         source_category_id, is_required, is_searchable, is_filterable, is_comparable, \
         sort_order, status, version, created_at, updated_at"
    };
}

macro_rules! price_list_columns {
    () => {
        "id, tenant_id, organization_id, price_list_no, name, currency_code, market_code, status, \
         starts_at, ends_at, version, created_at, updated_at"
    };
}

macro_rules! spu_columns {
    () => {
        "id, tenant_id, organization_id, spu_no, category_id, name, title, subtitle, description, \
         product_type, status, sales_status, published_at, version, created_at, updated_at"
    };
}

macro_rules! sku_columns {
    () => {
        "id, tenant_id, organization_id, spu_id, sku_no, variant_signature, name, title, \
         currency_code, price_scale, sale_price_minor, list_price_minor, fulfillment_type, \
         inventory_tracking, status, sales_status, metadata, published_at, version, created_at, \
         updated_at"
    };
}

macro_rules! media_columns {
    () => {
        "id, tenant_id, organization_id, owner_type, owner_id, media_role, media_resource_id, \
         resource_snapshot, alt_text, sort_order, status, version, created_at, updated_at"
    };
}

const LIST_CATEGORIES_SQL: &str = concat!(
    "SELECT ",
    category_columns!(),
    " FROM commerce_product_category
     WHERE tenant_id = $1
       AND deleted_at IS NULL
       AND ($2::BIGINT IS NULL OR organization_id = $2)
       AND ($3::BIGINT IS NULL OR parent_id = $3)
       AND ($4::TEXT IS NULL OR status = $4)
     ORDER BY sort_order ASC, id ASC
     LIMIT $5 OFFSET $6"
);

const COUNT_CATEGORIES_SQL: &str = "SELECT COUNT(*) FROM commerce_product_category
     WHERE tenant_id = $1
       AND deleted_at IS NULL
       AND ($2::BIGINT IS NULL OR organization_id = $2)
       AND ($3::BIGINT IS NULL OR parent_id = $3)
       AND ($4::TEXT IS NULL OR status = $4)";

const RETRIEVE_CATEGORY_SQL: &str = concat!(
    "SELECT ",
    category_columns!(),
    " FROM commerce_product_category
     WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL"
);

const INSERT_CATEGORY_SQL: &str = concat!(
    "INSERT INTO commerce_product_category
         (id, tenant_id, organization_id, category_no, parent_id, path, depth, name, is_leaf,
          sort_order, status)
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, TRUE, $9, 'active')
     RETURNING ",
    category_columns!()
);

const LOCK_CATEGORY_SQL: &str = "SELECT parent_id, path, depth FROM commerce_product_category
     WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL
     FOR UPDATE";

// Advances the affected parent's `version` but carries no guard: this statement's target is never
// the row a caller holds an `If-Match` for. It still has to advance the version, because adding or
// removing a child changes what a copy of that parent describes.
const REFRESH_CATEGORY_LEAF_SQL: &str = "UPDATE commerce_product_category
     SET is_leaf = NOT EXISTS (
             SELECT 1 FROM commerce_product_category child
             WHERE child.tenant_id = $1 AND child.parent_id = $2 AND child.deleted_at IS NULL
         ),
         version = version + 1,
         updated_at = NOW()
     WHERE tenant_id = $1 AND id = $2";

// Same shape as the leaf refresh: it rewrites a whole subtree, so every row whose `path` moved has
// to look different to a caller that read it before the move. No guard, because none of these rows
// is the write's preconditioned target.
const MOVE_CATEGORY_SUBTREE_SQL: &str = "UPDATE commerce_product_category
     SET path = $1 || substring(path FROM $2),
         depth = depth + $3,
         version = version + 1,
         updated_at = NOW()
     WHERE tenant_id = $4
       AND deleted_at IS NULL
       AND left(path, $2 - 1) = $5";

// Deliberately does **not** advance `version`, and this is the one place where that needs saying.
// `parent_id` and the rest of the row are one rewrite of one row split across two statements for
// readability, and `UPDATE_CATEGORY_SQL` — which runs later in the same transaction and carries the
// precondition — is the statement that owns the row's advance. Advancing here as well would move the
// version out from under that guard and make every reparenting update answer `412` against the
// version the caller read one line earlier.
const REPOINT_CATEGORY_PARENT_SQL: &str = "UPDATE commerce_product_category
     SET parent_id = $1, updated_at = NOW()
     WHERE tenant_id = $2 AND id = $3";

const UPDATE_CATEGORY_SQL: &str = concat!(
    "UPDATE commerce_product_category
     SET name = COALESCE($1::TEXT, name),
         sort_order = COALESCE($2, sort_order),
         status = COALESCE($3::TEXT, status),
         version = version + 1,
         updated_at = NOW()
     WHERE tenant_id = $4 AND id = $5 AND deleted_at IS NULL AND version = $6
     RETURNING ",
    category_columns!()
);

const COUNT_LIVE_CHILDREN_SQL: &str = "SELECT COUNT(*) FROM commerce_product_category
     WHERE tenant_id = $1 AND parent_id = $2 AND deleted_at IS NULL";

const COUNT_LIVE_CATEGORY_PRODUCTS_SQL: &str = "SELECT COUNT(*) FROM commerce_product_spu
     WHERE tenant_id = $1 AND category_id = $2 AND deleted_at IS NULL";

const SOFT_DELETE_CATEGORY_SQL: &str = "UPDATE commerce_product_category
     SET deleted_at = NOW(), status = 'inactive', version = version + 1, updated_at = NOW()
     WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL AND version = $3";

const LIST_ATTRIBUTES_SQL: &str = concat!(
    "SELECT ",
    attribute_columns!(),
    " FROM commerce_product_attribute
     WHERE tenant_id = $1
       AND deleted_at IS NULL
       AND ($2::BIGINT IS NULL OR organization_id = $2)
       AND ($3::TEXT IS NULL OR status = $3)
     ORDER BY sort_order ASC, id ASC
     LIMIT $4 OFFSET $5"
);

const COUNT_ATTRIBUTES_SQL: &str = "SELECT COUNT(*) FROM commerce_product_attribute
     WHERE tenant_id = $1
       AND deleted_at IS NULL
       AND ($2::BIGINT IS NULL OR organization_id = $2)
       AND ($3::TEXT IS NULL OR status = $3)";

const INSERT_ATTRIBUTE_SQL: &str = concat!(
    "INSERT INTO commerce_product_attribute
         (id, tenant_id, organization_id, attribute_no, name, value_type, input_hint,
          is_multi_value, sort_order, status)
     VALUES ($1, $2, $3, $4, $5, 'enum', 'select', FALSE, 0, 'active')
     RETURNING ",
    attribute_columns!()
);

const INSERT_ATTRIBUTE_VALUE_SQL: &str = "INSERT INTO commerce_product_attribute_value
         (id, tenant_id, organization_id, attribute_id, value_code, display_value, sort_order,
          status)
     VALUES ($1, $2, $3, $4, $5, $5, $6, 'active')
     ON CONFLICT (tenant_id, attribute_id, value_code) WHERE deleted_at IS NULL
     DO NOTHING";

const LIST_PRICE_LISTS_SQL: &str = concat!(
    "SELECT ",
    price_list_columns!(),
    " FROM commerce_price_list
     WHERE tenant_id = $1
       AND deleted_at IS NULL
       AND ($2::BIGINT IS NULL OR organization_id = $2)
       AND ($3::TEXT IS NULL OR currency_code = $3)
       AND ($4::TEXT IS NULL OR market_code = $4)
       AND ($5::TEXT IS NULL OR status = $5)
     ORDER BY priority DESC, id ASC
     LIMIT $6 OFFSET $7"
);

const COUNT_PRICE_LISTS_SQL: &str = "SELECT COUNT(*) FROM commerce_price_list
     WHERE tenant_id = $1
       AND deleted_at IS NULL
       AND ($2::BIGINT IS NULL OR organization_id = $2)
       AND ($3::TEXT IS NULL OR currency_code = $3)
       AND ($4::TEXT IS NULL OR market_code = $4)
       AND ($5::TEXT IS NULL OR status = $5)";

const INSERT_PRICE_LIST_SQL: &str = concat!(
    "INSERT INTO commerce_price_list
         (id, tenant_id, organization_id, price_list_no, name, currency_code, market_code, status)
     VALUES ($1, $2, $3, $4, $4, $5, $6, 'active')
     RETURNING ",
    price_list_columns!()
);

const UPDATE_PRICE_LIST_SQL: &str = concat!(
    "UPDATE commerce_price_list
     SET status = COALESCE($1::TEXT, status),
         starts_at = COALESCE($2::TIMESTAMPTZ, starts_at),
         ends_at = COALESCE($3::TIMESTAMPTZ, ends_at),
         version = version + 1,
         updated_at = NOW()
     WHERE tenant_id = $4 AND id = $5 AND deleted_at IS NULL AND version = $6
     RETURNING ",
    price_list_columns!()
);

// ------------------------------------------------------- category attribute bindings
//
// A binding is the category template entry: which attribute a category uses, in which role, and
// whether it is required/searchable/filterable/comparable there. `source_category_id` records the
// category the binding was inherited from, and the baseline CHECK forbids pointing it at the
// category itself.

const LIST_CATEGORY_ATTRIBUTES_SQL: &str = concat!(
    "SELECT ",
    category_attribute_columns!(),
    " FROM commerce_product_category_attribute
     WHERE tenant_id = $1
       AND deleted_at IS NULL
       AND ($2::BIGINT IS NULL OR organization_id = $2)
       AND ($3::BIGINT IS NULL OR category_id = $3)
       AND ($4::BIGINT IS NULL OR attribute_id = $4)
       AND ($5::TEXT IS NULL OR status = $5)
     ORDER BY category_id ASC, sort_order ASC, id ASC
     LIMIT $6 OFFSET $7"
);

const COUNT_CATEGORY_ATTRIBUTES_SQL: &str =
    "SELECT COUNT(*) FROM commerce_product_category_attribute
     WHERE tenant_id = $1
       AND deleted_at IS NULL
       AND ($2::BIGINT IS NULL OR organization_id = $2)
       AND ($3::BIGINT IS NULL OR category_id = $3)
       AND ($4::BIGINT IS NULL OR attribute_id = $4)
       AND ($5::TEXT IS NULL OR status = $5)";

const INSERT_CATEGORY_ATTRIBUTE_SQL: &str = concat!(
    "INSERT INTO commerce_product_category_attribute
        (id, tenant_id, organization_id, category_id, attribute_id, attribute_role,
         source_category_id, is_required, is_searchable, is_filterable, is_comparable, sort_order)
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
     RETURNING ",
    category_attribute_columns!()
);

const UPDATE_CATEGORY_ATTRIBUTE_SQL: &str = concat!(
    "UPDATE commerce_product_category_attribute
     SET attribute_role = COALESCE($1, attribute_role),
         is_required = COALESCE($2, is_required),
         is_searchable = COALESCE($3, is_searchable),
         is_filterable = COALESCE($4, is_filterable),
         is_comparable = COALESCE($5, is_comparable),
         sort_order = COALESCE($6, sort_order),
         status = COALESCE($7, status),
         version = version + 1,
         updated_at = NOW()
     WHERE tenant_id = $8 AND id = $9 AND deleted_at IS NULL AND version = $10
     RETURNING ",
    category_attribute_columns!()
);

const SOFT_DELETE_CATEGORY_ATTRIBUTE_SQL: &str = "UPDATE commerce_product_category_attribute
     SET deleted_at = NOW(), version = version + 1, updated_at = NOW()
     WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL AND version = $3";

macro_rules! spu_list_select {
    () => {
        "SELECT commerce_product_spu.id, commerce_product_spu.tenant_id, \
         commerce_product_spu.organization_id, commerce_product_spu.spu_no, \
         commerce_product_spu.category_id, commerce_product_spu.name, \
         commerce_product_spu.title, commerce_product_spu.subtitle, \
         commerce_product_spu.description, commerce_product_spu.product_type, \
         commerce_product_spu.status, commerce_product_spu.sales_status, \
         commerce_product_spu.published_at, commerce_product_spu.created_at, \
         commerce_product_spu.updated_at"
    };
}

/// Free-text product search predicate, parameterised on its placeholder.
///
/// The pattern is concatenated inside PostgreSQL from a bound parameter, so the needle never becomes
/// SQL text. The three columns are the operator-visible handles on a product — its code and its two
/// names — and a `NULL` needle disables the predicate rather than matching nothing. The expansion
/// carries its own indentation and trailing newline so the four statements that use it stay
/// readable in the same shape as the predicates around them.
macro_rules! spu_search_filter {
    ($needle:literal) => {
        concat!(
            "       AND (",
            $needle,
            "::TEXT IS NULL OR commerce_product_spu.spu_no ILIKE '%' || ",
            $needle,
            " || '%' OR commerce_product_spu.title ILIKE '%' || ",
            $needle,
            " || '%' OR commerce_product_spu.name ILIKE '%' || ",
            $needle,
            " || '%')\n"
        )
    };
}

const LIST_SPUS_DEFAULT_SQL: &str = concat!(
    spu_list_select!(),
    "
     FROM commerce_product_spu
     WHERE commerce_product_spu.tenant_id = $1
       AND commerce_product_spu.deleted_at IS NULL
       AND ($2::BIGINT IS NULL OR commerce_product_spu.organization_id = $2)
",
    spu_search_filter!("$3"),
    "       AND ($4::BIGINT IS NULL OR commerce_product_spu.category_id = $4)
       AND ($5::TEXT IS NULL OR commerce_product_spu.product_type = $5)
       AND ($6::TEXT IS NULL OR commerce_product_spu.status = $6)
     ORDER BY commerce_product_spu.created_at DESC, commerce_product_spu.id DESC
     LIMIT $7 OFFSET $8"
);

const LIST_SPUS_PRICE_ASC_SQL: &str = concat!(
    spu_list_select!(),
    "
     FROM commerce_product_spu
     LEFT JOIN commerce_product_sku sku
       ON sku.spu_id = commerce_product_spu.id
      AND sku.tenant_id = commerce_product_spu.tenant_id
      AND sku.deleted_at IS NULL
      AND sku.sales_status = 'active'
     WHERE commerce_product_spu.tenant_id = $1
       AND commerce_product_spu.deleted_at IS NULL
       AND ($2::BIGINT IS NULL OR commerce_product_spu.organization_id = $2)
",
    spu_search_filter!("$3"),
    "       AND ($4::BIGINT IS NULL OR commerce_product_spu.category_id = $4)
       AND ($5::TEXT IS NULL OR commerce_product_spu.product_type = $5)
       AND ($6::TEXT IS NULL OR commerce_product_spu.status = $6)
     GROUP BY commerce_product_spu.id
     ORDER BY MIN(sku.sale_price_minor) ASC NULLS LAST, commerce_product_spu.id ASC
     LIMIT $7 OFFSET $8"
);

const LIST_SPUS_PRICE_DESC_SQL: &str = concat!(
    spu_list_select!(),
    "
     FROM commerce_product_spu
     LEFT JOIN commerce_product_sku sku
       ON sku.spu_id = commerce_product_spu.id
      AND sku.tenant_id = commerce_product_spu.tenant_id
      AND sku.deleted_at IS NULL
      AND sku.sales_status = 'active'
     WHERE commerce_product_spu.tenant_id = $1
       AND commerce_product_spu.deleted_at IS NULL
       AND ($2::BIGINT IS NULL OR commerce_product_spu.organization_id = $2)
",
    spu_search_filter!("$3"),
    "       AND ($4::BIGINT IS NULL OR commerce_product_spu.category_id = $4)
       AND ($5::TEXT IS NULL OR commerce_product_spu.product_type = $5)
       AND ($6::TEXT IS NULL OR commerce_product_spu.status = $6)
     GROUP BY commerce_product_spu.id
     ORDER BY MIN(sku.sale_price_minor) DESC NULLS LAST, commerce_product_spu.id ASC
     LIMIT $7 OFFSET $8"
);

const COUNT_SPUS_SQL: &str = concat!(
    "SELECT COUNT(*) FROM commerce_product_spu
     WHERE tenant_id = $1
       AND deleted_at IS NULL
       AND ($2::BIGINT IS NULL OR organization_id = $2)
",
    spu_search_filter!("$3"),
    "       AND ($4::BIGINT IS NULL OR category_id = $4)
       AND ($5::TEXT IS NULL OR product_type = $5)
       AND ($6::TEXT IS NULL OR status = $6)"
);

const RETRIEVE_SPU_SQL: &str = concat!(
    "SELECT ",
    spu_columns!(),
    " FROM commerce_product_spu
     WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL"
);

const INSERT_SPU_SQL: &str = concat!(
    "INSERT INTO commerce_product_spu
         (id, tenant_id, organization_id, spu_no, category_id, name, title, subtitle, description,
          product_type, status, sales_status)
     VALUES ($1, $2, $3, $4, $5, $6, $6, $7, $8, $9, 'draft', 'inactive')
     RETURNING ",
    spu_columns!()
);

/// Three-state `description`, two-state everything else.
///
/// `$3` says whether the caller mentioned the description at all and `$4` is what it said, because
/// `commerce_product_spu.description` is nullable: `COALESCE($4::TEXT, description)` would read a
/// cleared description and an unmentioned one as the same `NULL` and keep the stored text in both
/// cases, which is exactly the edit the field gained its third state to allow. `title` and `subtitle`
/// stay `COALESCE`d: `name` is `NOT NULL` and derived from `title`, so neither can be cleared.
const UPDATE_SPU_SQL: &str = concat!(
    "UPDATE commerce_product_spu
     SET title = COALESCE($1::TEXT, title),
         name = COALESCE($1::TEXT, name),
         subtitle = COALESCE($2::TEXT, subtitle),
         description = CASE WHEN $3::BOOLEAN THEN $4::TEXT ELSE description END,
         category_id = COALESCE($5, category_id),
         version = version + 1,
         updated_at = NOW()
     WHERE tenant_id = $6 AND id = $7 AND deleted_at IS NULL AND version = $8
     RETURNING ",
    spu_columns!()
);

const SOFT_DELETE_SPU_SQL: &str = "UPDATE commerce_product_spu
     SET deleted_at = NOW(), status = 'inactive', sales_status = 'inactive',
         version = version + 1, updated_at = NOW()
     WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL AND version = $3";

// The cascade that retires a product's SKUs. No guard — the caller's precondition names the
// product, not these SKUs — but the advance is required all the same: a caller holding a copy of one
// of these SKUs has to be told its version moved, or it will keep editing a row that is already
// retired.
const SOFT_DELETE_SPU_SKUS_SQL: &str = "UPDATE commerce_product_sku
     SET deleted_at = NOW(), status = 'inactive', sales_status = 'inactive',
         version = version + 1, updated_at = NOW()
     WHERE tenant_id = $1 AND spu_id = $2 AND deleted_at IS NULL";

const COUNT_LIVE_SPU_SKUS_SQL: &str = "SELECT COUNT(*) FROM commerce_product_sku
     WHERE tenant_id = $1 AND spu_id = $2 AND deleted_at IS NULL";

const PUBLISH_SPU_SQL: &str = concat!(
    "UPDATE commerce_product_spu
     SET status = 'active',
         sales_status = 'active',
         published_at = COALESCE(published_at, NOW()),
         version = version + 1,
         updated_at = NOW()
     WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL AND version = $3
     RETURNING ",
    spu_columns!()
);

const ARCHIVE_SPU_SQL: &str = concat!(
    "UPDATE commerce_product_spu
     SET status = 'archived', sales_status = 'inactive',
         version = version + 1, updated_at = NOW()
     WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL AND version = $3
     RETURNING ",
    spu_columns!()
);

const LIST_SKUS_SQL: &str = concat!(
    "SELECT ",
    sku_columns!(),
    " FROM commerce_product_sku
     WHERE tenant_id = $1
       AND deleted_at IS NULL
       AND ($2::BIGINT IS NULL OR organization_id = $2)
       AND ($3::BIGINT IS NULL OR spu_id = $3)
       AND ($4::TEXT IS NULL OR status = $4)
       AND ($5::BIGINT IS NULL OR EXISTS (
             SELECT 1 FROM commerce_product_sku_attribute sa
              WHERE sa.tenant_id = commerce_product_sku.tenant_id
                AND sa.sku_id = commerce_product_sku.id
                AND sa.attribute_value_id = $5
                AND sa.deleted_at IS NULL))
     ORDER BY id ASC
     LIMIT $6 OFFSET $7"
);

const COUNT_SKUS_SQL: &str = "SELECT COUNT(*) FROM commerce_product_sku
     WHERE tenant_id = $1
       AND deleted_at IS NULL
       AND ($2::BIGINT IS NULL OR organization_id = $2)
       AND ($3::BIGINT IS NULL OR spu_id = $3)
       AND ($4::TEXT IS NULL OR status = $4)
       AND ($5::BIGINT IS NULL OR EXISTS (
             SELECT 1 FROM commerce_product_sku_attribute sa
              WHERE sa.tenant_id = commerce_product_sku.tenant_id
                AND sa.sku_id = commerce_product_sku.id
                AND sa.attribute_value_id = $5
                AND sa.deleted_at IS NULL))";

const RETRIEVE_SKU_SQL: &str = concat!(
    "SELECT ",
    sku_columns!(),
    " FROM commerce_product_sku
     WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL"
);

const INSERT_SKU_SQL: &str = concat!(
    "INSERT INTO commerce_product_sku
         (id, tenant_id, organization_id, spu_id, sku_no, variant_signature, name, title,
          currency_code, price_scale, list_price_minor, sale_price_minor, fulfillment_type,
          inventory_tracking, metadata, status, sales_status)
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, 'draft', 'inactive')
     RETURNING ",
    sku_columns!()
);

/// Locks the SKU row and reads everything an update has to reason about.
///
/// `spu_id`, `sku_no`, and `organization_id` are read here rather than looked up later because the
/// variant signature and the replacement axis rows both need them, and they are already in the row
/// this statement locks: a second query would read the same tuple again, one lock later.
const LOCK_SKU_SQL: &str = "SELECT currency_code, price_scale, sale_price_minor, list_price_minor,
            spu_id, sku_no, organization_id
     FROM commerce_product_sku
     WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL
     FOR UPDATE";

const UPDATE_SKU_SQL: &str = concat!(
    "UPDATE commerce_product_sku
     SET name = COALESCE($1::TEXT, name),
         title = COALESCE($2::TEXT, title),
         sale_price_minor = $3,
         list_price_minor = $4,
         currency_code = $5,
         price_scale = $6,
         fulfillment_type = COALESCE($7::TEXT, fulfillment_type),
         inventory_tracking = COALESCE($8::TEXT, inventory_tracking),
         metadata = COALESCE($13::JSONB, metadata),
         status = COALESCE($9::TEXT, status),
         sales_status = CASE WHEN COALESCE($9::TEXT, status) = 'active' THEN 'active'
                             ELSE 'inactive' END,
         published_at = CASE WHEN COALESCE($9::TEXT, status) = 'active'
                             THEN COALESCE(published_at, NOW())
                             ELSE published_at END,
         version = version + 1,
         updated_at = NOW()
     WHERE tenant_id = $10 AND id = $11 AND deleted_at IS NULL AND version = $12
     RETURNING ",
    sku_columns!()
);

const SOFT_DELETE_SKU_SQL: &str = "UPDATE commerce_product_sku
     SET deleted_at = NOW(), status = 'inactive', sales_status = 'inactive',
         version = version + 1, updated_at = NOW()
     WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL AND version = $3";

const SELECT_CURRENCY_SQL: &str = "SELECT minor_unit_exponent, rounding_mode FROM commerce_currency
     WHERE code = $1 AND status = 'active'";

const CURRENCY_EXISTS_SQL: &str = "SELECT EXISTS (SELECT 1 FROM commerce_currency WHERE code = $1)";

// ------------------------------------------------------------------ variant axis
//
// The SKU's sales axes live in `commerce_product_sku_attribute` as (attribute_id,
// attribute_value_id) pairs, but the caller submits **value ids only**. Resolving the attribute from
// the value inside the same transaction is what makes "the attribute and the value disagree"
// unrepresentable rather than merely unlikely: there is one submitted id per axis, and both stored
// ids come from that one row.
//
// The signature is written in *business* keys, not in snowflakes: `attribute_no=value_code` per
// axis, ordered by `attribute_no`, joined with `;`. A signature built from ids would change if a
// tenant ever re-created an attribute, and `uk_commerce_product_sku_variant` would then let the same
// logical variant exist twice. This is also the convention the baseline seed's reference rows
// already use, which `tests/contract/catalog-variant-signature-closure.test.mjs` recomputes from the
// seed itself.

const RESOLVE_AXIS_VALUES_SQL: &str =
    "SELECT av.id, av.attribute_id, av.value_code, av.display_value,
            av.sort_order, a.attribute_no
     FROM commerce_product_attribute_value av
     JOIN commerce_product_attribute a ON a.id = av.attribute_id
     WHERE av.tenant_id = $1
       AND av.id = ANY($2)
       AND av.deleted_at IS NULL
       AND a.deleted_at IS NULL
       AND av.status = 'active'
       AND a.status = 'active'";

/// Reads the SPU that owns a SKU. Its `category_id` decides which sales axes are legitimate.
const SPU_CATEGORY_SQL: &str = "SELECT category_id FROM commerce_product_spu
     WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL";

const CATEGORY_ACTIVE_SALES_AXES_SQL: &str =
    "SELECT attribute_id FROM commerce_product_category_attribute
     WHERE tenant_id = $1
       AND category_id = $2
       AND attribute_role = 'sales'
       AND status = 'active'
       AND deleted_at IS NULL";

const INSERT_SKU_AXIS_SQL: &str = "INSERT INTO commerce_product_sku_attribute
         (id, tenant_id, organization_id, sku_id, attribute_id, attribute_value_id, sort_order)
     VALUES ($1, $2, $3, $4, $5, $6, $7)";

/// Retires the current axis set before a replacement is written.
///
/// Soft-delete rather than `DELETE`, because `uk_commerce_product_sku_attribute_axis` is a *partial*
/// unique index on `deleted_at IS NULL`: the retired row must survive to keep the index honest
/// about what the SKU used to be.
const SOFT_DELETE_SKU_AXES_SQL: &str = "UPDATE commerce_product_sku_attribute
     SET deleted_at = NOW(), updated_at = NOW()
     WHERE tenant_id = $1 AND sku_id = $2 AND deleted_at IS NULL";

// No advance, for the same reason `REPOINT_CATEGORY_PARENT_SQL` has none: it writes one column of
// the row that `UPDATE_SKU_SQL` is about to rewrite under the caller's precondition, in the same
// transaction, and that second statement owns the row's advance.
const UPDATE_SKU_VARIANT_SIGNATURE_SQL: &str = "UPDATE commerce_product_sku
     SET variant_signature = $1, updated_at = NOW()
     WHERE tenant_id = $2 AND id = $3";

/// Whether another live SKU of the same product already carries this signature.
///
/// `exclude_id` is `NULL` on create, where no row can match the id being allocated, and the row's
/// own id on update, where the SKU being edited obviously still holds its previous signature.
const VARIANT_TAKEN_SQL: &str = "SELECT EXISTS (SELECT 1 FROM commerce_product_sku
     WHERE tenant_id = $1
       AND spu_id = $2
       AND variant_signature = $3
       AND deleted_at IS NULL
       AND ($4::BIGINT IS NULL OR id <> $4))";

/// Loads the axes of a set of SKUs in one statement.
///
/// A per-SKU lookup would be a query per row of a list page; the `ANY ($2)` form makes reading the
/// axes cost the same for one SKU and for a page of them.
const LIST_SKU_AXES_SQL: &str =
    "SELECT sa.sku_id, sa.attribute_id, sa.attribute_value_id, sa.sort_order,
            a.attribute_no, av.value_code, av.display_value
     FROM commerce_product_sku_attribute sa
     JOIN commerce_product_attribute a ON a.id = sa.attribute_id
     JOIN commerce_product_attribute_value av ON av.id = sa.attribute_value_id
     WHERE sa.tenant_id = $1
       AND sa.sku_id = ANY($2)
       AND sa.deleted_at IS NULL
     ORDER BY sa.sku_id ASC, a.attribute_no ASC, sa.attribute_id ASC";

// ------------------------------------------------------------------ product media
//
// `commerce_product_media` stores a stable reference (`media_resource_id`, owned by Drive) plus a
// read-model projection (`resource_snapshot`). There is no `url` column and no object key:
// `MEDIA_RESOURCE_SPEC` section 5 forbids a business table from making a presigned URL its system of
// record, and section 6 forbids a standalone `imageUrl` field on a product payload.
//
// `owner_id` has no foreign key because PostgreSQL cannot express "this BIGINT points at one of four
// tables". The four statements below are the constraint the DDL cannot declare: an attachment whose
// owner does not exist is refused inside the same transaction that would insert it.

const LIST_MEDIA_SQL: &str = concat!(
    "SELECT ",
    media_columns!(),
    " FROM commerce_product_media
     WHERE tenant_id = $1
       AND deleted_at IS NULL
       AND ($2::BIGINT IS NULL OR organization_id = $2)
       AND ($3::TEXT IS NULL OR owner_type = $3)
       AND ($4::BIGINT IS NULL OR owner_id = $4)
       AND ($5::TEXT IS NULL OR media_role = $5)
       AND ($6::TEXT IS NULL OR status = $6)
     ORDER BY owner_type ASC, owner_id ASC, media_role ASC, sort_order ASC, id ASC
     LIMIT $7 OFFSET $8"
);

const COUNT_MEDIA_SQL: &str = "SELECT COUNT(*) FROM commerce_product_media
     WHERE tenant_id = $1
       AND deleted_at IS NULL
       AND ($2::BIGINT IS NULL OR organization_id = $2)
       AND ($3::TEXT IS NULL OR owner_type = $3)
       AND ($4::BIGINT IS NULL OR owner_id = $4)
       AND ($5::TEXT IS NULL OR media_role = $5)
       AND ($6::TEXT IS NULL OR status = $6)";

const OWNER_SPU_EXISTS_SQL: &str =
    "SELECT EXISTS (SELECT 1 FROM commerce_product_spu WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL)";

const OWNER_SKU_EXISTS_SQL: &str =
    "SELECT EXISTS (SELECT 1 FROM commerce_product_sku WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL)";

const OWNER_CATEGORY_EXISTS_SQL: &str =
    "SELECT EXISTS (SELECT 1 FROM commerce_product_category WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL)";

const OWNER_ATTRIBUTE_VALUE_EXISTS_SQL: &str =
    "SELECT EXISTS (SELECT 1 FROM commerce_product_attribute_value WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL)";

/// Reads the stored owner kind of one attachment.
///
/// The role/owner-kind pair is constrained by `ck_commerce_product_media_owner_role`, and on an
/// update the caller sends a role without an owner kind. Re-reading the stored kind inside the
/// transaction that locks the row is what lets the rule be re-checked against the row actually being
/// written instead of against one the caller guessed at.
const LOCK_MEDIA_SQL: &str = "SELECT owner_type FROM commerce_product_media
     WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL
     FOR UPDATE";

const INSERT_MEDIA_SQL: &str = concat!(
    "INSERT INTO commerce_product_media
         (id, tenant_id, organization_id, owner_type, owner_id, media_role, media_resource_id,
          resource_snapshot, alt_text, sort_order, status)
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'active')
     RETURNING ",
    media_columns!()
);

const UPDATE_MEDIA_SQL: &str = concat!(
    "UPDATE commerce_product_media
     SET media_role = COALESCE($1::TEXT, media_role),
         media_resource_id = COALESCE($2::BIGINT, media_resource_id),
         resource_snapshot = COALESCE($3::JSONB, resource_snapshot),
         alt_text = COALESCE($4::TEXT, alt_text),
         sort_order = COALESCE($5::BIGINT, sort_order),
         status = COALESCE($6::TEXT, status),
         version = version + 1,
         updated_at = NOW()
     WHERE tenant_id = $7 AND id = $8 AND deleted_at IS NULL AND version = $9
     RETURNING ",
    media_columns!()
);

const SOFT_DELETE_MEDIA_SQL: &str = "UPDATE commerce_product_media
     SET deleted_at = NOW(), status = 'inactive', version = version + 1, updated_at = NOW()
     WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL AND version = $3";

// ------------------------------------------------------- guarded-miss probes
//
// A guarded statement that matched no row has two causes and they are not interchangeable: the row
// is gone, which is a `404`, or the row is live and has moved past the version the caller read,
// which is a `412`. The statement cannot tell them apart — "no row matched" is "no row matched"
// — so the repository asks a second question, by primary key, and answers from the row's own state.
//
// The follow-up read is not part of a snapshot with the failed write, and it does not need to be.
// `version` only ever increases, so a row found here cannot be one whose version equals the
// caller's; and a row hidden here by a concurrent retirement is genuinely gone, which is what the
// `404` says. Neither answer can be manufactured by a race.

const PROBE_CATEGORY_VERSION_SQL: &str = "SELECT version FROM commerce_product_category
     WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL";

const PROBE_PRICE_LIST_VERSION_SQL: &str = "SELECT version FROM commerce_price_list
     WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL";

const PROBE_CATEGORY_ATTRIBUTE_VERSION_SQL: &str =
    "SELECT version FROM commerce_product_category_attribute
     WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL";

const PROBE_SPU_VERSION_SQL: &str = "SELECT version FROM commerce_product_spu
     WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL";

const PROBE_SKU_VERSION_SQL: &str = "SELECT version FROM commerce_product_sku
     WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL";

const PROBE_MEDIA_VERSION_SQL: &str = "SELECT version FROM commerce_product_media
     WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL";

/// Selects the compiled statement that orders SPUs by the requested sort key.
///
/// Only whitelisted keys select a nontrivial statement; every other value falls back to
/// `created_at DESC`. Each candidate is a static literal, so no user input reaches SQL text. The two
/// price orderings aggregate the live `sale_price_minor` of each SPU's SKUs, which is only comparable
/// inside one currency — a mixed-currency catalog needs a currency filter before that order means
/// anything (see `TECH_ARCHITECTURE.md` section 9).
fn spu_list_sql(sort: Option<&str>) -> &'static str {
    match sort {
        Some("price-asc") => LIST_SPUS_PRICE_ASC_SQL,
        Some("price-desc") => LIST_SPUS_PRICE_DESC_SQL,
        _ => LIST_SPUS_DEFAULT_SQL,
    }
}

/// Mints one BIGINT primary key from an injected generator.
///
/// A free function as well as a method, because the axis writer and the media writer need an id
/// without owning the store: they run inside a transaction the store opened, and passing the
/// generator is narrower than passing `&self`.
fn next_snowflake(ids: &Arc<dyn IdGenerator>) -> Result<i64, CommerceServiceError> {
    let raw = ids
        .next_id()
        .map_err(|error| CommerceServiceError::storage(format!("id generation failed: {error}")))?;
    parse_id("generated id", &raw)
}

#[derive(Clone)]
pub struct PostgresCommerceCatalogStore {
    pool: PgPool,
    ids: Arc<dyn IdGenerator>,
}

impl PostgresCommerceCatalogStore {
    /// Builds a store over the authoritative PostgreSQL pool and an injected id generator.
    ///
    /// The generator is a constructor argument rather than a process global so the composition root
    /// owns the node identity, and so a test can supply a deterministic sequence.
    pub fn new(pool: PgPool, ids: Arc<dyn IdGenerator>) -> Self {
        Self { pool, ids }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Mints the next BIGINT primary key.
    fn next_id(&self) -> Result<i64, CommerceServiceError> {
        next_snowflake(&self.ids)
    }

    // ---------------------------------------------------------------- categories

    pub async fn list_categories(
        &self,
        query: &CategoryListQuery,
    ) -> Result<Vec<CategoryRecord>, CommerceServiceError> {
        let limit = query.page_size.unwrap_or(20).min(200);
        let offset = (query.page.unwrap_or(1) - 1).max(0) * limit;

        let rows = sqlx::query(LIST_CATEGORIES_SQL)
            .bind(parse_id("tenant_id", &query.tenant_id)?)
            .bind(parse_optional_id(
                "organization_id",
                query.organization_id.as_deref(),
            )?)
            .bind(parse_optional_id("parent_id", query.parent_id.as_deref())?)
            .bind(query.status.as_deref())
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("failed to list categories", error))?;

        Ok(rows.iter().map(map_category_row).collect())
    }

    pub async fn count_categories(
        &self,
        query: &CategoryListQuery,
    ) -> Result<i64, CommerceServiceError> {
        sqlx::query_scalar(COUNT_CATEGORIES_SQL)
            .bind(parse_id("tenant_id", &query.tenant_id)?)
            .bind(parse_optional_id(
                "organization_id",
                query.organization_id.as_deref(),
            )?)
            .bind(parse_optional_id("parent_id", query.parent_id.as_deref())?)
            .bind(query.status.as_deref())
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("failed to count categories", error))
    }

    pub async fn retrieve_category(
        &self,
        query: &CategoryRetrieveQuery,
    ) -> Result<Option<CategoryRecord>, CommerceServiceError> {
        let row = sqlx::query(RETRIEVE_CATEGORY_SQL)
            .bind(parse_id("tenant_id", &query.tenant_id)?)
            .bind(parse_id("category_id", &query.category_id)?)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("failed to retrieve category", error))?;

        Ok(row.as_ref().map(map_category_row))
    }

    /// Creates a category and maintains the tree invariants of its parent.
    ///
    /// `path` is self-inclusive (`/1000/` for a root, `/1000/1010/` for its child) and `depth` counts
    /// ancestors, matching the baseline seed. Building the path needs the row's own id, which is why
    /// the id is minted before the INSERT.
    pub async fn create_category(
        &self,
        command: &CreateCategoryCommand,
    ) -> Result<CategoryRecord, CommerceServiceError> {
        let tenant_id = parse_id("tenant_id", &command.tenant_id)?;
        let organization_id = parse_id("organization_id", &command.organization_id)?;
        let parent_id = parse_optional_id("parent_id", command.parent_id.as_deref())?;
        let id = self.next_id()?;

        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("failed to open category transaction", error))?;

        let (path, depth) = match parent_id {
            None => (format!("/{id}/"), 0_i64),
            Some(parent) => {
                let parent_row = lock_category(&mut transaction, tenant_id, parent).await?;
                let depth = parent_row.depth + 1;
                (format!("{}{id}/", parent_row.path), depth)
            }
        };

        let row = sqlx::query(INSERT_CATEGORY_SQL)
            .bind(id)
            .bind(tenant_id)
            .bind(organization_id)
            .bind(&command.category_no)
            .bind(parent_id)
            .bind(&path)
            .bind(depth)
            .bind(&command.name)
            .bind(command.sort_order)
            .fetch_one(&mut *transaction)
            .await
            .map_err(|error| store_error("failed to create category", error))?;

        if let Some(parent) = parent_id {
            refresh_leaf_state(&mut transaction, tenant_id, parent).await?;
        }

        transaction
            .commit()
            .await
            .map_err(|error| store_error("failed to commit category creation", error))?;

        Ok(map_category_row(&row))
    }

    /// Updates a category, moving its whole subtree when the parent changes.
    ///
    /// A move rewrites `path` and `depth` for every descendant in one set-based statement rather than
    /// walking the tree: the subtree is exactly `left(path, len(old_path)) = old_path`, so a move is
    /// two statements regardless of tree size.
    pub async fn update_category(
        &self,
        command: &UpdateCategoryCommand,
    ) -> Result<GuardedWrite<CategoryRecord>, CommerceServiceError> {
        let tenant_id = parse_id("tenant_id", &command.tenant_id)?;
        let id = parse_id("category_id", &command.category_id)?;
        let requested_parent = parse_optional_id("parent_id", command.parent_id.as_deref())?;

        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("failed to open category transaction", error))?;

        let current = lock_category(&mut transaction, tenant_id, id).await?;
        let previous_parent = current.parent_id;

        if let Some(new_parent) = requested_parent {
            if new_parent != previous_parent.unwrap_or(i64::MIN) {
                if new_parent == id {
                    return Err(CommerceServiceError::validation(
                        "a category cannot be its own parent",
                    ));
                }
                let parent_row = lock_category(&mut transaction, tenant_id, new_parent).await?;
                // A category may not move under one of its own descendants: that would detach the
                // subtree from every root and make `path` a cycle.
                if parent_row.path.starts_with(&current.path) {
                    return Err(CommerceServiceError::validation(
                        "a category cannot be moved under one of its own descendants",
                    ));
                }

                let new_prefix = format!("{}{id}/", parent_row.path);
                let depth_delta = parent_row.depth + 1 - current.depth;
                let old_path = current.path.clone();
                let cut = i32::try_from(old_path.chars().count())
                    .unwrap_or(i32::MAX)
                    .saturating_add(1);

                sqlx::query(MOVE_CATEGORY_SUBTREE_SQL)
                    .bind(&new_prefix)
                    .bind(cut)
                    .bind(depth_delta)
                    .bind(tenant_id)
                    .bind(&old_path)
                    .execute(&mut *transaction)
                    .await
                    .map_err(|error| store_error("failed to move category subtree", error))?;

                sqlx::query(REPOINT_CATEGORY_PARENT_SQL)
                    .bind(new_parent)
                    .bind(tenant_id)
                    .bind(id)
                    .execute(&mut *transaction)
                    .await
                    .map_err(|error| store_error("failed to repoint category parent", error))?;

                if let Some(previous) = previous_parent {
                    refresh_leaf_state(&mut transaction, tenant_id, previous).await?;
                }
                refresh_leaf_state(&mut transaction, tenant_id, new_parent).await?;
            }
        }

        let row = sqlx::query(UPDATE_CATEGORY_SQL)
            .bind(command.name.as_deref())
            .bind(command.sort_order)
            .bind(command.status.map(LifecycleStatus::as_storage_str))
            .bind(tenant_id)
            .bind(id)
            .bind(command.expected_version)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|error| store_error("failed to update category", error))?;

        let Some(row) = row else {
            // The subtree move and the leaf refresh above already ran in this transaction, so
            // returning here drops it and rolls them back. A stale precondition must not leave a
            // half-moved subtree behind.
            return classify_guarded_miss(
                &mut *transaction,
                PROBE_CATEGORY_VERSION_SQL,
                tenant_id,
                id,
                command.expected_version,
                "category",
            )
            .await
            .map(GuardedWrite::StaleVersion);
        };

        transaction
            .commit()
            .await
            .map_err(|error| store_error("failed to commit category update", error))?;

        Ok(GuardedWrite::Applied(map_category_row(&row)))
    }

    /// Retires a category.
    ///
    /// Refuses while live children or live products still point at it: soft-deleting a node with
    /// children would leave the tree with an unreachable middle, and the `path` index would keep
    /// matching descendants whose ancestor is invisible.
    pub async fn delete_category(
        &self,
        command: &DeleteCategoryCommand,
    ) -> Result<GuardedWrite<()>, CommerceServiceError> {
        let tenant_id = parse_id("tenant_id", &command.tenant_id)?;
        let id = parse_id("category_id", &command.category_id)?;

        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("failed to open category transaction", error))?;

        let current = lock_category(&mut transaction, tenant_id, id).await?;

        let live_children: i64 = sqlx::query_scalar(COUNT_LIVE_CHILDREN_SQL)
            .bind(tenant_id)
            .bind(id)
            .fetch_one(&mut *transaction)
            .await
            .map_err(|error| store_error("failed to count child categories", error))?;
        if live_children > 0 {
            return Err(CommerceServiceError::conflict(
                "category still has child categories",
            ));
        }

        let live_products: i64 = sqlx::query_scalar(COUNT_LIVE_CATEGORY_PRODUCTS_SQL)
            .bind(tenant_id)
            .bind(id)
            .fetch_one(&mut *transaction)
            .await
            .map_err(|error| store_error("failed to count category products", error))?;
        if live_products > 0 {
            return Err(CommerceServiceError::conflict(
                "category still has products assigned to it",
            ));
        }

        let affected = sqlx::query(SOFT_DELETE_CATEGORY_SQL)
            .bind(tenant_id)
            .bind(id)
            .bind(command.expected_version)
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error("failed to delete category", error))?
            .rows_affected();

        if affected == 0 {
            return classify_guarded_miss(
                &mut *transaction,
                PROBE_CATEGORY_VERSION_SQL,
                tenant_id,
                id,
                command.expected_version,
                "category",
            )
            .await
            .map(GuardedWrite::StaleVersion);
        }

        if let Some(parent) = current.parent_id {
            refresh_leaf_state(&mut transaction, tenant_id, parent).await?;
        }

        transaction
            .commit()
            .await
            .map_err(|error| store_error("failed to commit category deletion", error))?;

        Ok(GuardedWrite::Applied(()))
    }

    // ---------------------------------------------------------------- attributes

    pub async fn list_attributes(
        &self,
        query: &AttributeListQuery,
    ) -> Result<Vec<AttributeRecord>, CommerceServiceError> {
        let limit = query.page_size.unwrap_or(20).min(200);
        let offset = (query.page.unwrap_or(1) - 1).max(0) * limit;

        let rows = sqlx::query(LIST_ATTRIBUTES_SQL)
            .bind(parse_id("tenant_id", &query.tenant_id)?)
            .bind(parse_optional_id(
                "organization_id",
                query.organization_id.as_deref(),
            )?)
            .bind(query.status.as_deref())
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("failed to list attributes", error))?;

        Ok(rows.iter().map(map_attribute_row).collect())
    }

    pub async fn count_attributes(
        &self,
        query: &AttributeListQuery,
    ) -> Result<i64, CommerceServiceError> {
        sqlx::query_scalar(COUNT_ATTRIBUTES_SQL)
            .bind(parse_id("tenant_id", &query.tenant_id)?)
            .bind(parse_optional_id(
                "organization_id",
                query.organization_id.as_deref(),
            )?)
            .bind(query.status.as_deref())
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("failed to count attributes", error))
    }

    /// Creates an attribute dictionary entry plus its enumerated values.
    ///
    /// `value_type` is fixed at `enum` and `input_hint` at `select` because the create command does
    /// not carry them yet; both are valid baseline values, and the pairing satisfies
    /// `ck_commerce_product_attribute_multi_value_needs_enum`. Exposing the remaining value types is
    /// tracked in `TECH_ARCHITECTURE.md` section 9.
    pub async fn create_attribute(
        &self,
        command: &CreateAttributeCommand,
    ) -> Result<AttributeRecord, CommerceServiceError> {
        let tenant_id = parse_id("tenant_id", &command.tenant_id)?;
        let organization_id = parse_id("organization_id", &command.organization_id)?;
        let id = self.next_id()?;

        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("failed to open attribute transaction", error))?;

        let row = sqlx::query(INSERT_ATTRIBUTE_SQL)
            .bind(id)
            .bind(tenant_id)
            .bind(organization_id)
            .bind(&command.attribute_no)
            .bind(&command.name)
            .fetch_one(&mut *transaction)
            .await
            .map_err(|error| store_error("failed to create attribute", error))?;

        for (index, value) in command.values.iter().enumerate() {
            let value_id = self.next_id()?;
            sqlx::query(INSERT_ATTRIBUTE_VALUE_SQL)
                .bind(value_id)
                .bind(tenant_id)
                .bind(organization_id)
                .bind(id)
                .bind(value)
                .bind(i64::try_from(index).unwrap_or(i64::MAX))
                .execute(&mut *transaction)
                .await
                .map_err(|error| store_error("failed to create attribute value", error))?;
        }

        transaction
            .commit()
            .await
            .map_err(|error| store_error("failed to commit attribute creation", error))?;

        Ok(map_attribute_row(&row))
    }

    // ---------------------------------------------------------------- price lists

    pub async fn list_price_lists(
        &self,
        query: &PriceListListQuery,
    ) -> Result<Vec<PriceListRecord>, CommerceServiceError> {
        let limit = query.page_size.unwrap_or(20).min(200);
        let offset = (query.page.unwrap_or(1) - 1).max(0) * limit;

        let rows = sqlx::query(LIST_PRICE_LISTS_SQL)
            .bind(parse_id("tenant_id", &query.tenant_id)?)
            .bind(parse_optional_id(
                "organization_id",
                query.organization_id.as_deref(),
            )?)
            .bind(query.currency_code.as_deref())
            .bind(query.market_code.as_deref())
            .bind(query.status.as_deref())
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("failed to list price lists", error))?;

        Ok(rows.iter().map(map_price_list_row).collect())
    }

    pub async fn count_price_lists(
        &self,
        query: &PriceListListQuery,
    ) -> Result<i64, CommerceServiceError> {
        sqlx::query_scalar(COUNT_PRICE_LISTS_SQL)
            .bind(parse_id("tenant_id", &query.tenant_id)?)
            .bind(parse_optional_id(
                "organization_id",
                query.organization_id.as_deref(),
            )?)
            .bind(query.currency_code.as_deref())
            .bind(query.market_code.as_deref())
            .bind(query.status.as_deref())
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("failed to count price lists", error))
    }

    /// Creates a price list.
    ///
    /// `name` is required by the baseline but absent from the create command, so it is derived from
    /// the business key. Making it an explicit input is tracked in `TECH_ARCHITECTURE.md` section 9.
    pub async fn create_price_list(
        &self,
        command: &CreatePriceListCommand,
    ) -> Result<PriceListRecord, CommerceServiceError> {
        let tenant_id = parse_id("tenant_id", &command.tenant_id)?;
        let organization_id = parse_id("organization_id", &command.organization_id)?;
        let id = self.next_id()?;
        ensure_currency_exists(&self.pool, &command.currency_code).await?;

        let row = sqlx::query(INSERT_PRICE_LIST_SQL)
            .bind(id)
            .bind(tenant_id)
            .bind(organization_id)
            .bind(&command.price_list_no)
            .bind(&command.currency_code)
            .bind(command.market_code.as_deref())
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("failed to create price list", error))?;

        Ok(map_price_list_row(&row))
    }

    pub async fn update_price_list(
        &self,
        command: &UpdatePriceListCommand,
    ) -> Result<GuardedWrite<PriceListRecord>, CommerceServiceError> {
        let tenant_id = parse_id("tenant_id", &command.tenant_id)?;
        let id = parse_id("price_list_id", &command.price_list_id)?;

        if let (Some(starts_at), Some(ends_at)) = (&command.starts_at, &command.ends_at) {
            let starts = parse_timestamp("starts_at", starts_at)?;
            let ends = parse_timestamp("ends_at", ends_at)?;
            if ends <= starts {
                return Err(CommerceServiceError::validation(
                    "ends_at must be later than starts_at",
                ));
            }
        }

        let row = sqlx::query(UPDATE_PRICE_LIST_SQL)
            .bind(command.status.map(LifecycleStatus::as_storage_str))
            .bind(command.starts_at.as_deref())
            .bind(command.ends_at.as_deref())
            .bind(tenant_id)
            .bind(id)
            .bind(command.expected_version)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("failed to update price list", error))?;

        let Some(row) = row else {
            return classify_guarded_miss(
                &self.pool,
                PROBE_PRICE_LIST_VERSION_SQL,
                tenant_id,
                id,
                command.expected_version,
                "price list",
            )
            .await
            .map(GuardedWrite::StaleVersion);
        };
        Ok(GuardedWrite::Applied(map_price_list_row(&row)))
    }

    // ------------------------------------------------ category attribute bindings

    pub async fn list_category_attributes(
        &self,
        query: &CategoryAttributeListQuery,
    ) -> Result<Vec<CategoryAttributeRecord>, CommerceServiceError> {
        let limit = query.page_size.unwrap_or(20).min(200);
        let offset = (query.page.unwrap_or(1) - 1).max(0) * limit;

        let rows = sqlx::query(LIST_CATEGORY_ATTRIBUTES_SQL)
            .bind(parse_id("tenant_id", &query.tenant_id)?)
            .bind(parse_optional_id(
                "organization_id",
                query.organization_id.as_deref(),
            )?)
            .bind(parse_optional_id(
                "category_id",
                query.category_id.as_deref(),
            )?)
            .bind(parse_optional_id(
                "attribute_id",
                query.attribute_id.as_deref(),
            )?)
            .bind(query.status.as_deref())
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("failed to list category attributes", error))?;

        Ok(rows.iter().map(map_category_attribute_row).collect())
    }

    pub async fn count_category_attributes(
        &self,
        query: &CategoryAttributeListQuery,
    ) -> Result<i64, CommerceServiceError> {
        sqlx::query_scalar(COUNT_CATEGORY_ATTRIBUTES_SQL)
            .bind(parse_id("tenant_id", &query.tenant_id)?)
            .bind(parse_optional_id(
                "organization_id",
                query.organization_id.as_deref(),
            )?)
            .bind(parse_optional_id(
                "category_id",
                query.category_id.as_deref(),
            )?)
            .bind(parse_optional_id(
                "attribute_id",
                query.attribute_id.as_deref(),
            )?)
            .bind(query.status.as_deref())
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("failed to count category attributes", error))
    }

    /// Binds one attribute to one category as a template entry.
    ///
    /// `uk_commerce_product_category_attribute_binding` is a partial unique index over
    /// `(tenant_id, category_id, attribute_id) WHERE deleted_at IS NULL`. Re-binding an attribute
    /// that was unbound earlier is therefore legal and yields a fresh row, while a duplicate live
    /// binding surfaces as `23505` and becomes a `409`.
    ///
    /// Both foreign keys are left to PostgreSQL rather than pre-checked: a missing category or
    /// attribute surfaces as `23503`, which maps to a `422` naming the referenced record. A
    /// pre-check would only add a race with no additional guarantee.
    pub async fn create_category_attribute(
        &self,
        command: &CreateCategoryAttributeCommand,
    ) -> Result<CategoryAttributeRecord, CommerceServiceError> {
        let id = self.next_id()?;

        let row = sqlx::query(INSERT_CATEGORY_ATTRIBUTE_SQL)
            .bind(id)
            .bind(parse_id("tenant_id", &command.tenant_id)?)
            .bind(parse_id("organization_id", &command.organization_id)?)
            .bind(parse_id("category_id", &command.category_id)?)
            .bind(parse_id("attribute_id", &command.attribute_id)?)
            .bind(command.role.as_storage_str())
            .bind(parse_optional_id(
                "source_category_id",
                command.source_category_id.as_deref(),
            )?)
            .bind(command.required)
            .bind(command.searchable)
            .bind(command.filterable)
            .bind(command.comparable)
            .bind(command.sort_order)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("failed to create category attribute", error))?;

        Ok(map_category_attribute_row(&row))
    }

    pub async fn update_category_attribute(
        &self,
        command: &UpdateCategoryAttributeCommand,
    ) -> Result<GuardedWrite<CategoryAttributeRecord>, CommerceServiceError> {
        let tenant_id = parse_id("tenant_id", &command.tenant_id)?;
        let id = parse_id("binding_id", &command.binding_id)?;

        let row = sqlx::query(UPDATE_CATEGORY_ATTRIBUTE_SQL)
            .bind(command.role.map(AttributeRole::as_storage_str))
            .bind(command.required)
            .bind(command.searchable)
            .bind(command.filterable)
            .bind(command.comparable)
            .bind(command.sort_order)
            .bind(command.status.map(LifecycleStatus::as_storage_str))
            .bind(tenant_id)
            .bind(id)
            .bind(command.expected_version)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("failed to update category attribute", error))?;

        let Some(row) = row else {
            return classify_guarded_miss(
                &self.pool,
                PROBE_CATEGORY_ATTRIBUTE_VERSION_SQL,
                tenant_id,
                id,
                command.expected_version,
                "category attribute",
            )
            .await
            .map(GuardedWrite::StaleVersion);
        };
        Ok(GuardedWrite::Applied(map_category_attribute_row(&row)))
    }

    pub async fn delete_category_attribute(
        &self,
        command: &DeleteCategoryAttributeCommand,
    ) -> Result<GuardedWrite<()>, CommerceServiceError> {
        let tenant_id = parse_id("tenant_id", &command.tenant_id)?;
        let id = parse_id("binding_id", &command.binding_id)?;

        let affected = sqlx::query(SOFT_DELETE_CATEGORY_ATTRIBUTE_SQL)
            .bind(tenant_id)
            .bind(id)
            .bind(command.expected_version)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("failed to delete category attribute", error))?
            .rows_affected();

        if affected == 0 {
            return classify_guarded_miss(
                &self.pool,
                PROBE_CATEGORY_ATTRIBUTE_VERSION_SQL,
                tenant_id,
                id,
                command.expected_version,
                "category attribute",
            )
            .await
            .map(GuardedWrite::StaleVersion);
        }

        Ok(GuardedWrite::Applied(()))
    }

    // ---------------------------------------------------------------- SPUs

    pub async fn list_spus(
        &self,
        query: &ProductSpuListQuery,
    ) -> Result<Vec<SpuRecord>, CommerceServiceError> {
        let limit = query.page_size.unwrap_or(20).min(200);
        let offset = (query.page.unwrap_or(1) - 1).max(0) * limit;

        let rows = sqlx::query(spu_list_sql(query.sort.as_deref()))
            .bind(parse_id("tenant_id", &query.tenant_id)?)
            .bind(parse_optional_id(
                "organization_id",
                query.organization_id.as_deref(),
            )?)
            .bind(query.q.as_deref())
            .bind(parse_optional_id(
                "category_id",
                query.category_id.as_deref(),
            )?)
            .bind(query.product_type.as_deref())
            .bind(query.status.as_deref())
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("failed to list spus", error))?;

        Ok(rows.iter().map(map_spu_row).collect())
    }

    pub async fn count_spus(
        &self,
        query: &ProductSpuListQuery,
    ) -> Result<i64, CommerceServiceError> {
        sqlx::query_scalar(COUNT_SPUS_SQL)
            .bind(parse_id("tenant_id", &query.tenant_id)?)
            .bind(parse_optional_id(
                "organization_id",
                query.organization_id.as_deref(),
            )?)
            .bind(query.q.as_deref())
            .bind(parse_optional_id(
                "category_id",
                query.category_id.as_deref(),
            )?)
            .bind(query.product_type.as_deref())
            .bind(query.status.as_deref())
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("failed to count spus", error))
    }

    pub async fn retrieve_spu(
        &self,
        query: &ProductSpuRetrieveQuery,
    ) -> Result<Option<SpuRecord>, CommerceServiceError> {
        let row = sqlx::query(RETRIEVE_SPU_SQL)
            .bind(parse_id("tenant_id", &query.tenant_id)?)
            .bind(parse_id("spu_id", &query.spu_id)?)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("failed to retrieve spu", error))?;

        Ok(row.as_ref().map(map_spu_row))
    }

    /// Creates a SPU in `draft`.
    ///
    /// `name` is the default-locale display name and is derived from `title`, matching how the
    /// baseline seed pairs them. `sales_status` starts `inactive`, which is what makes a freshly
    /// created product unbuyable until it is published.
    pub async fn create_spu(
        &self,
        command: &CreateProductSpuCommand,
    ) -> Result<SpuRecord, CommerceServiceError> {
        let tenant_id = parse_id("tenant_id", &command.tenant_id)?;
        let organization_id = parse_id("organization_id", &command.organization_id)?;
        let category_id = parse_id("category_id", &command.category_id)?;
        let id = self.next_id()?;

        let row = sqlx::query(INSERT_SPU_SQL)
            .bind(id)
            .bind(tenant_id)
            .bind(organization_id)
            .bind(&command.spu_no)
            .bind(category_id)
            .bind(&command.title)
            .bind(command.subtitle.as_deref())
            .bind(command.description.as_deref())
            .bind(command.product_type.as_storage_str())
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("failed to create spu", error))?;

        Ok(map_spu_row(&row))
    }

    pub async fn update_spu(
        &self,
        command: &UpdateProductSpuCommand,
    ) -> Result<GuardedWrite<SpuRecord>, CommerceServiceError> {
        let tenant_id = parse_id("tenant_id", &command.tenant_id)?;
        let id = parse_id("spu_id", &command.spu_id)?;

        // `description` binds twice on purpose: the boolean says whether the caller mentioned it, and
        // the text is what they said. A `None` text behind a `true` flag is the clearing write, so it
        // must reach PostgreSQL as `NULL` rather than as `sqlx`'s "no value bound here" — which is
        // why the value is taken from the inner `Option` and not from `as_deref()` on the outer one.
        let description = command
            .description
            .as_ref()
            .and_then(|value| value.as_deref());

        let row = sqlx::query(UPDATE_SPU_SQL)
            .bind(command.title.as_deref())
            .bind(command.subtitle.as_deref())
            .bind(command.description.is_some())
            .bind(description)
            .bind(parse_optional_id(
                "category_id",
                command.category_id.as_deref(),
            )?)
            .bind(tenant_id)
            .bind(id)
            .bind(command.expected_version)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("failed to update spu", error))?;

        let Some(row) = row else {
            return classify_guarded_miss(
                &self.pool,
                PROBE_SPU_VERSION_SQL,
                tenant_id,
                id,
                command.expected_version,
                "product",
            )
            .await
            .map(GuardedWrite::StaleVersion);
        };
        Ok(GuardedWrite::Applied(map_spu_row(&row)))
    }

    /// Retires a SPU and every SKU under it.
    ///
    /// The SKUs are retired in the same transaction: leaving them live would keep rows in the
    /// sellable index whose parent product is invisible.
    pub async fn delete_spu(
        &self,
        command: &DeleteProductSpuCommand,
    ) -> Result<GuardedWrite<()>, CommerceServiceError> {
        let tenant_id = parse_id("tenant_id", &command.tenant_id)?;
        let id = parse_id("spu_id", &command.spu_id)?;

        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("failed to open spu transaction", error))?;

        let affected = sqlx::query(SOFT_DELETE_SPU_SQL)
            .bind(tenant_id)
            .bind(id)
            .bind(command.expected_version)
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error("failed to delete spu", error))?
            .rows_affected();

        if affected == 0 {
            return classify_guarded_miss(
                &mut *transaction,
                PROBE_SPU_VERSION_SQL,
                tenant_id,
                id,
                command.expected_version,
                "product",
            )
            .await
            .map(GuardedWrite::StaleVersion);
        }

        sqlx::query(SOFT_DELETE_SPU_SKUS_SQL)
            .bind(tenant_id)
            .bind(id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error("failed to delete spu skus", error))?;

        transaction
            .commit()
            .await
            .map_err(|error| store_error("failed to commit spu deletion", error))?;

        Ok(GuardedWrite::Applied(()))
    }

    /// Publishes a SPU: `active` and sellable in one transition.
    ///
    /// Refuses when the product has no live SKU. The baseline would accept the write, but a published
    /// product with nothing to buy is a broken storefront rather than a valid state, and
    /// `ck_commerce_product_spu_sales_requires_published` already treats publish as the moment a
    /// product becomes sellable.
    pub async fn publish_spu(
        &self,
        command: &PublishSpuCommand,
    ) -> Result<GuardedWrite<SpuRecord>, CommerceServiceError> {
        let tenant_id = parse_id("tenant_id", &command.tenant_id)?;
        let id = parse_id("spu_id", &command.spu_id)?;

        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("failed to open spu transaction", error))?;

        let live_skus: i64 = sqlx::query_scalar(COUNT_LIVE_SPU_SKUS_SQL)
            .bind(tenant_id)
            .bind(id)
            .fetch_one(&mut *transaction)
            .await
            .map_err(|error| store_error("failed to count spu skus", error))?;
        if live_skus == 0 {
            return Err(CommerceServiceError::conflict(
                "a product needs at least one sku before it can be published",
            ));
        }

        let row = sqlx::query(PUBLISH_SPU_SQL)
            .bind(tenant_id)
            .bind(id)
            .bind(command.expected_version)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|error| store_error("failed to publish spu", error))?;

        let Some(row) = row else {
            return classify_guarded_miss(
                &mut *transaction,
                PROBE_SPU_VERSION_SQL,
                tenant_id,
                id,
                command.expected_version,
                "product",
            )
            .await
            .map(GuardedWrite::StaleVersion);
        };

        transaction
            .commit()
            .await
            .map_err(|error| store_error("failed to commit spu publication", error))?;

        Ok(GuardedWrite::Applied(map_spu_row(&row)))
    }

    /// Archives a SPU and pulls it from every sellable index.
    pub async fn archive_spu(
        &self,
        command: &ArchiveSpuCommand,
    ) -> Result<GuardedWrite<SpuRecord>, CommerceServiceError> {
        let tenant_id = parse_id("tenant_id", &command.tenant_id)?;
        let id = parse_id("spu_id", &command.spu_id)?;

        let row = sqlx::query(ARCHIVE_SPU_SQL)
            .bind(tenant_id)
            .bind(id)
            .bind(command.expected_version)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("failed to archive spu", error))?;

        let Some(row) = row else {
            return classify_guarded_miss(
                &self.pool,
                PROBE_SPU_VERSION_SQL,
                tenant_id,
                id,
                command.expected_version,
                "product",
            )
            .await
            .map(GuardedWrite::StaleVersion);
        };
        Ok(GuardedWrite::Applied(map_spu_row(&row)))
    }

    // ---------------------------------------------------------------- SKUs

    /// Lists SKUs, optionally narrowed to one sales-axis value.
    ///
    /// The axis filter is an `EXISTS` over `commerce_product_sku_attribute` rather than a join,
    /// because a join would multiply a SKU by its number of axes and then need `DISTINCT` — which
    /// would in turn break the `LIMIT`. `idx_commerce_product_sku_attribute_tenant_value` is exactly
    /// `(tenant_id, attribute_value_id, sku_id)`, so the subquery is an index probe.
    pub async fn list_skus(
        &self,
        query: &ProductSkuListQuery,
    ) -> Result<Vec<SkuRecord>, CommerceServiceError> {
        let limit = query.page_size.unwrap_or(20).min(200);
        let offset = (query.page.unwrap_or(1) - 1).max(0) * limit;

        let rows = sqlx::query(LIST_SKUS_SQL)
            .bind(parse_id("tenant_id", &query.tenant_id)?)
            .bind(parse_optional_id(
                "organization_id",
                query.organization_id.as_deref(),
            )?)
            .bind(parse_optional_id("spu_id", query.spu_id.as_deref())?)
            .bind(query.status.as_deref())
            .bind(parse_optional_id(
                "attribute_value_id",
                query.attribute_value_id.as_deref(),
            )?)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("failed to list skus", error))?;

        let mut skus: Vec<SkuRecord> = rows
            .iter()
            .map(map_sku_row)
            .collect::<Result<Vec<_>, _>>()?;
        let tenant_id = parse_id("tenant_id", &query.tenant_id)?;
        attach_sku_axes(&self.pool, tenant_id, &mut skus).await?;
        Ok(skus)
    }

    pub async fn count_skus(
        &self,
        query: &ProductSkuListQuery,
    ) -> Result<i64, CommerceServiceError> {
        sqlx::query_scalar(COUNT_SKUS_SQL)
            .bind(parse_id("tenant_id", &query.tenant_id)?)
            .bind(parse_optional_id(
                "organization_id",
                query.organization_id.as_deref(),
            )?)
            .bind(parse_optional_id("spu_id", query.spu_id.as_deref())?)
            .bind(query.status.as_deref())
            .bind(parse_optional_id(
                "attribute_value_id",
                query.attribute_value_id.as_deref(),
            )?)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("failed to count skus", error))
    }

    pub async fn retrieve_sku(
        &self,
        query: &ProductSkuRetrieveQuery,
    ) -> Result<Option<SkuRecord>, CommerceServiceError> {
        let tenant_id = parse_id("tenant_id", &query.tenant_id)?;
        let row = sqlx::query(RETRIEVE_SKU_SQL)
            .bind(tenant_id)
            .bind(parse_id("sku_id", &query.sku_id)?)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("failed to retrieve sku", error))?;

        let Some(row) = row.as_ref() else {
            return Ok(None);
        };
        // The axes are loaded through the same helper the list path uses, so a retrieved SKU and a
        // listed SKU cannot disagree about which positions they occupy.
        let mut skus = vec![map_sku_row(row)?];
        attach_sku_axes(&self.pool, tenant_id, &mut skus).await?;
        let sku = skus.swap_remove(0);
        Ok(Some(sku))
    }

    /// Creates a SKU, its sales axes, and its variant signature in one transaction.
    ///
    /// The order matters and is the reason this is not two statements: the axis rows and the
    /// signature are derived from each other, so a SKU inserted without its axes would briefly carry
    /// a signature that does not describe it, and `uk_commerce_product_sku_variant` would be
    /// enforcing the wrong thing for that window. Everything the caller can observe commits at once.
    pub async fn create_sku(
        &self,
        command: &CreateProductSkuCommand,
    ) -> Result<SkuRecord, CommerceServiceError> {
        let tenant_id = parse_id("tenant_id", &command.tenant_id)?;
        let organization_id = parse_id("organization_id", &command.organization_id)?;
        let spu_id = parse_id("spu_id", &command.spu_id)?;
        let id = self.next_id()?;

        let currency = resolve_currency(&self.pool, &command.currency_code).await?;
        let sale_price_minor = command.sale_price_minor;
        let list_price_minor = command.list_price_minor;
        ensure_sale_not_above_list(sale_price_minor, list_price_minor)?;

        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("failed to open sku transaction", error))?;

        let axes = resolve_sku_axes(
            &mut transaction,
            tenant_id,
            spu_id,
            &command.attribute_value_ids,
        )
        .await?;
        let variant_signature = build_variant_signature(&command.sku_no, &axes)?;
        ensure_variant_available(
            &mut transaction,
            tenant_id,
            spu_id,
            &variant_signature,
            None,
        )
        .await?;

        let row = sqlx::query(INSERT_SKU_SQL)
            .bind(id)
            .bind(tenant_id)
            .bind(organization_id)
            .bind(spu_id)
            .bind(&command.sku_no)
            .bind(&variant_signature)
            .bind(&command.name)
            .bind(&command.title)
            .bind(&command.currency_code)
            .bind(currency.scale)
            .bind(list_price_minor)
            .bind(sale_price_minor)
            .bind(command.fulfillment_type.as_storage_str())
            .bind(command.inventory_tracking.as_storage_str())
            .bind(&command.metadata)
            .fetch_one(&mut *transaction)
            .await
            .map_err(|error| store_error("failed to create sku", error))?;

        write_sku_axes(
            &self.ids,
            &mut transaction,
            tenant_id,
            organization_id,
            id,
            &axes,
        )
        .await?;

        transaction
            .commit()
            .await
            .map_err(|error| store_error("failed to commit sku creation", error))?;

        let mut sku = map_sku_row(&row)?;
        sku.attribute_values = axes;
        Ok(sku)
    }

    /// Updates a SKU.
    ///
    /// Updates a SKU, replacing its sales axes when they are supplied.
    ///
    /// A currency change must restate the affected amounts. Reinterpreting an existing minor amount
    /// under a new scale would silently turn 64000 CNY cents into 64000 JPY yen; requiring the caller
    /// to state the price in the new currency keeps the write explicit.
    ///
    /// An axis change is applied **before** the row update, so the `RETURNING` clause hands back the
    /// signature the row now carries. Doing it the other way round would return a record whose
    /// `variant_signature` describes the axes it no longer has.
    pub async fn update_sku(
        &self,
        command: &UpdateProductSkuCommand,
    ) -> Result<GuardedWrite<SkuRecord>, CommerceServiceError> {
        let tenant_id = parse_id("tenant_id", &command.tenant_id)?;
        let id = parse_id("sku_id", &command.sku_id)?;

        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("failed to open sku transaction", error))?;

        let current = sqlx::query(LOCK_SKU_SQL)
            .bind(tenant_id)
            .bind(id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|error| store_error("failed to load sku for update", error))?
            .ok_or_else(|| CommerceServiceError::not_found("sku was not found"))?;

        let current_currency: String = current
            .try_get("currency_code")
            .map_err(|error| store_error("failed to read sku currency", error))?;
        let current_sale: i64 = current
            .try_get("sale_price_minor")
            .map_err(|error| store_error("failed to read sku sale price", error))?;
        let current_list: Option<i64> = current
            .try_get("list_price_minor")
            .map_err(|error| store_error("failed to read sku list price", error))?;
        let spu_id: i64 = current
            .try_get("spu_id")
            .map_err(|error| store_error("failed to read sku product", error))?;
        let sku_no: String = current
            .try_get("sku_no")
            .map_err(|error| store_error("failed to read sku number", error))?;
        let organization_id: i64 = current
            .try_get("organization_id")
            .map_err(|error| store_error("failed to read sku organization", error))?;

        let effective_currency = command
            .currency_code
            .clone()
            .unwrap_or_else(|| current_currency.clone());
        let currency_changed = effective_currency != current_currency;
        if currency_changed {
            if command.sale_price_minor.is_none() {
                return Err(CommerceServiceError::validation(
                    "changing currency_code requires salePriceMinor to be restated in the new currency",
                ));
            }
            if current_list.is_some() && command.list_price_minor.is_none() {
                return Err(CommerceServiceError::validation(
                    "changing currency_code requires listPriceMinor to be restated or cleared",
                ));
            }
        }

        let currency = resolve_currency(&self.pool, &effective_currency).await?;
        let sale_price_minor = command.sale_price_minor.unwrap_or(current_sale);
        // `Some(None)` clears and `Some(Some(_))` replaces; both are an explicit statement about the
        // reference price, so neither is caught by the currency-change guard above, which only
        // refuses a *silent* omission.
        let list_price_minor = match command.list_price_minor {
            Some(minor) => minor,
            // Reachable only when nothing was stored: the guard refuses a currency change that
            // leaves an existing reference price unaddressed, because a minor count cannot be
            // reinterpreted under a new scale.
            None if currency_changed => None,
            None => current_list,
        };
        ensure_sale_not_above_list(sale_price_minor, list_price_minor)?;

        // `None` keeps the stored axes, `Some(vec![])` clears them, anything else replaces them.
        let mut replaced_axes: Option<Vec<SkuAxisRecord>> = None;
        if let Some(value_ids) = command.attribute_value_ids.as_deref() {
            let axes = resolve_sku_axes(&mut transaction, tenant_id, spu_id, value_ids).await?;
            let signature = build_variant_signature(&sku_no, &axes)?;
            ensure_variant_available(&mut transaction, tenant_id, spu_id, &signature, Some(id))
                .await?;

            sqlx::query(SOFT_DELETE_SKU_AXES_SQL)
                .bind(tenant_id)
                .bind(id)
                .execute(&mut *transaction)
                .await
                .map_err(|error| store_error("failed to retire sku axes", error))?;

            write_sku_axes(
                &self.ids,
                &mut transaction,
                tenant_id,
                organization_id,
                id,
                &axes,
            )
            .await?;

            sqlx::query(UPDATE_SKU_VARIANT_SIGNATURE_SQL)
                .bind(&signature)
                .bind(tenant_id)
                .bind(id)
                .execute(&mut *transaction)
                .await
                .map_err(|error| store_error("failed to update sku variant signature", error))?;

            replaced_axes = Some(axes);
        }

        // `sales_status` is derived, never supplied: the baseline only allows a sellable row whose own
        // status is `active`, and the sellable index keys on this column.
        let row = sqlx::query(UPDATE_SKU_SQL)
            .bind(command.name.as_deref())
            .bind(command.title.as_deref())
            .bind(sale_price_minor)
            .bind(list_price_minor)
            .bind(&effective_currency)
            .bind(currency.scale)
            .bind(command.fulfillment_type.map(|value| value.as_storage_str()))
            .bind(
                command
                    .inventory_tracking
                    .map(|value| value.as_storage_str()),
            )
            .bind(command.status.map(|value| value.as_storage_str()))
            .bind(tenant_id)
            .bind(id)
            .bind(command.expected_version)
            .bind(command.metadata.as_ref())
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|error| store_error("failed to update sku", error))?;

        let Some(row) = row else {
            // The axis rewrite and the recomputed signature above are already in this transaction,
            // so returning here rolls them back rather than leaving axes that belong to no version.
            return classify_guarded_miss(
                &mut *transaction,
                PROBE_SKU_VERSION_SQL,
                tenant_id,
                id,
                command.expected_version,
                "sku",
            )
            .await
            .map(GuardedWrite::StaleVersion);
        };

        transaction
            .commit()
            .await
            .map_err(|error| store_error("failed to commit sku update", error))?;

        let mut sku = map_sku_row(&row)?;
        match replaced_axes {
            Some(axes) => sku.attribute_values = axes,
            None => {
                let mut skus = vec![sku];
                attach_sku_axes(&self.pool, tenant_id, &mut skus).await?;
                sku = skus.swap_remove(0);
            }
        }
        Ok(GuardedWrite::Applied(sku))
    }

    pub async fn delete_sku(
        &self,
        command: &DeleteProductSkuCommand,
    ) -> Result<GuardedWrite<()>, CommerceServiceError> {
        let tenant_id = parse_id("tenant_id", &command.tenant_id)?;
        let id = parse_id("sku_id", &command.sku_id)?;

        let affected = sqlx::query(SOFT_DELETE_SKU_SQL)
            .bind(tenant_id)
            .bind(id)
            .bind(command.expected_version)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("failed to delete sku", error))?
            .rows_affected();

        if affected == 0 {
            return classify_guarded_miss(
                &self.pool,
                PROBE_SKU_VERSION_SQL,
                tenant_id,
                id,
                command.expected_version,
                "sku",
            )
            .await
            .map(GuardedWrite::StaleVersion);
        }
        Ok(GuardedWrite::Applied(()))
    }

    // ---------------------------------------------------------------- media

    pub async fn list_media(
        &self,
        query: &MediaListQuery,
    ) -> Result<Vec<MediaRecord>, CommerceServiceError> {
        let limit = query.page_size.unwrap_or(20).min(200);
        let offset = (query.page.unwrap_or(1) - 1).max(0) * limit;

        let rows = sqlx::query(LIST_MEDIA_SQL)
            .bind(parse_id("tenant_id", &query.tenant_id)?)
            .bind(parse_optional_id(
                "organization_id",
                query.organization_id.as_deref(),
            )?)
            .bind(query.owner_type.as_deref())
            .bind(parse_optional_id("owner_id", query.owner_id.as_deref())?)
            .bind(query.media_role.as_deref())
            .bind(query.status.as_deref())
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("failed to list media", error))?;

        rows.iter().map(map_media_row).collect()
    }

    pub async fn count_media(&self, query: &MediaListQuery) -> Result<i64, CommerceServiceError> {
        sqlx::query_scalar(COUNT_MEDIA_SQL)
            .bind(parse_id("tenant_id", &query.tenant_id)?)
            .bind(parse_optional_id(
                "organization_id",
                query.organization_id.as_deref(),
            )?)
            .bind(query.owner_type.as_deref())
            .bind(parse_optional_id("owner_id", query.owner_id.as_deref())?)
            .bind(query.media_role.as_deref())
            .bind(query.status.as_deref())
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("failed to count media", error))
    }

    /// Attaches one media resource to one owner.
    ///
    /// The owner is verified before the row is written, in the same transaction. There is no foreign
    /// key to do it — `owner_id` points at one of four tables — so this is the only place the
    /// constraint exists at all; without it a typo in `ownerId` would silently attach a product's
    /// main image to a product that does not exist.
    ///
    /// `media_resource_id` is derived from the snapshot through the service crate's
    /// `media_resource_identity`, which is the same function `CreateMediaCommand::validate` uses. The
    /// two therefore cannot disagree about which resource the row references.
    pub async fn create_media(
        &self,
        command: &CreateMediaCommand,
    ) -> Result<MediaRecord, CommerceServiceError> {
        let tenant_id = parse_id("tenant_id", &command.tenant_id)?;
        let organization_id = parse_id("organization_id", &command.organization_id)?;
        let owner_id = parse_id("owner_id", &command.owner_id)?;
        let resource_snapshot = command.media_resource_id()?;
        let id = self.next_id()?;

        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("failed to open media transaction", error))?;

        ensure_media_owner(&mut transaction, tenant_id, command.owner_type, owner_id).await?;

        let row = sqlx::query(INSERT_MEDIA_SQL)
            .bind(id)
            .bind(tenant_id)
            .bind(organization_id)
            .bind(command.owner_type.as_storage_str())
            .bind(owner_id)
            .bind(command.media_role.as_storage_str())
            .bind(resource_snapshot)
            .bind(&command.resource_snapshot)
            .bind(command.alt_text.as_deref())
            .bind(command.sort_order)
            .fetch_one(&mut *transaction)
            .await
            .map_err(|error| store_error("failed to create media", error))?;

        transaction
            .commit()
            .await
            .map_err(|error| store_error("failed to commit media creation", error))?;

        map_media_row(&row)
    }

    /// Updates one media attachment.
    ///
    /// The row's **stored** `owner_type` is re-read under a row lock, because the role/owner-kind
    /// pair is constrained by `ck_commerce_product_media_owner_role` and this operation does not
    /// accept an owner kind. Checking the submitted role against a locally assumed owner would be
    /// validating a different row than the one being written.
    pub async fn update_media(
        &self,
        command: &UpdateMediaCommand,
    ) -> Result<GuardedWrite<MediaRecord>, CommerceServiceError> {
        let tenant_id = parse_id("tenant_id", &command.tenant_id)?;
        let id = parse_id("media_id", &command.media_id)?;
        let replacement_resource = command.media_resource_id()?;

        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("failed to open media transaction", error))?;

        let owner_type_raw: String = sqlx::query_scalar(LOCK_MEDIA_SQL)
            .bind(tenant_id)
            .bind(id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|error| store_error("failed to load media for update", error))?
            .ok_or_else(|| CommerceServiceError::not_found("media was not found"))?;

        if let Some(role) = command.media_role {
            let owner_type = MediaOwnerType::from_storage_str("owner_type", &owner_type_raw)?;
            if !owner_type.admits_role(role) {
                return Err(CommerceServiceError::validation(format!(
                    "media_role `{}` is not permitted for owner_type `{}`: a dictionary value carries \
                     an image swatch, so only main_image, sku_image, and gallery_image are available",
                    role.as_storage_str(),
                    owner_type.as_storage_str(),
                )));
            }
        }

        let row = sqlx::query(UPDATE_MEDIA_SQL)
            .bind(command.media_role.map(|role| role.as_storage_str()))
            .bind(replacement_resource)
            .bind(command.resource_snapshot.as_ref())
            .bind(command.alt_text.as_deref())
            .bind(command.sort_order)
            .bind(command.status.map(|status| status.as_storage_str()))
            .bind(tenant_id)
            .bind(id)
            .bind(command.expected_version)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|error| store_error("failed to update media", error))?;

        let Some(row) = row else {
            return classify_guarded_miss(
                &mut *transaction,
                PROBE_MEDIA_VERSION_SQL,
                tenant_id,
                id,
                command.expected_version,
                "media",
            )
            .await
            .map(GuardedWrite::StaleVersion);
        };

        transaction
            .commit()
            .await
            .map_err(|error| store_error("failed to commit media update", error))?;

        Ok(GuardedWrite::Applied(map_media_row(&row)?))
    }

    pub async fn delete_media(
        &self,
        command: &DeleteMediaCommand,
    ) -> Result<GuardedWrite<()>, CommerceServiceError> {
        let tenant_id = parse_id("tenant_id", &command.tenant_id)?;
        let id = parse_id("media_id", &command.media_id)?;

        let affected = sqlx::query(SOFT_DELETE_MEDIA_SQL)
            .bind(tenant_id)
            .bind(id)
            .bind(command.expected_version)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("failed to delete media", error))?
            .rows_affected();

        if affected == 0 {
            return classify_guarded_miss(
                &self.pool,
                PROBE_MEDIA_VERSION_SQL,
                tenant_id,
                id,
                command.expected_version,
                "media",
            )
            .await
            .map(GuardedWrite::StaleVersion);
        }
        Ok(GuardedWrite::Applied(()))
    }
}

// ------------------------------------------------------------------ category tree

struct LockedCategory {
    parent_id: Option<i64>,
    path: String,
    depth: i64,
}

/// Locks one live category row for update, or reports that it does not exist.
async fn lock_category(
    transaction: &mut Transaction<'_, Postgres>,
    tenant_id: i64,
    id: i64,
) -> Result<LockedCategory, CommerceServiceError> {
    let row = sqlx::query(LOCK_CATEGORY_SQL)
        .bind(tenant_id)
        .bind(id)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(|error| store_error("failed to lock category", error))?
        .ok_or_else(|| CommerceServiceError::not_found("category was not found"))?;

    let parent_id = row
        .try_get("parent_id")
        .map_err(|error| store_error("failed to read category parent", error))?;
    let path = row
        .try_get("path")
        .map_err(|error| store_error("failed to read category path", error))?;
    let depth = row
        .try_get::<i16, _>("depth")
        .map_err(|error| store_error("failed to read category depth", error))?;

    Ok(LockedCategory {
        parent_id,
        path,
        depth: i64::from(depth),
    })
}

/// Recomputes `is_leaf` for one category from its live children.
async fn refresh_leaf_state(
    transaction: &mut Transaction<'_, Postgres>,
    tenant_id: i64,
    id: i64,
) -> Result<(), CommerceServiceError> {
    sqlx::query(REFRESH_CATEGORY_LEAF_SQL)
        .bind(tenant_id)
        .bind(id)
        .execute(&mut **transaction)
        .await
        .map_err(|error| store_error("failed to refresh category leaf state", error))?;

    Ok(())
}

// ------------------------------------------------------------------ money + currency

/// A currency row's `minor_unit_exponent`, which is what a money row snapshots as its `price_scale`.
struct ResolvedCurrency {
    scale: i16,
}

/// Reads the money scale and rounding of one active currency.
///
/// The exponent is read rather than assumed, which is the whole point of keeping currency data in a
/// table instead of in code (`DATABASE_SPEC` section 14 / DB095).
///
/// The registry row is validated through the money kernel before its scale is trusted: an unknown
/// `rounding_mode` token or an out-of-range exponent is a data defect that would otherwise be
/// copied onto every price row as a scale nothing can render. Since the wire already carries minor
/// units (`API_SPEC` section 13.2.1), the resolved `MoneyUnit` has no conversion left to perform —
/// only the `SMALLINT` is carried forward.
async fn resolve_currency(
    pool: &PgPool,
    code: &str,
) -> Result<ResolvedCurrency, CommerceServiceError> {
    let row = sqlx::query(SELECT_CURRENCY_SQL)
        .bind(code)
        .fetch_optional(pool)
        .await
        .map_err(|error| store_error("failed to resolve currency", error))?
        .ok_or_else(|| {
            CommerceServiceError::validation(format!(
                "currency `{code}` is not an active entry in commerce_currency"
            ))
        })?;

    let scale: i16 = row
        .try_get("minor_unit_exponent")
        .map_err(|error| store_error("failed to read currency scale", error))?;
    let rounding: String = row
        .try_get("rounding_mode")
        .map_err(|error| store_error("failed to read currency rounding mode", error))?;

    let scale_u8 = u8::try_from(scale).map_err(|_| {
        CommerceServiceError::validation(format!("currency `{code}` declares an unusable scale"))
    })?;
    MoneyUnit::from_registry(code, scale_u8, &rounding).map_err(|error| {
        CommerceServiceError::validation(format!("currency `{code}` is unusable: {error}"))
    })?;

    Ok(ResolvedCurrency { scale })
}

async fn ensure_currency_exists(pool: &PgPool, code: &str) -> Result<(), CommerceServiceError> {
    let exists: bool = sqlx::query_scalar(CURRENCY_EXISTS_SQL)
        .bind(code)
        .fetch_one(pool)
        .await
        .map_err(|error| store_error("failed to check currency", error))?;
    if !exists {
        return Err(CommerceServiceError::validation(format!(
            "currency `{code}` is not registered in commerce_currency"
        )));
    }
    Ok(())
}

/// Enforces `ck_commerce_product_sku_sale_not_above_list` before the row reaches the database.
///
/// The comparison is on two values of the same currency and the same unit (minor units of
/// `currency_code`), so it is a plain integer comparison with no rescaling. It stays in the
/// repository rather than the command because an update can supply only one of the two sides and
/// the other then comes from the locked row.
fn ensure_sale_not_above_list(
    sale_price_minor: i64,
    list_price_minor: Option<i64>,
) -> Result<(), CommerceServiceError> {
    if let Some(list) = list_price_minor {
        if sale_price_minor > list {
            return Err(CommerceServiceError::validation(
                "salePriceMinor must not exceed listPriceMinor",
            ));
        }
    }
    Ok(())
}

// ------------------------------------------------------------------ variant axis assembly

/// Turns the submitted value ids into the SKU's axis rows.
///
/// Four things are checked here, and each one is a rule the schema cannot state:
///
/// 1. every submitted id resolves to a live, active dictionary value — an unresolved id is a caller
///    error, never a silently dropped axis;
/// 2. the attribute each value belongs to is derived from the value's own row, so a request can
///    never name an attribute and a value that disagree;
/// 3. every resolved attribute is an **active `sales` axis of the SPU's category**. This is what
///    makes `variant_signature` mean what the baseline says it means; a `parameter` attribute is
///    descriptive metadata and must not split a SKU;
/// 4. the submitted set covers the category's axis set **exactly**. A partial set would satisfy
///    `uk_commerce_product_sku_variant` while leaving the combination incomplete, so two SKUs of the
///    same product could both omit the axis that distinguishes them.
///
/// An empty submission is legal only when the category declares no sales axis. The rows come back
/// ordered by `attribute_no`, which is what makes the signature deterministic.
async fn resolve_sku_axes(
    transaction: &mut Transaction<'_, Postgres>,
    tenant_id: i64,
    spu_id: i64,
    value_ids: &[String],
) -> Result<Vec<SkuAxisRecord>, CommerceServiceError> {
    let category_id: i64 = sqlx::query_scalar(SPU_CATEGORY_SQL)
        .bind(tenant_id)
        .bind(spu_id)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(|error| store_error("failed to load the product's category", error))?
        .ok_or_else(|| CommerceServiceError::not_found("the product was not found"))?;

    let declared: Vec<i64> = sqlx::query_scalar(CATEGORY_ACTIVE_SALES_AXES_SQL)
        .bind(tenant_id)
        .bind(category_id)
        .fetch_all(&mut **transaction)
        .await
        .map_err(|error| store_error("failed to load the category's sales axes", error))?;

    if value_ids.is_empty() {
        if !declared.is_empty() {
            return Err(CommerceServiceError::validation(format!(
                "this product's category declares {} sales axis(es), so attributeValueIds must be \
                 submitted",
                declared.len()
            )));
        }
        return Ok(Vec::new());
    }

    let mut requested = Vec::with_capacity(value_ids.len());
    for raw in value_ids {
        requested.push(parse_id("attribute_value_ids", raw)?);
    }

    let rows = sqlx::query(RESOLVE_AXIS_VALUES_SQL)
        .bind(tenant_id)
        .bind(&requested)
        .fetch_all(&mut **transaction)
        .await
        .map_err(|error| store_error("failed to resolve the submitted axis values", error))?;

    if rows.len() != requested.len() {
        let resolved: Vec<i64> = rows.iter().map(|row| bigint_cell(row, "id")).collect();
        let missing: Vec<String> = requested
            .iter()
            .filter(|id| !resolved.contains(id))
            .map(|id| id.to_string())
            .collect();
        return Err(CommerceServiceError::validation(format!(
            "attributeValueIds names value(s) that are not live dictionary values of this tenant \
             or whose attribute is inactive: {}",
            missing.join(", ")
        )));
    }

    let mut axes = Vec::with_capacity(rows.len());
    for row in rows.iter() {
        let attribute_id = bigint_cell(row, "attribute_id");
        if !declared.contains(&attribute_id) {
            return Err(CommerceServiceError::validation(format!(
                "attribute `{}` is not an active sales axis of this product's category",
                string_cell(row, "attribute_no")
            )));
        }
        axes.push(SkuAxisRecord {
            attribute_id,
            attribute_value_id: bigint_cell(row, "id"),
            attribute_no: string_cell(row, "attribute_no"),
            value_code: string_cell(row, "value_code"),
            display_value: string_cell(row, "display_value"),
            sort_order: bigint_cell(row, "sort_order"),
        });
    }

    axes.sort_by(|left, right| {
        left.attribute_no
            .cmp(&right.attribute_no)
            .then(left.attribute_id.cmp(&right.attribute_id))
    });

    let mut submitted: Vec<i64> = axes.iter().map(|axis| axis.attribute_id).collect();
    submitted.sort_unstable();
    submitted.dedup();
    let mut expected = declared.clone();
    expected.sort_unstable();
    if submitted != expected {
        let missing: Vec<String> = expected
            .iter()
            .filter(|id| !submitted.contains(id))
            .map(|id| id.to_string())
            .collect();
        return Err(CommerceServiceError::validation(format!(
            "attributeValueIds must name exactly one value per sales axis of the product's \
             category; no value was submitted for attribute id(s) {}",
            missing.join(", ")
        )));
    }

    Ok(axes)
}

/// Builds the SKU's `variant_signature` from its axes.
///
/// One `attribute_no=value_code` term per axis, ordered by `attribute_no`, joined with `;`. Business
/// keys rather than snowflakes, because the signature is what `uk_commerce_product_sku_variant`
/// compares: an id-based signature would change the day a tenant re-created an attribute, and the
/// same logical variant could then exist twice.
///
/// The axes are sorted *here* rather than relied upon from the caller. The signature has to be a
/// function of the axis **set**, or two orderings of one combination would produce two signatures and
/// the unique index would admit both. `resolve_sku_axes` sorts already and the read path orders by
/// `attribute_no`, so sorting again costs a handful of comparisons — and it means a rule this
/// function depends on for its correctness does not live only in its callers.
///
/// A SKU with no axes falls back to its own `sku_no`. The empty combination is not writable — the
/// baseline requires `char_length(variant_signature) BETWEEN 1 AND 500` — and `sku_no` is unique per
/// tenant, so the fallback keeps the index meaningful without inventing an axis.
fn build_variant_signature(
    sku_no: &str,
    axes: &[SkuAxisRecord],
) -> Result<String, CommerceServiceError> {
    if axes.is_empty() {
        return Ok(sku_no.to_owned());
    }

    let mut ordered: Vec<&SkuAxisRecord> = axes.iter().collect();
    ordered.sort_by(|left, right| {
        left.attribute_no
            .cmp(&right.attribute_no)
            .then(left.attribute_id.cmp(&right.attribute_id))
    });

    let signature = ordered
        .iter()
        .map(|axis| format!("{}={}", axis.attribute_no, axis.value_code))
        .collect::<Vec<_>>()
        .join(";");

    if signature.chars().count() > SKU_VARIANT_SIGNATURE_MAX_CHARS {
        return Err(CommerceServiceError::validation(format!(
            "the axis combination is {SKU_VARIANT_SIGNATURE_MAX_CHARS} characters or shorter \
             (ck_commerce_product_sku_variant_signature); this one is {} characters",
            signature.chars().count()
        )));
    }

    Ok(signature)
}

/// Refuses a signature already taken by another live SKU of the same product.
///
/// `uk_commerce_product_sku_variant` would reject the write anyway, but it would do so as a `23505`
/// naming an index rather than the combination the caller chose. Checking first turns "one sellable
/// unit per axis combination" into a `409` that says which combination is taken.
async fn ensure_variant_available(
    transaction: &mut Transaction<'_, Postgres>,
    tenant_id: i64,
    spu_id: i64,
    variant_signature: &str,
    exclude_id: Option<i64>,
) -> Result<(), CommerceServiceError> {
    let taken: bool = sqlx::query_scalar(VARIANT_TAKEN_SQL)
        .bind(tenant_id)
        .bind(spu_id)
        .bind(variant_signature)
        .bind(exclude_id)
        .fetch_one(&mut **transaction)
        .await
        .map_err(|error| store_error("failed to check the variant signature", error))?;

    if taken {
        return Err(CommerceServiceError::conflict(format!(
            "variant_signature `{variant_signature}` is already used by a live SKU of this product"
        )));
    }
    Ok(())
}

/// Writes the axis rows of one SKU.
async fn write_sku_axes(
    ids: &Arc<dyn IdGenerator>,
    transaction: &mut Transaction<'_, Postgres>,
    tenant_id: i64,
    organization_id: i64,
    sku_id: i64,
    axes: &[SkuAxisRecord],
) -> Result<(), CommerceServiceError> {
    for axis in axes {
        sqlx::query(INSERT_SKU_AXIS_SQL)
            .bind(next_snowflake(ids)?)
            .bind(tenant_id)
            .bind(organization_id)
            .bind(sku_id)
            .bind(axis.attribute_id)
            .bind(axis.attribute_value_id)
            .bind(axis.sort_order)
            .execute(&mut **transaction)
            .await
            .map_err(|error| store_error("failed to write a sku axis", error))?;
    }
    Ok(())
}

/// Loads and attaches the axes of every SKU in a page, in one statement.
async fn attach_sku_axes(
    pool: &PgPool,
    tenant_id: i64,
    skus: &mut [SkuRecord],
) -> Result<(), CommerceServiceError> {
    if skus.is_empty() {
        return Ok(());
    }

    let sku_ids: Vec<i64> = skus.iter().map(|sku| sku.id).collect();
    let rows = sqlx::query(LIST_SKU_AXES_SQL)
        .bind(tenant_id)
        .bind(&sku_ids)
        .fetch_all(pool)
        .await
        .map_err(|error| store_error("failed to load sku axes", error))?;

    let mut grouped: HashMap<i64, Vec<SkuAxisRecord>> = HashMap::new();
    for row in rows.iter() {
        let (sku_id, axis) = map_sku_axis_row(row);
        grouped.entry(sku_id).or_default().push(axis);
    }

    for sku in skus.iter_mut() {
        sku.attribute_values = grouped.remove(&sku.id).unwrap_or_default();
    }
    Ok(())
}

// ------------------------------------------------------------------ media ownership

/// Refuses an attachment whose owner does not exist.
///
/// `commerce_product_media.owner_id` carries no foreign key: PostgreSQL cannot express "this BIGINT
/// points at one of four tables", and four nullable FK columns would let one row claim two owners
/// and force every read to guess which one is live. Verifying the owner against the table its
/// `owner_type` names is therefore the only place this constraint exists.
async fn ensure_media_owner(
    transaction: &mut Transaction<'_, Postgres>,
    tenant_id: i64,
    owner_type: MediaOwnerType,
    owner_id: i64,
) -> Result<(), CommerceServiceError> {
    let statement = match owner_type {
        MediaOwnerType::Spu => OWNER_SPU_EXISTS_SQL,
        MediaOwnerType::Sku => OWNER_SKU_EXISTS_SQL,
        MediaOwnerType::Category => OWNER_CATEGORY_EXISTS_SQL,
        MediaOwnerType::AttributeValue => OWNER_ATTRIBUTE_VALUE_EXISTS_SQL,
    };

    let exists: bool = sqlx::query_scalar(statement)
        .bind(tenant_id)
        .bind(owner_id)
        .fetch_one(&mut **transaction)
        .await
        .map_err(|error| store_error("failed to verify the media owner", error))?;

    if !exists {
        return Err(CommerceServiceError::validation(format!(
            "owner_id `{owner_id}` is not a live `{}` of this tenant",
            owner_type.as_storage_str()
        )));
    }
    Ok(())
}

// ------------------------------------------------------------------ row mappers

fn map_category_row(row: &sqlx::postgres::PgRow) -> CategoryRecord {
    CategoryRecord {
        id: bigint_cell(row, "id"),
        tenant_id: bigint_cell(row, "tenant_id"),
        organization_id: bigint_cell(row, "organization_id"),
        category_no: string_cell(row, "category_no"),
        parent_id: optional_bigint_cell(row, "parent_id"),
        path: string_cell(row, "path"),
        depth: i64::from(smallint_cell(row, "depth")),
        is_leaf: boolean_cell(row, "is_leaf"),
        name: string_cell(row, "name"),
        sort_order: bigint_cell(row, "sort_order"),
        status: string_cell(row, "status"),
        version: bigint_cell(row, "version"),
        created_at: timestamp_cell(row, "created_at"),
        updated_at: timestamp_cell(row, "updated_at"),
    }
}

fn map_attribute_row(row: &sqlx::postgres::PgRow) -> AttributeRecord {
    AttributeRecord {
        id: bigint_cell(row, "id"),
        tenant_id: bigint_cell(row, "tenant_id"),
        organization_id: bigint_cell(row, "organization_id"),
        attribute_no: string_cell(row, "attribute_no"),
        name: string_cell(row, "name"),
        value_type: string_cell(row, "value_type"),
        status: string_cell(row, "status"),
        sort_order: bigint_cell(row, "sort_order"),
        version: bigint_cell(row, "version"),
        created_at: timestamp_cell(row, "created_at"),
        updated_at: timestamp_cell(row, "updated_at"),
    }
}

fn map_price_list_row(row: &sqlx::postgres::PgRow) -> PriceListRecord {
    PriceListRecord {
        id: bigint_cell(row, "id"),
        tenant_id: bigint_cell(row, "tenant_id"),
        organization_id: bigint_cell(row, "organization_id"),
        price_list_no: string_cell(row, "price_list_no"),
        name: string_cell(row, "name"),
        currency_code: string_cell(row, "currency_code"),
        market_code: optional_string_cell(row, "market_code"),
        status: string_cell(row, "status"),
        starts_at: optional_timestamp_cell(row, "starts_at"),
        ends_at: optional_timestamp_cell(row, "ends_at"),
        version: bigint_cell(row, "version"),
        created_at: timestamp_cell(row, "created_at"),
        updated_at: timestamp_cell(row, "updated_at"),
    }
}

fn map_spu_row(row: &sqlx::postgres::PgRow) -> SpuRecord {
    SpuRecord {
        id: bigint_cell(row, "id"),
        tenant_id: bigint_cell(row, "tenant_id"),
        organization_id: bigint_cell(row, "organization_id"),
        spu_no: string_cell(row, "spu_no"),
        category_id: bigint_cell(row, "category_id"),
        name: string_cell(row, "name"),
        title: optional_string_cell(row, "title"),
        subtitle: optional_string_cell(row, "subtitle"),
        description: optional_string_cell(row, "description"),
        product_type: string_cell(row, "product_type"),
        status: string_cell(row, "status"),
        sales_status: string_cell(row, "sales_status"),
        published_at: optional_timestamp_cell(row, "published_at"),
        version: bigint_cell(row, "version"),
        created_at: timestamp_cell(row, "created_at"),
        updated_at: timestamp_cell(row, "updated_at"),
    }
}

/// Maps one `commerce_product_sku` row.
///
/// `attribute_values` starts empty because the axes live in a second table and are loaded for a
/// whole page at once. Callers attach them through [`attach_sku_axes`]; leaving the field defaulted
/// here rather than synthesising a placeholder is what keeps "this SKU has no axes" and "the axes
/// were not loaded" from looking the same to a reader of the record.
pub(crate) fn map_sku_row(row: &sqlx::postgres::PgRow) -> Result<SkuRecord, CommerceServiceError> {
    // Fallible for the same reason `map_media_row` is: `metadata` is `NOT NULL DEFAULT '{}'`, so a
    // decode failure means the statement selected the wrong column type — not that the capability
    // declared nothing. Turning that into an empty object would make "no metadata" and "metadata
    // that failed to load" indistinguishable to the caller.
    let metadata: Value = row
        .try_get("metadata")
        .map_err(|error| store_error("failed to read sku metadata", error))?;

    Ok(SkuRecord {
        id: bigint_cell(row, "id"),
        tenant_id: bigint_cell(row, "tenant_id"),
        organization_id: bigint_cell(row, "organization_id"),
        spu_id: bigint_cell(row, "spu_id"),
        sku_no: string_cell(row, "sku_no"),
        variant_signature: string_cell(row, "variant_signature"),
        name: optional_string_cell(row, "name"),
        title: optional_string_cell(row, "title"),
        currency_code: string_cell(row, "currency_code"),
        price_scale: i64::from(smallint_cell(row, "price_scale")),
        sale_price_minor: bigint_cell(row, "sale_price_minor"),
        list_price_minor: optional_bigint_cell(row, "list_price_minor"),
        fulfillment_type: string_cell(row, "fulfillment_type"),
        inventory_tracking: string_cell(row, "inventory_tracking"),
        status: string_cell(row, "status"),
        sales_status: string_cell(row, "sales_status"),
        published_at: optional_timestamp_cell(row, "published_at"),
        version: bigint_cell(row, "version"),
        created_at: timestamp_cell(row, "created_at"),
        updated_at: timestamp_cell(row, "updated_at"),
        attribute_values: Vec::new(),
        metadata,
    })
}

fn map_sku_axis_row(row: &sqlx::postgres::PgRow) -> (i64, SkuAxisRecord) {
    (
        bigint_cell(row, "sku_id"),
        SkuAxisRecord {
            attribute_id: bigint_cell(row, "attribute_id"),
            attribute_value_id: bigint_cell(row, "attribute_value_id"),
            attribute_no: string_cell(row, "attribute_no"),
            value_code: string_cell(row, "value_code"),
            display_value: string_cell(row, "display_value"),
            sort_order: bigint_cell(row, "sort_order"),
        },
    )
}

/// Maps one `commerce_product_media` row.
///
/// Fallible because of the `JSONB` column: `resource_snapshot` is `NOT NULL DEFAULT '{}'`, so a
/// decode failure means the statement selected the wrong column type, and that has to surface as an
/// error rather than as an empty projection that looks like a row nobody filled in.
fn map_media_row(row: &sqlx::postgres::PgRow) -> Result<MediaRecord, CommerceServiceError> {
    let resource_snapshot: Value = row
        .try_get("resource_snapshot")
        .map_err(|error| store_error("failed to read media resource snapshot", error))?;

    Ok(MediaRecord {
        id: bigint_cell(row, "id"),
        tenant_id: bigint_cell(row, "tenant_id"),
        organization_id: bigint_cell(row, "organization_id"),
        owner_type: string_cell(row, "owner_type"),
        owner_id: bigint_cell(row, "owner_id"),
        media_role: string_cell(row, "media_role"),
        media_resource_id: bigint_cell(row, "media_resource_id"),
        resource_snapshot,
        alt_text: optional_string_cell(row, "alt_text"),
        sort_order: bigint_cell(row, "sort_order"),
        status: string_cell(row, "status"),
        version: bigint_cell(row, "version"),
        created_at: timestamp_cell(row, "created_at"),
        updated_at: timestamp_cell(row, "updated_at"),
    })
}

fn map_category_attribute_row(row: &sqlx::postgres::PgRow) -> CategoryAttributeRecord {
    CategoryAttributeRecord {
        id: bigint_cell(row, "id"),
        tenant_id: bigint_cell(row, "tenant_id"),
        organization_id: bigint_cell(row, "organization_id"),
        category_id: bigint_cell(row, "category_id"),
        attribute_id: bigint_cell(row, "attribute_id"),
        attribute_role: string_cell(row, "attribute_role"),
        source_category_id: optional_bigint_cell(row, "source_category_id"),
        required: boolean_cell(row, "is_required"),
        searchable: boolean_cell(row, "is_searchable"),
        filterable: boolean_cell(row, "is_filterable"),
        comparable: boolean_cell(row, "is_comparable"),
        sort_order: bigint_cell(row, "sort_order"),
        status: string_cell(row, "status"),
        version: bigint_cell(row, "version"),
        created_at: timestamp_cell(row, "created_at"),
        updated_at: timestamp_cell(row, "updated_at"),
    }
}

// ------------------------------------------------------------------ cells

fn string_cell(row: &sqlx::postgres::PgRow, column: &str) -> String {
    row.try_get::<String, _>(column).unwrap_or_default()
}

fn optional_string_cell(row: &sqlx::postgres::PgRow, column: &str) -> Option<String> {
    row.try_get::<Option<String>, _>(column).ok().flatten()
}

fn bigint_cell(row: &sqlx::postgres::PgRow, column: &str) -> i64 {
    row.try_get::<i64, _>(column).unwrap_or_default()
}

fn optional_bigint_cell(row: &sqlx::postgres::PgRow, column: &str) -> Option<i64> {
    row.try_get::<Option<i64>, _>(column).ok().flatten()
}

fn smallint_cell(row: &sqlx::postgres::PgRow, column: &str) -> i16 {
    row.try_get::<i16, _>(column).unwrap_or_default()
}

fn boolean_cell(row: &sqlx::postgres::PgRow, column: &str) -> bool {
    row.try_get::<bool, _>(column).unwrap_or_default()
}

/// Reads a `TIMESTAMPTZ` as a millisecond-precision RFC 3339 UTC string.
fn timestamp_cell(row: &sqlx::postgres::PgRow, column: &str) -> String {
    row.try_get::<DateTime<Utc>, _>(column)
        .map(|value| value.to_rfc3339_opts(SecondsFormat::Millis, true))
        .unwrap_or_default()
}

fn optional_timestamp_cell(row: &sqlx::postgres::PgRow, column: &str) -> Option<String> {
    row.try_get::<Option<DateTime<Utc>>, _>(column)
        .ok()
        .flatten()
        .map(|value| value.to_rfc3339_opts(SecondsFormat::Millis, true))
}

// ------------------------------------------------------------------ conversions + errors

/// Parses a wire id into the `BIGINT` the schema stores.
///
/// Ids travel as decimal strings (`API_SPEC` section 13.6) and every lookup here is by primary key,
/// so a non-numeric id is a malformed request rather than a database error.
fn parse_id(field: &str, raw: &str) -> Result<i64, CommerceServiceError> {
    raw.trim().parse::<i64>().map_err(|_| {
        CommerceServiceError::validation(format!("{field} must be a decimal int64 id, got `{raw}`"))
    })
}

fn parse_optional_id(field: &str, raw: Option<&str>) -> Result<Option<i64>, CommerceServiceError> {
    raw.map(|value| parse_id(field, value)).transpose()
}

fn parse_timestamp(field: &str, raw: &str) -> Result<DateTime<Utc>, CommerceServiceError> {
    DateTime::parse_from_rfc3339(raw.trim())
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| {
            CommerceServiceError::validation(format!(
                "{field} must be an RFC 3339 timestamp, got `{raw}`"
            ))
        })
}

/// Resolves a guarded statement that matched no row into the outcome it actually stands for.
///
/// "No row matched" has two causes and the transport has to answer them differently: the row is gone,
/// which is a `404`, or the row is live and has moved past the version the caller read, which is a
/// `412` (`API_SPEC` section 17). A single statement cannot separate them, so this asks a second
/// question by primary key and answers from the row's own state.
///
/// The follow-up read is deliberately **not** a separate resource-resolution step that a caller could
/// race: it runs on the same executor the failed write ran on, so in a transaction it sees that
/// transaction's own view. And the answer cannot be manufactured by a race either — `version` only
/// increases, so a row found here can never be one whose version equals the caller's, and a row
/// hidden here by a concurrent retirement is genuinely gone, which is exactly what the `404` says.
async fn classify_guarded_miss(
    executor: impl sqlx::Executor<'_, Database = Postgres>,
    probe_sql: &'static str,
    tenant_id: i64,
    id: i64,
    expected_version: i64,
    subject: &str,
) -> Result<StaleVersion, CommerceServiceError> {
    let current: Option<i64> = sqlx::query_scalar(probe_sql)
        .bind(tenant_id)
        .bind(id)
        .fetch_optional(executor)
        .await
        .map_err(|error| store_error("failed to resolve a guarded write", error))?;

    match current {
        Some(actual) => Ok(StaleVersion {
            expected: expected_version,
            actual,
        }),
        None => Err(CommerceServiceError::not_found(format!(
            "{subject} was not found"
        ))),
    }
}

/// Maps a driver failure onto the error kind the HTTP envelope can report honestly.
///
/// Integrity violations are mapped by SQLSTATE rather than by message text: a duplicate business key
/// is a `409`, a violated CHECK or a missing reference is a `422`, and anything else stays a `5xx`
/// without leaking schema internals to the caller.
fn store_error(context: &str, error: sqlx::Error) -> CommerceServiceError {
    tracing::error!(%error, context, "merchandise catalog statement failed");
    if let Some(database) = error.as_database_error() {
        match database.code().as_deref() {
            Some("23505") => {
                return CommerceServiceError::conflict(format!(
                    "{context}: a live record with the same business key already exists"
                ))
            }
            Some("23514") => {
                return CommerceServiceError::validation(format!(
                    "{context}: the request violates a catalog integrity rule"
                ))
            }
            Some("23503") => {
                return CommerceServiceError::validation(format!(
                    "{context}: a referenced record does not exist"
                ))
            }
            _ => {}
        }
    }
    CommerceServiceError::storage(format!("{context}: {error}"))
}

#[cfg(test)]
mod variant_signature_tests {
    //! The variant signature is the value `uk_commerce_product_sku_variant` compares, so it decides
    //! whether two SKUs are the same sellable unit. These tests pin the three properties that make it
    //! that value rather than an arbitrary string: it is built from **business** keys, it is a
    //! function of the axis *set* rather than of the order they were resolved in, and it stays inside
    //! the baseline's `char_length` bound.
    //!
    //! `tests/contract/catalog-variant-signature-closure.test.mjs` recomputes the same convention from
    //! the baseline seed and compares it with the signatures the seed stores, which is what ties this
    //! implementation to real data rather than to its own doc comment.

    use super::*;

    fn axis(attribute_no: &str, value_code: &str) -> SkuAxisRecord {
        SkuAxisRecord {
            attribute_id: 0,
            attribute_value_id: 0,
            attribute_no: attribute_no.to_owned(),
            value_code: value_code.to_owned(),
            display_value: String::new(),
            sort_order: 0,
        }
    }

    #[test]
    fn the_signature_is_business_key_terms_ordered_by_attribute_no() {
        // Deliberately not supplied in `attribute_no` order: the seed's own convention is
        // `period=annual;tier=basic`, and `period` sorts before `tier` regardless of submission order.
        let axes = [axis("tier", "basic"), axis("period", "annual")];
        assert_eq!(
            build_variant_signature("basic-annual", &axes),
            Ok("period=annual;tier=basic".to_owned())
        );
    }

    #[test]
    fn the_signature_does_not_depend_on_the_order_the_axes_arrive_in() {
        let ascending = [axis("period", "annual"), axis("tier", "basic")];
        let descending = [axis("tier", "basic"), axis("period", "annual")];
        assert_eq!(
            build_variant_signature("basic-annual", &ascending),
            build_variant_signature("basic-annual", &descending),
            "two orderings of one axis set must not produce two signatures, or the unique index admits both"
        );
    }

    #[test]
    fn a_sku_with_no_axes_falls_back_to_its_own_sku_no() {
        // The empty combination is not writable: `ck_commerce_product_sku_variant_signature` requires
        // at least one character, and `sku_no` is unique per tenant.
        assert_eq!(
            build_variant_signature("basic-annual", &[]),
            Ok("basic-annual".to_owned())
        );
    }

    #[test]
    fn a_combination_over_the_baseline_bound_is_refused() {
        let long = "x".repeat(SKU_VARIANT_SIGNATURE_MAX_CHARS);
        assert!(
            build_variant_signature("any", &[axis("period", &long)]).is_err(),
            "the baseline bounds variant_signature at {SKU_VARIANT_SIGNATURE_MAX_CHARS} characters"
        );

        let exact = "x".repeat(SKU_VARIANT_SIGNATURE_MAX_CHARS - "period=".chars().count());
        assert!(
            build_variant_signature("any", &[axis("period", &exact)]).is_ok(),
            "the bound is inclusive"
        );
    }

    #[test]
    fn the_terms_are_separated_and_carry_one_equals_sign_each() {
        // The separator and the `=` are the format a console parses and a future migration would have
        // to split on, so they are pinned rather than left to `format!` to decide.
        let signature = build_variant_signature(
            "any",
            &[
                axis("period", "annual"),
                axis("tier", "basic"),
                axis("plan", "pro"),
            ],
        )
        .expect("three short axes are within the bound");
        assert_eq!(signature, "period=annual;plan=pro;tier=basic");
        assert_eq!(signature.matches(';').count(), 2);
        assert_eq!(signature.matches('=').count(), 3);
    }
}
