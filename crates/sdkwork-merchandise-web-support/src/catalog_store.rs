//! Merchandise HTTP DTOs, response mappers, and the router's plug point.
//!
//! # This crate does not own the store port
//!
//! `CatalogRepositoryPort` belongs to `sdkwork-merchandise-service`, which declares it as a provided
//! port, and is implemented by `sdkwork-merchandise-repository-sqlx`. This crate only *consumes* it:
//! it holds `Arc<dyn CatalogRepositoryPort>` in [`CatalogState`] and hands it to the route handlers.
//! The declaration used to live here, together with an `impl` for the sqlx store, which made a
//! transport crate the owner of the persistence contract and forced it to depend on the concrete
//! repository — the reverse of the direction the port exists to establish.
//!
//! # Wire contract
//!
//! Every `BIGINT` column is serialized as a **decimal string** (`API_SPEC` section 13.6). A browser
//! silently rounds an int64 sent as a JSON number past `Number.MAX_SAFE_INTEGER` (2^53), and the
//! rounded id is then replayed into a lookup that returns the wrong row — or none. Narrow columns
//! (`depth`, `price_scale`) are also sent as strings: the rule is keyed on the wire type a field is
//! declared with, not on the width of the column behind it, and a reader that mostly sees string
//! integers should not have to special-case two fields.
//!
//! Money is exposed as exact integer minor units plus the scale snapshotted on the row. There is no
//! major-unit amount on the wire: a bare `"640.00"` cannot say whether it means 64000 or 640000, and
//! the currency's exponent lives in `commerce_currency` rather than in the caller's head.
//!
//! The SKU row's `price_scale` is published as `minorUnitExponent`. It is a unit exponent, not an
//! amount, and a field named `priceScale` is classified as money by the `API_SPEC` section 13.2
//! validator, which would then demand an `x-sdkwork-money-unit` the value does not have.

use std::sync::Arc;

use sdkwork_merchandise_service::{
    AttributeRecord, CatalogRepositoryPort, CategoryAttributeRecord, CategoryRecord, MediaRecord,
    PriceListRecord, SkuRecord, SpuRecord,
};
use serde::{Deserialize, Serialize};

pub use crate::http_envelope::{
    catalog_error_response, expected_version_from_if_match, not_found_response,
    stale_version_response, success_accepted, success_created_resource, success_list,
    success_no_content, success_offset_page, success_resource, success_resource_with_etag,
    unauthorized_response, validation_response, CatalogJson,
};

/// The router's plug point.
///
/// `store` is the service-owned port, not a concrete store: the handlers below cannot name a
/// database, and the composition root decides which implementation arrives. That is why the type
/// is `Arc<dyn CatalogRepositoryPort>` and why this crate has no repository dependency at all.
#[derive(Clone)]
pub struct CatalogState {
    pub store: Arc<dyn CatalogRepositoryPort>,
}

/// Query parameters for the category collection.
///
/// There is deliberately no `organization_id` here. Organization scope is carried by the
/// authenticated runtime context (`IamAppContext`), never by a caller-supplied query parameter: a
/// list endpoint that lets the caller choose the organization is a scope-confusion hazard, and the
/// create endpoints already refuse to run without a context organization. The OpenAPI for
/// `/backend/v3/api/catalog/categories` declares no such parameter either.
#[derive(Debug, Deserialize)]
pub struct CategoryQueryParams {
    pub parent_id: Option<String>,
    pub status: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

/// Query parameters for the attribute dictionary collection.
///
/// `scope` was removed from the model (`commerce_product_attribute` has no scope column), so it is
/// no longer accepted here nor declared in the OpenAPI.
#[derive(Debug, Deserialize)]
pub struct AttributeQueryParams {
    pub status: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct PriceListQueryParams {
    currency_code: Option<String>,
    market_code: Option<String>,
    status: Option<String>,
    page: Option<i64>,
    page_size: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct CategoryAttributeQueryParams {
    category_id: Option<String>,
    attribute_id: Option<String>,
    status: Option<String>,
    page: Option<i64>,
    page_size: Option<i64>,
}

/// Query parameters for the product collection.
///
/// `q` is free-text search across the product's own identifier and titles. There is no `cursor`
/// here: this collection paginates by offset, and the retired `/catalog/spus` collection was its
/// only cursor user.
#[derive(Debug, Deserialize)]
pub struct ProductListQueryParams {
    pub q: Option<String>,
    pub category_id: Option<String>,
    pub product_type: Option<String>,
    pub status: Option<String>,
    pub sort: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

/// Query parameters for the SKU collection.
///
/// The parent filter is `product_id`, matching the OpenAPI parameter name and the rest of the
/// product-facing vocabulary (`/backend/v3/api/catalog/products/{productId}`). Internally it selects
/// `commerce_product_sku.spu_id`.
///
/// `attribute_value_id` narrows the page to the SKUs sitting on one sales-axis value. It is the
/// buyer-facing question ("which of these are red"), and it is served by
/// `idx_commerce_product_sku_attribute_tenant_value` rather than by a join the caller would have to
/// paginate around.
#[derive(Debug, Deserialize)]
struct SkuListQueryParams {
    product_id: Option<String>,
    attribute_value_id: Option<String>,
    status: Option<String>,
    page: Option<i64>,
    page_size: Option<i64>,
}

/// Query parameters for the media collection.
///
/// `owner_type` and `owner_id` are declared as one filter pair. Together they address "this
/// product's images"; alone, an id is ambiguous across the four owner tables, so the pair is what a
/// caller is expected to send. `media_role` narrows within an owner without addressing it, which is
/// how a gallery and a detail strip are requested separately.
#[derive(Debug, Deserialize)]
struct MediaQueryParams {
    owner_type: Option<String>,
    owner_id: Option<String>,
    media_role: Option<String>,
    status: Option<String>,
    page: Option<i64>,
    page_size: Option<i64>,
}

// ------------------------------------------------------------------ request bodies
//
// Every write body is `deny_unknown_fields`, matching the `additionalProperties: false` the
// authored OpenAPI declares for create/update bodies (`API_SPEC` section 12). Without the serde
// attribute the document would promise a closed body while the adapter silently ignored whatever
// else arrived — the class of contract the request-body closure gate exists to prevent. The
// rejection is answered through `CatalogJson`, so a caller sees the same `400` problem envelope as
// any other malformed input.
//
// Field names are camelCase on the wire (`API_SPEC` section 13), so the Rust names here are the
// snake_case spelling of the documented property. Identifiers and amounts are `String` because
// `API_SPEC` section 13.6 carries every int64 as a JSON string; the adapter parses them into `i64`
// before a command is built.

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateCategoryBody {
    category_no: String,
    parent_id: Option<String>,
    name: String,
    sort_order: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UpdateCategoryBody {
    parent_id: Option<String>,
    name: Option<String>,
    sort_order: Option<String>,
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateAttributeBody {
    attribute_no: String,
    name: String,
    values: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreatePriceListBody {
    price_list_no: String,
    currency_code: String,
    market_code: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UpdatePriceListBody {
    status: Option<String>,
    starts_at: Option<String>,
    ends_at: Option<String>,
}

/// Create-body for one category attribute binding.
///
/// `role` defaults to `parameter` when omitted: a binding that has not been classified is
/// descriptive metadata, which is the only role that never changes how an SKU splits. A caller that
/// omits it therefore cannot accidentally declare a sales axis.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateCategoryAttributeBody {
    category_id: String,
    attribute_id: String,
    role: Option<String>,
    required: Option<bool>,
    searchable: Option<bool>,
    filterable: Option<bool>,
    comparable: Option<bool>,
    source_category_id: Option<String>,
    sort_order: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UpdateCategoryAttributeBody {
    role: Option<String>,
    required: Option<bool>,
    searchable: Option<bool>,
    filterable: Option<bool>,
    comparable: Option<bool>,
    sort_order: Option<String>,
    status: Option<String>,
}

/// Create-SPU body.
///
/// `category_id` is required: `commerce_product_spu.category_id` is `NOT NULL`, so omitting it is a
/// malformed request rather than a product without a home.
///
/// `product_no` is the adapter's spelling of the command's `spu_no`. The product-facing HTTP
/// vocabulary is `product` (`/catalog/products/{productId}`), and the request body belongs to that
/// surface, so the anti-corruption mapping applies here and not only in the URL.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateSpuBody {
    pub product_no: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub description: Option<String>,
    pub product_type: String,
    pub category_id: String,
}

/// Update-SPU body.
///
/// `description` carries the same three states as `listPriceMinor` in the update-SKU body, for the
/// same reason: `commerce_product_spu.description` is a nullable column, so `NULL` is how a product
/// is stored with no description, and an omitted field must not be read as a request to clear it. It
/// decodes through [`deserialize_present_option`], so an explicit JSON `null` arrives as `Some(None)`
/// and a stated string as `Some(Some(text))`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateSpuBody {
    pub title: Option<String>,
    pub subtitle: Option<String>,
    #[serde(default, deserialize_with = "deserialize_present_option")]
    pub description: Option<Option<String>>,
    pub category_id: Option<String>,
}

/// Create-SKU body.
///
/// The two amounts are **minor-unit int64 strings** (`API_SPEC` section 13.2.1): `640` means 640
/// fen of `currency_code`, and the currency's exponent is resolved server-side only to snapshot
/// `price_scale`. No layer on this path divides or multiplies by a literal.
///
/// `attribute_value_ids` is the SKU's position on each sales axis, submitted as dictionary value
/// ids. The attribute behind each value is derived server-side, so a request cannot name an
/// attribute and a value that disagree. The repository compares the resolved set with the product's
/// category template and refuses a partial combination.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateSkuBody {
    product_id: String,
    sku_no: String,
    name: String,
    title: String,
    sale_price_minor: String,
    list_price_minor: Option<String>,
    currency_code: String,
    fulfillment_type: String,
    inventory_tracking: String,
    attribute_value_ids: Option<Vec<String>>,
}

/// Update-SKU body.
///
/// `attribute_value_ids` distinguishes "not mentioned" from "cleared", so a price-only edit cannot
/// silently erase a variant's axes. `list_price_minor` needs the same three states and cannot get
/// them from a bare `Option`, because an integer has no in-band empty value the way a list has `[]`;
/// it is decoded through [`deserialize_present_option`] so an explicit JSON `null` survives as
/// "clear it" instead of collapsing into "not mentioned".
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UpdateSkuBody {
    name: Option<String>,
    title: Option<String>,
    sale_price_minor: Option<String>,
    #[serde(default, deserialize_with = "deserialize_present_option")]
    list_price_minor: Option<Option<String>>,
    currency_code: Option<String>,
    fulfillment_type: Option<String>,
    inventory_tracking: Option<String>,
    status: Option<String>,
    attribute_value_ids: Option<Vec<String>>,
}

/// Distinguishes an absent JSON key from an explicit `null` on a nullable body field.
///
/// `Option<T>`'s own `Deserialize` maps both to `None`, and for a three-state field that collapse is
/// the whole defect: absent means "leave the stored value alone" while `null` means "this product no
/// longer has one". Both `listPriceMinor` and `description` need the distinction, because neither an
/// integer nor a nullable text column has an in-band empty value to carry "clear" instead. Wrapping
/// the field in a second `Option` and marking it `default`
/// restores the missing state — `default` supplies the outer `None` when the key is absent, and
/// this function supplies `Some` whenever the key is present, so a present `null` arrives as
/// `Some(None)` and a present value as `Some(Some(value))`.
fn deserialize_present_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

/// Create-media body.
///
/// `resource` is a `MediaResource` (`MEDIA_RESOURCE_SPEC` section 3), which is why it is carried as
/// a JSON document rather than as a flat set of Rust fields: the authored contract publishes the
/// schema, and the domain validates its required keys and its closed top level. Its `id` becomes
/// `commerce_product_media.media_resource_id` and the document itself becomes
/// `resource_snapshot` — the reference and the projection are written from one input, so they cannot
/// describe different files.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateMediaBody {
    owner_type: String,
    owner_id: String,
    media_role: String,
    resource: serde_json::Value,
    alt_text: Option<String>,
    sort_order: Option<String>,
}

/// Update-media body.
///
/// There is deliberately no `ownerType`/`ownerId`: the owner of an attachment is fixed at creation,
/// and moving one is a delete plus a create. `resource` replaces the reference and the snapshot
/// together, so a swap cannot leave the row pointing at one file while describing another.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UpdateMediaBody {
    media_role: Option<String>,
    resource: Option<serde_json::Value>,
    alt_text: Option<String>,
    sort_order: Option<String>,
    status: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryResponse {
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    id: i64,
    category_no: String,
    #[serde(with = "sdkwork_utils_rust::serde_int64::option")]
    parent_id: Option<i64>,
    path: String,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    depth: i64,
    is_leaf: bool,
    name: String,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    sort_order: i64,
    status: String,
    created_at: String,
    updated_at: String,
    /// Monotonic row version, bumped by every write to the row.
    ///
    /// This is the number `If-Match` expects on the next update or delete, and the same value the
    /// response's `ETag` carries. `API_SPEC` section 17 makes the row version the precondition for
    /// optimistic concurrency, so a client that has read a resource can always name what it read.
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    version: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttributeResponse {
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    id: i64,
    attribute_no: String,
    name: String,
    value_type: String,
    status: String,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    sort_order: i64,
    created_at: String,
    updated_at: String,
    /// Monotonic row version, bumped by every write to the row.
    ///
    /// This is the number `If-Match` expects on the next update or delete, and the same value the
    /// response's `ETag` carries. `API_SPEC` section 17 makes the row version the precondition for
    /// optimistic concurrency, so a client that has read a resource can always name what it read.
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    version: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductResponse {
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    id: i64,
    product_no: String,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    category_id: i64,
    name: String,
    title: Option<String>,
    subtitle: Option<String>,
    description: Option<String>,
    product_type: String,
    status: String,
    sales_status: String,
    published_at: Option<String>,
    created_at: String,
    updated_at: String,
    /// Monotonic row version, bumped by every write to the row.
    ///
    /// This is the number `If-Match` expects on the next update or delete, and the same value the
    /// response's `ETag` carries. `API_SPEC` section 17 makes the row version the precondition for
    /// optimistic concurrency, so a client that has read a resource can always name what it read.
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    version: i64,
}

/// One sales axis of an SKU, as published inside [`SkuResponse`].
///
/// This is the read side of the variant write path: the create and update bodies accept dictionary
/// value ids, so the response has to hand them back alongside the business keys a console renders.
/// `variantSignature` carries the same information in one string; these rows are what a variant
/// matrix is built from.
///
/// The `View` suffix, rather than `Response`, is load-bearing. `tests/static/api-response-body-closure`
/// reads every `*Response` struct in this file as *the* published resource of that name, and asserts a
/// one-to-one mapping onto the contract's resource schemas. This type is a nested read model inside
/// [`SkuResponse`] — there is no `SkuAxis` resource and no operation returns one — so giving it the
/// `Response` suffix would make the gate demand a schema that must not exist.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkuAxisView {
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    attribute_id: i64,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    attribute_value_id: i64,
    attribute_no: String,
    value_code: String,
    display_value: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkuResponse {
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    id: i64,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    product_id: i64,
    sku_no: String,
    variant_signature: String,
    name: Option<String>,
    title: Option<String>,
    currency_code: String,
    /// Published as `minorUnitExponent`. `priceScale` is classified as money by the `API_SPEC`
    /// section 13.2 validator, and this value is a unit exponent, not an amount.
    #[serde(rename = "minorUnitExponent", with = "sdkwork_utils_rust::serde_int64")]
    price_scale: i64,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    sale_price_minor: i64,
    #[serde(with = "sdkwork_utils_rust::serde_int64::option")]
    list_price_minor: Option<i64>,
    fulfillment_type: String,
    inventory_tracking: String,
    status: String,
    sales_status: String,
    published_at: Option<String>,
    created_at: String,
    updated_at: String,
    /// Always present, empty when the SKU's category declares no sales axis. It is a list rather
    /// than a nullable field because "no axes" and "axes not loaded" must not look the same.
    attribute_values: Vec<SkuAxisView>,
    /// Monotonic row version, bumped by every write to the row.
    ///
    /// This is the number `If-Match` expects on the next update or delete, and the same value the
    /// response's `ETag` carries. `API_SPEC` section 17 makes the row version the precondition for
    /// optimistic concurrency, so a client that has read a resource can always name what it read.
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    version: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceListResponse {
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    id: i64,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    tenant_id: i64,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    organization_id: i64,
    price_list_no: String,
    name: String,
    currency_code: String,
    market_code: Option<String>,
    status: String,
    starts_at: Option<String>,
    ends_at: Option<String>,
    created_at: String,
    updated_at: String,
    /// Monotonic row version, bumped by every write to the row.
    ///
    /// This is the number `If-Match` expects on the next update or delete, and the same value the
    /// response's `ETag` carries. `API_SPEC` section 17 makes the row version the precondition for
    /// optimistic concurrency, so a client that has read a resource can always name what it read.
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    version: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryAttributeResponse {
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    id: i64,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    tenant_id: i64,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    organization_id: i64,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    category_id: i64,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    attribute_id: i64,
    attribute_role: String,
    #[serde(with = "sdkwork_utils_rust::serde_int64::option")]
    source_category_id: Option<i64>,
    required: bool,
    searchable: bool,
    filterable: bool,
    comparable: bool,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    sort_order: i64,
    status: String,
    created_at: String,
    updated_at: String,
    /// Monotonic row version, bumped by every write to the row.
    ///
    /// This is the number `If-Match` expects on the next update or delete, and the same value the
    /// response's `ETag` carries. `API_SPEC` section 17 makes the row version the precondition for
    /// optimistic concurrency, so a client that has read a resource can always name what it read.
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    version: i64,
}

/// One media attachment on the wire.
///
/// `mediaResourceId` is the stable Drive identity and `resourceSnapshot` is the read-model
/// projection, published as the shared `MediaResource` schema. There is no `url` field and no
/// `imageUrl`: `MEDIA_RESOURCE_SPEC` section 6 names `commerce_product_media.url` as non-standard,
/// and a delivery URL that expires cannot be a resource's identity. When a caller needs a URL it
/// reads `resourceSnapshot.url`, which the contract documents as a delivery hint rather than as the
/// system of record.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaResponse {
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    id: i64,
    owner_type: String,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    owner_id: i64,
    media_role: String,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    media_resource_id: i64,
    resource_snapshot: serde_json::Value,
    alt_text: Option<String>,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    sort_order: i64,
    status: String,
    created_at: String,
    updated_at: String,
    /// Monotonic row version, bumped by every write to the row.
    ///
    /// This is the number `If-Match` expects on the next update or delete, and the same value the
    /// response's `ETag` carries. `API_SPEC` section 17 makes the row version the precondition for
    /// optimistic concurrency, so a client that has read a resource can always name what it read.
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    version: i64,
}

pub fn map_category(value: CategoryRecord) -> CategoryResponse {
    CategoryResponse {
        id: value.id,
        category_no: value.category_no,
        parent_id: value.parent_id,
        path: value.path,
        depth: value.depth,
        is_leaf: value.is_leaf,
        name: value.name,
        sort_order: value.sort_order,
        status: value.status,
        created_at: value.created_at,
        version: value.version,
        updated_at: value.updated_at,
    }
}

pub fn map_attribute(value: AttributeRecord) -> AttributeResponse {
    AttributeResponse {
        id: value.id,
        attribute_no: value.attribute_no,
        name: value.name,
        value_type: value.value_type,
        status: value.status,
        sort_order: value.sort_order,
        created_at: value.created_at,
        version: value.version,
        updated_at: value.updated_at,
    }
}

/// Maps the domain's `SpuRecord` onto the HTTP `Product` resource.
///
/// The domain and the storage column keep the `spu` name; every HTTP-visible name is `product`, so
/// the translation happens here and nowhere else. `productId` is also the name the request bodies,
/// the `/products/{productId}` path, and the list filters already use.
pub fn map_product(value: SpuRecord) -> ProductResponse {
    ProductResponse {
        id: value.id,
        product_no: value.spu_no,
        category_id: value.category_id,
        name: value.name,
        title: value.title,
        subtitle: value.subtitle,
        description: value.description,
        product_type: value.product_type,
        status: value.status,
        sales_status: value.sales_status,
        published_at: value.published_at,
        created_at: value.created_at,
        version: value.version,
        updated_at: value.updated_at,
    }
}

pub fn map_sku(value: SkuRecord) -> SkuResponse {
    SkuResponse {
        id: value.id,
        product_id: value.spu_id,
        sku_no: value.sku_no,
        variant_signature: value.variant_signature,
        name: value.name,
        title: value.title,
        currency_code: value.currency_code,
        price_scale: value.price_scale,
        sale_price_minor: value.sale_price_minor,
        list_price_minor: value.list_price_minor,
        fulfillment_type: value.fulfillment_type,
        inventory_tracking: value.inventory_tracking,
        status: value.status,
        sales_status: value.sales_status,
        published_at: value.published_at,
        created_at: value.created_at,
        version: value.version,
        updated_at: value.updated_at,
        attribute_values: value
            .attribute_values
            .into_iter()
            .map(|axis| SkuAxisView {
                attribute_id: axis.attribute_id,
                attribute_value_id: axis.attribute_value_id,
                attribute_no: axis.attribute_no,
                value_code: axis.value_code,
                display_value: axis.display_value,
            })
            .collect(),
    }
}

pub fn map_media(value: MediaRecord) -> MediaResponse {
    MediaResponse {
        id: value.id,
        owner_type: value.owner_type,
        owner_id: value.owner_id,
        media_role: value.media_role,
        media_resource_id: value.media_resource_id,
        resource_snapshot: value.resource_snapshot,
        alt_text: value.alt_text,
        sort_order: value.sort_order,
        status: value.status,
        created_at: value.created_at,
        version: value.version,
        updated_at: value.updated_at,
    }
}

fn map_price_list(value: PriceListRecord) -> PriceListResponse {
    PriceListResponse {
        id: value.id,
        tenant_id: value.tenant_id,
        organization_id: value.organization_id,
        price_list_no: value.price_list_no,
        name: value.name,
        currency_code: value.currency_code,
        market_code: value.market_code,
        status: value.status,
        starts_at: value.starts_at,
        ends_at: value.ends_at,
        created_at: value.created_at,
        version: value.version,
        updated_at: value.updated_at,
    }
}

fn map_category_attribute(value: CategoryAttributeRecord) -> CategoryAttributeResponse {
    CategoryAttributeResponse {
        id: value.id,
        tenant_id: value.tenant_id,
        organization_id: value.organization_id,
        category_id: value.category_id,
        attribute_id: value.attribute_id,
        attribute_role: value.attribute_role,
        source_category_id: value.source_category_id,
        required: value.required,
        searchable: value.searchable,
        filterable: value.filterable,
        comparable: value.comparable,
        sort_order: value.sort_order,
        status: value.status,
        created_at: value.created_at,
        version: value.version,
        updated_at: value.updated_at,
    }
}

#[cfg(test)]
mod tests {
    //! Pins the one thing the three-state fields are entirely about: the difference between a key that
    //! was **absent** and a key that was **`null`**. Every other property of an update body is visible
    //! in its type; this one is exactly what a single `Option` erases, so it is asserted at the decoder
    //! rather than described in a comment. `description` on the update-SPU body is asserted here too:
    //! it is the same defect on a nullable-text column, where the in-band trick that spares
    //! `attribute_value_ids` (an empty list) does not exist.

    use super::{UpdateSkuBody, UpdateSpuBody};

    fn decode(payload: &str) -> UpdateSkuBody {
        serde_json::from_str(payload).expect("the update body must decode")
    }

    fn decode_spu(payload: &str) -> UpdateSpuBody {
        serde_json::from_str(payload).expect("the update-SPU body must decode")
    }

    #[test]
    fn an_omitted_reference_price_is_not_a_clearing_write() {
        let body = decode(r#"{"title":"Amended"}"#);

        assert_eq!(
            body.list_price_minor, None,
            "an absent key must leave the stored reference price alone; decoding it as a clear would \
             let a title-only edit erase the strike-through figure"
        );
    }

    #[test]
    fn an_explicit_null_reference_price_is_a_clearing_write() {
        let body = decode(r#"{"listPriceMinor":null}"#);

        assert_eq!(
            body.list_price_minor,
            Some(None),
            "an explicit null is the caller saying the product no longer has a reference price"
        );
    }

    #[test]
    fn a_stated_reference_price_survives_as_the_wire_string() {
        let body = decode(r#"{"listPriceMinor":"12900"}"#);

        assert_eq!(
            body.list_price_minor,
            Some(Some("12900".to_owned())),
            "a restated price must reach the handler as the decimal string API_SPEC section 13.6 \
             requires, not as a JSON number a browser may have rounded"
        );
    }

    #[test]
    fn an_omitted_description_is_not_a_clearing_write() {
        let body = decode_spu(r#"{"title":"Amended"}"#);

        assert_eq!(
            body.description, None,
            "a title-only edit must not read as 'remove the product description'; both are `None` to \
             a single-Option decoder, which is the whole reason this field is three-state"
        );
    }

    #[test]
    fn an_explicit_null_description_is_a_clearing_write() {
        let body = decode_spu(r#"{"description":null}"#);

        assert_eq!(
            body.description,
            Some(None),
            "an explicit null is the caller saying the product no longer has a description"
        );
    }

    #[test]
    fn a_stated_description_survives_verbatim() {
        let body = decode_spu(r#"{"description":"Notarised on demand."}"#);

        assert_eq!(
            body.description,
            Some(Some("Notarised on demand.".to_owned())),
            "a restated description must reach the handler unchanged, including when it is the empty \
             string, which is a stated description and not a clear"
        );
    }
}

#[path = "backend_catalog_router.rs"]
mod backend_catalog_router;

pub use backend_catalog_router::build_backend_catalog_router;
