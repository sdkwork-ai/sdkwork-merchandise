use sdkwork_contract_service::CommerceServiceError;
use serde_json::Value;

use crate::domain::{
    AttributeRole, FulfillmentType, InventoryTrackingMode, LifecycleStatus, MediaOwnerType,
    MediaRole, ProductStatus, ProductType,
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
    /// The row version the caller read, taken from the request's `If-Match` precondition.
    ///
    /// A mismatch is not a malformed request: it means somebody else wrote the row since this
    /// caller read it, so the caller's copy is stale and the write would silently discard their
    /// change. The repository compares this against `version` inside the same statement that
    /// writes, which is what makes the comparison race-free.
    pub expected_version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeleteCategoryCommand {
    pub tenant_id: String,
    pub category_id: String,
    /// The row version the caller read, taken from the request's `If-Match` precondition.
    ///
    /// A mismatch is not a malformed request: it means somebody else wrote the row since this
    /// caller read it, so the caller's copy is stale and the write would silently discard their
    /// change. The repository compares this against `version` inside the same statement that
    /// writes, which is what makes the comparison race-free.
    pub expected_version: i64,
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
    /// The row version the caller read, taken from the request's `If-Match` precondition.
    ///
    /// A mismatch is not a malformed request: it means somebody else wrote the row since this
    /// caller read it, so the caller's copy is stale and the write would silently discard their
    /// change. The repository compares this against `version` inside the same statement that
    /// writes, which is what makes the comparison race-free.
    pub expected_version: i64,
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
    /// The product description. Three-state: `None` leaves the stored text alone, `Some(None)` clears
    /// it, and `Some(Some(text))` replaces it.
    ///
    /// Clearing is stated rather than inferred because `commerce_product_spu.description` is a
    /// nullable column, so `NULL` **is** the absent description and there is no in-band empty value to
    /// stand for it — unlike `metadata`, whose `{}` genuinely means "no metadata". Without the third
    /// state, "this product no longer has a description" and "this edit did not mention the
    /// description" would be one instruction, and the repository's `COALESCE` would resolve both to
    /// "keep whatever is there".
    ///
    /// `subtitle` keeps its two states on purpose: nothing removes a subtitle, and a third state with
    /// no consumer is speculation. `title` cannot need one — `commerce_product_spu.name` is derived
    /// from it and is `NOT NULL` with a `char_length BETWEEN 1 AND 300` CHECK.
    pub description: Option<Option<String>>,
    pub category_id: Option<String>,
    /// The row version the caller read, taken from the request's `If-Match` precondition.
    ///
    /// A mismatch is not a malformed request: it means somebody else wrote the row since this
    /// caller read it, so the caller's copy is stale and the write would silently discard their
    /// change. The repository compares this against `version` inside the same statement that
    /// writes, which is what makes the comparison race-free.
    pub expected_version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeleteProductSpuCommand {
    pub tenant_id: String,
    pub spu_id: String,
    /// The row version the caller read, taken from the request's `If-Match` precondition.
    ///
    /// A mismatch is not a malformed request: it means somebody else wrote the row since this
    /// caller read it, so the caller's copy is stale and the write would silently discard their
    /// change. The repository compares this against `version` inside the same statement that
    /// writes, which is what makes the comparison race-free.
    pub expected_version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishSpuCommand {
    pub tenant_id: String,
    pub spu_id: String,
    /// The row version the caller read, taken from the request's `If-Match` precondition.
    ///
    /// A mismatch is not a malformed request: it means somebody else wrote the row since this
    /// caller read it, so the caller's copy is stale and the write would silently discard their
    /// change. The repository compares this against `version` inside the same statement that
    /// writes, which is what makes the comparison race-free.
    pub expected_version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveSpuCommand {
    pub tenant_id: String,
    pub spu_id: String,
    /// The row version the caller read, taken from the request's `If-Match` precondition.
    ///
    /// A mismatch is not a malformed request: it means somebody else wrote the row since this
    /// caller read it, so the caller's copy is stale and the write would silently discard their
    /// change. The repository compares this against `version` inside the same statement that
    /// writes, which is what makes the comparison race-free.
    pub expected_version: i64,
}

/// Creates a sellable SKU.
///
/// `sale_price_minor` is the amount being charged and `list_price_minor` the optional reference
/// price, both as **integer minor units** of `currency_code`. `API_SPEC` section 13.2.1 fixes the
/// wire unit at the currency's smallest indivisible unit (fen for CNY, yen for JPY), so the amount
/// arrives already minor and nothing on the way in multiplies or divides it. The currency's
/// `minor_unit_exponent` is still read from `commerce_currency` — it is what the row's
/// `price_scale` snapshot records, not a conversion factor.
///
/// # The variant axis
///
/// `attribute_value_ids` is the SKU's position on each sales axis: the caller submits the
/// **dictionary value ids** and nothing else. The attribute behind each value is derived from
/// `commerce_product_attribute_value` inside the same transaction, which is what makes it
/// impossible for a row to name an attribute and a value that disagree.
///
/// The repository turns the submitted set into `variant_signature`, and
/// `uk_commerce_product_sku_variant` makes "one live SKU per axis combination" enforceable. A SKU
/// whose category declares no sales axis must submit an empty set and falls back to its own
/// `sku_no` as the signature — see `docs/architecture/tech/TECH_ARCHITECTURE.md` section 3.
#[derive(Clone, Debug, PartialEq)]
pub struct CreateProductSkuCommand {
    pub tenant_id: String,
    pub organization_id: String,
    pub spu_id: String,
    pub sku_no: String,
    pub name: String,
    pub title: String,
    pub sale_price_minor: i64,
    pub list_price_minor: Option<i64>,
    pub currency_code: String,
    pub fulfillment_type: FulfillmentType,
    pub inventory_tracking: InventoryTrackingMode,
    pub attribute_value_ids: Vec<String>,
    /// Capability-owned metadata for a SKU that sells a service rather than a physical good.
    ///
    /// A capability whose SKU carries fields the catalog has no column for — a notary matter's
    /// `spec`, for instance — writes them here and reads them back unchanged. The baseline keeps one
    /// carrier per fact, so this is **not** a second home for anything that already has one: sales
    /// axes belong to `commerce_product_sku_attribute`, translations to `*_translation`, and display
    /// copy to the text columns. Must be a JSON object; `{}` means "no capability metadata".
    ///
    /// `Eq` is deliberately not derived, matching `CreateMediaCommand`: `serde_json::Value` is only
    /// `PartialEq`, because two JSON numbers are equal when the reals they denote are.
    pub metadata: Value,
}

/// Updates a SKU.
///
/// `attribute_value_ids` is a three-state field, and the states are deliberately distinct:
/// `None` leaves the axes untouched, `Some(vec![])` clears them, and any other `Some` replaces the
/// whole set. A `Vec` that could only append would make "this SKU is no longer sold in red"
/// inexpressible, and a `Vec` that always replaced would erase the axes of every price-only edit.
///
/// `metadata` is a three-state field for the same reason: `None` preserves the stored object and
/// `Some(object)` replaces it, so a price-only edit never disturbs the capability's own fields.
///
/// `list_price_minor` is the third three-state field here, and it is the one the earlier two-state
/// shape could not express. `None` leaves the stored reference price alone, `Some(None)` clears it,
/// and `Some(Some(minor))` replaces it. With a plain `Option<i64>` the first two collapsed into one
/// value, so "this product no longer has a strike-through price" had no representation at all and a
/// caller could only ever restate a price. The HTTP contract publishes the distinction
/// (`listPriceMinor` is nullable), and the repository applies it inside the same statement that
/// reads the row, so nothing has to be inferred from a sentinel amount.
#[derive(Clone, Debug, PartialEq)]
pub struct UpdateProductSkuCommand {
    pub tenant_id: String,
    pub sku_id: String,
    pub name: Option<String>,
    pub title: Option<String>,
    pub sale_price_minor: Option<i64>,
    pub list_price_minor: Option<Option<i64>>,
    pub currency_code: Option<String>,
    pub fulfillment_type: Option<FulfillmentType>,
    pub inventory_tracking: Option<InventoryTrackingMode>,
    pub status: Option<ProductStatus>,
    pub attribute_value_ids: Option<Vec<String>>,
    pub metadata: Option<Value>,
    /// The row version the caller read, taken from the request's `If-Match` precondition.
    ///
    /// A mismatch is not a malformed request: it means somebody else wrote the row since this
    /// caller read it, so the caller's copy is stale and the write would silently discard their
    /// change. The repository compares this against `version` inside the same statement that
    /// writes, which is what makes the comparison race-free.
    pub expected_version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeleteProductSkuCommand {
    pub tenant_id: String,
    pub sku_id: String,
    /// The row version the caller read, taken from the request's `If-Match` precondition.
    ///
    /// A mismatch is not a malformed request: it means somebody else wrote the row since this
    /// caller read it, so the caller's copy is stale and the write would silently discard their
    /// change. The repository compares this against `version` inside the same statement that
    /// writes, which is what makes the comparison race-free.
    pub expected_version: i64,
}

/// Attaches one media resource to one catalog owner.
///
/// # The reference and the snapshot are two different things
///
/// `resource_snapshot` is the caller's `MediaResource` (`MEDIA_RESOURCE_SPEC` section 3) and
/// `media_resource_id` is derived from its `id`. Bytes, object keys, upload sessions, retention,
/// and presigned grants belong to Drive and are never stored here; what this command carries is the
/// stable *identity* plus a read-model projection of the descriptive fields, exactly as
/// `MEDIA_RESOURCE_SPEC` section 5 requires. `commerce_product_media` has no `url` column, and a
/// presigned URL must never be the system of record.
///
/// The owner is a `(owner_type, owner_id)` pair rather than a nullable foreign key per owner table:
/// PostgreSQL cannot express "this BIGINT points at one of four tables", so the repository verifies
/// the owner exists for the declared kind instead of leaving an orphan row behind.
#[derive(Clone, Debug, PartialEq)]
pub struct CreateMediaCommand {
    pub tenant_id: String,
    pub organization_id: String,
    pub owner_type: MediaOwnerType,
    pub owner_id: String,
    pub media_role: MediaRole,
    pub resource_snapshot: serde_json::Value,
    pub alt_text: Option<String>,
    pub sort_order: i64,
}

/// Updates one media attachment.
///
/// The owner is not a field: moving an attachment between owners is a delete plus a create. What
/// changes is which resource occupies the slot, how it is described, and whether it is live.
///
/// `media_role` is re-evaluated against the row's **stored** `owner_type` by the repository, because
/// `ck_commerce_product_media_owner_role` constrains the pair and the caller cannot name the owner
/// kind on this operation.
#[derive(Clone, Debug, PartialEq)]
pub struct UpdateMediaCommand {
    pub tenant_id: String,
    pub media_id: String,
    pub media_role: Option<MediaRole>,
    pub resource_snapshot: Option<serde_json::Value>,
    pub alt_text: Option<String>,
    pub sort_order: Option<i64>,
    pub status: Option<LifecycleStatus>,
    /// The row version the caller read, taken from the request's `If-Match` precondition.
    ///
    /// A mismatch is not a malformed request: it means somebody else wrote the row since this
    /// caller read it, so the caller's copy is stale and the write would silently discard their
    /// change. The repository compares this against `version` inside the same statement that
    /// writes, which is what makes the comparison race-free.
    pub expected_version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeleteMediaCommand {
    pub tenant_id: String,
    pub media_id: String,
    /// The row version the caller read, taken from the request's `If-Match` precondition.
    ///
    /// A mismatch is not a malformed request: it means somebody else wrote the row since this
    /// caller read it, so the caller's copy is stale and the write would silently discard their
    /// change. The repository compares this against `version` inside the same statement that
    /// writes, which is what makes the comparison race-free.
    pub expected_version: i64,
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
    /// The row version the caller read, taken from the request's `If-Match` precondition.
    ///
    /// A mismatch is not a malformed request: it means somebody else wrote the row since this
    /// caller read it, so the caller's copy is stale and the write would silently discard their
    /// change. The repository compares this against `version` inside the same statement that
    /// writes, which is what makes the comparison race-free.
    pub expected_version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeleteCategoryAttributeCommand {
    pub tenant_id: String,
    pub binding_id: String,
    /// The row version the caller read, taken from the request's `If-Match` precondition.
    ///
    /// A mismatch is not a malformed request: it means somebody else wrote the row since this
    /// caller read it, so the caller's copy is stale and the write would silently discard their
    /// change. The repository compares this against `version` inside the same statement that
    /// writes, which is what makes the comparison race-free.
    pub expected_version: i64,
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

/// `CreateCategoryCommand` validates its name against the baseline's `char_length` bound.
///
/// `ck_commerce_product_category_name_length` admits 1..200 characters. Without the check here a
/// 201-character name travels to PostgreSQL and returns as a `23514` the caller cannot act on, and
/// the contract could not honestly declare the bound either.
impl CreateCategoryCommand {
    pub fn validate(&self) -> Result<(), CommerceServiceError> {
        crate::validation::require_non_empty("tenant_id", &self.tenant_id)?;
        crate::validation::require_non_empty("organization_id", &self.organization_id)?;
        crate::validation::require_non_empty("category_no", &self.category_no)?;
        crate::validation::require_non_empty("name", &self.name)?;
        crate::validation::require_within_chars(
            "name",
            &self.name,
            crate::validation::CATEGORY_NAME_MAX_CHARS,
        )?;
        Ok(())
    }
}
impl UpdateCategoryCommand {
    pub fn validate(&self) -> Result<(), CommerceServiceError> {
        crate::validation::require_non_empty("tenant_id", &self.tenant_id)?;
        crate::validation::require_non_empty("category_id", &self.category_id)?;
        if let Some(name) = self.name.as_deref() {
            crate::validation::require_non_empty("name", name)?;
            crate::validation::require_within_chars(
                "name",
                name,
                crate::validation::CATEGORY_NAME_MAX_CHARS,
            )?;
        }
        Ok(())
    }
}
impl_required_text_command!(DeleteCategoryCommand, tenant_id, category_id);
/// `CreateAttributeCommand` validates its values as well as its text fields.
///
/// The repository inserts one `commerce_product_attribute_value` row per entry, and the baseline
/// pins that row with `char_length(display_value) BETWEEN 1 AND 200`. Validating here keeps a blank
/// or over-long value a `422` naming the field instead of a `23514` raised mid-transaction inside
/// PostgreSQL.
///
/// `name` carries `ck_commerce_product_attribute_name_length` (1..100) for the same reason.
impl CreateAttributeCommand {
    pub fn validate(&self) -> Result<(), CommerceServiceError> {
        crate::validation::require_non_empty("tenant_id", &self.tenant_id)?;
        crate::validation::require_non_empty("organization_id", &self.organization_id)?;
        crate::validation::require_non_empty("attribute_no", &self.attribute_no)?;
        crate::validation::require_non_empty("name", &self.name)?;
        crate::validation::require_within_chars(
            "name",
            &self.name,
            crate::validation::ATTRIBUTE_NAME_MAX_CHARS,
        )?;
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
/// `CreatePriceListCommand` validates `price_list_no` against the derived name's bound.
///
/// `commerce_price_list.name` is `NOT NULL` with `char_length(name) BETWEEN 1 AND 200` and the
/// repository derives it from `price_list_no`, so the number is what must fit.
impl CreatePriceListCommand {
    pub fn validate(&self) -> Result<(), CommerceServiceError> {
        crate::validation::require_non_empty("tenant_id", &self.tenant_id)?;
        crate::validation::require_non_empty("organization_id", &self.organization_id)?;
        crate::validation::require_non_empty("price_list_no", &self.price_list_no)?;
        crate::validation::require_non_empty("currency_code", &self.currency_code)?;
        crate::validation::require_within_chars(
            "price_list_no",
            &self.price_list_no,
            crate::validation::PRICE_LIST_NO_MAX_CHARS,
        )?;
        Ok(())
    }
}
impl_required_text_command!(UpdatePriceListCommand, tenant_id, price_list_id);
// `product_type` and `category_id` are typed/required, so presence is guaranteed by the type
// rather than by a runtime string check.
//
// `title` bounds `commerce_product_spu.name`, which the repository derives from it and the baseline
// pins with `char_length(name) BETWEEN 1 AND 300`.
impl CreateProductSpuCommand {
    pub fn validate(&self) -> Result<(), CommerceServiceError> {
        crate::validation::require_non_empty("tenant_id", &self.tenant_id)?;
        crate::validation::require_non_empty("organization_id", &self.organization_id)?;
        crate::validation::require_non_empty("spu_no", &self.spu_no)?;
        crate::validation::require_non_empty("title", &self.title)?;
        crate::validation::require_non_empty("category_id", &self.category_id)?;
        crate::validation::require_within_chars(
            "title",
            &self.title,
            crate::validation::SPU_TITLE_MAX_CHARS,
        )?;
        Ok(())
    }
}
impl UpdateProductSpuCommand {
    pub fn validate(&self) -> Result<(), CommerceServiceError> {
        crate::validation::require_non_empty("tenant_id", &self.tenant_id)?;
        crate::validation::require_non_empty("spu_id", &self.spu_id)?;
        if let Some(title) = self.title.as_deref() {
            crate::validation::require_non_empty("title", title)?;
            crate::validation::require_within_chars(
                "title",
                title,
                crate::validation::SPU_TITLE_MAX_CHARS,
            )?;
        }
        Ok(())
    }
}
impl_required_text_command!(DeleteProductSpuCommand, tenant_id, spu_id);
impl_required_text_command!(PublishSpuCommand, tenant_id, spu_id);
impl_required_text_command!(ArchiveSpuCommand, tenant_id, spu_id);
/// `CreateProductSkuCommand` validates its prices and its axis set as well as its text fields.
///
/// The baseline pins both amounts with `>= 0` and the wire pattern is `^[0-9]+$`, so a negative
/// value is a caller error rather than a price. It is checked here so a non-HTTP caller cannot put
/// a value the CHECK would reject in front of PostgreSQL, and so the HTTP caller sees a named
/// `422` instead of a `23514`.
///
/// The axis set must be non-empty per entry and free of duplicates. The repository resolves each
/// value id to its attribute and compares the resolved set with the category's declared sales axes;
/// neither question can be answered from the command alone.
impl CreateProductSkuCommand {
    pub fn validate(&self) -> Result<(), CommerceServiceError> {
        crate::validation::require_non_empty("tenant_id", &self.tenant_id)?;
        crate::validation::require_non_empty("organization_id", &self.organization_id)?;
        crate::validation::require_non_empty("spu_id", &self.spu_id)?;
        crate::validation::require_non_empty("sku_no", &self.sku_no)?;
        crate::validation::require_non_empty("name", &self.name)?;
        crate::validation::require_non_empty("title", &self.title)?;
        crate::validation::require_non_empty("currency_code", &self.currency_code)?;
        crate::validation::require_non_negative_minor("sale_price_minor", self.sale_price_minor)?;
        if let Some(list) = self.list_price_minor {
            crate::validation::require_non_negative_minor("list_price_minor", list)?;
        }
        crate::validation::require_distinct("attribute_value_ids", &self.attribute_value_ids)?;
        require_metadata_object(&self.metadata)?;
        Ok(())
    }
}
impl UpdateProductSkuCommand {
    pub fn validate(&self) -> Result<(), CommerceServiceError> {
        crate::validation::require_non_empty("tenant_id", &self.tenant_id)?;
        crate::validation::require_non_empty("sku_id", &self.sku_id)?;
        if let Some(name) = self.name.as_deref() {
            crate::validation::require_non_empty("name", name)?;
        }
        if let Some(title) = self.title.as_deref() {
            crate::validation::require_non_empty("title", title)?;
        }
        if let Some(minor) = self.sale_price_minor {
            crate::validation::require_non_negative_minor("sale_price_minor", minor)?;
        }
        // `Some(None)` is the clearing write, not an absent field, so only a restated amount is
        // range-checked; there is nothing to check about "no reference price".
        if let Some(Some(minor)) = self.list_price_minor {
            crate::validation::require_non_negative_minor("list_price_minor", minor)?;
        }
        if let Some(currency_code) = self.currency_code.as_deref() {
            crate::validation::require_non_empty("currency_code", currency_code)?;
        }
        if let Some(axis) = self.attribute_value_ids.as_deref() {
            crate::validation::require_distinct("attribute_value_ids", axis)?;
        }
        if let Some(metadata) = self.metadata.as_ref() {
            require_metadata_object(metadata)?;
        }
        Ok(())
    }
}

/// Rejects a capability metadata payload that is not a JSON object.
///
/// `commerce_product_sku.metadata` is `NOT NULL DEFAULT '{}'` and every reader treats it as an object
/// map, where `{}` means "this SKU declares no capability metadata". Storing an array or a scalar
/// would write a shape no reader can interpret, so it is refused here as a `422` naming the field
/// instead of being left for a consumer to trip over.
fn require_metadata_object(metadata: &Value) -> Result<(), CommerceServiceError> {
    if metadata.is_object() {
        Ok(())
    } else {
        Err(CommerceServiceError::validation(
            "metadata must be a JSON object",
        ))
    }
}
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

/// `CreateMediaCommand` validates the owner pair, the role/owner-kind cross-check, and the snapshot.
///
/// `ck_commerce_product_media_owner_role` rejects an `attribute_value` owner carrying a non-image
/// role. The rule is asked of [`MediaOwnerType::admits_role`] rather than restated here, so the
/// adapter and the domain cannot hold two different readings of the same constraint.
///
/// The snapshot is validated through [`CreateMediaCommand::media_resource_id`], which is also what
/// the repository calls to fill the column: a non-HTTP caller that skips `validate` still cannot
/// reach SQL with a snapshot the column cannot represent.
impl CreateMediaCommand {
    pub fn validate(&self) -> Result<(), CommerceServiceError> {
        crate::validation::require_non_empty("tenant_id", &self.tenant_id)?;
        crate::validation::require_non_empty("organization_id", &self.organization_id)?;
        crate::validation::require_non_empty("owner_id", &self.owner_id)?;
        crate::validation::require_non_negative("sort_order", self.sort_order)?;
        if !self.owner_type.admits_role(self.media_role) {
            return Err(CommerceServiceError::validation(format!(
                "media_role `{}` is not permitted for owner_type `{}`: a dictionary value carries an \
                 image swatch, so only main_image, sku_image, and gallery_image are available",
                self.media_role.as_storage_str(),
                self.owner_type.as_storage_str(),
            )));
        }
        self.media_resource_id()?;
        Ok(())
    }

    /// The Drive resource identity this attachment points at.
    ///
    /// Derived from the snapshot rather than accepted as a separate field, so the reference and the
    /// projection it was read from cannot disagree.
    pub fn media_resource_id(&self) -> Result<i64, CommerceServiceError> {
        crate::validation::media_resource_identity("resource_snapshot", &self.resource_snapshot)
    }
}

/// `UpdateMediaCommand` validates the snapshot when one is being written.
///
/// The role/owner-kind pair is not checked here: the owner kind is not a field of this command, and
/// the repository re-evaluates the pair against the row's **stored** `owner_type` inside the same
/// transaction that locks the row. Validating a role against an owner the caller never named would
/// be checking a different row than the one being updated.
impl UpdateMediaCommand {
    pub fn validate(&self) -> Result<(), CommerceServiceError> {
        crate::validation::require_non_empty("tenant_id", &self.tenant_id)?;
        crate::validation::require_non_empty("media_id", &self.media_id)?;
        if let Some(sort_order) = self.sort_order {
            crate::validation::require_non_negative("sort_order", sort_order)?;
        }
        if let Some(snapshot) = self.resource_snapshot.as_ref() {
            crate::validation::media_resource_identity("resource_snapshot", snapshot)?;
        }
        Ok(())
    }

    /// The Drive resource identity a replacement snapshot points at, if one was submitted.
    pub fn media_resource_id(&self) -> Result<Option<i64>, CommerceServiceError> {
        self.resource_snapshot
            .as_ref()
            .map(|snapshot| {
                crate::validation::media_resource_identity("resource_snapshot", snapshot)
            })
            .transpose()
    }
}

impl_required_text_command!(DeleteMediaCommand, tenant_id, media_id);
