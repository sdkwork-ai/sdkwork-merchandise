//! The catalog repository port and its read models.
//!
//! # This crate owns the port; nothing here opens a connection
//!
//! `CatalogRepositoryPort` is declared here because this crate owns the catalog contract — the
//! same crate that owns the commands, the queries, and the response-facing vocabulary. The
//! implementation lives in `sdkwork-merchandise-repository-sqlx` and is bound here, in the crate
//! that names the trait, rather than in the crate that happens to hold the pool. The composition
//! root resolves `sdkwork_merchandise_service::CatalogRepositoryPort` and injects an
//! implementation downward; the HTTP layer names only the trait.
//!
//! # Why the signatures look the way they do
//!
//! Every method returns [`CommerceCatalogFuture`]. Persistence is I/O, and the port is consumed as
//! `Arc<dyn CatalogRepositoryPort>` from a router handler, so the trait must be dyn-compatible:
//! a native `async fn` in a trait is not, and this workspace carries no `async-trait` dependency.
//! Boxing the future is what buys both object safety and the ability to await inside.
//!
//! Records are the read models defined below rather than driver rows: the mapper that builds them
//! is the only place that knows a column name, which is what keeps SQL out of the adapters.

use std::future::Future;
use std::pin::Pin;

use crate::{commands::*, queries::*};
use sdkwork_contract_service::CommerceServiceError;

pub const CATALOG_REPOSITORY_PORT: &str = "catalog.repository";
pub const IDEMPOTENCY_REPOSITORY_PORT: &str = "idempotency.repository";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CatalogRepositoryCommand {
    CreateCategory,
    UpdateCategory,
    DeleteCategory,
    CreateAttribute,
    BindCategoryAttribute,
    UpdateCategoryAttribute,
    DeleteCategoryAttribute,
    CreateSpu,
    UpdateSpu,
    DeleteSpu,
    PublishSpu,
    ArchiveSpu,
    CreateSku,
    UpdateSku,
    DeleteSku,
    CreatePriceList,
    UpdatePriceList,
    CreateMedia,
    UpdateMedia,
    DeleteMedia,
}

pub struct CatalogPortRequirement;

impl CatalogPortRequirement {
    /// Every write the catalog repository port is required to support.
    ///
    /// This list is the port's completeness contract: a host that wires only a
    /// subset of these commands cannot serve the published route set.
    pub fn standard_commands() -> Vec<CatalogRepositoryCommand> {
        vec![
            CatalogRepositoryCommand::CreateCategory,
            CatalogRepositoryCommand::UpdateCategory,
            CatalogRepositoryCommand::DeleteCategory,
            CatalogRepositoryCommand::CreateAttribute,
            CatalogRepositoryCommand::BindCategoryAttribute,
            CatalogRepositoryCommand::UpdateCategoryAttribute,
            CatalogRepositoryCommand::DeleteCategoryAttribute,
            CatalogRepositoryCommand::CreateSpu,
            CatalogRepositoryCommand::UpdateSpu,
            CatalogRepositoryCommand::DeleteSpu,
            CatalogRepositoryCommand::PublishSpu,
            CatalogRepositoryCommand::ArchiveSpu,
            CatalogRepositoryCommand::CreateSku,
            CatalogRepositoryCommand::UpdateSku,
            CatalogRepositoryCommand::DeleteSku,
            CatalogRepositoryCommand::CreatePriceList,
            CatalogRepositoryCommand::UpdatePriceList,
            CatalogRepositoryCommand::CreateMedia,
            CatalogRepositoryCommand::UpdateMedia,
            CatalogRepositoryCommand::DeleteMedia,
        ]
    }
}

/// Read model of one `commerce_product_category` row.
///
/// Nullable columns are `Option`, non-nullable columns are not: the read model mirrors the schema
/// rather than guessing, so a caller cannot mistake an absent value for an empty one.
///
/// `path` is self-inclusive — `/` for a root, `/1000/` for its child, `/1000/1010/` for a
/// grandchild — which is what makes a subtree scan a single `path LIKE '/1000/%'` instead of a
/// recursive walk. `depth` is the number of ancestors, so a root has depth 0.
#[derive(Clone, Debug)]
pub struct CategoryRecord {
    pub id: i64,
    pub tenant_id: i64,
    pub organization_id: i64,
    pub category_no: String,
    pub parent_id: Option<i64>,
    pub path: String,
    pub depth: i64,
    pub is_leaf: bool,
    pub name: String,
    pub sort_order: i64,
    pub status: String,
    /// The row's optimistic-concurrency version.
    ///
    /// One column carries both directions of the same fact. On a read it is the version the caller
    /// must echo back as `If-Match` on its next write; on a write it is the version the row is at
    /// *after* the write, which is what to echo back next. Because the baseline advances one column
    /// and the precondition compares that same column, there is exactly one notion of "how old is
    /// this copy", and a caller cannot hold a version of one thing while editing another.
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// Read model of one `commerce_product_attribute` row.
///
/// There is no `scope`: the baseline decides what an attribute *means* in a category through
/// `commerce_product_category_attribute.attribute_role`, because the same attribute is a sales axis
/// in one category and a plain parameter in another.
#[derive(Clone, Debug)]
pub struct AttributeRecord {
    pub id: i64,
    pub tenant_id: i64,
    pub organization_id: i64,
    pub attribute_no: String,
    pub name: String,
    pub value_type: String,
    pub status: String,
    pub sort_order: i64,
    /// The row's optimistic-concurrency version.
    ///
    /// One column carries both directions of the same fact. On a read it is the version the caller
    /// must echo back as `If-Match` on its next write; on a write it is the version the row is at
    /// *after* the write, which is what to echo back next. Because the baseline advances one column
    /// and the precondition compares that same column, there is exactly one notion of "how old is
    /// this copy", and a caller cannot hold a version of one thing while editing another.
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug)]
pub struct AttributeValueRecord {
    pub id: i64,
    pub tenant_id: i64,
    pub attribute_id: i64,
    pub value_code: String,
    pub display_value: String,
    pub sort_order: i64,
    pub status: String,
}

/// Read model of one `commerce_product_spu` row.
///
/// There is no `visible_surfaces` column in the baseline: which surface may display a product is a
/// presentation decision, not product master data.
///
/// `sales_status` is derived from `status` by the repository on every write
/// (`active` if and only if `status = 'active'`), so it is never set independently.
#[derive(Clone, Debug)]
pub struct SpuRecord {
    pub id: i64,
    pub tenant_id: i64,
    pub organization_id: i64,
    pub spu_no: String,
    pub category_id: i64,
    pub name: String,
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub description: Option<String>,
    pub product_type: String,
    pub status: String,
    pub sales_status: String,
    pub published_at: Option<String>,
    /// The row's optimistic-concurrency version.
    ///
    /// One column carries both directions of the same fact. On a read it is the version the caller
    /// must echo back as `If-Match` on its next write; on a write it is the version the row is at
    /// *after* the write, which is what to echo back next. Because the baseline advances one column
    /// and the precondition compares that same column, there is exactly one notion of "how old is
    /// this copy", and a caller cannot hold a version of one thing while editing another.
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// Read model of one `commerce_product_sku` row.
///
/// Money is exposed as exact integer minor units plus the scale snapshotted at write time. There is
/// deliberately no major-unit string here: a bare `"640.00"` cannot say whether it means 64000 or
/// 640000, which is the ambiguity `price_scale` exists to remove.
///
/// `variant_signature` is the deterministic signature of the SKU's sales-axis combination, built by
/// the repository from `commerce_product_sku_attribute`. A SKU whose category declares no sales axis
/// has the empty combination, and falls back to `sku_no` — still satisfying
/// `uk_commerce_product_sku_variant`, without pretending an axis exists.
///
/// `attribute_values` is the same combination spelled out as rows. It is carried on the record
/// rather than fetched by a second call because a variant that cannot be read back is a variant no
/// console can render: the write path accepts value ids, so the read path has to return them.
#[derive(Clone, Debug)]
pub struct SkuRecord {
    pub id: i64,
    pub tenant_id: i64,
    pub organization_id: i64,
    pub spu_id: i64,
    pub sku_no: String,
    pub variant_signature: String,
    pub name: Option<String>,
    pub title: Option<String>,
    pub currency_code: String,
    pub price_scale: i64,
    pub sale_price_minor: i64,
    pub list_price_minor: Option<i64>,
    pub fulfillment_type: String,
    pub inventory_tracking: String,
    pub status: String,
    pub sales_status: String,
    pub published_at: Option<String>,
    /// The row's optimistic-concurrency version.
    ///
    /// One column carries both directions of the same fact. On a read it is the version the caller
    /// must echo back as `If-Match` on its next write; on a write it is the version the row is at
    /// *after* the write, which is what to echo back next. Because the baseline advances one column
    /// and the precondition compares that same column, there is exactly one notion of "how old is
    /// this copy", and a caller cannot hold a version of one thing while editing another.
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
    pub attribute_values: Vec<SkuAxisRecord>,
    /// Capability-owned metadata, carried verbatim from whoever owns the capability.
    ///
    /// `commerce_product_sku.metadata` is `NOT NULL DEFAULT '{}'`, so a SKU that declares none reads
    /// back as the empty object rather than `null`. See [`CreateProductSkuCommand::metadata`] for what
    /// this column is for and, more importantly, what it is not for.
    ///
    /// [`CreateProductSkuCommand::metadata`]: crate::commands::CreateProductSkuCommand::metadata
    pub metadata: serde_json::Value,
}

#[derive(Clone, Debug)]
pub struct PriceListRecord {
    pub id: i64,
    pub tenant_id: i64,
    pub organization_id: i64,
    pub price_list_no: String,
    pub name: String,
    pub currency_code: String,
    pub market_code: Option<String>,
    pub status: String,
    pub starts_at: Option<String>,
    pub ends_at: Option<String>,
    /// The row's optimistic-concurrency version.
    ///
    /// One column carries both directions of the same fact. On a read it is the version the caller
    /// must echo back as `If-Match` on its next write; on a write it is the version the row is at
    /// *after* the write, which is what to echo back next. Because the baseline advances one column
    /// and the precondition compares that same column, there is exactly one notion of "how old is
    /// this copy", and a caller cannot hold a version of one thing while editing another.
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// Read model of one `commerce_product_category_attribute` row.
///
/// `attribute_role` is the parameter/sales/key split and `source_category_id` records inheritance
/// from another category, which is why one attribute can be a sales axis here and a specification
/// there. `comparable` means the attribute's values are meaningfully compared across products
/// (Magento's `is_comparable`), as opposed to being free text.
#[derive(Clone, Debug)]
pub struct CategoryAttributeRecord {
    pub id: i64,
    pub tenant_id: i64,
    pub organization_id: i64,
    pub category_id: i64,
    pub attribute_id: i64,
    pub attribute_role: String,
    pub source_category_id: Option<i64>,
    pub required: bool,
    pub searchable: bool,
    pub filterable: bool,
    pub comparable: bool,
    pub sort_order: i64,
    pub status: String,
    /// The row's optimistic-concurrency version.
    ///
    /// One column carries both directions of the same fact. On a read it is the version the caller
    /// must echo back as `If-Match` on its next write; on a write it is the version the row is at
    /// *after* the write, which is what to echo back next. Because the baseline advances one column
    /// and the precondition compares that same column, there is exactly one notion of "how old is
    /// this copy", and a caller cannot hold a version of one thing while editing another.
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
}

pub type CommerceCatalogFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, CommerceServiceError>> + Send + 'a>>;

/// The outcome of a write that carries an `If-Match` precondition.
///
/// # Why the staleness lives on the success channel rather than in the error
///
/// A guarded write has three outcomes and the transport has to answer all three differently
/// (`API_SPEC` section 17): the caller's version matched and the row is written (`200`/`204`), the
/// row is live but carries a version the caller did not read (`412`), or there is no such live row
/// (`404`). The third is already an error and stays one. The second is not an error in the sense
/// the error type expresses — nothing failed, the caller simply read a copy that has since moved —
/// and it is deliberately *not* reported as
/// [`CommerceServiceError::conflict`](sdkwork_contract_service::CommerceServiceError::conflict),
/// because `409` already means something else on these same operations: a duplicate business key
/// the caller can rename. Reporting both as `409` would erase the distinction the contract
/// publishes, and having the transport recover it by matching the message text would be exactly the
/// classification-by-string the shared contract type forbids.
///
/// The repository is the only layer that can separate the second outcome from the third honestly:
/// it re-reads the row inside the same transaction that failed to write it, so the two are
/// distinguished by *state* rather than by an error code or a message.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StaleVersion {
    /// The version the request's `If-Match` named.
    pub expected: i64,
    /// The version the row actually carries now.
    pub actual: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GuardedWrite<T> {
    /// The statement matched on the caller's version; the row now carries `expected_version + 1`.
    Applied(T),
    /// The row is live but has moved past the version the caller read, so nothing was written.
    StaleVersion(StaleVersion),
}

/// Read model of one `commerce_product_media` row.
///
/// # Why the snapshot is an opaque `Value` and the reference is not
///
/// `media_resource_id` is a first-class `i64` because it is the stable identity
/// `MEDIA_RESOURCE_SPEC` section 5 requires a business table to store, and because
/// `commerce_product_media.media_resource_id` is `BIGINT NOT NULL`. `resource_snapshot` is the
/// column's `JSONB` projection, and it stays an opaque document here for the same reason: it is a
/// read-model cache of descriptive fields owned by Drive, so a Rust struct would be a second,
/// silently diverging definition of `MediaResource` next to the one the authored contract publishes.
/// The contract is the definition; this is the carrier.
///
/// Money, storage object keys, and presigned URLs are deliberately absent: `commerce_product_media`
/// has no such column, and `url` on the wire is documented as a delivery hint inside the snapshot
/// rather than as an identity.
#[derive(Clone, Debug)]
pub struct MediaRecord {
    pub id: i64,
    pub tenant_id: i64,
    pub organization_id: i64,
    pub owner_type: String,
    pub owner_id: i64,
    pub media_role: String,
    pub media_resource_id: i64,
    pub resource_snapshot: serde_json::Value,
    pub alt_text: Option<String>,
    pub sort_order: i64,
    pub status: String,
    /// The row's optimistic-concurrency version.
    ///
    /// One column carries both directions of the same fact. On a read it is the version the caller
    /// must echo back as `If-Match` on its next write; on a write it is the version the row is at
    /// *after* the write, which is what to echo back next. Because the baseline advances one column
    /// and the precondition compares that same column, there is exactly one notion of "how old is
    /// this copy", and a caller cannot hold a version of one thing while editing another.
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// Read model of one `commerce_product_sku_attribute` row.
///
/// Both ids are carried, not just the value id, because the pair *is* the axis: a caller reading a
/// variant back needs to know which axis the value sits on, and re-deriving that from the value id
/// would be a second query for information this row already has.
///
/// `attribute_no` and `value_code` are the business keys the signature is written in. They are read,
/// never written: the submitted value id is the only input to an axis write.
#[derive(Clone, Debug)]
pub struct SkuAxisRecord {
    pub attribute_id: i64,
    pub attribute_value_id: i64,
    pub attribute_no: String,
    pub value_code: String,
    pub display_value: String,
    pub sort_order: i64,
}

/// One offset page of a catalog collection.
///
/// `page` and `page_size` are echoed back rather than returned as the caller sent them, because the
/// port applies the defaults (page 1, 20 rows) when the caller omits them. Applying them here — in
/// the crate that owns the port — is what keeps every list surface paginating identically instead
/// of each route choosing its own fallback. `total_items` is the count over the whole filtered set,
/// not the length of `items`.
#[derive(Debug)]
pub struct CatalogOffsetPage<T> {
    pub items: Vec<T>,
    pub page: i64,
    pub page_size: i64,
    pub total_items: i64,
}

impl<T> CatalogOffsetPage<T> {
    /// Fills in the port's own pagination defaults.
    ///
    /// Public because the implementation lives in a repository crate and must be able to build the
    /// value; the defaults are still decided here so no implementation can invent a different one.
    pub fn new(items: Vec<T>, page: Option<i64>, page_size: Option<i64>, total_items: i64) -> Self {
        Self {
            items,
            page: page.unwrap_or(1),
            page_size: page_size.unwrap_or(20),
            total_items,
        }
    }
}

/// What the catalog surface requires from persistence.
///
/// Two properties of this declaration are load bearing, and both were absent before:
///
/// * **It is asynchronous.** Persistence is I/O, so a synchronous port is not a stricter contract —
///   it is an unsatisfiable one. The sqlx store could never have implemented the previous
///   signatures, and did not.
/// * **It is declared, and satisfied, under the name the composition specification advertises.**
///   `specs/component.spec.json` publishes `sdkwork_merchandise_service::CatalogRepositoryPort` as a
///   provided port; `specs/../sdkwork-merchandise-repository-sqlx` binds
///   `impl CatalogRepositoryPort for PostgresCommerceCatalogStore`. A declared port with no
///   implementation is worse than an absent one, because composition resolves successfully against
///   a contract that no code can honour.
///
/// `retrieve_*` answers `Option<Record>` rather than an error for a missing row: absence is an
/// ordinary outcome the route answers with `404`, not a failure of the read model. `list_*` and
/// `list_*_page` are both on the port because they are genuinely different questions — one is
/// "give me the rows", the other is "give me a bounded window plus the size of the whole set" — and
/// the second cannot be derived from the first. Every method that mutates an existing row answers
/// [`GuardedWrite`] instead of the bare record, because such a write has a third outcome the bare
/// record cannot express; the thirteen methods are exactly the non-`create_` mutators, and
/// `create_*` is deliberately outside the set: there is no prior version to precondition on.
pub trait CatalogRepositoryPort: Send + Sync {
    fn list_categories<'a>(
        &'a self,
        query: CategoryListQuery,
    ) -> CommerceCatalogFuture<'a, Vec<CategoryRecord>>;

    fn list_categories_page<'a>(
        &'a self,
        query: CategoryListQuery,
    ) -> CommerceCatalogFuture<'a, CatalogOffsetPage<CategoryRecord>>;

    fn retrieve_category<'a>(
        &'a self,
        query: CategoryRetrieveQuery,
    ) -> CommerceCatalogFuture<'a, Option<CategoryRecord>>;

    fn create_category<'a>(
        &'a self,
        command: CreateCategoryCommand,
    ) -> CommerceCatalogFuture<'a, CategoryRecord>;

    fn update_category<'a>(
        &'a self,
        command: UpdateCategoryCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<CategoryRecord>>;

    fn delete_category<'a>(
        &'a self,
        command: DeleteCategoryCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<()>>;

    fn list_attributes<'a>(
        &'a self,
        query: AttributeListQuery,
    ) -> CommerceCatalogFuture<'a, Vec<AttributeRecord>>;

    fn list_attributes_page<'a>(
        &'a self,
        query: AttributeListQuery,
    ) -> CommerceCatalogFuture<'a, CatalogOffsetPage<AttributeRecord>>;

    fn create_attribute<'a>(
        &'a self,
        command: CreateAttributeCommand,
    ) -> CommerceCatalogFuture<'a, AttributeRecord>;

    fn list_price_lists<'a>(
        &'a self,
        query: PriceListListQuery,
    ) -> CommerceCatalogFuture<'a, Vec<PriceListRecord>>;

    fn list_price_lists_page<'a>(
        &'a self,
        query: PriceListListQuery,
    ) -> CommerceCatalogFuture<'a, CatalogOffsetPage<PriceListRecord>>;

    fn create_price_list<'a>(
        &'a self,
        command: CreatePriceListCommand,
    ) -> CommerceCatalogFuture<'a, PriceListRecord>;

    fn update_price_list<'a>(
        &'a self,
        command: UpdatePriceListCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<PriceListRecord>>;

    fn list_category_attributes<'a>(
        &'a self,
        query: CategoryAttributeListQuery,
    ) -> CommerceCatalogFuture<'a, Vec<CategoryAttributeRecord>>;

    fn list_category_attributes_page<'a>(
        &'a self,
        query: CategoryAttributeListQuery,
    ) -> CommerceCatalogFuture<'a, CatalogOffsetPage<CategoryAttributeRecord>>;

    fn create_category_attribute<'a>(
        &'a self,
        command: CreateCategoryAttributeCommand,
    ) -> CommerceCatalogFuture<'a, CategoryAttributeRecord>;

    fn update_category_attribute<'a>(
        &'a self,
        command: UpdateCategoryAttributeCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<CategoryAttributeRecord>>;

    fn delete_category_attribute<'a>(
        &'a self,
        command: DeleteCategoryAttributeCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<()>>;

    fn list_spus<'a>(
        &'a self,
        query: ProductSpuListQuery,
    ) -> CommerceCatalogFuture<'a, Vec<SpuRecord>>;

    fn list_spus_page<'a>(
        &'a self,
        query: ProductSpuListQuery,
    ) -> CommerceCatalogFuture<'a, CatalogOffsetPage<SpuRecord>>;

    fn retrieve_spu<'a>(
        &'a self,
        query: ProductSpuRetrieveQuery,
    ) -> CommerceCatalogFuture<'a, Option<SpuRecord>>;

    fn create_spu<'a>(
        &'a self,
        command: CreateProductSpuCommand,
    ) -> CommerceCatalogFuture<'a, SpuRecord>;

    fn update_spu<'a>(
        &'a self,
        command: UpdateProductSpuCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<SpuRecord>>;

    fn publish_spu<'a>(
        &'a self,
        command: PublishSpuCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<SpuRecord>>;

    fn archive_spu<'a>(
        &'a self,
        command: ArchiveSpuCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<SpuRecord>>;

    fn delete_spu<'a>(
        &'a self,
        command: DeleteProductSpuCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<()>>;

    fn list_skus<'a>(
        &'a self,
        query: ProductSkuListQuery,
    ) -> CommerceCatalogFuture<'a, Vec<SkuRecord>>;

    fn list_skus_page<'a>(
        &'a self,
        query: ProductSkuListQuery,
    ) -> CommerceCatalogFuture<'a, CatalogOffsetPage<SkuRecord>>;

    fn retrieve_sku<'a>(
        &'a self,
        query: ProductSkuRetrieveQuery,
    ) -> CommerceCatalogFuture<'a, Option<SkuRecord>>;

    fn create_sku<'a>(
        &'a self,
        command: CreateProductSkuCommand,
    ) -> CommerceCatalogFuture<'a, SkuRecord>;

    fn update_sku<'a>(
        &'a self,
        command: UpdateProductSkuCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<SkuRecord>>;

    fn delete_sku<'a>(
        &'a self,
        command: DeleteProductSkuCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<()>>;

    fn list_media<'a>(
        &'a self,
        query: MediaListQuery,
    ) -> CommerceCatalogFuture<'a, Vec<MediaRecord>>;

    fn list_media_page<'a>(
        &'a self,
        query: MediaListQuery,
    ) -> CommerceCatalogFuture<'a, CatalogOffsetPage<MediaRecord>>;

    fn create_media<'a>(
        &'a self,
        command: CreateMediaCommand,
    ) -> CommerceCatalogFuture<'a, MediaRecord>;

    fn update_media<'a>(
        &'a self,
        command: UpdateMediaCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<MediaRecord>>;

    fn delete_media<'a>(
        &'a self,
        command: DeleteMediaCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<()>>;
}
