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
    pub created_at: String,
    pub updated_at: String,
}

/// Read model of one `commerce_product_sku` row.
///
/// Money is exposed as exact integer minor units plus the scale snapshotted at write time. There is
/// deliberately no major-unit string here: a bare `"640.00"` cannot say whether it means 64000 or
/// 640000, which is the ambiguity `price_scale` exists to remove.
///
/// `variant_signature` is the deterministic signature of the SKU's sales-axis combination. Until
/// sales axes are an API input it falls back to `sku_no`, which still satisfies the
/// one-live-SKU-per-signature unique index without pretending an axis exists.
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
    pub created_at: String,
    pub updated_at: String,
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
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug)]
pub struct PriceListItemRecord {
    pub id: i64,
    pub tenant_id: i64,
    pub price_list_id: i64,
    pub sku_id: i64,
    pub currency_code: String,
    pub price_scale: i64,
    pub price_minor: i64,
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
    pub created_at: String,
    pub updated_at: String,
}

pub trait CatalogRepositoryPort: Send + Sync {
    fn list_categories(
        &self,
        query: &CategoryListQuery,
    ) -> Result<Vec<CategoryRecord>, CommerceServiceError>;

    fn create_category(
        &self,
        command: &CreateCategoryCommand,
    ) -> Result<CategoryRecord, CommerceServiceError>;

    fn update_category(
        &self,
        command: &UpdateCategoryCommand,
    ) -> Result<CategoryRecord, CommerceServiceError>;

    fn delete_category(&self, command: &DeleteCategoryCommand) -> Result<(), CommerceServiceError>;

    fn list_attributes(
        &self,
        query: &AttributeListQuery,
    ) -> Result<Vec<AttributeRecord>, CommerceServiceError>;

    fn create_attribute(
        &self,
        command: &CreateAttributeCommand,
    ) -> Result<AttributeRecord, CommerceServiceError>;

    fn list_price_lists(
        &self,
        query: &PriceListListQuery,
    ) -> Result<Vec<PriceListRecord>, CommerceServiceError>;

    fn create_price_list(
        &self,
        command: &CreatePriceListCommand,
    ) -> Result<PriceListRecord, CommerceServiceError>;

    fn update_price_list(
        &self,
        command: &UpdatePriceListCommand,
    ) -> Result<PriceListRecord, CommerceServiceError>;

    fn list_category_attributes(
        &self,
        query: &CategoryAttributeListQuery,
    ) -> Result<Vec<CategoryAttributeRecord>, CommerceServiceError>;

    fn create_category_attribute(
        &self,
        command: &CreateCategoryAttributeCommand,
    ) -> Result<CategoryAttributeRecord, CommerceServiceError>;

    fn update_category_attribute(
        &self,
        command: &UpdateCategoryAttributeCommand,
    ) -> Result<CategoryAttributeRecord, CommerceServiceError>;

    fn delete_category_attribute(
        &self,
        command: &DeleteCategoryAttributeCommand,
    ) -> Result<(), CommerceServiceError>;

    fn list_spus(
        &self,
        query: &ProductSpuListQuery,
    ) -> Result<Vec<SpuRecord>, CommerceServiceError>;

    fn retrieve_spu(
        &self,
        query: &ProductSpuRetrieveQuery,
    ) -> Result<Option<SpuRecord>, CommerceServiceError>;

    fn create_spu(
        &self,
        command: &CreateProductSpuCommand,
    ) -> Result<SpuRecord, CommerceServiceError>;

    fn update_spu(
        &self,
        command: &UpdateProductSpuCommand,
    ) -> Result<SpuRecord, CommerceServiceError>;

    fn delete_spu(&self, command: &DeleteProductSpuCommand) -> Result<(), CommerceServiceError>;

    fn publish_spu(&self, command: &PublishSpuCommand) -> Result<SpuRecord, CommerceServiceError>;

    fn archive_spu(&self, command: &ArchiveSpuCommand) -> Result<SpuRecord, CommerceServiceError>;

    fn list_skus(
        &self,
        query: &ProductSkuListQuery,
    ) -> Result<Vec<SkuRecord>, CommerceServiceError>;

    fn retrieve_sku(
        &self,
        query: &ProductSkuRetrieveQuery,
    ) -> Result<Option<SkuRecord>, CommerceServiceError>;

    fn create_sku(
        &self,
        command: &CreateProductSkuCommand,
    ) -> Result<SkuRecord, CommerceServiceError>;

    fn update_sku(
        &self,
        command: &UpdateProductSkuCommand,
    ) -> Result<SkuRecord, CommerceServiceError>;

    fn delete_sku(&self, command: &DeleteProductSkuCommand) -> Result<(), CommerceServiceError>;
}
