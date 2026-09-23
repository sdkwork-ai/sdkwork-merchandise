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
        ],
        vec![
            "categories.management.list",
            "attributes.management.list",
            "categoryAttributes.list",
            "priceLists.list",
            "products.management.list",
            "products.management.retrieve",
            "skus.list",
            "media.list",
        ],
        vec![
            crate::ports::CATALOG_REPOSITORY_PORT,
            crate::ports::IDEMPOTENCY_REPOSITORY_PORT,
        ],
        true,
    )
}
