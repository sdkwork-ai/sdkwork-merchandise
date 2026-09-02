//! Shared merchandise store port, HTTP DTOs, and response mappers.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use sdkwork_contract_service::CommerceServiceError;
use sdkwork_merchandise_repository_sqlx::PostgresCommerceCatalogStore;
use sdkwork_merchandise_service::{
    AddCartItemCommand, AddressListQuery, AddressRecord, ArchiveSpuCommand, AttributeListQuery,
    AttributeRecord, CartItemRecord, CartRetrieveQuery, CategoryAttributeListQuery,
    CategoryAttributeRecord, CategoryListQuery, CategoryRecord, CategoryRetrieveQuery,
    CreateAddressCommand, CreateAttributeCommand, CreateCategoryAttributeCommand,
    CreateCategoryCommand, CreatePriceListCommand, CreateProductSkuCommand,
    CreateProductSpuCommand, DeleteAddressCommand, DeleteCategoryAttributeCommand,
    DeleteCategoryCommand, DeleteProductSkuCommand, DeleteProductSpuCommand, PriceListItemRecord,
    PriceListListQuery, PriceListRecord, ProductSkuListQuery, ProductSkuRetrieveQuery,
    ProductSpuListQuery, ProductSpuRetrieveQuery, PublishSpuCommand, RemoveCartItemCommand,
    SetDefaultAddressCommand, SkuPriceRetrieveQuery, SkuRecord, SpuRecord, UpdateAddressCommand,
    UpdateCartItemCommand, UpdateCategoryAttributeCommand, UpdateCategoryCommand,
    UpdatePriceListCommand, UpdateProductSkuCommand, UpdateProductSpuCommand,
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

    fn list_cart_items<'a>(
        &'a self,
        query: CartRetrieveQuery,
    ) -> CommerceCatalogFuture<'a, Vec<CartItemRecord>>;

    fn list_cart_items_page<'a>(
        &'a self,
        query: CartRetrieveQuery,
    ) -> CommerceCatalogFuture<'a, CatalogOffsetPage<CartItemRecord>>;

    fn add_cart_item<'a>(
        &'a self,
        command: AddCartItemCommand,
    ) -> CommerceCatalogFuture<'a, CartItemRecord>;

    fn update_cart_item<'a>(
        &'a self,
        command: UpdateCartItemCommand,
    ) -> CommerceCatalogFuture<'a, CartItemRecord>;

    fn remove_cart_item<'a>(
        &'a self,
        command: RemoveCartItemCommand,
    ) -> CommerceCatalogFuture<'a, ()>;

    fn list_addresses<'a>(
        &'a self,
        query: AddressListQuery,
    ) -> CommerceCatalogFuture<'a, Vec<AddressRecord>>;

    fn list_addresses_page<'a>(
        &'a self,
        query: AddressListQuery,
    ) -> CommerceCatalogFuture<'a, CatalogOffsetPage<AddressRecord>>;

    fn create_address<'a>(
        &'a self,
        command: CreateAddressCommand,
    ) -> CommerceCatalogFuture<'a, AddressRecord>;

    fn update_address<'a>(
        &'a self,
        command: UpdateAddressCommand,
    ) -> CommerceCatalogFuture<'a, AddressRecord>;

    fn delete_address<'a>(&'a self, command: DeleteAddressCommand)
        -> CommerceCatalogFuture<'a, ()>;

    fn set_default_address<'a>(
        &'a self,
        command: SetDefaultAddressCommand,
    ) -> CommerceCatalogFuture<'a, AddressRecord>;
}

#[derive(Clone)]
pub struct CatalogState {
    pub store: Arc<dyn CommerceCatalogStore>,
}

#[derive(Debug, Deserialize)]
pub struct CategoryQueryParams {
    pub organization_id: Option<String>,
    pub parent_id: Option<String>,
    pub status: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct AttributeQueryParams {
    pub organization_id: Option<String>,
    pub status: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct PriceListQueryParams {
    organization_id: Option<String>,
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CategoryAttributeQueryParams {
    organization_id: Option<String>,
    category_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SpuListQueryParams {
    pub organization_id: Option<String>,
    pub category_id: Option<String>,
    pub product_type: Option<String>,
    pub status: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct SkuListQueryParams {
    organization_id: Option<String>,
    spu_id: Option<String>,
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateCategoryAttributeBody {
    category_id: String,
    attribute_id: String,
    required: Option<bool>,
    searchable: Option<bool>,
    filterable: Option<bool>,
    sort_order: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateCategoryAttributeBody {
    required: Option<bool>,
    searchable: Option<bool>,
    filterable: Option<bool>,
    sort_order: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSpuBody {
    pub spu_no: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub description: Option<String>,
    pub product_type: String,
    pub category_id: Option<String>,
    pub visible_surfaces: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSpuBody {
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub description: Option<String>,
    pub category_id: Option<String>,
    pub visible_surfaces: Option<String>,
}

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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddCartItemBody {
    pub sku_id: String,
    pub quantity: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCartItemBody {
    pub quantity: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAddressBody {
    pub receiver_name: String,
    pub receiver_phone: String,
    pub country_code: String,
    pub province: String,
    pub city: String,
    pub detail_address: String,
    pub is_default: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAddressBody {
    pub receiver_name: Option<String>,
    pub receiver_phone: Option<String>,
    pub province: Option<String>,
    pub city: Option<String>,
    pub detail_address: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryResponse {
    id: String,
    category_no: String,
    parent_id: Option<String>,
    path: String,
    level_no: i64,
    name: String,
    sort_order: i64,
    status: String,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttributeResponse {
    id: String,
    attribute_no: String,
    name: String,
    value_type: String,
    scope: String,
    status: String,
    sort_order: i64,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpuResponse {
    id: String,
    spu_no: String,
    title: String,
    subtitle: Option<String>,
    description: Option<String>,
    product_type: String,
    category_id: Option<String>,
    status: String,
    published_at: Option<String>,
    visible_surfaces: String,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkuResponse {
    id: String,
    spu_id: String,
    sku_no: String,
    name: String,
    title: String,
    price_amount: String,
    original_price_amount: Option<String>,
    currency_code: String,
    fulfillment_type: String,
    inventory_tracking: String,
    status: String,
    published_at: Option<String>,
    spec_json: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceListResponse {
    id: String,
    tenant_id: String,
    organization_id: Option<String>,
    price_list_no: String,
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
    id: String,
    tenant_id: String,
    price_list_id: String,
    sku_id: String,
    price_amount: String,
    currency_code: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryAttributeResponse {
    id: String,
    tenant_id: String,
    organization_id: Option<String>,
    category_id: String,
    attribute_id: String,
    required: bool,
    searchable: bool,
    filterable: bool,
    sort_order: i64,
    status: String,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CartItemResponse {
    id: String,
    sku_id: String,
    quantity: i64,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddressResponse {
    id: String,
    receiver_name: String,
    receiver_phone: String,
    country_code: String,
    province: String,
    city: String,
    detail_address: String,
    is_default: bool,
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
        _query: CategoryAttributeListQuery,
    ) -> CommerceCatalogFuture<'a, Vec<CategoryAttributeRecord>> {
        Box::pin(async move {
            Err(CommerceServiceError::storage(
                "category attribute listing is not yet implemented for postgres",
            ))
        })
    }

    fn create_category_attribute<'a>(
        &'a self,
        _command: CreateCategoryAttributeCommand,
    ) -> CommerceCatalogFuture<'a, CategoryAttributeRecord> {
        Box::pin(async move {
            Err(CommerceServiceError::storage(
                "category attribute creation is not yet implemented for postgres",
            ))
        })
    }

    fn update_category_attribute<'a>(
        &'a self,
        _command: UpdateCategoryAttributeCommand,
    ) -> CommerceCatalogFuture<'a, CategoryAttributeRecord> {
        Box::pin(async move {
            Err(CommerceServiceError::storage(
                "category attribute update is not yet implemented for postgres",
            ))
        })
    }

    fn delete_category_attribute<'a>(
        &'a self,
        _command: DeleteCategoryAttributeCommand,
    ) -> CommerceCatalogFuture<'a, ()> {
        Box::pin(async move {
            Err(CommerceServiceError::storage(
                "category attribute deletion is not yet implemented for postgres",
            ))
        })
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
        _query: SkuPriceRetrieveQuery,
    ) -> CommerceCatalogFuture<'a, Vec<PriceListItemRecord>> {
        Box::pin(async move {
            Err(CommerceServiceError::storage(
                "sku price retrieval is not yet implemented for postgres",
            ))
        })
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

    fn list_cart_items<'a>(
        &'a self,
        query: CartRetrieveQuery,
    ) -> CommerceCatalogFuture<'a, Vec<CartItemRecord>> {
        Box::pin(async move { self.list_cart_items(&query).await })
    }

    fn list_cart_items_page<'a>(
        &'a self,
        query: CartRetrieveQuery,
    ) -> CommerceCatalogFuture<'a, CatalogOffsetPage<CartItemRecord>> {
        Box::pin(async move {
            let items = self.list_cart_items(&query).await?;
            let total_items = self.count_cart_items(&query).await?;
            Ok(CatalogOffsetPage::new(
                items,
                query.page,
                query.page_size,
                total_items,
            ))
        })
    }

    fn add_cart_item<'a>(
        &'a self,
        command: AddCartItemCommand,
    ) -> CommerceCatalogFuture<'a, CartItemRecord> {
        Box::pin(async move { self.add_cart_item(&command).await })
    }

    fn update_cart_item<'a>(
        &'a self,
        command: UpdateCartItemCommand,
    ) -> CommerceCatalogFuture<'a, CartItemRecord> {
        Box::pin(async move { self.update_cart_item(&command).await })
    }

    fn remove_cart_item<'a>(
        &'a self,
        command: RemoveCartItemCommand,
    ) -> CommerceCatalogFuture<'a, ()> {
        Box::pin(async move { self.remove_cart_item(&command).await })
    }

    fn list_addresses<'a>(
        &'a self,
        query: AddressListQuery,
    ) -> CommerceCatalogFuture<'a, Vec<AddressRecord>> {
        Box::pin(async move { self.list_addresses(&query).await })
    }

    fn list_addresses_page<'a>(
        &'a self,
        query: AddressListQuery,
    ) -> CommerceCatalogFuture<'a, CatalogOffsetPage<AddressRecord>> {
        Box::pin(async move {
            let items = self.list_addresses(&query).await?;
            let total_items = self.count_addresses(&query).await?;
            Ok(CatalogOffsetPage::new(
                items,
                query.page,
                query.page_size,
                total_items,
            ))
        })
    }

    fn create_address<'a>(
        &'a self,
        command: CreateAddressCommand,
    ) -> CommerceCatalogFuture<'a, AddressRecord> {
        Box::pin(async move { self.create_address(&command).await })
    }

    fn update_address<'a>(
        &'a self,
        command: UpdateAddressCommand,
    ) -> CommerceCatalogFuture<'a, AddressRecord> {
        Box::pin(async move { self.update_address(&command).await })
    }

    fn delete_address<'a>(
        &'a self,
        command: DeleteAddressCommand,
    ) -> CommerceCatalogFuture<'a, ()> {
        Box::pin(async move { self.delete_address(&command).await })
    }

    fn set_default_address<'a>(
        &'a self,
        command: SetDefaultAddressCommand,
    ) -> CommerceCatalogFuture<'a, AddressRecord> {
        Box::pin(async move { self.set_default_address(&command).await })
    }
}

pub fn map_category(value: CategoryRecord) -> CategoryResponse {
    CategoryResponse {
        id: value.id,
        category_no: value.category_no,
        parent_id: value.parent_id,
        path: value.path,
        level_no: value.level_no,
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
        scope: value.scope,
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
        title: value.title,
        subtitle: value.subtitle,
        description: value.description,
        product_type: value.product_type,
        category_id: value.category_id,
        status: value.status,
        published_at: value.published_at,
        visible_surfaces: value.visible_surfaces,
        created_at: value.created_at,
        updated_at: value.updated_at,
    }
}

pub fn map_sku(value: SkuRecord) -> SkuResponse {
    SkuResponse {
        id: value.id,
        spu_id: value.spu_id,
        sku_no: value.sku_no,
        name: value.name,
        title: value.title,
        price_amount: value.price_amount,
        original_price_amount: value.original_price_amount,
        currency_code: value.currency_code,
        fulfillment_type: value.fulfillment_type,
        inventory_tracking: value.inventory_tracking,
        status: value.status,
        published_at: value.published_at,
        spec_json: value.spec_json,
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
        price_amount: value.price_amount,
        currency_code: value.currency_code,
    }
}

fn map_category_attribute(value: CategoryAttributeRecord) -> CategoryAttributeResponse {
    CategoryAttributeResponse {
        id: value.id,
        tenant_id: value.tenant_id,
        organization_id: value.organization_id,
        category_id: value.category_id,
        attribute_id: value.attribute_id,
        required: value.required,
        searchable: value.searchable,
        filterable: value.filterable,
        sort_order: value.sort_order,
        status: value.status,
        created_at: value.created_at,
        updated_at: value.updated_at,
    }
}

pub fn map_cart_item(value: CartItemRecord) -> CartItemResponse {
    CartItemResponse {
        id: value.id,
        sku_id: value.sku_id,
        quantity: value.quantity,
        created_at: value.created_at,
        updated_at: value.updated_at,
    }
}

pub fn map_address(value: AddressRecord) -> AddressResponse {
    AddressResponse {
        id: value.id,
        receiver_name: value.receiver_name,
        receiver_phone: value.receiver_phone,
        country_code: value.country_code,
        province: value.province,
        city: value.city,
        detail_address: value.detail_address,
        is_default: value.is_default,
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
