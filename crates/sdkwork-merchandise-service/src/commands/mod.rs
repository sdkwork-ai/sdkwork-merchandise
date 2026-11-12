use sdkwork_contract_service::CommerceServiceError;
use serde_json::Value;

use crate::domain::{
    AttributeInputHint, AttributeRole, AttributeValueType, FulfillmentType, InventoryPolicy,
    InventoryTrackingMode, LifecycleStatus, MediaOwnerType, MediaRole, ProductStatus, ProductType,
    SpuAttributeRole,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateCategoryCommand {
    /// The locale the text fields are written in. Omitted — or the default language — writes the base row; any other locale writes a translation row and leaves the base row alone.
    pub locale: Option<String>,
    /// The authenticated principal this write is attributed to.
    pub actor: String,
    pub tenant_id: String,
    pub organization_id: String,
    pub category_no: String,
    pub parent_id: Option<String>,
    /// The category's position on the back-office tree, when the tenant keeps one.
    ///
    /// The two trees are independent: a category can sit in the merchandising tree only, the
    /// back-office tree only, or both. The baseline constrains the column with a foreign key and a
    /// self-reference refusal, so the repository lets PostgreSQL answer a missing parent or a
    /// self-pointing id rather than pre-checking either.
    pub back_parent_id: Option<String>,
    pub name: String,
    pub sort_order: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateCategoryCommand {
    /// The locale the text fields are written in. Omitted — or the default language — writes the base row; any other locale writes a translation row and leaves the base row alone.
    pub locale: Option<String>,
    /// The authenticated principal this write is attributed to.
    pub actor: String,
    pub tenant_id: String,
    pub category_id: String,
    pub parent_id: Option<String>,
    /// Three-state: the back-tree position is removable without touching the front tree, and the
    /// column is nullable, so an omitted key keeps the stored position while an explicit null
    /// removes the category from the back-office tree.
    pub back_parent_id: Option<Option<String>>,
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
    /// The authenticated principal this write is attributed to.
    pub actor: String,
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

/// Creates an attribute dictionary entry plus, for an `enum` attribute, its enumerated values.
///
/// `value_type` decides what the attribute accepts: an `enum` selects from the dictionary this
/// command seeds, and `text`/`number`/`bool`/`date` accept typed free input — which is why those
/// four must arrive with an empty `values` list. `is_multi_value` requires `enum` because a
/// multi-valued attribute needs discrete options to be multi-valued *of*
/// (`ck_commerce_product_attribute_multi_value_needs_enum`).
///
/// `value_type`, `input_hint`, and `is_multi_value` are **create-time facts** and have no update
/// path: changing an attribute's type after values exist would strand every stored value that no
/// longer fits, and the update command deliberately does not carry them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateAttributeCommand {
    /// The locale the text fields are written in. Omitted — or the default language — writes the base row; any other locale writes a translation row and leaves the base row alone.
    pub locale: Option<String>,
    /// The authenticated principal this write is attributed to.
    pub actor: String,
    pub tenant_id: String,
    pub organization_id: String,
    pub attribute_no: String,
    pub name: String,
    pub value_type: AttributeValueType,
    pub input_hint: AttributeInputHint,
    pub is_multi_value: bool,
    pub unit_symbol: Option<String>,
    pub sort_order: i64,
    pub values: Vec<String>,
}

/// Updates an attribute dictionary entry.
///
/// `name`, `unit_symbol`, `sort_order`, and `status` are the mutable surface; the type facts are
/// create-time only (see [`CreateAttributeCommand`]). `unit_symbol` is three-state because the
/// column is nullable: `None` leaves it alone, `Some(None)` clears it, `Some(Some(unit))` replaces
/// it. Retiring an attribute is `status = inactive`; deletion is not offered, because live category
/// bindings and SKU axes reference the row and a soft delete would strand them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateAttributeCommand {
    /// The locale the text fields are written in. Omitted — or the default language — writes the base row; any other locale writes a translation row and leaves the base row alone.
    pub locale: Option<String>,
    /// The authenticated principal this write is attributed to.
    pub actor: String,
    pub tenant_id: String,
    pub attribute_id: String,
    pub name: Option<String>,
    /// The unit the attribute's values are denominated in (`cm`, `kg`). Three-state for the same
    /// reason `description` on the update-SPU command is: the column is nullable, so an omitted
    /// field and a stated removal are different instructions.
    pub unit_symbol: Option<Option<String>>,
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

/// Creates one dictionary value on an `enum` attribute.
///
/// `color_hex` and `media_resource_id` are the presentation metadata a console renders a colourway
/// with (`color_hex` is the swatch, the media resource is its picture); both are optional and only
/// meaningful for attributes whose values are displayed. The attribute itself must be an `enum` —
/// a free-text attribute has no dictionary to extend — and the repository re-checks that inside the
/// same transaction, because the command cannot see the attribute row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateAttributeValueCommand {
    /// The locale the text fields are written in. Omitted — or the default language — writes the base row; any other locale writes a translation row and leaves the base row alone.
    pub locale: Option<String>,
    /// The authenticated principal this write is attributed to.
    pub actor: String,
    pub tenant_id: String,
    pub organization_id: String,
    pub attribute_id: String,
    pub value_code: String,
    pub display_value: String,
    /// The swatch colour, `#RRGGBB`. The baseline's
    /// `ck_commerce_product_attribute_value_color_hex` enforces the same pattern.
    pub color_hex: Option<String>,
    /// The Drive resource id of the value's picture, as an int64 string.
    pub media_resource_id: Option<String>,
    pub sort_order: i64,
}

/// Updates one dictionary value.
///
/// `color_hex` and `media_resource_id` are three-state because both columns are nullable. The
/// `attribute_id` and `value_code` pair is the row's business identity and is not mutable: moving a
/// value to another attribute or renaming its key is a delete plus a create, not an edit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateAttributeValueCommand {
    /// The locale the text fields are written in. Omitted — or the default language — writes the base row; any other locale writes a translation row and leaves the base row alone.
    pub locale: Option<String>,
    /// The authenticated principal this write is attributed to.
    pub actor: String,
    pub tenant_id: String,
    pub attribute_value_id: String,
    pub display_value: Option<String>,
    pub color_hex: Option<Option<String>>,
    pub media_resource_id: Option<Option<String>>,
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

/// Deletes one dictionary value.
///
/// The delete is a soft delete, but a value that a live SKU variant still sits on cannot be
/// retired: the repository refuses while any live `commerce_product_sku_attribute` row references
/// the value, because a variant whose axis value is gone is a variant the console can no longer
/// render and the buyer can no longer select.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeleteAttributeValueCommand {
    /// The authenticated principal this write is attributed to.
    pub actor: String,
    pub tenant_id: String,
    pub attribute_value_id: String,
    /// The row version the caller read, taken from the request's `If-Match` precondition.
    ///
    /// A mismatch is not a malformed request: it means somebody else wrote the row since this
    /// caller read it, so the caller's copy is stale and the write would silently discard their
    /// change. The repository compares this against `version` inside the same statement that
    /// writes, which is what makes the comparison race-free.
    pub expected_version: i64,
}

/// One entry of a product's key/parameter attribute set.
///
/// The value is carried by exactly one of two channels: `attribute_value_id` for an attribute whose
/// value comes from the dictionary, or `raw_value` for the free-text spellings a dictionary does not
/// hold (`ck_commerce_product_spu_attribute_carrier` refuses a row with neither or both). Which one
/// is legitimate is a property of the attribute's `value_type`, which the repository reads from the
/// dictionary row inside the same transaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductAttributeValueInput {
    pub attribute_id: String,
    pub role: SpuAttributeRole,
    pub attribute_value_id: Option<String>,
    pub raw_value: Option<String>,
    pub sort_order: i64,
}

/// Replaces a product's whole key/parameter attribute set.
///
/// Replacement — not per-row patching — is the semantics because the set is small, is always
/// rendered together on the product detail page, and a partial edit cannot express "this product no
/// longer lists a warranty term". The replacement is guarded on the **SPU's** version: the set is
/// part of the product aggregate, so two concurrent editors of one product must not both succeed,
/// and the SPU row's version is bumped by the write to keep the next `If-Match` honest.
///
/// Every submitted attribute must be bound — active — in the product's category template with the
/// same role: the template is what decides *what belongs on a product of this category*, and an
/// attribute outside it is a specification no category declared.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplaceProductAttributesCommand {
    /// The authenticated principal this write is attributed to.
    pub actor: String,
    pub tenant_id: String,
    pub spu_id: String,
    pub attributes: Vec<ProductAttributeValueInput>,
    /// The SPU row version the caller read, taken from the request's `If-Match` precondition.
    ///
    /// A mismatch is not a malformed request: it means somebody else wrote the row since this
    /// caller read it, so the caller's copy is stale and the write would silently discard their
    /// change. The repository compares this against `version` inside the same statement that
    /// writes, which is what makes the comparison race-free.
    pub expected_version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreatePriceListCommand {
    /// The authenticated principal this write is attributed to.
    pub actor: String,
    pub tenant_id: String,
    pub organization_id: String,
    pub price_list_no: String,
    pub name: String,
    pub currency_code: String,
    pub market_code: Option<String>,
    /// The B2B customer segment the list applies to, when it is segment-scoped.
    pub customer_segment: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdatePriceListCommand {
    /// The authenticated principal this write is attributed to.
    pub actor: String,
    pub tenant_id: String,
    pub price_list_id: String,
    pub name: Option<String>,
    /// Three-state: the column is nullable, so an omitted key keeps the stored segment while an
    /// explicit null detaches the list from any segment.
    pub customer_segment: Option<Option<String>>,
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
    /// The locale the text fields are written in. Omitted — or the default language — writes the base row; any other locale writes a translation row and leaves the base row alone.
    pub locale: Option<String>,
    /// The authenticated principal this write is attributed to.
    pub actor: String,
    pub tenant_id: String,
    pub organization_id: String,
    pub spu_no: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub description: Option<String>,
    pub product_type: ProductType,
    pub category_id: String,
    /// The brand this product belongs to, as an int64 string.
    pub brand_id: Option<String>,
    /// The manufacturer's standard model number under the brand, when one is declared.
    pub model_no: Option<String>,
    /// The product-level barcode (GTIN/EAN), unique per tenant while the row is live
    /// (uk_commerce_product_spu_tenant_barcode). GS1 mints GTINs globally; a multi-tenant SaaS
    /// enforces uniqueness inside the tenant, which is the scope the baseline's index declares.
    pub barcode: Option<String>,
    /// Ship weight in grams, the figure logistics rates a shipment from.
    pub weight_gram: Option<i64>,
    /// Shipping volume in millilitres.
    pub volume_ml: Option<i64>,
    /// The tenant's tax-classification key for this product (`standard`, `reduced`, ...).
    pub tax_class_code: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateProductSpuCommand {
    /// The locale the text fields are written in. Omitted — or the default language — writes the base row; any other locale writes a translation row and leaves the base row alone.
    pub locale: Option<String>,
    /// The authenticated principal this write is attributed to.
    pub actor: String,
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
    /// The brand this product belongs to. Three-state: an explicit null detaches the product
    /// from its brand.
    pub brand_id: Option<Option<String>>,
    /// The manufacturer's model number. Three-state: the column is nullable, so an omitted key
    /// keeps the stored value and an explicit null removes it.
    pub model_no: Option<Option<String>>,
    /// The product-level barcode. Three-state for the same reason; a restated barcode must be
    /// unique per tenant, which the baseline's partial unique index enforces.
    pub barcode: Option<Option<String>>,
    /// Ship weight in grams. Three-state.
    pub weight_gram: Option<Option<i64>>,
    /// Shipping volume in millilitres. Three-state.
    pub volume_ml: Option<Option<i64>>,
    /// The tenant's tax-classification key. Three-state.
    pub tax_class_code: Option<Option<String>>,
    /// The product's lifecycle status, settable to any of the four values the SKU also accepts.
    ///
    /// The SPU and the SKU share one status vocabulary
    /// (`commerce_product_spu.status` and `commerce_product_sku.status` carry the same CHECK), so a
    /// caller that can move a SKU to `inactive` has to be able to move its product there too. It
    /// could not: the reachable set used to be `draft` (only at insert), `active`, and `archived`,
    /// and `delete` reached `inactive` only by also hiding the row. No caller could therefore state
    /// "this product is inactive" without retiring it.
    ///
    /// `None` leaves the stored status alone; there is no third state because the column is
    /// `NOT NULL` and an unset status is not a status.
    ///
    /// # `draft` is a state to leave, not one to return to
    ///
    /// `ck_commerce_product_spu_published_at` is `published_at IS NULL OR status <> 'draft'`, and
    /// nothing clears `published_at` once a publish has recorded it — the baseline says as much:
    /// "Publishing is a one-way transition with a recorded instant". So this field is genuinely
    /// settable to `draft` only while the row has never been published; afterwards PostgreSQL
    /// refuses it, and the SQL repository's `store_error` answers with that constraint's name rather
    /// than a generic integrity message. The same rule binds the SKU through
    /// `ck_commerce_product_sku_published_at`, so it is the model and not an asymmetry.
    ///
    /// `sales_status` is not a parameter: the repository derives it from this value on every write
    /// (`active` if and only if `status = 'active'`), the same way `update_sku` does, because the
    /// baseline only admits a sellable row whose own status is `active`.
    pub status: Option<ProductStatus>,
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
    /// The authenticated principal this write is attributed to.
    pub actor: String,
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
    /// The authenticated principal this write is attributed to.
    pub actor: String,
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
    /// The authenticated principal this write is attributed to.
    pub actor: String,
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
    /// The locale the text fields are written in. Omitted — or the default language — writes the base row; any other locale writes a translation row and leaves the base row alone.
    pub locale: Option<String>,
    /// The authenticated principal this write is attributed to.
    pub actor: String,
    pub tenant_id: String,
    pub organization_id: String,
    pub spu_id: String,
    pub sku_no: String,
    pub name: String,
    pub title: String,
    pub sale_price_minor: i64,
    pub list_price_minor: Option<i64>,
    /// The cost of acquiring one unit, in the same minor unit. It is what a margin needs, and it
    /// is deliberately a column rather than a price-list row: cost is not market-segmented.
    pub cost_price_minor: Option<i64>,
    /// The SKU-level barcode (GTIN/EAN), unique per tenant while the row is live.
    pub barcode: Option<String>,
    /// Ship weight in grams, the figure logistics rates a shipment from.
    pub weight_gram: Option<i64>,
    /// The quantity a single order must buy at least. Defaults to 1.
    pub min_order_quantity: Option<i64>,
    /// The quantity a single order may buy at most, when capped.
    pub max_order_quantity: Option<i64>,
    /// The step the orderable quantity moves in. Defaults to 1.
    pub step_quantity: Option<i64>,
    /// The unit the buyer names (a box, a can), when declared.
    pub package_unit: Option<String>,
    /// How many sellable units one package holds, when declared.
    pub units_per_package: Option<i64>,
    pub currency_code: String,
    pub fulfillment_type: FulfillmentType,
    pub inventory_tracking: InventoryTrackingMode,
    /// What happens when a tracked SKU's stock runs out. `backorder` requires
    /// `inventory_tracking = quantity`, the pairing
    /// `ck_commerce_product_sku_policy_requires_tracking` enforces in storage.
    pub inventory_policy: InventoryPolicy,
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
    /// The locale the text fields are written in. Omitted — or the default language — writes the base row; any other locale writes a translation row and leaves the base row alone.
    pub locale: Option<String>,
    /// The authenticated principal this write is attributed to.
    pub actor: String,
    pub tenant_id: String,
    pub sku_id: String,
    pub name: Option<String>,
    pub title: Option<String>,
    pub sale_price_minor: Option<i64>,
    pub list_price_minor: Option<Option<i64>>,
    /// Three-state, like `list_price_minor`: the column is nullable, so an omitted key keeps the
    /// stored cost and an explicit null removes it.
    pub cost_price_minor: Option<Option<i64>>,
    /// Three-state. A restated barcode must be unique per tenant.
    pub barcode: Option<Option<String>>,
    /// Three-state.
    pub weight_gram: Option<Option<i64>>,
    /// The trade constraints are three-state: an explicit null removes the cap (or the
    /// package facts) while an omitted key keeps the stored value. The NOT NULL defaults
    /// (min 1, step 1) are restated as values, never cleared.
    pub min_order_quantity: Option<i64>,
    pub max_order_quantity: Option<Option<i64>>,
    pub step_quantity: Option<i64>,
    pub package_unit: Option<Option<String>>,
    pub units_per_package: Option<Option<i64>>,
    pub currency_code: Option<String>,
    pub fulfillment_type: Option<FulfillmentType>,
    pub inventory_tracking: Option<InventoryTrackingMode>,
    /// `backorder` requires the SKU to be (or become) quantity-tracked; the repository checks the
    /// pairing against the *effective* values after the merge, because a command that only states
    /// the policy cannot see the stored tracking mode.
    pub inventory_policy: Option<InventoryPolicy>,
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
    /// The authenticated principal this write is attributed to.
    pub actor: String,
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
    /// The authenticated principal this write is attributed to.
    pub actor: String,
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
    /// The authenticated principal this write is attributed to.
    pub actor: String,
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
    /// The authenticated principal this write is attributed to.
    pub actor: String,
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

/// Creates one brand.
///
/// The brand is master data the SPU points at, so the name is normalized here
/// and every product of the brand reads the same row. Logo media reuses the
/// media surface with `owner_type = 'brand'` / `media_role = 'logo'`; this
/// command carries no media of its own.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateBrandCommand {
    /// The locale the text fields are written in. Omitted — or the default language — writes the base row; any other locale writes a translation row and leaves the base row alone.
    pub locale: Option<String>,
    pub actor: String,
    pub tenant_id: String,
    pub organization_id: String,
    pub brand_no: String,
    pub name: String,
    pub description: Option<String>,
    pub sort_order: i64,
}

/// Updates one brand.
///
/// `description` is three-state because the column is nullable: an omitted key
/// keeps the stored text and an explicit null removes it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateBrandCommand {
    /// The locale the text fields are written in. Omitted — or the default language — writes the base row; any other locale writes a translation row and leaves the base row alone.
    pub locale: Option<String>,
    pub actor: String,
    pub tenant_id: String,
    pub brand_id: String,
    pub name: Option<String>,
    pub description: Option<Option<String>>,
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

/// Deletes one brand (soft delete).
///
/// Products pointing at the brand keep their reference: `spu.brand_id` has no
/// `ON DELETE` cascade, and a soft delete leaves the row — and the products'
/// history — intact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeleteBrandCommand {
    pub actor: String,
    pub tenant_id: String,
    pub brand_id: String,
    /// The row version the caller read, taken from the request's `If-Match` precondition.
    ///
    /// A mismatch is not a malformed request: it means somebody else wrote the row since this
    /// caller read it, so the caller's copy is stale and the write would silently discard their
    /// change. The repository compares this against `version` inside the same statement that
    /// writes, which is what makes the comparison race-free.
    pub expected_version: i64,
}

/// Creates one priced row of a price list.
///
/// The list is the currency authority: `currency_code` and `price_scale` are **inherited** from the
/// list row inside the transaction, because `fk_commerce_price_list_item_list_currency` refuses a
/// row whose currency disagrees with its list — the two layers of the price system must quote the
/// same money. `min_quantity` is the ladder step the row prices; two rows of one list may not
/// price the same SKU at the same step (`uk_commerce_price_list_item_tier`).
///
/// The row exists alongside `commerce_product_sku.sale_price_minor`, not instead of it: the base
/// price is what every list falls back to, and the resolution order (item, then sale, then list) is
/// executed by the read model, not here.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreatePriceListItemCommand {
    /// The authenticated principal this write is attributed to.
    pub actor: String,
    pub tenant_id: String,
    pub organization_id: String,
    pub price_list_id: String,
    pub sku_id: String,
    /// The price at this ladder step, in the list's minor unit.
    pub price_minor: i64,
    /// The quantity this step starts at. Defaults to 1, the first rung of the ladder.
    pub min_quantity: i64,
}

/// Updates one priced row.
///
/// `min_quantity` is mutable but stays under the tier uniqueness: restating it to a step another
/// row of the same list already prices is a `409`, not a silent merge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdatePriceListItemCommand {
    /// The authenticated principal this write is attributed to.
    pub actor: String,
    pub tenant_id: String,
    pub price_list_item_id: String,
    pub price_minor: Option<i64>,
    pub min_quantity: Option<i64>,
    pub status: Option<LifecycleStatus>,
    /// The row version the caller read, taken from the request's `If-Match` precondition.
    ///
    /// A mismatch is not a malformed request: it means somebody else wrote the row since this
    /// caller read it, so the caller's copy is stale and the write would silently discard their
    /// change. The repository compares this against `version` inside the same statement that
    /// writes, which is what makes the comparison race-free.
    pub expected_version: i64,
}

/// Deletes one priced row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeletePriceListItemCommand {
    /// The authenticated principal this write is attributed to.
    pub actor: String,
    pub tenant_id: String,
    pub price_list_item_id: String,
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
    /// The authenticated principal this write is attributed to.
    pub actor: String,
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
    /// The authenticated principal this write is attributed to.
    pub actor: String,
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
    /// The authenticated principal this write is attributed to.
    pub actor: String,
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
        crate::validation::require_non_empty("actor", &self.actor)?;
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
        crate::validation::require_non_empty("actor", &self.actor)?;
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
///
/// The type pairing is validated here rather than left to the baseline CHECK, because both rules
/// are about what the caller asked for, not what the row looks like: a multi-value flag on a
/// non-enum attribute is a request error (`422` naming the field), and dictionary values on a
/// free-text attribute are a category mistake the CHECK cannot even see.
impl CreateAttributeCommand {
    pub fn validate(&self) -> Result<(), CommerceServiceError> {
        crate::validation::require_non_empty("actor", &self.actor)?;
        crate::validation::require_non_empty("tenant_id", &self.tenant_id)?;
        crate::validation::require_non_empty("organization_id", &self.organization_id)?;
        crate::validation::require_non_empty("attribute_no", &self.attribute_no)?;
        crate::validation::require_non_empty("name", &self.name)?;
        crate::validation::require_within_chars(
            "name",
            &self.name,
            crate::validation::ATTRIBUTE_NAME_MAX_CHARS,
        )?;
        crate::validation::require_non_negative("sort_order", self.sort_order)?;
        if self.is_multi_value && self.value_type != AttributeValueType::Enum {
            return Err(CommerceServiceError::validation(
                "is_multi_value requires value_type `enum`: a multi-valued attribute needs \
                 discrete options to be multi-valued of",
            ));
        }
        if self.value_type != AttributeValueType::Enum && !self.values.is_empty() {
            return Err(CommerceServiceError::validation(
                "values are only valid for value_type `enum`: a free-text attribute has no \
                 dictionary to seed",
            ));
        }
        if let Some(unit) = self.unit_symbol.as_deref() {
            crate::validation::require_non_empty("unit_symbol", unit)?;
        }
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
impl UpdateAttributeCommand {
    pub fn validate(&self) -> Result<(), CommerceServiceError> {
        crate::validation::require_non_empty("actor", &self.actor)?;
        crate::validation::require_non_empty("tenant_id", &self.tenant_id)?;
        crate::validation::require_non_empty("attribute_id", &self.attribute_id)?;
        if let Some(name) = self.name.as_deref() {
            crate::validation::require_non_empty("name", name)?;
            crate::validation::require_within_chars(
                "name",
                name,
                crate::validation::ATTRIBUTE_NAME_MAX_CHARS,
            )?;
        }
        // `Some(None)` is the clearing write, not an absent field, so only a restated unit is
        // checked; there is nothing to check about "no unit".
        if let Some(unit) = self.unit_symbol.as_ref().and_then(Option::as_deref) {
            crate::validation::require_non_empty("unit_symbol", unit)?;
        }
        if let Some(sort_order) = self.sort_order {
            crate::validation::require_non_negative("sort_order", sort_order)?;
        }
        Ok(())
    }
}
/// `CreateAttributeValueCommand` validates the swatch pattern against the baseline CHECK.
///
/// `ck_commerce_product_attribute_value_color_hex` is `^#[0-9A-Fa-f]{6}$`; checking it here keeps a
/// malformed colour a named `422` instead of a `23514` raised mid-transaction.
impl CreateAttributeValueCommand {
    pub fn validate(&self) -> Result<(), CommerceServiceError> {
        crate::validation::require_non_empty("actor", &self.actor)?;
        crate::validation::require_non_empty("tenant_id", &self.tenant_id)?;
        crate::validation::require_non_empty("organization_id", &self.organization_id)?;
        crate::validation::require_non_empty("attribute_id", &self.attribute_id)?;
        crate::validation::require_non_empty("value_code", &self.value_code)?;
        crate::validation::require_non_empty("display_value", &self.display_value)?;
        crate::validation::require_within_chars(
            "display_value",
            &self.display_value,
            crate::validation::ATTRIBUTE_VALUE_MAX_CHARS,
        )?;
        crate::validation::require_non_negative("sort_order", self.sort_order)?;
        if let Some(color) = self.color_hex.as_deref() {
            crate::validation::require_color_hex("color_hex", color)?;
        }
        if let Some(resource) = self.media_resource_id.as_deref() {
            crate::validation::require_non_empty("media_resource_id", resource)?;
        }
        Ok(())
    }
}
impl UpdateAttributeValueCommand {
    pub fn validate(&self) -> Result<(), CommerceServiceError> {
        crate::validation::require_non_empty("actor", &self.actor)?;
        crate::validation::require_non_empty("tenant_id", &self.tenant_id)?;
        crate::validation::require_non_empty("attribute_value_id", &self.attribute_value_id)?;
        if let Some(display) = self.display_value.as_deref() {
            crate::validation::require_non_empty("display_value", display)?;
            crate::validation::require_within_chars(
                "display_value",
                display,
                crate::validation::ATTRIBUTE_VALUE_MAX_CHARS,
            )?;
        }
        // Both three-state fields are cleared by `Some(None)`, so only a restated value is checked.
        if let Some(color) = self.color_hex.as_ref().and_then(Option::as_deref) {
            crate::validation::require_color_hex("color_hex", color)?;
        }
        if let Some(resource) = self.media_resource_id.as_ref().and_then(Option::as_deref) {
            crate::validation::require_non_empty("media_resource_id", resource)?;
        }
        if let Some(sort_order) = self.sort_order {
            crate::validation::require_non_negative("sort_order", sort_order)?;
        }
        Ok(())
    }
}
impl_required_text_command!(DeleteAttributeValueCommand, tenant_id, attribute_value_id);
/// `ReplaceProductAttributesCommand` validates each entry's carrier exclusivity and the set's
/// distinctness before any SQL runs.
///
/// `ck_commerce_product_spu_attribute_carrier` refuses a row with neither or both carriers, and
/// `uk_commerce_product_spu_attribute_value` admits one live value per `(spu_id, attribute_id)` —
/// so a duplicate attribute in the submitted set would collide with itself on insert. Both are
/// caller errors, named here rather than raised mid-transaction.
impl ReplaceProductAttributesCommand {
    pub fn validate(&self) -> Result<(), CommerceServiceError> {
        crate::validation::require_non_empty("actor", &self.actor)?;
        crate::validation::require_non_empty("tenant_id", &self.tenant_id)?;
        crate::validation::require_non_empty("spu_id", &self.spu_id)?;
        let mut seen = std::collections::BTreeSet::new();
        for entry in &self.attributes {
            crate::validation::require_non_empty("attribute_id", &entry.attribute_id)?;
            match (
                entry.attribute_value_id.as_deref(),
                entry.raw_value.as_deref(),
            ) {
                (None, None) => {
                    return Err(CommerceServiceError::validation(format!(
                        "attributes[{attribute_id}] must carry exactly one of attribute_value_id \
                         or raw_value",
                        attribute_id = entry.attribute_id,
                    )));
                }
                (Some(_), Some(_)) => {
                    return Err(CommerceServiceError::validation(format!(
                        "attributes[{attribute_id}] must carry exactly one of attribute_value_id \
                         or raw_value, not both",
                        attribute_id = entry.attribute_id,
                    )));
                }
                (Some(value), None) => {
                    crate::validation::require_non_empty("attribute_value_id", value)?;
                }
                (None, Some(raw)) => {
                    crate::validation::require_non_empty("raw_value", raw)?;
                }
            }
            crate::validation::require_non_negative("sort_order", entry.sort_order)?;
            if !seen.insert(entry.attribute_id.trim().to_owned()) {
                return Err(CommerceServiceError::validation(format!(
                    "attributes contains `{attribute}` more than once: one value per attribute",
                    attribute = entry.attribute_id,
                )));
            }
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
        crate::validation::require_non_empty("actor", &self.actor)?;
        crate::validation::require_non_empty("tenant_id", &self.tenant_id)?;
        crate::validation::require_non_empty("organization_id", &self.organization_id)?;
        crate::validation::require_non_empty("price_list_no", &self.price_list_no)?;
        crate::validation::require_non_empty("name", &self.name)?;
        crate::validation::require_non_empty("currency_code", &self.currency_code)?;
        crate::validation::require_within_chars(
            "price_list_no",
            &self.price_list_no,
            crate::validation::PRICE_LIST_NO_MAX_CHARS,
        )?;
        crate::validation::require_within_chars(
            "name",
            &self.name,
            crate::validation::PRICE_LIST_NO_MAX_CHARS,
        )?;
        Ok(())
    }
}
impl CreateBrandCommand {
    pub fn validate(&self) -> Result<(), CommerceServiceError> {
        crate::validation::require_non_empty("actor", &self.actor)?;
        crate::validation::require_non_empty("tenant_id", &self.tenant_id)?;
        crate::validation::require_non_empty("organization_id", &self.organization_id)?;
        crate::validation::require_non_empty("brand_no", &self.brand_no)?;
        crate::validation::require_non_empty("name", &self.name)?;
        crate::validation::require_within_chars(
            "name",
            &self.name,
            crate::validation::BRAND_NAME_MAX_CHARS,
        )?;
        crate::validation::require_non_negative("sort_order", self.sort_order)?;
        Ok(())
    }
}
impl UpdateBrandCommand {
    pub fn validate(&self) -> Result<(), CommerceServiceError> {
        crate::validation::require_non_empty("actor", &self.actor)?;
        crate::validation::require_non_empty("tenant_id", &self.tenant_id)?;
        crate::validation::require_non_empty("brand_id", &self.brand_id)?;
        if let Some(name) = self.name.as_deref() {
            crate::validation::require_non_empty("name", name)?;
            crate::validation::require_within_chars(
                "name",
                name,
                crate::validation::BRAND_NAME_MAX_CHARS,
            )?;
        }
        if let Some(description) = self.description.as_ref().and_then(Option::as_deref) {
            crate::validation::require_non_empty("description", description)?;
        }
        if let Some(sort_order) = self.sort_order {
            crate::validation::require_non_negative("sort_order", sort_order)?;
        }
        Ok(())
    }
}
impl_required_text_command!(DeleteBrandCommand, actor, tenant_id, brand_id);
impl CreatePriceListItemCommand {
    pub fn validate(&self) -> Result<(), CommerceServiceError> {
        crate::validation::require_non_empty("actor", &self.actor)?;
        crate::validation::require_non_empty("tenant_id", &self.tenant_id)?;
        crate::validation::require_non_empty("organization_id", &self.organization_id)?;
        crate::validation::require_non_empty("price_list_id", &self.price_list_id)?;
        crate::validation::require_non_empty("sku_id", &self.sku_id)?;
        crate::validation::require_non_negative_minor("price_minor", self.price_minor)?;
        if self.min_quantity < 1 {
            return Err(CommerceServiceError::validation(
                "min_quantity must be at least 1: it is the quantity a ladder step starts at",
            ));
        }
        Ok(())
    }
}
impl UpdatePriceListItemCommand {
    pub fn validate(&self) -> Result<(), CommerceServiceError> {
        crate::validation::require_non_empty("actor", &self.actor)?;
        crate::validation::require_non_empty("tenant_id", &self.tenant_id)?;
        crate::validation::require_non_empty("price_list_item_id", &self.price_list_item_id)?;
        if let Some(minor) = self.price_minor {
            crate::validation::require_non_negative_minor("price_minor", minor)?;
        }
        if let Some(min_quantity) = self.min_quantity {
            if min_quantity < 1 {
                return Err(CommerceServiceError::validation(
                    "min_quantity must be at least 1: it is the quantity a ladder step starts at",
                ));
            }
        }
        Ok(())
    }
}
impl_required_text_command!(DeletePriceListItemCommand, tenant_id, price_list_item_id);
impl_required_text_command!(UpdatePriceListCommand, tenant_id, price_list_id);
// `product_type` and `category_id` are typed/required, so presence is guaranteed by the type
// rather than by a runtime string check.
//
// `title` bounds `commerce_product_spu.name`, which the repository derives from it and the baseline
// pins with `char_length(name) BETWEEN 1 AND 300`.
impl CreateProductSpuCommand {
    pub fn validate(&self) -> Result<(), CommerceServiceError> {
        crate::validation::require_non_empty("actor", &self.actor)?;
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
        if let Some(weight) = self.weight_gram {
            crate::validation::require_non_negative("weight_gram", weight)?;
        }
        if let Some(volume) = self.volume_ml {
            crate::validation::require_non_negative("volume_ml", volume)?;
        }
        Ok(())
    }
}
impl UpdateProductSpuCommand {
    pub fn validate(&self) -> Result<(), CommerceServiceError> {
        crate::validation::require_non_empty("actor", &self.actor)?;
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
        // The three-state columns are cleared by `Some(None)`, so only a restated value is
        // range-checked; there is nothing to check about "remove it".
        if let Some(Some(weight)) = self.weight_gram {
            crate::validation::require_non_negative("weight_gram", weight)?;
        }
        if let Some(Some(volume)) = self.volume_ml {
            crate::validation::require_non_negative("volume_ml", volume)?;
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
        crate::validation::require_non_empty("actor", &self.actor)?;
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
        if let Some(cost) = self.cost_price_minor {
            crate::validation::require_non_negative_minor("cost_price_minor", cost)?;
        }
        if let Some(weight) = self.weight_gram {
            crate::validation::require_non_negative("weight_gram", weight)?;
        }
        // The pairing the baseline CHECK enforces, answered before SQL: a refusal here names both
        // fields, where a 23514 names only the constraint.
        if self.inventory_policy == InventoryPolicy::Backorder
            && self.inventory_tracking != InventoryTrackingMode::Tracked
        {
            return Err(CommerceServiceError::validation(
                "inventory_policy `backorder` requires inventory_tracking `quantity`: an untracked \
                 SKU has no stock to run out of",
            ));
        }
        crate::validation::require_distinct("attribute_value_ids", &self.attribute_value_ids)?;
        require_metadata_object(&self.metadata)?;
        Ok(())
    }
}
impl UpdateProductSkuCommand {
    pub fn validate(&self) -> Result<(), CommerceServiceError> {
        crate::validation::require_non_empty("actor", &self.actor)?;
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
        if let Some(Some(minor)) = self.cost_price_minor {
            crate::validation::require_non_negative_minor("cost_price_minor", minor)?;
        }
        if let Some(Some(weight)) = self.weight_gram {
            crate::validation::require_non_negative("weight_gram", weight)?;
        }
        if let Some(min) = self.min_order_quantity {
            if min < 1 {
                return Err(CommerceServiceError::validation(
                    "min_order_quantity must be at least 1: it is what a single order buys",
                ));
            }
        }
        if let Some(step) = self.step_quantity {
            if step < 1 {
                return Err(CommerceServiceError::validation(
                    "step_quantity must be at least 1: it is what the orderable quantity moves in",
                ));
            }
        }
        if let Some(Some(max)) = self.max_order_quantity {
            crate::validation::require_non_negative("max_order_quantity", max)?;
        }
        if let Some(Some(units)) = self.units_per_package {
            if units < 1 {
                return Err(CommerceServiceError::validation(
                    "units_per_package must be at least 1",
                ));
            }
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
        crate::validation::require_non_empty("actor", &self.actor)?;
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
        crate::validation::require_non_empty("actor", &self.actor)?;
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
        crate::validation::require_non_empty("actor", &self.actor)?;
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
