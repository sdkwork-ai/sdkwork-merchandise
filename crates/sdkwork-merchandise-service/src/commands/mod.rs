use sdkwork_contract_service::{CommerceMoney, CommerceServiceError};

use crate::domain::{
    AttributeRole, FulfillmentType, InventoryTrackingMode, LifecycleStatus, ProductStatus,
    ProductType,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateCategoryCommand {
    pub tenant_id: String,
    pub organization_id: String,
    pub category_no: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub sort_order: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateCategoryCommand {
    pub tenant_id: String,
    pub category_id: String,
    pub parent_id: Option<String>,
    pub name: Option<String>,
    pub sort_order: Option<i64>,
    pub status: Option<LifecycleStatus>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeleteCategoryCommand {
    pub tenant_id: String,
    pub category_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateAttributeCommand {
    pub tenant_id: String,
    pub organization_id: String,
    pub attribute_no: String,
    pub name: String,
    pub values: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreatePriceListCommand {
    pub tenant_id: String,
    pub organization_id: String,
    pub price_list_no: String,
    pub currency_code: String,
    pub market_code: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdatePriceListCommand {
    pub tenant_id: String,
    pub price_list_id: String,
    pub status: Option<LifecycleStatus>,
    pub starts_at: Option<String>,
    pub ends_at: Option<String>,
}

/// Creates a product SPU.
///
/// `category_id` is required: `commerce_product_spu.category_id` is `NOT NULL`, because a product
/// outside every category cannot be merchandised, filtered, or browsed.
///
/// There is no `name` field. `commerce_product_spu.name` is the default-locale display name and
/// the baseline carries it as a required column, so the repository derives it from `title` — the
/// same pairing the baseline catalog seed uses. Other locales live in
/// `commerce_product_spu_translation`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateProductSpuCommand {
    pub tenant_id: String,
    pub organization_id: String,
    pub spu_no: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub description: Option<String>,
    pub product_type: ProductType,
    pub category_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateProductSpuCommand {
    pub tenant_id: String,
    pub spu_id: String,
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub description: Option<String>,
    pub category_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeleteProductSpuCommand {
    pub tenant_id: String,
    pub spu_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishSpuCommand {
    pub tenant_id: String,
    pub spu_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveSpuCommand {
    pub tenant_id: String,
    pub spu_id: String,
}

/// Creates a sellable SKU.
///
/// `price_amount` is the major-denomination amount being charged and `original_price_amount` is the
/// optional reference price. The repository resolves the currency's `minor_unit_exponent` from
/// `commerce_currency` and stores exact minor units; no layer divides or multiplies by a literal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateProductSkuCommand {
    pub tenant_id: String,
    pub organization_id: String,
    pub spu_id: String,
    pub sku_no: String,
    pub name: String,
    pub title: String,
    pub price_amount: CommerceMoney,
    pub original_price_amount: Option<CommerceMoney>,
    pub currency_code: String,
    pub fulfillment_type: FulfillmentType,
    pub inventory_tracking: InventoryTrackingMode,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateProductSkuCommand {
    pub tenant_id: String,
    pub sku_id: String,
    pub name: Option<String>,
    pub title: Option<String>,
    pub price_amount: Option<CommerceMoney>,
    pub original_price_amount: Option<CommerceMoney>,
    pub currency_code: Option<String>,
    pub fulfillment_type: Option<FulfillmentType>,
    pub inventory_tracking: Option<InventoryTrackingMode>,
    pub status: Option<ProductStatus>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeleteProductSkuCommand {
    pub tenant_id: String,
    pub sku_id: String,
}

/// Binds an attribute into a category's template.
///
/// `role` is the reason this command exists at all: the same attribute is a sales axis in one
/// category and a plain specification in another, so the parameter/sales split is decided here, per
/// category, and never on `commerce_product_attribute`.
///
/// `source_category_id` records that this binding was inherited from another category rather than
/// authored here. The baseline constrains it with `source_category_id IS NULL OR
/// source_category_id <> category_id`, so a category cannot claim to inherit from itself.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateCategoryAttributeCommand {
    pub tenant_id: String,
    pub organization_id: String,
    pub category_id: String,
    pub attribute_id: String,
    pub role: AttributeRole,
    pub required: bool,
    pub searchable: bool,
    pub filterable: bool,
    pub comparable: bool,
    pub source_category_id: Option<String>,
    pub sort_order: i64,
}

/// Updates one binding.
///
/// Omitted fields are left unchanged. `role` is changeable because reclassifying an attribute is a
/// normal merchandising edit, and `status` is how a binding is retired from the template without
/// destroying the history.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateCategoryAttributeCommand {
    pub tenant_id: String,
    pub binding_id: String,
    pub role: Option<AttributeRole>,
    pub required: Option<bool>,
    pub searchable: Option<bool>,
    pub filterable: Option<bool>,
    pub comparable: Option<bool>,
    pub sort_order: Option<i64>,
    pub status: Option<LifecycleStatus>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeleteCategoryAttributeCommand {
    pub tenant_id: String,
    pub binding_id: String,
}

macro_rules! impl_required_text_command {
    ($cmd:ident, $($field:ident),+) => {
        impl $cmd {
            pub fn validate(&self) -> Result<(), CommerceServiceError> {
                $(
                    crate::validation::require_non_empty(stringify!($field), &self.$field)?;
                )+
                Ok(())
            }
        }
    };
}

impl_required_text_command!(
    CreateCategoryCommand,
    tenant_id,
    organization_id,
    category_no,
    name
);
impl_required_text_command!(UpdateCategoryCommand, tenant_id, category_id);
impl_required_text_command!(DeleteCategoryCommand, tenant_id, category_id);
/// `CreateAttributeCommand` validates its values as well as its text fields.
///
/// The repository inserts one `commerce_product_attribute_value` row per entry, and the baseline
/// pins that row with `char_length(display_value) BETWEEN 1 AND 200`. Validating here keeps a blank
/// or over-long value a `422` naming the field instead of a `23514` raised mid-transaction inside
/// PostgreSQL.
impl CreateAttributeCommand {
    pub fn validate(&self) -> Result<(), CommerceServiceError> {
        crate::validation::require_non_empty("tenant_id", &self.tenant_id)?;
        crate::validation::require_non_empty("organization_id", &self.organization_id)?;
        crate::validation::require_non_empty("attribute_no", &self.attribute_no)?;
        crate::validation::require_non_empty("name", &self.name)?;
        for value in &self.values {
            crate::validation::require_non_empty("values[]", value)?;
            crate::validation::require_within_chars(
                "values[]",
                value,
                crate::validation::ATTRIBUTE_VALUE_MAX_CHARS,
            )?;
        }
        Ok(())
    }
}
impl_required_text_command!(
    CreatePriceListCommand,
    tenant_id,
    organization_id,
    price_list_no,
    currency_code
);
impl_required_text_command!(UpdatePriceListCommand, tenant_id, price_list_id);
// `product_type` and `category_id` are typed/required, so presence is guaranteed by the type
// rather than by a runtime string check.
impl_required_text_command!(
    CreateProductSpuCommand,
    tenant_id,
    organization_id,
    spu_no,
    title,
    category_id
);
impl_required_text_command!(UpdateProductSpuCommand, tenant_id, spu_id);
impl_required_text_command!(DeleteProductSpuCommand, tenant_id, spu_id);
impl_required_text_command!(PublishSpuCommand, tenant_id, spu_id);
impl_required_text_command!(ArchiveSpuCommand, tenant_id, spu_id);
impl_required_text_command!(
    CreateProductSkuCommand,
    tenant_id,
    organization_id,
    spu_id,
    sku_no,
    name,
    title,
    currency_code
);
impl_required_text_command!(UpdateProductSkuCommand, tenant_id, sku_id);
impl_required_text_command!(DeleteProductSkuCommand, tenant_id, sku_id);
/// `CreateCategoryAttributeCommand` validates its inheritance source as well as its text fields.
///
/// `ck_commerce_product_category_attribute_source_not_self` rejects
/// `source_category_id = category_id`. Checking it here keeps the failure a `422` naming the field
/// instead of a `23514` raised mid-transaction inside PostgreSQL.
impl CreateCategoryAttributeCommand {
    pub fn validate(&self) -> Result<(), CommerceServiceError> {
        crate::validation::require_non_empty("tenant_id", &self.tenant_id)?;
        crate::validation::require_non_empty("organization_id", &self.organization_id)?;
        crate::validation::require_non_empty("category_id", &self.category_id)?;
        crate::validation::require_non_empty("attribute_id", &self.attribute_id)?;
        if let Some(source) = self.source_category_id.as_deref() {
            if source.trim() == self.category_id.trim() {
                return Err(CommerceServiceError::validation(
                    "source_category_id must differ from category_id: a category cannot inherit an \
                     attribute binding from itself",
                ));
            }
        }
        Ok(())
    }
}
impl_required_text_command!(UpdateCategoryAttributeCommand, tenant_id, binding_id);
impl_required_text_command!(DeleteCategoryAttributeCommand, tenant_id, binding_id);
