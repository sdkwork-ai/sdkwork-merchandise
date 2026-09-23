//! Shared HTTP support for merchandise adapters; this crate does not own an API surface.
//!
//! It owns the request DTOs, the response mappers, the error envelope, and the route table — and it
//! deliberately does not own the store port. `CatalogRepositoryPort` is declared by
//! `sdkwork-merchandise-service` and implemented by `sdkwork-merchandise-repository-sqlx`; this
//! crate only accepts one as `Arc<dyn ...>` when the composition root mounts the routes.

pub mod catalog_store;
pub mod http_envelope;
pub mod subject;

pub use catalog_store::{
    build_backend_catalog_router, map_attribute, map_category, map_product, map_sku,
    AttributeQueryParams, CatalogState, CategoryQueryParams, CreateSpuBody, ProductListQueryParams,
    UpdateSpuBody,
};
pub use http_envelope::{
    catalog_error_response, expected_version_from_if_match, not_found_response,
    stale_version_response, success_accepted, success_created_resource, success_list,
    success_no_content, success_offset_page, success_resource, success_resource_with_etag,
    unauthorized_response, validation_response, CatalogJson,
};
