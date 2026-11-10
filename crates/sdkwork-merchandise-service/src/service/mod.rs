use sdkwork_contract_service::CommerceServiceContract;

pub fn catalog_service_contract() -> CommerceServiceContract {
    // Capability tokens are the registered route operation identities, verbatim.
    // Keeping them identical to `sdks/_route-manifests/backend-api/
    // sdkwork-routes-merchandise-backend-api.route-manifest.json` makes the
    // contract checkable as a plain set equality, so a capability that no route
    // serves (the historical `catalog.categorySeeds`, `cart.*`, and
    // `addresses.*` tokens) cannot be declared again. The capability namespace
    // is carried by `service_name` ("commerce.catalog"), not by the tokens.
    //
    // Carts and buyer addresses are owned by `sdkwork-catalog`; inventory is
    // owned by `sdkwork-inventory`. Neither appears here.
    CommerceServiceContract::new(
        "catalog",
        "commerce.catalog",
        vec![
            "categories.create",
            "categories.update",
            "categories.delete",
            "attributes.create",
            "attributes.update",
            "attributeValues.create",
            "attributeValues.update",
            "attributeValues.delete",
            "categoryAttributes.create",
            "categoryAttributes.update",
            "categoryAttributes.delete",
            "priceListItems.create",
            "priceListItems.update",
            "priceListItems.delete",
            "priceLists.create",
            "priceLists.update",
            "products.attributes.update",
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
        ],
        vec![
            "categories.management.list",
            "categories.management.retrieve",
            "attributes.management.list",
            "attributes.management.retrieve",
            "attributeValues.list",
            "categoryAttributes.list",
            "priceListItems.list",
            "priceLists.list",
            "priceLists.retrieve",
            "products.management.list",
            "products.management.retrieve",
            "skus.list",
            "skus.retrieve",
            "media.list",
        ],
        vec![
            crate::ports::CATALOG_REPOSITORY_PORT,
            crate::ports::IDEMPOTENCY_REPOSITORY_PORT,
        ],
        true,
    )
}
