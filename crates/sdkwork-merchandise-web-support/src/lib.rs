//! Shared HTTP support for merchandise adapters; this crate does not own an API surface.

pub mod catalog_store;
pub mod http_envelope;
pub mod subject;

pub use catalog_store::{
    backend_catalog_router_with_postgres_pool, build_backend_catalog_router, map_address,
    map_attribute, map_cart_item, map_category, map_price_list_item, map_sku, map_spu,
    AddCartItemBody, AttributeQueryParams, CatalogState, CategoryQueryParams,
    CommerceCatalogFuture, CommerceCatalogStore, CreateAddressBody, CreateSpuBody,
    SpuListQueryParams, UpdateAddressBody, UpdateCartItemBody, UpdateSpuBody,
};
pub use http_envelope::{
    catalog_system_response, not_found_response, success_accepted, success_created_resource,
    success_list, success_no_content, success_offset_page, success_resource, unauthorized_response,
    validation_response,
};
