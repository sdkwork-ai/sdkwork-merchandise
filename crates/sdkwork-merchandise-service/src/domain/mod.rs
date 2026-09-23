//! Catalog domain vocabulary.
//!
//! Every enum here owns the **storage vocabulary** of one or more baseline CHECK constraints.
//! `as_storage_str` and `from_storage_str` are exact inverses, and the accepted set is the set the
//! database accepts — no more, no less. The pairs are pinned by the
//! `storage_vocabulary_is_accepted_by_the_baseline_check_constraints` test, because a value that
//! drifts from the DDL only fails later, at INSERT time, inside the database, far from the code
//! that produced it.
//!
//! Producers translate at the boundary: an HTTP string is parsed into one of these types by the
//! router, so the repository always receives a value the schema can store.

use sdkwork_contract_service::CommerceServiceError;

/// `commerce_product_spu.product_type`.
///
/// Storage values are deliberately shorter than the variant names where the schema uses the
/// business noun instead of the operation: a points-recharge product is stored as `points`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProductType {
    Physical,
    /// Stored as `digital`. The schema vocabulary is `digital`, not `virtual`.
    Virtual,
    Membership,
    /// Stored as `points`.
    PointsRecharge,
    Service,
}

/// `commerce_product_spu.status` and `commerce_product_sku.status`.
///
/// Both tables share `('draft', 'active', 'inactive', 'archived')`, so one type owns it.
///
/// There is deliberately no `deleted` variant: the baseline CHECK does not admit one. Deletion is
/// the `deleted_at`/`deleted_by` soft-delete pair, and a `status = 'deleted'` write would violate
/// the constraint rather than hide the row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProductStatus {
    Draft,
    Active,
    Inactive,
    Archived,
}

/// `commerce_product_sku.fulfillment_type`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FulfillmentType {
    /// Stored as `physical`.
    PhysicalShipment,
    /// Stored as `digital`.
    VirtualDelivery,
    MembershipActivation,
    /// Stored as `points_topup`.
    PointsCredit,
    /// Stored as `service`: nothing is delivered, but the SKU is still sellable.
    NoDelivery,
}

/// `commerce_product_sku.inventory_tracking`.
///
/// The baseline also constrains `inventory_policy` to `('deny', 'backorder')` with
/// `inventory_tracking = 'quantity' OR inventory_policy = 'deny'`. This type therefore describes
/// only whether stock is counted; the policy is derived from it, never set independently.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InventoryTrackingMode {
    /// Stored as `quantity`.
    Tracked,
    /// Stored as `none`.
    Untracked,
}

/// The shared `('active', 'inactive')` lifecycle vocabulary.
///
/// `commerce_product_category`, `commerce_product_attribute`, `commerce_product_category_attribute`
/// and `commerce_price_list` each declare the same two-value CHECK. Declaring it once here means a
/// fifth hand-rolled string pair cannot appear in a fifth place.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecycleStatus {
    Active,
    Inactive,
}

/// `commerce_product_category_attribute.attribute_role`.
///
/// This is the parameter/sales split that makes a category template meaningful, and it is why the
/// role lives on the **binding** rather than on `commerce_product_attribute`: one attribute is a
/// sales axis in the category that sells by it and a plain specification elsewhere.
///
/// * [`Key`](Self::Key) — identifies the product itself (brand, model, ISBN). One value per product,
///   searchable, and never a buyer-selectable choice.
/// * [`Sales`](Self::Sales) — a buyer-selectable option whose combination defines an SKU. These are
///   the axes `variant_signature` will be built from.
/// * [`Parameter`](Self::Parameter) — descriptive or filtering metadata that never splits an SKU.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttributeRole {
    /// Stored as `key`.
    Key,
    /// Stored as `sales`.
    Sales,
    /// Stored as `parameter`.
    Parameter,
}

// There is deliberately no draft family in this module: the catalog write model is the command set
// in `crate::commands`, which `crate::ports::CatalogRepositoryPort` consumes. A parallel draft type
// per table would be a second name for every field, with no single owner of the write vocabulary.

impl AttributeRole {
    #[must_use]
    pub const fn as_storage_str(self) -> &'static str {
        match self {
            Self::Key => "key",
            Self::Sales => "sales",
            Self::Parameter => "parameter",
        }
    }

    pub fn from_storage_str(raw: &str) -> Result<Self, CommerceServiceError> {
        let value = match raw {
            "key" => Self::Key,
            "sales" => Self::Sales,
            "parameter" => Self::Parameter,
            _ => return Err(unknown_value("attribute_role", raw, ATTRIBUTE_ROLE_VALUES)),
        };
        Ok(value)
    }
}

impl ProductType {
    #[must_use]
    pub const fn as_storage_str(self) -> &'static str {
        match self {
            Self::Physical => "physical",
            Self::Virtual => "digital",
            Self::Membership => "membership",
            Self::PointsRecharge => "points",
            Self::Service => "service",
        }
    }

    pub fn from_storage_str(raw: &str) -> Result<Self, CommerceServiceError> {
        let value = match raw {
            "physical" => Self::Physical,
            "digital" => Self::Virtual,
            "membership" => Self::Membership,
            "points" => Self::PointsRecharge,
            "service" => Self::Service,
            _ => return Err(unknown_value("product_type", raw, PRODUCT_TYPE_VALUES)),
        };
        Ok(value)
    }
}

impl ProductStatus {
    #[must_use]
    pub const fn as_storage_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Active => "active",
            Self::Inactive => "inactive",
            Self::Archived => "archived",
        }
    }

    pub fn from_storage_str(raw: &str) -> Result<Self, CommerceServiceError> {
        let value = match raw {
            "draft" => Self::Draft,
            "active" => Self::Active,
            "inactive" => Self::Inactive,
            "archived" => Self::Archived,
            _ => return Err(unknown_value("status", raw, PRODUCT_STATUS_VALUES)),
        };
        Ok(value)
    }
}

impl FulfillmentType {
    #[must_use]
    pub const fn as_storage_str(self) -> &'static str {
        match self {
            Self::PhysicalShipment => "physical",
            Self::VirtualDelivery => "digital",
            Self::MembershipActivation => "membership_activation",
            Self::PointsCredit => "points_topup",
            Self::NoDelivery => "service",
        }
    }

    pub fn from_storage_str(raw: &str) -> Result<Self, CommerceServiceError> {
        let value = match raw {
            "physical" => Self::PhysicalShipment,
            "digital" => Self::VirtualDelivery,
            "membership_activation" => Self::MembershipActivation,
            "points_topup" => Self::PointsCredit,
            "service" => Self::NoDelivery,
            _ => {
                return Err(unknown_value(
                    "fulfillment_type",
                    raw,
                    FULFILLMENT_TYPE_VALUES,
                ))
            }
        };
        Ok(value)
    }
}

impl InventoryTrackingMode {
    #[must_use]
    pub const fn as_storage_str(self) -> &'static str {
        match self {
            Self::Tracked => "quantity",
            Self::Untracked => "none",
        }
    }

    pub fn from_storage_str(raw: &str) -> Result<Self, CommerceServiceError> {
        let value = match raw {
            "quantity" => Self::Tracked,
            "none" => Self::Untracked,
            _ => {
                return Err(unknown_value(
                    "inventory_tracking",
                    raw,
                    INVENTORY_TRACKING_VALUES,
                ))
            }
        };
        Ok(value)
    }
}

impl LifecycleStatus {
    #[must_use]
    pub const fn as_storage_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Inactive => "inactive",
        }
    }

    pub fn from_storage_str(field: &str, raw: &str) -> Result<Self, CommerceServiceError> {
        let value = match raw {
            "active" => Self::Active,
            "inactive" => Self::Inactive,
            _ => return Err(unknown_value(field, raw, LIFECYCLE_STATUS_VALUES)),
        };
        Ok(value)
    }
}

const PRODUCT_TYPE_VALUES: &str = "physical, digital, membership, points, service";
const PRODUCT_STATUS_VALUES: &str = "draft, active, inactive, archived";
const FULFILLMENT_TYPE_VALUES: &str =
    "physical, digital, membership_activation, points_topup, service";
const INVENTORY_TRACKING_VALUES: &str = "none, quantity";
const LIFECYCLE_STATUS_VALUES: &str = "active, inactive";
const ATTRIBUTE_ROLE_VALUES: &str = "key, sales, parameter";

/// Rejects a value the baseline CHECK would reject, naming the permitted set.
///
/// The message carries the accepted values so an operator can fix the request without reading the
/// DDL, and so the failure is a `422` at the boundary instead of a `23514` from PostgreSQL.
fn unknown_value(field: &str, raw: &str, permitted: &str) -> CommerceServiceError {
    CommerceServiceError::validation(format!(
        "{field} `{raw}` is not one of the permitted values: {permitted}"
    ))
}
