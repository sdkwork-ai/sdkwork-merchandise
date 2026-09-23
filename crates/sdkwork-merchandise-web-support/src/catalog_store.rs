//! Shared merchandise store port, HTTP DTOs, and response mappers.
//!
//! # Wire contract
//!
//! Every `BIGINT` column is serialized as a **decimal string** (`API_SPEC` section 13.6). A browser
//! silently rounds an int64 sent as a JSON number past `Number.MAX_SAFE_INTEGER` (2^53), and the
//! rounded id is then replayed into a lookup that returns the wrong row — or none. `SMALLINT` columns
//! (`depth`, `price_scale`) stay numbers because they cannot exceed the safe range.
//!
//! Money is exposed as exact integer minor units plus the scale snapshotted on the row. There is no
//! major-unit amount on the wire: a bare `"640.00"` cannot say whether it means 64000 or 640000, and
//! the currency's exponent lives in `commerce_currency` rather than in the caller's head.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use sdkwork_contract_service::CommerceServiceError;
use sdkwork_merchandise_repository_sqlx::PostgresCommerceCatalogStore;
use sdkwork_merchandise_service::{
    ArchiveSpuCommand, AttributeListQuery, AttributeRecord, CategoryAttributeListQuery,
    CategoryAttributeRecord, CategoryListQuery, CategoryRecord, CategoryRetrieveQuery,
    CreateAttributeCommand, CreateCategoryAttributeCommand, CreateCategoryCommand,
    CreatePriceListCommand, CreateProductSkuCommand, CreateProductSpuCommand,
    DeleteCategoryAttributeCommand, DeleteCategoryCommand, DeleteProductSkuCommand,
    DeleteProductSpuCommand, PriceListItemRecord, PriceListListQuery, PriceListRecord,
    ProductSkuListQuery, ProductSkuRetrieveQuery, ProductSpuListQuery, ProductSpuRetrieveQuery,
    PublishSpuCommand, SkuPriceRetrieveQuery, SkuRecord, SpuRecord, UpdateCategoryAttributeCommand,
    UpdateCategoryCommand, UpdatePriceListCommand, UpdateProductSkuCommand,
    UpdateProductSpuCommand,
};
use serde::{Deserialize, Serialize};

pub use crate::http_envelope::{
    catalog_system_response, not_found_response, success_accepted, success_created_resource,
    success_list, success_no_content, success_offset_page, success_resource, unauthorized_response,
    validation_response,
};

pub type CommerceCatalogFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, CommerceServiceError>> + Send + 'a>>;

#[derive(Debug)]
pub struct CatalogOffsetPage<T> {
    pub items: Vec<T>,
    pub page: i64,
    pub page_size: i64,
    pub total_items: i64,
}

impl<T> CatalogOffsetPage<T> {
    fn new(items: Vec<T>, page: Option<i64>, page_size: Option<i64>, total_items: i64) -> Self {
        Self {
            items,
            page: page.unwrap_or(1),
            page_size: page_size.unwrap_or(20),
            total_items,
        }
    }
}

pub trait CommerceCatalogStore: Send + Sync {
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
    ) -> CommerceCatalogFuture<'a, CategoryRecord>;

    fn delete_category<'a>(
        &'a self,
        command: DeleteCategoryCommand,
    ) -> CommerceCatalogFuture<'a, ()>;

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
    ) -> CommerceCatalogFuture<'a, PriceListRecord>;

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
    ) -> CommerceCatalogFuture<'a, CategoryAttributeRecord>;

    fn delete_category_attribute<'a>(
        &'a self,
        command: DeleteCategoryAttributeCommand,
    ) -> CommerceCatalogFuture<'a, ()>;

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
    ) -> CommerceCatalogFuture<'a, SpuRecord>;

    fn publish_spu<'a>(
        &'a self,
        command: PublishSpuCommand,
    ) -> CommerceCatalogFuture<'a, SpuRecord>;

    fn archive_spu<'a>(
        &'a self,
        command: ArchiveSpuCommand,
    ) -> CommerceCatalogFuture<'a, SpuRecord>;

    fn delete_spu<'a>(&'a self, command: DeleteProductSpuCommand) -> CommerceCatalogFuture<'a, ()>;

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

    fn retrieve_sku_prices<'a>(
        &'a self,
        query: SkuPriceRetrieveQuery,
    ) -> CommerceCatalogFuture<'a, Vec<PriceListItemRecord>>;

    fn create_sku<'a>(
        &'a self,
        command: CreateProductSkuCommand,
    ) -> CommerceCatalogFuture<'a, SkuRecord>;

    fn update_sku<'a>(
        &'a self,
        command: UpdateProductSkuCommand,
    ) -> CommerceCatalogFuture<'a, SkuRecord>;

    fn delete_sku<'a>(&'a self, command: DeleteProductSkuCommand) -> CommerceCatalogFuture<'a, ()>;
}

#[derive(Clone)]
pub struct CatalogState {
    pub store: Arc<dyn CommerceCatalogStore>,
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
#[derive(Debug, Deserialize)]
struct SkuListQueryParams {
    product_id: Option<String>,
    status: Option<String>,
    page: Option<i64>,
    page_size: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateCategoryBody {
    category_no: String,
    parent_id: Option<String>,
    name: String,
    sort_order: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateCategoryBody {
    parent_id: Option<String>,
    name: Option<String>,
    sort_order: Option<i64>,
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateAttributeBody {
    attribute_no: String,
    name: String,
    values: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreatePriceListBody {
    price_list_no: String,
    currency_code: String,
    market_code: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
struct CreateCategoryAttributeBody {
    category_id: String,
    attribute_id: String,
    role: Option<String>,
    required: Option<bool>,
    searchable: Option<bool>,
    filterable: Option<bool>,
    comparable: Option<bool>,
    source_category_id: Option<String>,
    sort_order: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateCategoryAttributeBody {
    role: Option<String>,
    required: Option<bool>,
    searchable: Option<bool>,
    filterable: Option<bool>,
    comparable: Option<bool>,
    sort_order: Option<i64>,
    status: Option<String>,
}

/// Create-SPU body.
///
/// `category_id` is required: `commerce_product_spu.category_id` is `NOT NULL`, so omitting it is a
/// malformed request rather than a product without a home.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSpuBody {
    pub spu_no: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub description: Option<String>,
    pub product_type: String,
    pub category_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSpuBody {
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub description: Option<String>,
    pub category_id: Option<String>,
}

/// Create-SKU body.
///
/// `price_amount` and `original_price_amount` are major-denomination decimals; the repository resolves
/// the currency's scale and stores exact minor units.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateSkuBody {
    spu_id: String,
    sku_no: String,
    name: String,
    title: String,
    price_amount: String,
    original_price_amount: Option<String>,
    currency_code: String,
    fulfillment_type: String,
    inventory_tracking: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateSkuBody {
    name: Option<String>,
    title: Option<String>,
    price_amount: Option<String>,
    original_price_amount: Option<String>,
    currency_code: Option<String>,
    fulfillment_type: Option<String>,
    inventory_tracking: Option<String>,
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
    depth: i64,
    is_leaf: bool,
    name: String,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    sort_order: i64,
    status: String,
    created_at: String,
    updated_at: String,
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
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpuResponse {
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    id: i64,
    spu_no: String,
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
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkuResponse {
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    id: i64,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    spu_id: i64,
    sku_no: String,
    variant_signature: String,
    name: Option<String>,
    title: Option<String>,
    currency_code: String,
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
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceListItemResponse {
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    id: i64,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    tenant_id: i64,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    price_list_id: i64,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    sku_id: i64,
    currency_code: String,
    price_scale: i64,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    price_minor: i64,
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
}

impl CommerceCatalogStore for PostgresCommerceCatalogStore {
    fn list_categories<'a>(
        &'a self,
        query: CategoryListQuery,
    ) -> CommerceCatalogFuture<'a, Vec<CategoryRecord>> {
        Box::pin(async move { self.list_categories(&query).await })
    }

    fn list_categories_page<'a>(
        &'a self,
        query: CategoryListQuery,
    ) -> CommerceCatalogFuture<'a, CatalogOffsetPage<CategoryRecord>> {
        Box::pin(async move {
            let items = self.list_categories(&query).await?;
            let total_items = self.count_categories(&query).await?;
            Ok(CatalogOffsetPage::new(
                items,
                query.page,
                query.page_size,
                total_items,
            ))
        })
    }

    fn retrieve_category<'a>(
        &'a self,
        query: CategoryRetrieveQuery,
    ) -> CommerceCatalogFuture<'a, Option<CategoryRecord>> {
        Box::pin(async move { self.retrieve_category(&query).await })
    }

    fn create_category<'a>(
        &'a self,
        command: CreateCategoryCommand,
    ) -> CommerceCatalogFuture<'a, CategoryRecord> {
        Box::pin(async move { self.create_category(&command).await })
    }

    fn update_category<'a>(
        &'a self,
        command: UpdateCategoryCommand,
    ) -> CommerceCatalogFuture<'a, CategoryRecord> {
        Box::pin(async move { self.update_category(&command).await })
    }

    fn delete_category<'a>(
        &'a self,
        command: DeleteCategoryCommand,
    ) -> CommerceCatalogFuture<'a, ()> {
        Box::pin(async move { self.delete_category(&command).await })
    }

    fn list_attributes<'a>(
        &'a self,
        query: AttributeListQuery,
    ) -> CommerceCatalogFuture<'a, Vec<AttributeRecord>> {
        Box::pin(async move { self.list_attributes(&query).await })
    }

    fn list_attributes_page<'a>(
        &'a self,
        query: AttributeListQuery,
    ) -> CommerceCatalogFuture<'a, CatalogOffsetPage<AttributeRecord>> {
        Box::pin(async move {
            let items = self.list_attributes(&query).await?;
            let total_items = self.count_attributes(&query).await?;
            Ok(CatalogOffsetPage::new(
                items,
                query.page,
                query.page_size,
                total_items,
            ))
        })
    }

    fn create_attribute<'a>(
        &'a self,
        command: CreateAttributeCommand,
    ) -> CommerceCatalogFuture<'a, AttributeRecord> {
        Box::pin(async move { self.create_attribute(&command).await })
    }

    fn list_price_lists<'a>(
        &'a self,
        query: PriceListListQuery,
    ) -> CommerceCatalogFuture<'a, Vec<PriceListRecord>> {
        Box::pin(async move { self.list_price_lists(&query).await })
    }

    fn list_price_lists_page<'a>(
        &'a self,
        query: PriceListListQuery,
    ) -> CommerceCatalogFuture<'a, CatalogOffsetPage<PriceListRecord>> {
        Box::pin(async move {
            let items = self.list_price_lists(&query).await?;
            let total_items = self.count_price_lists(&query).await?;
            Ok(CatalogOffsetPage::new(
                items,
                query.page,
                query.page_size,
                total_items,
            ))
        })
    }

    fn create_price_list<'a>(
        &'a self,
        command: CreatePriceListCommand,
    ) -> CommerceCatalogFuture<'a, PriceListRecord> {
        Box::pin(async move { self.create_price_list(&command).await })
    }

    fn update_price_list<'a>(
        &'a self,
        command: UpdatePriceListCommand,
    ) -> CommerceCatalogFuture<'a, PriceListRecord> {
        Box::pin(async move { self.update_price_list(&command).await })
    }

    fn list_category_attributes<'a>(
        &'a self,
        query: CategoryAttributeListQuery,
    ) -> CommerceCatalogFuture<'a, Vec<CategoryAttributeRecord>> {
        Box::pin(async move { self.list_category_attributes(&query).await })
    }

    fn list_category_attributes_page<'a>(
        &'a self,
        query: CategoryAttributeListQuery,
    ) -> CommerceCatalogFuture<'a, CatalogOffsetPage<CategoryAttributeRecord>> {
        Box::pin(async move {
            let items = self.list_category_attributes(&query).await?;
            let total_items = self.count_category_attributes(&query).await?;
            Ok(CatalogOffsetPage::new(
                items,
                query.page,
                query.page_size,
                total_items,
            ))
        })
    }

    fn create_category_attribute<'a>(
        &'a self,
        command: CreateCategoryAttributeCommand,
    ) -> CommerceCatalogFuture<'a, CategoryAttributeRecord> {
        Box::pin(async move { self.create_category_attribute(&command).await })
    }

    fn update_category_attribute<'a>(
        &'a self,
        command: UpdateCategoryAttributeCommand,
    ) -> CommerceCatalogFuture<'a, CategoryAttributeRecord> {
        Box::pin(async move { self.update_category_attribute(&command).await })
    }

    fn delete_category_attribute<'a>(
        &'a self,
        command: DeleteCategoryAttributeCommand,
    ) -> CommerceCatalogFuture<'a, ()> {
        Box::pin(async move { self.delete_category_attribute(&command).await })
    }

    fn list_spus<'a>(
        &'a self,
        query: ProductSpuListQuery,
    ) -> CommerceCatalogFuture<'a, Vec<SpuRecord>> {
        Box::pin(async move { self.list_spus(&query).await })
    }

    fn list_spus_page<'a>(
        &'a self,
        query: ProductSpuListQuery,
    ) -> CommerceCatalogFuture<'a, CatalogOffsetPage<SpuRecord>> {
        Box::pin(async move {
            let items = self.list_spus(&query).await?;
            let total_items = self.count_spus(&query).await?;
            Ok(CatalogOffsetPage::new(
                items,
                query.page,
                query.page_size,
                total_items,
            ))
        })
    }

    fn retrieve_spu<'a>(
        &'a self,
        query: ProductSpuRetrieveQuery,
    ) -> CommerceCatalogFuture<'a, Option<SpuRecord>> {
        Box::pin(async move { self.retrieve_spu(&query).await })
    }

    fn create_spu<'a>(
        &'a self,
        command: CreateProductSpuCommand,
    ) -> CommerceCatalogFuture<'a, SpuRecord> {
        Box::pin(async move { self.create_spu(&command).await })
    }

    fn update_spu<'a>(
        &'a self,
        command: UpdateProductSpuCommand,
    ) -> CommerceCatalogFuture<'a, SpuRecord> {
        Box::pin(async move { self.update_spu(&command).await })
    }

    fn publish_spu<'a>(
        &'a self,
        command: PublishSpuCommand,
    ) -> CommerceCatalogFuture<'a, SpuRecord> {
        Box::pin(async move { self.publish_spu(&command).await })
    }

    fn archive_spu<'a>(
        &'a self,
        command: ArchiveSpuCommand,
    ) -> CommerceCatalogFuture<'a, SpuRecord> {
        Box::pin(async move { self.archive_spu(&command).await })
    }

    fn delete_spu<'a>(&'a self, command: DeleteProductSpuCommand) -> CommerceCatalogFuture<'a, ()> {
        Box::pin(async move { self.delete_spu(&command).await })
    }

    fn list_skus<'a>(
        &'a self,
        query: ProductSkuListQuery,
    ) -> CommerceCatalogFuture<'a, Vec<SkuRecord>> {
        Box::pin(async move { self.list_skus(&query).await })
    }

    fn list_skus_page<'a>(
        &'a self,
        query: ProductSkuListQuery,
    ) -> CommerceCatalogFuture<'a, CatalogOffsetPage<SkuRecord>> {
        Box::pin(async move {
            let items = self.list_skus(&query).await?;
            let total_items = self.count_skus(&query).await?;
            Ok(CatalogOffsetPage::new(
                items,
                query.page,
                query.page_size,
                total_items,
            ))
        })
    }

    fn retrieve_sku<'a>(
        &'a self,
        query: ProductSkuRetrieveQuery,
    ) -> CommerceCatalogFuture<'a, Option<SkuRecord>> {
        Box::pin(async move { self.retrieve_sku(&query).await })
    }

    fn retrieve_sku_prices<'a>(
        &'a self,
        query: SkuPriceRetrieveQuery,
    ) -> CommerceCatalogFuture<'a, Vec<PriceListItemRecord>> {
        Box::pin(async move { self.retrieve_sku_prices(&query).await })
    }

    fn create_sku<'a>(
        &'a self,
        command: CreateProductSkuCommand,
    ) -> CommerceCatalogFuture<'a, SkuRecord> {
        Box::pin(async move { self.create_sku(&command).await })
    }

    fn update_sku<'a>(
        &'a self,
        command: UpdateProductSkuCommand,
    ) -> CommerceCatalogFuture<'a, SkuRecord> {
        Box::pin(async move { self.update_sku(&command).await })
    }

    fn delete_sku<'a>(&'a self, command: DeleteProductSkuCommand) -> CommerceCatalogFuture<'a, ()> {
        Box::pin(async move { self.delete_sku(&command).await })
    }
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
        updated_at: value.updated_at,
    }
}

pub fn map_spu(value: SpuRecord) -> SpuResponse {
    SpuResponse {
        id: value.id,
        spu_no: value.spu_no,
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
        updated_at: value.updated_at,
    }
}

pub fn map_sku(value: SkuRecord) -> SkuResponse {
    SkuResponse {
        id: value.id,
        spu_id: value.spu_id,
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
        updated_at: value.updated_at,
    }
}

pub fn map_price_list_item(value: PriceListItemRecord) -> PriceListItemResponse {
    PriceListItemResponse {
        id: value.id,
        tenant_id: value.tenant_id,
        price_list_id: value.price_list_id,
        sku_id: value.sku_id,
        currency_code: value.currency_code,
        price_scale: value.price_scale,
        price_minor: value.price_minor,
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
        updated_at: value.updated_at,
    }
}

#[path = "backend_catalog_router.rs"]
mod backend_catalog_router;

pub use backend_catalog_router::{
    backend_catalog_router_with_postgres_pool, build_backend_catalog_router,
};
