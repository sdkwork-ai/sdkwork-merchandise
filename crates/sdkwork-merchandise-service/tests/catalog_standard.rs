//! Catalog domain standard tests.
//!
//! These tests pin the shape of the merchandise domain contract. They exist to
//! make four classes of defect impossible to reintroduce silently:
//!
//! 1. legacy product-table vocabulary and the free-form `spec_json` carrier;
//! 2. capability tokens that no registered route actually serves;
//! 3. foreign-domain baggage (carts, buyer addresses) declared inside the
//!    catalog capability;
//! 4. storage vocabulary that the baseline's CHECK constraints would reject.
//!
//! Every write-model assertion below goes through the command types the
//! repository port consumes. Testing a parallel draft family instead would pin
//! vocabulary that no production path can reach.

use sdkwork_merchandise_service::{
    catalog_service_contract, AttributeRole, CatalogPortRequirement, CatalogRepositoryCommand,
    CreateAttributeCommand, CreateCategoryCommand, CreateMediaCommand, CreateProductSkuCommand,
    CreateProductSpuCommand, DeleteMediaCommand, FulfillmentType, InventoryTrackingMode,
    LifecycleStatus, MediaOwnerType, MediaRole, ProductStatus, ProductType, UpdateMediaCommand,
};

const ROUTE_MANIFEST: &str =
    "../../sdks/_route-manifests/backend-api/sdkwork-routes-merchandise-backend-api.route-manifest.json";

const TENANT_ID: &str = "100001";
const ORGANIZATION_ID: &str = "0";

#[test]
fn catalog_domain_contract_uses_standard_spu_sku_terms_without_legacy_product_table_debt() {
    let crate_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let checked_sources = [
        "Cargo.toml",
        "README.md",
        "src/lib.rs",
        "src/domain/mod.rs",
        "src/commands/mod.rs",
        "src/queries/mod.rs",
        "src/ports/mod.rs",
        "src/service/mod.rs",
        "specs/README.md",
        "specs/component.spec.json",
    ];
    // Legacy product-table vocabulary, the deleted free-form JSON carrier, and
    // capability names that belong to other owners' domains.
    let banned_fragments = [
        concat!("commerce_", "product\""),
        concat!("commerce_", "sku\""),
        concat!("commerce_", "cart"),
        concat!("commerce_", "user_address"),
        concat!("product", "_id"),
        concat!("product", "_no"),
        "DeliveryMode",
        "delivery_mode",
        "parent_category_id",
        "sort_weight",
        "spec_json",
        "_sdkwork",
        "CartItem",
        "BuyerAddress",
        // A duplicate status enum would shadow the lifecycle axis.
        "SalesStatus",
        // The deleted single-SKU family and its nullable-patch carrier.
        concat!("Single", "Sku"),
        concat!("single", "Sku"),
        "NullablePatch",
        concat!("Sqlite", "CommerceCatalogStore"),
        // The deleted draft family: a second write model for the same rows.
        concat!("Product", "SpuDraft"),
        concat!("Product", "SkuDraft"),
        concat!("ProductCategory", "Draft"),
        concat!("ProductAttribute", "Draft"),
        // The retired `/catalog/spus` capability tokens. That collection was a
        // second name for `commerce_product_spu`, so its tokens must not reappear
        // as capability surfaces: `products.*` is the surviving vocabulary.
        concat!("spus", ".create"),
        concat!("spus", ".update"),
        concat!("spus", ".publish"),
        concat!("spus", ".archive"),
        concat!("spus", ".management.list"),
    ];

    let mut violations = Vec::new();
    for relative_path in checked_sources {
        let contents = std::fs::read_to_string(crate_root.join(relative_path))
            .unwrap_or_else(|error| panic!("failed to read {relative_path}: {error}"));
        for banned in banned_fragments {
            if contents.contains(banned) {
                violations.push(format!("{relative_path} contains `{banned}`"));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "catalog domain must use explicit SPU/SKU terminology without legacy product table fragments:\n{}",
        violations.join("\n")
    );
}

#[test]
fn catalog_repository_contract_exposes_required_commands() {
    assert_eq!(
        CatalogPortRequirement::standard_commands(),
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
        ],
    );
}

/// The capability set the service declares must be exactly the capability set
/// its registered routes serve.
///
/// The route manifest is generated from the authored OpenAPI document, so this
/// assertion transitively pins the service contract to the published API. A
/// capability declared without a route (the historical `catalog.categorySeeds`
/// and `cart.*` / `addresses.*` tokens) or a route without a declared
/// capability both fail here.
#[test]
fn catalog_service_contract_matches_registered_routes() {
    let manifest_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(ROUTE_MANIFEST);
    let manifest = std::fs::read_to_string(&manifest_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", manifest_path.display()));
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest).expect("route manifest must be valid JSON");

    let routes = manifest["routes"]
        .as_array()
        .expect("route manifest must contain a `routes` array");
    assert!(!routes.is_empty(), "route manifest must not be empty");

    let mut expected: Vec<&str> = routes
        .iter()
        .map(|route| {
            route["operationId"]
                .as_str()
                .expect("every route must carry an operationId")
        })
        .collect();
    expected.sort_unstable();

    let contract = catalog_service_contract();
    let mut declared: Vec<&str> = contract
        .write_commands
        .iter()
        .chain(contract.read_queries.iter())
        .copied()
        .collect();
    declared.sort_unstable();

    let missing_in_contract: Vec<&&str> =
        expected.iter().filter(|t| !declared.contains(t)).collect();
    let declared_without_route: Vec<&&str> =
        declared.iter().filter(|t| !expected.contains(t)).collect();

    assert!(
        missing_in_contract.is_empty(),
        "registered routes with no declared capability token: {missing_in_contract:?}"
    );
    assert!(
        declared_without_route.is_empty(),
        "declared capability tokens with no registered route: {declared_without_route:?}"
    );
    assert_eq!(
        declared.len(),
        expected.len(),
        "capability tokens must be unique and one-to-one with registered routes"
    );
}

#[test]
fn catalog_service_contract_exposes_domain_operations() {
    let contract = catalog_service_contract();

    assert_eq!(contract.domain, "catalog");
    assert_eq!(contract.service_name, "commerce.catalog");
    assert!(contract.validate().is_ok());
    for query in [
        "categories.management.list",
        "attributes.management.list",
        "priceLists.list",
        "categoryAttributes.list",
        "products.management.list",
        "products.management.retrieve",
        "skus.list",
        "media.list",
    ] {
        assert!(
            contract.read_queries.contains(&query),
            "catalog contract must expose read query {query}",
        );
    }
    for command in [
        "categories.create",
        "categories.update",
        "categories.delete",
        "attributes.create",
        "categoryAttributes.create",
        "categoryAttributes.update",
        "categoryAttributes.delete",
        "priceLists.create",
        "priceLists.update",
        "products.create",
        "products.update",
        "products.delete",
        "products.publish",
        "products.archive",
        "skus.create",
        "skus.update",
        "skus.delete",
        "media.create",
        "media.update",
        "media.delete",
    ] {
        assert!(
            contract.write_commands.contains(&command),
            "catalog contract must expose write command {command}",
        );
    }
}

/// The SPU write model carries the product-type vocabulary, and nothing else does.
///
/// `category_id` is a required `String`, not an `Option`, because
/// `commerce_product_spu.category_id` is `NOT NULL`: a SPU outside every category cannot be
/// merchandised, filtered, or browsed, so an absent category is a malformed request rather than a
/// product without a home.
#[test]
fn product_spu_command_owns_the_product_type_vocabulary() {
    let command = CreateProductSpuCommand {
        tenant_id: TENANT_ID.to_owned(),
        organization_id: ORGANIZATION_ID.to_owned(),
        spu_no: "spu-membership-pro".to_owned(),
        title: "Pro membership".to_owned(),
        subtitle: Some("Yearly billing".to_owned()),
        description: None,
        product_type: ProductType::Membership,
        category_id: "200001".to_owned(),
    };
    assert!(command.validate().is_ok());

    assert_eq!(ProductType::Physical.as_storage_str(), "physical");
    assert_eq!(ProductType::Virtual.as_storage_str(), "digital");
    assert_eq!(ProductType::Membership.as_storage_str(), "membership");
    assert_eq!(ProductType::PointsRecharge.as_storage_str(), "points");
    assert_eq!(ProductType::Service.as_storage_str(), "service");
    assert_eq!(ProductStatus::Active.as_storage_str(), "active");

    let mut homeless = command.clone();
    homeless.category_id = "   ".to_owned();
    assert!(
        homeless.validate().is_err(),
        "a blank category_id must be rejected at the boundary, not at INSERT time"
    );

    let mut anonymous = command;
    anonymous.spu_no = String::new();
    assert!(anonymous.validate().is_err());
}

/// The SKU write model carries the fulfillment and inventory vocabularies.
#[test]
fn product_sku_command_owns_the_fulfillment_and_inventory_vocabulary() {
    let command = CreateProductSkuCommand {
        tenant_id: TENANT_ID.to_owned(),
        organization_id: ORGANIZATION_ID.to_owned(),
        spu_id: "300001".to_owned(),
        sku_no: "sku-membership-month-pro".to_owned(),
        name: "Monthly Pro membership".to_owned(),
        title: "Monthly Pro membership".to_owned(),
        sale_price_minor: 6_990,
        list_price_minor: Some(12_900),
        currency_code: "CNY".to_owned(),
        fulfillment_type: FulfillmentType::MembershipActivation,
        inventory_tracking: InventoryTrackingMode::Untracked,
        attribute_value_ids: Vec::new(),
        // A seller that declares no capability metadata sends the empty object, which is what the
        // column defaults to. The test that follows pins the other half of the contract: a
        // non-object is refused before it reaches a statement.
        metadata: serde_json::json!({}),
    };
    assert!(command.validate().is_ok());

    let mut scalars = command.clone();
    scalars.metadata = serde_json::json!("not an object");
    assert!(
        scalars.validate().is_err(),
        "metadata is a carrier for a capability's own object, so a scalar must be rejected at the \
         boundary rather than stored as a JSONB string"
    );

    let mut lists = command.clone();
    lists.metadata = serde_json::json!([1, 2, 3]);
    assert!(
        lists.validate().is_err(),
        "an array would make `metadata.key` unaddressable and force every reader to re-derive the \
         shape; only an object is accepted"
    );

    assert_eq!(
        FulfillmentType::PhysicalShipment.as_storage_str(),
        "physical"
    );
    assert_eq!(FulfillmentType::VirtualDelivery.as_storage_str(), "digital");
    assert_eq!(
        FulfillmentType::MembershipActivation.as_storage_str(),
        "membership_activation"
    );
    assert_eq!(
        FulfillmentType::PointsCredit.as_storage_str(),
        "points_topup"
    );
    assert_eq!(FulfillmentType::NoDelivery.as_storage_str(), "service");
    assert_eq!(InventoryTrackingMode::Tracked.as_storage_str(), "quantity");
    assert_eq!(InventoryTrackingMode::Untracked.as_storage_str(), "none");

    // Money is major-denomination on the command and minor units in storage; the repository owns
    // the translation, so a SKU without a currency cannot be priced at all.
    let mut priceless = command;
    priceless.currency_code = String::new();
    assert!(priceless.validate().is_err());
}

/// Category and attribute writes, including the bound the attribute-value CHECK imposes.
#[test]
fn category_and_attribute_commands_own_their_write_vocabulary() {
    let category = CreateCategoryCommand {
        tenant_id: TENANT_ID.to_owned(),
        organization_id: ORGANIZATION_ID.to_owned(),
        category_no: "cat-membership".to_owned(),
        parent_id: None,
        name: "Membership".to_owned(),
        sort_order: 100,
    };
    assert!(category.validate().is_ok());

    let attribute = CreateAttributeCommand {
        tenant_id: TENANT_ID.to_owned(),
        organization_id: ORGANIZATION_ID.to_owned(),
        attribute_no: "attr-duration".to_owned(),
        name: "Duration".to_owned(),
        values: vec!["30 days".to_owned(), "365 days".to_owned()],
    };
    assert!(attribute.validate().is_ok());

    // An attribute with no enumerated values is legal: `value_type` decides whether the value set is
    // closed, and `commerce_product_attribute_value` carries no minimum-row constraint.
    let mut open_ended = attribute.clone();
    open_ended.values = Vec::new();
    assert!(open_ended.validate().is_ok());

    // `ck_commerce_product_attribute_value_display_length` is `BETWEEN 1 AND 200`, so a blank or
    // over-long value has no storage form.
    let mut blank = attribute.clone();
    blank.values = vec!["   ".to_owned()];
    assert!(blank.validate().is_err());

    let mut over_long = attribute;
    over_long.values = vec!["x".repeat(201)];
    assert!(over_long.validate().is_err());

    // `LifecycleStatus` is the shared two-value vocabulary; `deleted` is expressed by the
    // `deleted_at` soft-delete pair and has no storage form here.
    assert_eq!(LifecycleStatus::Active.as_storage_str(), "active");
    assert_eq!(LifecycleStatus::Inactive.as_storage_str(), "inactive");
    assert!(LifecycleStatus::from_storage_str("status", "deleted").is_err());
}

/// Every crate's machine-readable component contract must be as free of the
/// deleted model families as its source is.
///
/// `specs/component.spec.json` is the local contract index, so a port or
/// entrypoint that survives a deletion there is exactly as misleading as one
/// that survives in `src/`. Checking only source files left this index able to
/// advertise `singleSkuMerchandiseService` and
/// `backend_catalog_router_with_sqlite_pool` long after both were gone.
#[test]
fn component_specs_declare_no_deleted_port_families() {
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("service crate must live under <repo>/crates");
    let crates_dir = repo_root.join("crates");

    let banned = [
        "singlesku",
        "single_sku",
        "nullablepatch",
        "sqlitecom",
        "sqlite_pool",
    ];
    let mut checked = 0_usize;
    let mut violations = Vec::new();

    let mut entries: Vec<_> = std::fs::read_dir(&crates_dir)
        .expect("crates directory must be readable")
        .map(|entry| entry.expect("crate directory entry").path())
        .collect();
    entries.sort();

    for crate_dir in entries {
        let spec_path = crate_dir.join("specs/component.spec.json");
        if !spec_path.is_file() {
            continue;
        }
        checked += 1;
        let raw = std::fs::read_to_string(&spec_path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", spec_path.display()));
        let spec: serde_json::Value = serde_json::from_str(&raw)
            .unwrap_or_else(|error| panic!("{} must be valid JSON: {error}", spec_path.display()));
        let contracts = &spec["contracts"];

        for key in ["providedPorts", "requiredPorts", "runtimeEntrypoints"] {
            let items = contracts[key].as_array().cloned().unwrap_or_default();
            for item in items {
                let text = item
                    .as_str()
                    .map(str::to_owned)
                    .or_else(|| item["name"].as_str().map(str::to_owned))
                    .unwrap_or_default();
                let lowered = text.to_lowercase();
                for fragment in banned {
                    if lowered.contains(fragment) {
                        violations.push(format!(
                            "{}: contracts.{key} still declares `{text}`",
                            spec_path.display()
                        ));
                    }
                }
            }
        }
    }

    assert!(
        checked >= 6,
        "expected to check every crate's component spec, found only {checked}"
    );
    assert!(
        violations.is_empty(),
        "component contracts must not declare deleted model families:\n{}",
        violations.join("\n")
    );
}

/// The body of one `CREATE TABLE` statement, or `None` if the table is absent.
fn baseline_table_block<'a>(ddl: &'a str, table: &str) -> Option<&'a str> {
    let marker = format!("CREATE TABLE IF NOT EXISTS {table} (");
    let start = ddl.find(&marker)? + marker.len();
    let rest = &ddl[start..];
    let end = rest.find("\n);")?;
    Some(&rest[..end])
}

/// The values a `CHECK (<column> IN (...))` constraint permits, if present.
fn allowed_values(block: &str, column: &str) -> Vec<String> {
    let needle = format!("{column} IN (");
    let Some(position) = block.find(&needle) else {
        return Vec::new();
    };
    let rest = &block[position + needle.len()..];
    let Some(close) = rest.find(')') else {
        return Vec::new();
    };
    rest[..close]
        .split(',')
        .map(|raw| raw.trim().trim_matches('\'').to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

/// Every domain enum's persisted form must be one the baseline accepts.
///
/// The DDL pins these columns with `CHECK (<column> IN (...))`. An enum whose
/// `as_storage_str` drifts from that list fails at INSERT time, inside the
/// database, far from the code that produced the value -- which is exactly how
/// `virtual` survived against a schema that only accepts `digital`. This test
/// moves the failure back to the type that owns the vocabulary.
///
/// The comparison is scoped **per table**, not per enum: a column name alone is
/// ambiguous, because `attribute_role` is constrained twice with two different
/// sets -- `commerce_product_category_attribute` accepts `key`/`sales`/`parameter`,
/// while `commerce_product_spu_attribute` accepts only `key`/`parameter`. Binding
/// an enum to the wrong table would make the test pass against the wrong list.
#[test]
fn storage_vocabulary_is_accepted_by_the_baseline_check_constraints() {
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("service crate must live under <repo>/crates");
    let baseline = std::fs::read_to_string(
        repo_root.join("database/ddl/baseline/postgres/0001_merchandise_baseline.sql"),
    )
    .expect("merchandise baseline must be readable");

    let spu = baseline_table_block(&baseline, "commerce_product_spu")
        .expect("baseline must declare commerce_product_spu");
    let sku = baseline_table_block(&baseline, "commerce_product_sku")
        .expect("baseline must declare commerce_product_sku");
    let category_attribute = baseline_table_block(&baseline, "commerce_product_category_attribute")
        .expect("baseline must declare commerce_product_category_attribute");
    let media = baseline_table_block(&baseline, "commerce_product_media")
        .expect("baseline must declare commerce_product_media");

    let cases: [(&str, &str, &str, Vec<&str>); 7] = [
        (
            "commerce_product_spu",
            "product_type",
            "commerce_product_spu.product_type",
            vec![
                ProductType::Physical.as_storage_str(),
                ProductType::Virtual.as_storage_str(),
                ProductType::Membership.as_storage_str(),
                ProductType::PointsRecharge.as_storage_str(),
                ProductType::Service.as_storage_str(),
            ],
        ),
        (
            "commerce_product_spu",
            "status",
            "commerce_product_spu.status",
            vec![
                ProductStatus::Draft.as_storage_str(),
                ProductStatus::Active.as_storage_str(),
                ProductStatus::Inactive.as_storage_str(),
                ProductStatus::Archived.as_storage_str(),
            ],
        ),
        (
            "commerce_product_sku",
            "fulfillment_type",
            "commerce_product_sku.fulfillment_type",
            vec![
                FulfillmentType::PhysicalShipment.as_storage_str(),
                FulfillmentType::VirtualDelivery.as_storage_str(),
                FulfillmentType::MembershipActivation.as_storage_str(),
                FulfillmentType::PointsCredit.as_storage_str(),
                FulfillmentType::NoDelivery.as_storage_str(),
            ],
        ),
        (
            "commerce_product_sku",
            "inventory_tracking",
            "commerce_product_sku.inventory_tracking",
            vec![
                InventoryTrackingMode::Tracked.as_storage_str(),
                InventoryTrackingMode::Untracked.as_storage_str(),
            ],
        ),
        (
            "commerce_product_category_attribute",
            "attribute_role",
            "commerce_product_category_attribute.attribute_role",
            vec![
                AttributeRole::Key.as_storage_str(),
                AttributeRole::Sales.as_storage_str(),
                AttributeRole::Parameter.as_storage_str(),
            ],
        ),
        // `commerce_product_media` is the first table here whose vocabulary is paired rather than
        // free-standing: `owner_type` and `media_role` are two separate CHECKs, and a third CHECK
        // then narrows the pair. Comparing each column against its own set is what keeps the domain
        // enums honest; the pairing itself is asserted separately, because it is a rule about the
        // combination and no per-column comparison can see it.
        (
            "commerce_product_media",
            "owner_type",
            "commerce_product_media.owner_type",
            vec![
                MediaOwnerType::Spu.as_storage_str(),
                MediaOwnerType::Sku.as_storage_str(),
                MediaOwnerType::Category.as_storage_str(),
                MediaOwnerType::AttributeValue.as_storage_str(),
            ],
        ),
        (
            "commerce_product_media",
            "media_role",
            "commerce_product_media.media_role",
            vec![
                MediaRole::MainImage.as_storage_str(),
                MediaRole::GalleryImage.as_storage_str(),
                MediaRole::DetailImage.as_storage_str(),
                MediaRole::SkuImage.as_storage_str(),
                MediaRole::Video.as_storage_str(),
                MediaRole::Manual.as_storage_str(),
                MediaRole::Certificate.as_storage_str(),
            ],
        ),
    ];

    let mut violations = Vec::new();
    for (table, column, label, produced) in cases {
        let block = match table {
            "commerce_product_spu" => spu,
            "commerce_product_sku" => sku,
            "commerce_product_category_attribute" => category_attribute,
            "commerce_product_media" => media,
            other => panic!("vocabulary case names an unloaded table: {other}"),
        };
        let allowed = allowed_values(block, column);
        assert!(
            !allowed.is_empty(),
            "{label} has no CHECK ({column} IN (...)) set to compare against"
        );
        for value in produced {
            if !allowed.iter().any(|permitted| permitted == value) {
                violations.push(format!(
                    "{label}: domain emits `{value}`, baseline allows [{}]",
                    allowed.join(", ")
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "domain storage vocabulary must match the baseline CHECK constraints:\n{}",
        violations.join("\n")
    );
}

/// The media write model owns the owner/role pair, the resource reference, and the snapshot key set.
///
/// None of the three is visible to the per-column vocabulary test above:
///
/// 1. `ck_commerce_product_media_owner_role` constrains a **pair**. The permitted subset is read back
///    out of the DDL rather than restated, so the assertion is two-sided: moving `admits_role` or
///    moving the constraint both fail, and a rule that had been dropped on either side cannot pass by
///    agreeing with nothing.
/// 2. `media_resource_id` is *derived* from the snapshot, which is what keeps the reference and the
///    projection from describing different files. Derivation is asserted by moving `id` and watching
///    the derived value move with it; a function that returned a constant would satisfy an equality
///    check but not this one.
/// 3. The resource's top level is closed, and it is exactly the key set
///    `CreateMediaRequest.resource` publishes — the lists are read from the validator rather than
///    copied, so the contract and the validator cannot drift apart in this test's blind spot.
#[test]
fn media_commands_own_the_owner_role_pair_and_the_resource_identity() {
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("service crate must live under <repo>/crates");
    let baseline = std::fs::read_to_string(
        repo_root.join("database/ddl/baseline/postgres/0001_merchandise_baseline.sql"),
    )
    .expect("merchandise baseline must be readable");
    let media = baseline_table_block(&baseline, "commerce_product_media")
        .expect("baseline must declare commerce_product_media");

    // ---- (1) the cross CHECK, read back from the DDL
    let cross_start = media
        .find("ck_commerce_product_media_owner_role")
        .expect("baseline must declare ck_commerce_product_media_owner_role");
    let cross = &media[cross_start..];
    let cross = &cross[..cross.find("\n    CONSTRAINT").unwrap_or(cross.len())];

    let restricted_owner = cross
        .split("owner_type <> '")
        .nth(1)
        .and_then(|rest| rest.split('\'').next())
        .expect("the cross CHECK must name the owner kind it restricts");
    let permitted_roles: Vec<&str> = cross
        .split("media_role IN (")
        .nth(1)
        .and_then(|rest| rest.split(')').next())
        .expect("the cross CHECK must name the roles it permits")
        .split(',')
        .map(|value| value.trim().trim_matches('\''))
        .collect();
    assert_eq!(
        permitted_roles.len(),
        3,
        "a dictionary value's media is its swatch, so the subset is three images; saw {permitted_roles:?}"
    );

    let restricted = MediaOwnerType::from_storage_str("owner_type", restricted_owner)
        .expect("the constrained owner kind must be a domain value");
    let all_roles = [
        MediaRole::MainImage,
        MediaRole::GalleryImage,
        MediaRole::DetailImage,
        MediaRole::SkuImage,
        MediaRole::Video,
        MediaRole::Manual,
        MediaRole::Certificate,
    ];
    let (admitted, rejected): (Vec<MediaRole>, Vec<MediaRole>) = all_roles
        .into_iter()
        .partition(|role| permitted_roles.contains(&role.as_storage_str()));
    assert!(
        !admitted.is_empty() && !rejected.is_empty(),
        "the cross CHECK must both admit and reject roles, or comparing against it proves nothing"
    );
    for role in &admitted {
        assert!(
            restricted.admits_role(*role),
            "the cross CHECK admits `{}` on `{restricted_owner}` but admits_role rejects it",
            role.as_storage_str()
        );
    }
    for role in &rejected {
        assert!(
            !restricted.admits_role(*role),
            "the cross CHECK rejects `{}` on `{restricted_owner}` but admits_role admits it",
            role.as_storage_str()
        );
    }
    // The constraint names one owner kind, so every other kind carries the full role set.
    for owner in [
        MediaOwnerType::Spu,
        MediaOwnerType::Sku,
        MediaOwnerType::Category,
    ] {
        if owner == restricted {
            continue;
        }
        for role in all_roles {
            assert!(
                owner.admits_role(role),
                "`{}` carries every role; {} was refused",
                owner.as_storage_str(),
                role.as_storage_str()
            );
        }
    }

    // ---- (2) the reference is derived from the snapshot, not accepted beside it
    fn snapshot(id: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "kind": "image",
            "source": "drive",
            "uri": format!("drive://spaces/9001/nodes/{id}"),
            "mimeType": "image/jpeg",
        })
    }
    fn command(snapshot: serde_json::Value) -> CreateMediaCommand {
        CreateMediaCommand {
            tenant_id: TENANT_ID.to_owned(),
            organization_id: ORGANIZATION_ID.to_owned(),
            owner_type: MediaOwnerType::Spu,
            owner_id: "300001".to_owned(),
            media_role: MediaRole::MainImage,
            resource_snapshot: snapshot,
            alt_text: Some("Front view".to_owned()),
            sort_order: 0,
        }
    }

    let first = command(snapshot("1700000000000000001"));
    assert!(first.validate().is_ok());
    assert_eq!(first.media_resource_id(), Ok(1_700_000_000_000_000_001));

    let second = command(snapshot("1700000000000000002"));
    assert_eq!(
        second.media_resource_id(),
        Ok(1_700_000_000_000_000_002),
        "media_resource_id must be read out of the snapshot; an independent field could disagree with it"
    );

    // ---- (3) the key set is closed on both sides
    let mut every_documented_key = snapshot("1700000000000000001");
    {
        let object = every_documented_key
            .as_object_mut()
            .expect("the snapshot must be an object");
        for key in sdkwork_merchandise_service::validation::MEDIA_RESOURCE_OPTIONAL_KEYS {
            object
                .entry(key.to_owned())
                .or_insert(serde_json::Value::Null);
        }
    }
    assert!(
        command(every_documented_key.clone()).validate().is_ok(),
        "every key the contract publishes must be accepted, or the document overstates the schema"
    );

    // `bucketId` is the key `MEDIA_RESOURCE_SPEC` section 3 singles out as non-standard, so it is a
    // better probe than an invented name: it is exactly the field a caller migrating from a bare
    // object key would send.
    every_documented_key
        .as_object_mut()
        .expect("the snapshot must be an object")
        .insert("bucketId".to_owned(), serde_json::json!("shop-media"));
    assert!(
        command(every_documented_key).validate().is_err(),
        "the declared schema is closed; an undeclared key must be refused rather than dropped"
    );

    // A Drive-backed resource with no `uri` cannot be resolved to a file, so it has no storage form.
    let mut unresolvable = snapshot("1700000000000000001");
    unresolvable
        .as_object_mut()
        .expect("the snapshot must be an object")
        .remove("uri");
    assert!(command(unresolvable).validate().is_err());

    // The role/owner-kind cross-check is enforced at the boundary, not left to the CHECK constraint:
    // a swatch cannot be a manual, and the caller learns the permitted subset from the message.
    let mut swatch_manual = command(snapshot("1700000000000000001"));
    swatch_manual.owner_type = MediaOwnerType::AttributeValue;
    swatch_manual.media_role = MediaRole::Manual;
    let error = swatch_manual
        .validate()
        .expect_err("a dictionary value cannot carry a manual");
    assert!(
        error.message().contains("main_image") && error.message().contains("gallery_image"),
        "the refusal must name the permitted subset, got `{}`",
        error.message()
    );

    // An update re-checks the snapshot only when one is being written, and the owner kind is not a
    // field of it at all: the repository re-evaluates the pair against the row's stored owner.
    let untouched = UpdateMediaCommand {
        tenant_id: TENANT_ID.to_owned(),
        // The version the caller read. `validate` does not judge it — it is not a property of the
        // payload — but the command carries it because the repository compares it against the row.
        expected_version: 3,
        media_id: "1800000000000000001".to_owned(),
        media_role: None,
        resource_snapshot: None,
        alt_text: None,
        sort_order: None,
        status: None,
    };
    assert!(untouched.validate().is_ok());
    assert_eq!(untouched.media_resource_id(), Ok(None));

    let retargeted = UpdateMediaCommand {
        media_id: "1800000000000000001".to_owned(),
        sort_order: Some(-1),
        ..untouched
    };
    assert!(
        retargeted.validate().is_err(),
        "ck_commerce_product_media_sort_order is `>= 0`, so a negative rank must be refused at the boundary"
    );

    assert!(DeleteMediaCommand {
        tenant_id: TENANT_ID.to_owned(),
        expected_version: 3,
        media_id: "1800000000000000001".to_owned(),
    }
    .validate()
    .is_ok());
}
