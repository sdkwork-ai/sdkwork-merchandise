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
//! 2. **Money is exact integer minor units plus a scale.** The scale is *read* from
//!    `commerce_currency.minor_unit_exponent`; no layer multiplies or divides by a literal such as
//!    `/ 100`. `price_scale` is snapshotted onto the row so a historical amount stays readable if a
//!    currency's exponent is ever redefined.
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
//! * `variant_signature` falls back to `sku_no` until sales axes become an API input. The unique
//!   index still holds, but two colourways of one product cannot yet be distinguished by signature.

use std::sync::Arc;

use chrono::{DateTime, SecondsFormat, Utc};
use sdkwork_commerce_money::{Money, MoneyUnit};
use sdkwork_contract_service::{CommerceMoney, CommerceServiceError};
use sdkwork_database_id::IdGenerator;
use sdkwork_merchandise_service::{
    ArchiveSpuCommand, AttributeListQuery, AttributeRecord, AttributeRole,
    CategoryAttributeListQuery, CategoryAttributeRecord, CategoryListQuery, CategoryRecord,
    CategoryRetrieveQuery, CreateAttributeCommand, CreateCategoryAttributeCommand,
    CreateCategoryCommand, CreatePriceListCommand, CreateProductSkuCommand,
    CreateProductSpuCommand, DeleteCategoryAttributeCommand, DeleteCategoryCommand,
    DeleteProductSkuCommand, DeleteProductSpuCommand, LifecycleStatus, PriceListItemRecord,
    PriceListListQuery, PriceListRecord, ProductSkuListQuery, ProductSkuRetrieveQuery,
    ProductSpuListQuery, ProductSpuRetrieveQuery, PublishSpuCommand, SkuPriceRetrieveQuery,
    SkuRecord, SpuRecord, UpdateCategoryAttributeCommand, UpdateCategoryCommand,
    UpdatePriceListCommand, UpdateProductSkuCommand, UpdateProductSpuCommand,
};
use sqlx::{PgPool, Postgres, Row, Transaction};

// ------------------------------------------------------------------ statement fragments
//
// Macros rather than `const` items so `concat!` can splice them into literals. A `const` identifier
// would force `format!`, which `sqlx` rejects precisely because it is the shape SQL injection takes.

macro_rules! category_columns {
    () => {
        "id, tenant_id, organization_id, category_no, parent_id, path, depth, is_leaf, name, \
         sort_order, status, created_at, updated_at"
    };
}

macro_rules! attribute_columns {
    () => {
        "id, tenant_id, organization_id, attribute_no, name, value_type, status, sort_order, \
         created_at, updated_at"
    };
}

macro_rules! category_attribute_columns {
    () => {
        "id, tenant_id, organization_id, category_id, attribute_id, attribute_role, \
         source_category_id, is_required, is_searchable, is_filterable, is_comparable, \
         sort_order, status, created_at, updated_at"
    };
}

macro_rules! price_list_columns {
    () => {
        "id, tenant_id, organization_id, price_list_no, name, currency_code, market_code, status, \
         starts_at, ends_at, created_at, updated_at"
    };
}

macro_rules! spu_columns {
    () => {
        "id, tenant_id, organization_id, spu_no, category_id, name, title, subtitle, description, \
         product_type, status, sales_status, published_at, created_at, updated_at"
    };
}

macro_rules! sku_columns {
    () => {
        "id, tenant_id, organization_id, spu_id, sku_no, variant_signature, name, title, \
         currency_code, price_scale, sale_price_minor, list_price_minor, fulfillment_type, \
         inventory_tracking, status, sales_status, published_at, created_at, updated_at"
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

const REFRESH_CATEGORY_LEAF_SQL: &str = "UPDATE commerce_product_category
     SET is_leaf = NOT EXISTS (
             SELECT 1 FROM commerce_product_category child
             WHERE child.tenant_id = $1 AND child.parent_id = $2 AND child.deleted_at IS NULL
         ),
         updated_at = NOW()
     WHERE tenant_id = $1 AND id = $2";

const MOVE_CATEGORY_SUBTREE_SQL: &str = "UPDATE commerce_product_category
     SET path = $1 || substring(path FROM $2),
         depth = depth + $3,
         updated_at = NOW()
     WHERE tenant_id = $4
       AND deleted_at IS NULL
       AND left(path, $2 - 1) = $5";

const REPOINT_CATEGORY_PARENT_SQL: &str = "UPDATE commerce_product_category
     SET parent_id = $1, updated_at = NOW()
     WHERE tenant_id = $2 AND id = $3";

const UPDATE_CATEGORY_SQL: &str = concat!(
    "UPDATE commerce_product_category
     SET name = COALESCE($1::TEXT, name),
         sort_order = COALESCE($2, sort_order),
         status = COALESCE($3::TEXT, status),
         updated_at = NOW()
     WHERE tenant_id = $4 AND id = $5 AND deleted_at IS NULL
     RETURNING ",
    category_columns!()
);

const COUNT_LIVE_CHILDREN_SQL: &str = "SELECT COUNT(*) FROM commerce_product_category
     WHERE tenant_id = $1 AND parent_id = $2 AND deleted_at IS NULL";

const COUNT_LIVE_CATEGORY_PRODUCTS_SQL: &str = "SELECT COUNT(*) FROM commerce_product_spu
     WHERE tenant_id = $1 AND category_id = $2 AND deleted_at IS NULL";

const SOFT_DELETE_CATEGORY_SQL: &str = "UPDATE commerce_product_category
     SET deleted_at = NOW(), status = 'inactive', updated_at = NOW()
     WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL";

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
         updated_at = NOW()
     WHERE tenant_id = $4 AND id = $5 AND deleted_at IS NULL
     RETURNING ",
    price_list_columns!()
);

const RETRIEVE_SKU_PRICES_SQL: &str =
    "SELECT item.id, item.tenant_id, item.price_list_id, item.sku_id, item.currency_code,
            item.price_scale, item.price_minor
     FROM commerce_price_list_item item
     JOIN commerce_price_list list
       ON list.id = item.price_list_id
      AND list.tenant_id = item.tenant_id
      AND list.deleted_at IS NULL
     WHERE item.tenant_id = $1
       AND item.sku_id = $2
       AND item.deleted_at IS NULL
       AND item.status = 'active'
       AND list.status = 'active'
     ORDER BY list.priority DESC, item.min_quantity DESC, item.id ASC";

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
     WHERE tenant_id = $8 AND id = $9 AND deleted_at IS NULL
     RETURNING ",
    category_attribute_columns!()
);

const SOFT_DELETE_CATEGORY_ATTRIBUTE_SQL: &str = "UPDATE commerce_product_category_attribute
     SET deleted_at = NOW(), version = version + 1, updated_at = NOW()
     WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL";

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

const UPDATE_SPU_SQL: &str = concat!(
    "UPDATE commerce_product_spu
     SET title = COALESCE($1::TEXT, title),
         name = COALESCE($1::TEXT, name),
         subtitle = COALESCE($2::TEXT, subtitle),
         description = COALESCE($3::TEXT, description),
         category_id = COALESCE($4, category_id),
         updated_at = NOW()
     WHERE tenant_id = $5 AND id = $6 AND deleted_at IS NULL
     RETURNING ",
    spu_columns!()
);

const SOFT_DELETE_SPU_SQL: &str = "UPDATE commerce_product_spu
     SET deleted_at = NOW(), status = 'inactive', sales_status = 'inactive', updated_at = NOW()
     WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL";

const SOFT_DELETE_SPU_SKUS_SQL: &str = "UPDATE commerce_product_sku
     SET deleted_at = NOW(), status = 'inactive', sales_status = 'inactive', updated_at = NOW()
     WHERE tenant_id = $1 AND spu_id = $2 AND deleted_at IS NULL";

const COUNT_LIVE_SPU_SKUS_SQL: &str = "SELECT COUNT(*) FROM commerce_product_sku
     WHERE tenant_id = $1 AND spu_id = $2 AND deleted_at IS NULL";

const PUBLISH_SPU_SQL: &str = concat!(
    "UPDATE commerce_product_spu
     SET status = 'active',
         sales_status = 'active',
         published_at = COALESCE(published_at, NOW()),
         updated_at = NOW()
     WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL
     RETURNING ",
    spu_columns!()
);

const ARCHIVE_SPU_SQL: &str = concat!(
    "UPDATE commerce_product_spu
     SET status = 'archived', sales_status = 'inactive', updated_at = NOW()
     WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL
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
     ORDER BY id ASC
     LIMIT $5 OFFSET $6"
);

const COUNT_SKUS_SQL: &str = "SELECT COUNT(*) FROM commerce_product_sku
     WHERE tenant_id = $1
       AND deleted_at IS NULL
       AND ($2::BIGINT IS NULL OR organization_id = $2)
       AND ($3::BIGINT IS NULL OR spu_id = $3)
       AND ($4::TEXT IS NULL OR status = $4)";

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
          inventory_tracking, status, sales_status)
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, 'draft', 'inactive')
     RETURNING ",
    sku_columns!()
);

const LOCK_SKU_SQL: &str = "SELECT currency_code, price_scale, sale_price_minor, list_price_minor
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
         status = COALESCE($9::TEXT, status),
         sales_status = CASE WHEN COALESCE($9::TEXT, status) = 'active' THEN 'active'
                             ELSE 'inactive' END,
         published_at = CASE WHEN COALESCE($9::TEXT, status) = 'active'
                             THEN COALESCE(published_at, NOW())
                             ELSE published_at END,
         updated_at = NOW()
     WHERE tenant_id = $10 AND id = $11 AND deleted_at IS NULL
     RETURNING ",
    sku_columns!()
);

const SOFT_DELETE_SKU_SQL: &str = "UPDATE commerce_product_sku
     SET deleted_at = NOW(), status = 'inactive', sales_status = 'inactive', updated_at = NOW()
     WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL";

const SELECT_CURRENCY_SQL: &str = "SELECT minor_unit_exponent, rounding_mode FROM commerce_currency
     WHERE code = $1 AND status = 'active'";

const CURRENCY_EXISTS_SQL: &str = "SELECT EXISTS (SELECT 1 FROM commerce_currency WHERE code = $1)";

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
        let raw = self.ids.next_id().map_err(|error| {
            CommerceServiceError::storage(format!("id generation failed: {error}"))
        })?;
        parse_id("generated id", &raw)
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
    ) -> Result<CategoryRecord, CommerceServiceError> {
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
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|error| store_error("failed to update category", error))?;

        let row = row.ok_or_else(|| CommerceServiceError::not_found("category was not found"))?;

        transaction
            .commit()
            .await
            .map_err(|error| store_error("failed to commit category update", error))?;

        Ok(map_category_row(&row))
    }

    /// Retires a category.
    ///
    /// Refuses while live children or live products still point at it: soft-deleting a node with
    /// children would leave the tree with an unreachable middle, and the `path` index would keep
    /// matching descendants whose ancestor is invisible.
    pub async fn delete_category(
        &self,
        command: &DeleteCategoryCommand,
    ) -> Result<(), CommerceServiceError> {
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

        sqlx::query(SOFT_DELETE_CATEGORY_SQL)
            .bind(tenant_id)
            .bind(id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error("failed to delete category", error))?;

        if let Some(parent) = current.parent_id {
            refresh_leaf_state(&mut transaction, tenant_id, parent).await?;
        }

        transaction
            .commit()
            .await
            .map_err(|error| store_error("failed to commit category deletion", error))?;

        Ok(())
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
    ) -> Result<PriceListRecord, CommerceServiceError> {
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
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("failed to update price list", error))?;

        let row = row.ok_or_else(|| CommerceServiceError::not_found("price list was not found"))?;
        Ok(map_price_list_row(&row))
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
    ) -> Result<CategoryAttributeRecord, CommerceServiceError> {
        let row = sqlx::query(UPDATE_CATEGORY_ATTRIBUTE_SQL)
            .bind(command.role.map(AttributeRole::as_storage_str))
            .bind(command.required)
            .bind(command.searchable)
            .bind(command.filterable)
            .bind(command.comparable)
            .bind(command.sort_order)
            .bind(command.status.map(LifecycleStatus::as_storage_str))
            .bind(parse_id("tenant_id", &command.tenant_id)?)
            .bind(parse_id("binding_id", &command.binding_id)?)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("failed to update category attribute", error))?;

        let row =
            row.ok_or_else(|| CommerceServiceError::not_found("category attribute was not found"))?;
        Ok(map_category_attribute_row(&row))
    }

    pub async fn delete_category_attribute(
        &self,
        command: &DeleteCategoryAttributeCommand,
    ) -> Result<(), CommerceServiceError> {
        let result = sqlx::query(SOFT_DELETE_CATEGORY_ATTRIBUTE_SQL)
            .bind(parse_id("tenant_id", &command.tenant_id)?)
            .bind(parse_id("binding_id", &command.binding_id)?)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("failed to delete category attribute", error))?;

        if result.rows_affected() == 0 {
            return Err(CommerceServiceError::not_found(
                "category attribute was not found",
            ));
        }

        Ok(())
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
    ) -> Result<SpuRecord, CommerceServiceError> {
        let tenant_id = parse_id("tenant_id", &command.tenant_id)?;
        let id = parse_id("spu_id", &command.spu_id)?;

        let row = sqlx::query(UPDATE_SPU_SQL)
            .bind(command.title.as_deref())
            .bind(command.subtitle.as_deref())
            .bind(command.description.as_deref())
            .bind(parse_optional_id(
                "category_id",
                command.category_id.as_deref(),
            )?)
            .bind(tenant_id)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("failed to update spu", error))?;

        let row = row.ok_or_else(|| CommerceServiceError::not_found("product was not found"))?;
        Ok(map_spu_row(&row))
    }

    /// Retires a SPU and every SKU under it.
    ///
    /// The SKUs are retired in the same transaction: leaving them live would keep rows in the
    /// sellable index whose parent product is invisible.
    pub async fn delete_spu(
        &self,
        command: &DeleteProductSpuCommand,
    ) -> Result<(), CommerceServiceError> {
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
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error("failed to delete spu", error))?
            .rows_affected();

        if affected == 0 {
            return Err(CommerceServiceError::not_found("product was not found"));
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

        Ok(())
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
    ) -> Result<SpuRecord, CommerceServiceError> {
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
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|error| store_error("failed to publish spu", error))?;

        let row = row.ok_or_else(|| CommerceServiceError::not_found("product was not found"))?;

        transaction
            .commit()
            .await
            .map_err(|error| store_error("failed to commit spu publication", error))?;

        Ok(map_spu_row(&row))
    }

    /// Archives a SPU and pulls it from every sellable index.
    pub async fn archive_spu(
        &self,
        command: &ArchiveSpuCommand,
    ) -> Result<SpuRecord, CommerceServiceError> {
        let row = sqlx::query(ARCHIVE_SPU_SQL)
            .bind(parse_id("tenant_id", &command.tenant_id)?)
            .bind(parse_id("spu_id", &command.spu_id)?)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("failed to archive spu", error))?;

        let row = row.ok_or_else(|| CommerceServiceError::not_found("product was not found"))?;
        Ok(map_spu_row(&row))
    }

    // ---------------------------------------------------------------- SKUs

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
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("failed to list skus", error))?;

        Ok(rows.iter().map(map_sku_row).collect())
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
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("failed to count skus", error))
    }

    pub async fn retrieve_sku(
        &self,
        query: &ProductSkuRetrieveQuery,
    ) -> Result<Option<SkuRecord>, CommerceServiceError> {
        let row = sqlx::query(RETRIEVE_SKU_SQL)
            .bind(parse_id("tenant_id", &query.tenant_id)?)
            .bind(parse_id("sku_id", &query.sku_id)?)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("failed to retrieve sku", error))?;

        Ok(row.as_ref().map(map_sku_row))
    }

    /// Creates a SKU, resolving the currency's scale and storing exact minor units.
    pub async fn create_sku(
        &self,
        command: &CreateProductSkuCommand,
    ) -> Result<SkuRecord, CommerceServiceError> {
        let tenant_id = parse_id("tenant_id", &command.tenant_id)?;
        let organization_id = parse_id("organization_id", &command.organization_id)?;
        let spu_id = parse_id("spu_id", &command.spu_id)?;
        let id = self.next_id()?;

        let currency = resolve_currency(&self.pool, &command.currency_code).await?;
        let sale_price_minor = to_minor(&command.price_amount, &currency, "price_amount")?;
        let list_price_minor = command
            .original_price_amount
            .as_ref()
            .map(|amount| to_minor(amount, &currency, "original_price_amount"))
            .transpose()?;
        ensure_sale_not_above_list(sale_price_minor, list_price_minor)?;

        // `variant_signature` is the sales-axis combination of this SKU. Sales axes are not an API
        // input yet, so the SKU number stands in: it is stable, unique per tenant, and keeps
        // `uk_commerce_product_sku_variant` meaningful instead of collapsing every SKU of a SPU onto
        // one signature.
        let variant_signature = command.sku_no.clone();

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
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("failed to create sku", error))?;

        Ok(map_sku_row(&row))
    }

    /// Updates a SKU.
    ///
    /// A currency change must restate the affected amounts. Reinterpreting an existing minor amount
    /// under a new scale would silently turn 64000 CNY cents into 64000 JPY yen; requiring the caller
    /// to state the price in the new currency keeps the write explicit.
    pub async fn update_sku(
        &self,
        command: &UpdateProductSkuCommand,
    ) -> Result<SkuRecord, CommerceServiceError> {
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

        let effective_currency = command
            .currency_code
            .clone()
            .unwrap_or_else(|| current_currency.clone());
        let currency_changed = effective_currency != current_currency;
        if currency_changed {
            if command.price_amount.is_none() {
                return Err(CommerceServiceError::validation(
                    "changing currency_code requires price_amount to be restated in the new currency",
                ));
            }
            if current_list.is_some() && command.original_price_amount.is_none() {
                return Err(CommerceServiceError::validation(
                    "changing currency_code requires original_price_amount to be restated or cleared",
                ));
            }
        }

        let currency = resolve_currency(&self.pool, &effective_currency).await?;
        let sale_price_minor = match &command.price_amount {
            Some(amount) => to_minor(amount, &currency, "price_amount")?,
            None => current_sale,
        };
        let list_price_minor = match &command.original_price_amount {
            Some(amount) => Some(to_minor(amount, &currency, "original_price_amount")?),
            None if currency_changed => None,
            None => current_list,
        };
        ensure_sale_not_above_list(sale_price_minor, list_price_minor)?;

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
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|error| store_error("failed to update sku", error))?;

        let row = row.ok_or_else(|| CommerceServiceError::not_found("sku was not found"))?;

        transaction
            .commit()
            .await
            .map_err(|error| store_error("failed to commit sku update", error))?;

        Ok(map_sku_row(&row))
    }

    pub async fn delete_sku(
        &self,
        command: &DeleteProductSkuCommand,
    ) -> Result<(), CommerceServiceError> {
        let affected = sqlx::query(SOFT_DELETE_SKU_SQL)
            .bind(parse_id("tenant_id", &command.tenant_id)?)
            .bind(parse_id("sku_id", &command.sku_id)?)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("failed to delete sku", error))?
            .rows_affected();

        if affected == 0 {
            return Err(CommerceServiceError::not_found("sku was not found"));
        }
        Ok(())
    }

    /// Lists the price-list overrides that apply to one SKU.
    pub async fn retrieve_sku_prices(
        &self,
        query: &SkuPriceRetrieveQuery,
    ) -> Result<Vec<PriceListItemRecord>, CommerceServiceError> {
        let rows = sqlx::query(RETRIEVE_SKU_PRICES_SQL)
            .bind(parse_id("tenant_id", &query.tenant_id)?)
            .bind(parse_id("sku_id", &query.sku_id)?)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("failed to retrieve sku prices", error))?;

        Ok(rows.iter().map(map_price_list_item_row).collect())
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

/// A currency row's scale, as both a `MoneyUnit` and the `SMALLINT` stored on each money row.
struct ResolvedCurrency {
    unit: MoneyUnit,
    scale: i16,
}

/// Reads the money scale and rounding of one active currency.
///
/// The exponent is read rather than assumed, which is the whole point of keeping currency data in a
/// table instead of in code (`DATABASE_SPEC` section 14 / DB095).
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
    let unit = MoneyUnit::from_registry(code, scale_u8, &rounding).map_err(|error| {
        CommerceServiceError::validation(format!("currency `{code}` is unusable: {error}"))
    })?;

    Ok(ResolvedCurrency { unit, scale })
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

/// Converts a major-denomination amount into exact minor units at the currency's scale.
///
/// Excess fractional digits are rejected rather than rounded: silently changing an amount on the
/// write path is how a storefront ends up charging a price nobody entered.
fn to_minor(
    amount: &CommerceMoney,
    currency: &ResolvedCurrency,
    field: &str,
) -> Result<i64, CommerceServiceError> {
    let parsed = Money::parse(amount.as_str(), currency.unit)
        .map_err(|error| CommerceServiceError::validation(format!("{field}: {error}")))?;
    i64::try_from(parsed.minor()).map_err(|_| {
        CommerceServiceError::validation(format!("{field} does not fit a 64-bit minor amount"))
    })
}

/// Enforces `ck_commerce_product_sku_sale_not_above_list` before the row reaches the database.
fn ensure_sale_not_above_list(
    sale_price_minor: i64,
    list_price_minor: Option<i64>,
) -> Result<(), CommerceServiceError> {
    if let Some(list) = list_price_minor {
        if sale_price_minor > list {
            return Err(CommerceServiceError::validation(
                "price_amount must not exceed original_price_amount",
            ));
        }
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
        created_at: timestamp_cell(row, "created_at"),
        updated_at: timestamp_cell(row, "updated_at"),
    }
}

fn map_price_list_item_row(row: &sqlx::postgres::PgRow) -> PriceListItemRecord {
    PriceListItemRecord {
        id: bigint_cell(row, "id"),
        tenant_id: bigint_cell(row, "tenant_id"),
        price_list_id: bigint_cell(row, "price_list_id"),
        sku_id: bigint_cell(row, "sku_id"),
        currency_code: string_cell(row, "currency_code"),
        price_scale: i64::from(smallint_cell(row, "price_scale")),
        price_minor: bigint_cell(row, "price_minor"),
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
        created_at: timestamp_cell(row, "created_at"),
        updated_at: timestamp_cell(row, "updated_at"),
    }
}

pub(crate) fn map_sku_row(row: &sqlx::postgres::PgRow) -> SkuRecord {
    SkuRecord {
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
        created_at: timestamp_cell(row, "created_at"),
        updated_at: timestamp_cell(row, "updated_at"),
    }
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

/// Declares one of the four unimplemented `category_attributes` operations.
///
/// The routes are published but have no implementation; see `TECH_ARCHITECTURE.md` section 9. The
/// helper exists so the gap is declared in one place with a typed error instead of four copies of a
/// message.
pub fn unimplemented_operation(operation: &str) -> CommerceServiceError {
    CommerceServiceError::unsupported_capability(format!(
        "{operation} is not implemented for the postgres catalog store yet"
    ))
}
