//! Merchandise routes published through the SDKWork Shop backend authority.
//!
//! # Validation happens here
//!
//! Every enumerated request field is parsed into the domain type that owns its vocabulary before a
//! command is built. A value the baseline CHECK would reject therefore fails as a `422` naming the
//! permitted set, instead of travelling to PostgreSQL and coming back as a `23514` the caller cannot
//! act on.
//!
//! # The body is typed end to end
//!
//! Each write route extracts `CatalogJson<T>` rather than `Json<T>`, and every `T` is closed with
//! `deny_unknown_fields`. The authored contract declares a named schema with
//! `additionalProperties: false` for each of these bodies, and the two sides are compared field by
//! field by `tests/static/api-request-body-closure.test.mjs`. Extraction itself answers a body the
//! DTO cannot absorb with the same `400` problem envelope the domain validations below use, so a
//! caller never receives an unenveloped, uncorrelated `422` from the framework.

use std::sync::Arc;

use axum::extract::{Extension, Path, Query, State};
use axum::response::Response;
use axum::routing::{get, patch, post};
use axum::Router;
use sdkwork_iam_context_service::IamAppContext;
use sdkwork_merchandise_service::{
    ArchiveSpuCommand, AttributeListQuery, AttributeRole, CategoryAttributeListQuery,
    CategoryListQuery, CreateAttributeCommand, CreateCategoryAttributeCommand,
    CreateCategoryCommand, CreateMediaCommand, CreatePriceListCommand, CreateProductSkuCommand,
    CreateProductSpuCommand, DeleteCategoryAttributeCommand, DeleteCategoryCommand,
    DeleteMediaCommand, DeleteProductSkuCommand, DeleteProductSpuCommand, FulfillmentType,
    GuardedWrite, InventoryTrackingMode, LifecycleStatus, MediaListQuery, MediaOwnerType,
    MediaRole, PriceListListQuery, ProductSkuListQuery, ProductSpuListQuery,
    ProductSpuRetrieveQuery, ProductStatus, ProductType, PublishSpuCommand,
    UpdateCategoryAttributeCommand, UpdateCategoryCommand, UpdateMediaCommand,
    UpdatePriceListCommand, UpdateProductSkuCommand, UpdateProductSpuCommand,
};

use super::{
    catalog_error_response, expected_version_from_if_match, map_attribute, map_category,
    map_category_attribute, map_media, map_price_list, map_product, map_sku, not_found_response,
    stale_version_response, success_created_resource, success_no_content, success_offset_page,
    success_resource_with_etag, unauthorized_response, validation_response, AttributeQueryParams,
    CatalogJson, CatalogRepositoryPort, CatalogState, CategoryAttributeQueryParams,
    CategoryQueryParams, CreateAttributeBody, CreateCategoryAttributeBody, CreateCategoryBody,
    CreateMediaBody, CreatePriceListBody, CreateSkuBody, CreateSpuBody, MediaQueryParams,
    PriceListQueryParams, ProductListQueryParams, SkuListQueryParams, UpdateCategoryAttributeBody,
    UpdateCategoryBody, UpdateMediaBody, UpdatePriceListBody, UpdateSkuBody, UpdateSpuBody,
};
use crate::subject::app_runtime_subject_from_extension;

/// Returns the parsed value, or answers the request with a `422` naming the permitted values.
///
/// The vocabulary comes from the domain types, so the message a caller sees and the set the database
/// accepts are the same set by construction.
///
/// Optional fields spell their own `match body.field.as_deref()` instead: the `None` arm must yield
/// `None` without calling the parser, and a macro that took the raw string would lose the domain
/// type it parses into.
macro_rules! parse_or_422 {
    ($expression:expr) => {
        match $expression {
            Ok(value) => value,
            Err(error) => return validation_response(error.message()),
        }
    };
}

/// Parses an `API_SPEC` section 13.6 int64-string into the native integer the domain uses.
///
/// Every int64 travels as a JSON string so a browser never rounds a snowflake id or a minor-unit
/// amount; the HTTP adapter is the layer that turns it back, which is why this lives here and not
/// in the repository. The message names the wire field, so a caller fixing a payload sees the same
/// spelling it sent.
fn parse_int64(field: &str, raw: &str) -> Result<i64, String> {
    raw.trim()
        .parse::<i64>()
        .map_err(|_| format!("{field} must be a decimal int64 string, got `{raw}`"))
}

fn parse_optional_int64(field: &str, raw: Option<&str>) -> Result<Option<i64>, String> {
    raw.map(|value| parse_int64(field, value)).transpose()
}

/// Returns the parsed value, or answers the request with the platform `400` envelope.
macro_rules! parse_int64_or_400 {
    ($field:expr, $raw:expr) => {
        match parse_int64($field, $raw) {
            Ok(value) => value,
            Err(message) => return validation_response(message),
        }
    };
}

macro_rules! parse_optional_int64_or_400 {
    ($field:expr, $raw:expr) => {
        match parse_optional_int64($field, $raw) {
            Ok(value) => value,
            Err(message) => return validation_response(message),
        }
    };
}

/// Mounts the catalog routes over a store the caller has already constructed.
///
/// The store arrives as the service-owned port, so this crate never names a database, a pool, or a
/// concrete repository. Construction belongs to the composition root
/// (`sdkwork-merchandise-service-host`), which is the only layer allowed to know both the port and
/// its implementation.
pub fn build_backend_catalog_router(store: Arc<dyn CatalogRepositoryPort>) -> Router {
    Router::new()
        .route(
            "/backend/v3/api/catalog/categories",
            get(backend_list_categories).post(backend_create_category),
        )
        .route(
            "/backend/v3/api/catalog/categories/{categoryId}",
            patch(backend_update_category).delete(backend_delete_category),
        )
        .route(
            "/backend/v3/api/catalog/products",
            get(backend_list_products).post(backend_create_product),
        )
        .route(
            "/backend/v3/api/catalog/products/{productId}",
            get(backend_retrieve_product)
                .patch(backend_update_product)
                .delete(backend_delete_product),
        )
        .route(
            "/backend/v3/api/catalog/products/{productId}/publish",
            post(backend_publish_product),
        )
        .route(
            "/backend/v3/api/catalog/products/{productId}/archive",
            post(backend_archive_product),
        )
        .route(
            "/backend/v3/api/catalog/skus",
            get(backend_list_skus).post(backend_create_sku),
        )
        .route(
            "/backend/v3/api/catalog/skus/{skuId}",
            patch(backend_update_sku).delete(backend_delete_sku),
        )
        .route(
            "/backend/v3/api/catalog/attributes",
            get(backend_list_attributes).post(backend_create_attribute),
        )
        .route(
            "/backend/v3/api/catalog/category_attributes",
            get(backend_list_category_attributes).post(backend_create_category_attribute),
        )
        .route(
            "/backend/v3/api/catalog/category_attributes/{bindingId}",
            patch(backend_update_category_attribute).delete(backend_delete_category_attribute),
        )
        .route(
            "/backend/v3/api/catalog/price_lists",
            get(backend_list_price_lists).post(backend_create_price_list),
        )
        .route(
            "/backend/v3/api/catalog/price_lists/{priceListId}",
            patch(backend_update_price_list),
        )
        .route(
            "/backend/v3/api/catalog/media",
            get(backend_list_media).post(backend_create_media),
        )
        .route(
            "/backend/v3/api/catalog/media/{mediaId}",
            patch(backend_update_media).delete(backend_delete_media),
        )
        .with_state(CatalogState { store })
}

async fn backend_list_categories(
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Query(params): Query<CategoryQueryParams>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };
    let query = match CategoryListQuery::new(
        &subject.tenant_id,
        subject.organization_id.as_deref(),
        params.parent_id.as_deref(),
        params.status.as_deref(),
        params.page,
        params.page_size,
    ) {
        Ok(query) => query,
        Err(error) => return validation_response(error.message()),
    };
    match state.store.list_categories_page(query).await {
        Ok(data) => success_offset_page(
            data.items.into_iter().map(map_category).collect(),
            data.page,
            data.page_size,
            data.total_items,
        ),
        Err(error) => catalog_error_response("category list is unavailable", error),
    }
}

async fn backend_create_category(
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    CatalogJson(body): CatalogJson<CreateCategoryBody>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };
    let organization_id = match subject.organization_id.as_deref() {
        Some(id) => id.to_owned(),
        None => return validation_response("organization_id is required"),
    };
    let sort_order =
        parse_optional_int64_or_400!("sortOrder", body.sort_order.as_deref()).unwrap_or(0);
    let command = CreateCategoryCommand {
        tenant_id: subject.tenant_id,
        organization_id,
        category_no: body.category_no,
        parent_id: body.parent_id,
        name: body.name,
        sort_order,
    };
    match command.validate() {
        Ok(()) => {}
        Err(error) => return validation_response(error.message()),
    }
    match state.store.create_category(command).await {
        Ok(data) => success_created_resource(map_category(data)),
        Err(error) => catalog_error_response("failed to create category", error),
    }
}

async fn backend_update_category(
    headers: axum::http::HeaderMap,
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Path(category_id): Path<String>,
    CatalogJson(body): CatalogJson<UpdateCategoryBody>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };

    // Read before the body is used for anything, so a request that did not say which version it is
    // editing is refused with `428` rather than being allowed to reach a write.
    let expected_version = match expected_version_from_if_match(&headers) {
        Ok(version) => version,
        Err(response) => return *response,
    };
    let status = match body.status.as_deref() {
        Some(raw) => Some(parse_or_422!(LifecycleStatus::from_storage_str(
            "status", raw
        ))),
        None => None,
    };
    let sort_order = parse_optional_int64_or_400!("sortOrder", body.sort_order.as_deref());
    let command = UpdateCategoryCommand {
        tenant_id: subject.tenant_id,
        expected_version,
        category_id,
        parent_id: body.parent_id,
        name: body.name,
        sort_order,
        status,
    };
    match command.validate() {
        Ok(()) => {}
        Err(error) => return validation_response(error.message()),
    }
    match state.store.update_category(command).await {
        Ok(GuardedWrite::Applied(data)) => {
            // The row's version *after* the write, which is what the next `If-Match` must name.
            let version = data.version;
            success_resource_with_etag(map_category(data), version)
        }
        Ok(GuardedWrite::StaleVersion(stale)) => stale_version_response("category", stale),
        Err(error) => catalog_error_response("failed to update category", error),
    }
}

async fn backend_delete_category(
    headers: axum::http::HeaderMap,
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Path(category_id): Path<String>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };

    // Read before the body is used for anything, so a request that did not say which version it is
    // editing is refused with `428` rather than being allowed to reach a write.
    let expected_version = match expected_version_from_if_match(&headers) {
        Ok(version) => version,
        Err(response) => return *response,
    };
    let command = DeleteCategoryCommand {
        tenant_id: subject.tenant_id,
        expected_version,
        category_id,
    };
    match command.validate() {
        Ok(()) => {}
        Err(error) => return validation_response(error.message()),
    }
    match state.store.delete_category(command).await {
        Ok(GuardedWrite::Applied(())) => success_no_content(),
        Ok(GuardedWrite::StaleVersion(stale)) => stale_version_response("category", stale),
        Err(error) => catalog_error_response("failed to delete category", error),
    }
}

async fn backend_list_products(
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Query(params): Query<ProductListQueryParams>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };
    let query = match ProductSpuListQuery::new(
        &subject.tenant_id,
        subject.organization_id.as_deref(),
        params.q.as_deref(),
        params.category_id.as_deref(),
        params.product_type.as_deref(),
        params.status.as_deref(),
        params.sort.as_deref(),
        params.page,
        params.page_size,
    ) {
        Ok(query) => query,
        Err(error) => return validation_response(error.message()),
    };
    match state.store.list_spus_page(query).await {
        Ok(data) => success_offset_page(
            data.items.into_iter().map(map_product).collect(),
            data.page,
            data.page_size,
            data.total_items,
        ),
        Err(error) => catalog_error_response("product list is unavailable", error),
    }
}

async fn backend_retrieve_product(
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Path(product_id): Path<String>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };
    let query = match ProductSpuRetrieveQuery::new(&subject.tenant_id, &product_id) {
        Ok(query) => query,
        Err(error) => return validation_response(error.message()),
    };
    match state.store.retrieve_spu(query).await {
        Ok(Some(data)) => {
            // `GET` publishes the version as `ETag` as well as in the body, so a client can hold
            // the header and hand it straight back as `If-Match` without parsing the payload.
            let version = data.version;
            success_resource_with_etag(map_product(data), version)
        }
        Ok(None) => not_found_response("product was not found"),
        Err(error) => catalog_error_response("product read model is unavailable", error),
    }
}

async fn backend_create_product(
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    CatalogJson(body): CatalogJson<CreateSpuBody>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };
    let organization_id = match subject.organization_id.as_deref() {
        Some(id) => id.to_owned(),
        None => return validation_response("organization_id is required"),
    };
    let product_type = parse_or_422!(ProductType::from_storage_str(&body.product_type));
    let command = CreateProductSpuCommand {
        tenant_id: subject.tenant_id,
        organization_id,
        spu_no: body.product_no,
        title: body.title,
        subtitle: body.subtitle,
        description: body.description,
        product_type,
        category_id: body.category_id,
    };
    match command.validate() {
        Ok(()) => {}
        Err(error) => return validation_response(error.message()),
    }
    match state.store.create_spu(command).await {
        Ok(data) => success_created_resource(map_product(data)),
        Err(error) => catalog_error_response("failed to create product", error),
    }
}

async fn backend_update_product(
    headers: axum::http::HeaderMap,
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Path(product_id): Path<String>,
    CatalogJson(body): CatalogJson<UpdateSpuBody>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };

    // Read before the body is used for anything, so a request that did not say which version it is
    // editing is refused with `428` rather than being allowed to reach a write.
    let expected_version = match expected_version_from_if_match(&headers) {
        Ok(version) => version,
        Err(response) => return *response,
    };
    let command = UpdateProductSpuCommand {
        tenant_id: subject.tenant_id,
        expected_version,
        spu_id: product_id,
        title: body.title,
        subtitle: body.subtitle,
        description: body.description,
        category_id: body.category_id,
    };
    match command.validate() {
        Ok(()) => {}
        Err(error) => return validation_response(error.message()),
    }
    match state.store.update_spu(command).await {
        Ok(GuardedWrite::Applied(data)) => {
            // The row's version *after* the write, which is what the next `If-Match` must name.
            let version = data.version;
            success_resource_with_etag(map_product(data), version)
        }
        Ok(GuardedWrite::StaleVersion(stale)) => stale_version_response("product", stale),
        Err(error) => catalog_error_response("failed to update product", error),
    }
}

async fn backend_delete_product(
    headers: axum::http::HeaderMap,
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Path(product_id): Path<String>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };

    // Read before the body is used for anything, so a request that did not say which version it is
    // editing is refused with `428` rather than being allowed to reach a write.
    let expected_version = match expected_version_from_if_match(&headers) {
        Ok(version) => version,
        Err(response) => return *response,
    };
    let command = DeleteProductSpuCommand {
        tenant_id: subject.tenant_id,
        expected_version,
        spu_id: product_id,
    };
    match command.validate() {
        Ok(()) => {}
        Err(error) => return validation_response(error.message()),
    }
    match state.store.delete_spu(command).await {
        Ok(GuardedWrite::Applied(())) => success_no_content(),
        Ok(GuardedWrite::StaleVersion(stale)) => stale_version_response("product", stale),
        Err(error) => catalog_error_response("failed to delete product", error),
    }
}

async fn backend_publish_product(
    headers: axum::http::HeaderMap,
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Path(product_id): Path<String>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };

    // Read before the body is used for anything, so a request that did not say which version it is
    // editing is refused with `428` rather than being allowed to reach a write.
    let expected_version = match expected_version_from_if_match(&headers) {
        Ok(version) => version,
        Err(response) => return *response,
    };
    let command = PublishSpuCommand {
        tenant_id: subject.tenant_id,
        expected_version,
        spu_id: product_id,
    };
    match command.validate() {
        Ok(()) => {}
        Err(error) => return validation_response(error.message()),
    }
    match state.store.publish_spu(command).await {
        Ok(GuardedWrite::Applied(data)) => {
            // The row's version *after* the write, which is what the next `If-Match` must name.
            let version = data.version;
            success_resource_with_etag(map_product(data), version)
        }
        Ok(GuardedWrite::StaleVersion(stale)) => stale_version_response("product", stale),
        Err(error) => catalog_error_response("failed to publish product", error),
    }
}

async fn backend_archive_product(
    headers: axum::http::HeaderMap,
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Path(product_id): Path<String>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };

    // Read before the body is used for anything, so a request that did not say which version it is
    // editing is refused with `428` rather than being allowed to reach a write.
    let expected_version = match expected_version_from_if_match(&headers) {
        Ok(version) => version,
        Err(response) => return *response,
    };
    let command = ArchiveSpuCommand {
        tenant_id: subject.tenant_id,
        expected_version,
        spu_id: product_id,
    };
    match command.validate() {
        Ok(()) => {}
        Err(error) => return validation_response(error.message()),
    }
    match state.store.archive_spu(command).await {
        Ok(GuardedWrite::Applied(data)) => {
            // The row's version *after* the write, which is what the next `If-Match` must name.
            let version = data.version;
            success_resource_with_etag(map_product(data), version)
        }
        Ok(GuardedWrite::StaleVersion(stale)) => stale_version_response("product", stale),
        Err(error) => catalog_error_response("failed to archive product", error),
    }
}

async fn backend_list_skus(
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Query(params): Query<SkuListQueryParams>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };
    let query = match ProductSkuListQuery::new(
        &subject.tenant_id,
        subject.organization_id.as_deref(),
        params.product_id.as_deref(),
        params.attribute_value_id.as_deref(),
        params.status.as_deref(),
        params.page,
        params.page_size,
    ) {
        Ok(query) => query,
        Err(error) => return validation_response(error.message()),
    };
    match state.store.list_skus_page(query).await {
        Ok(data) => success_offset_page(
            data.items.into_iter().map(map_sku).collect(),
            data.page,
            data.page_size,
            data.total_items,
        ),
        Err(error) => catalog_error_response("sku list is unavailable", error),
    }
}

async fn backend_create_sku(
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    CatalogJson(body): CatalogJson<CreateSkuBody>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };
    let organization_id = match subject.organization_id.as_deref() {
        Some(id) => id.to_owned(),
        None => return validation_response("organization_id is required"),
    };
    let sale_price_minor = parse_int64_or_400!("salePriceMinor", &body.sale_price_minor);
    let list_price_minor =
        parse_optional_int64_or_400!("listPriceMinor", body.list_price_minor.as_deref());
    let fulfillment_type = parse_or_422!(FulfillmentType::from_storage_str(&body.fulfillment_type));
    let inventory_tracking = parse_or_422!(InventoryTrackingMode::from_storage_str(
        &body.inventory_tracking
    ));
    let command = CreateProductSkuCommand {
        tenant_id: subject.tenant_id,
        organization_id,
        spu_id: body.product_id,
        sku_no: body.sku_no,
        name: body.name,
        title: body.title,
        sale_price_minor,
        list_price_minor,
        currency_code: body.currency_code,
        fulfillment_type,
        inventory_tracking,
        attribute_value_ids: body.attribute_value_ids.unwrap_or_default(),
        // The capability metadata slot is deliberately not on this route. `CreateSkuRequest` is a
        // closed schema (`additionalProperties: false`), so a field this body never declares cannot
        // reach the command through HTTP; a capability that owns metadata writes it in-process,
        // through the same service this handler calls. Sending `{}` states that plainly: a catalog
        // seller created over HTTP declares no capability metadata, which is also the column default.
        metadata: serde_json::json!({}),
    };
    match command.validate() {
        Ok(()) => {}
        Err(error) => return validation_response(error.message()),
    }
    match state.store.create_sku(command).await {
        Ok(data) => success_created_resource(map_sku(data)),
        Err(error) => catalog_error_response("failed to create sku", error),
    }
}

async fn backend_update_sku(
    headers: axum::http::HeaderMap,
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Path(sku_id): Path<String>,
    CatalogJson(body): CatalogJson<UpdateSkuBody>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };

    // Read before the body is used for anything, so a request that did not say which version it is
    // editing is refused with `428` rather than being allowed to reach a write.
    let expected_version = match expected_version_from_if_match(&headers) {
        Ok(version) => version,
        Err(response) => return *response,
    };
    let sale_price_minor =
        parse_optional_int64_or_400!("salePriceMinor", body.sale_price_minor.as_deref());
    let list_price_minor =
        parse_optional_int64_or_400!("listPriceMinor", body.list_price_minor.as_deref());
    let fulfillment_type = match body.fulfillment_type.as_deref() {
        Some(raw) => Some(parse_or_422!(FulfillmentType::from_storage_str(raw))),
        None => None,
    };
    let inventory_tracking = match body.inventory_tracking.as_deref() {
        Some(raw) => Some(parse_or_422!(InventoryTrackingMode::from_storage_str(raw))),
        None => None,
    };
    let status = match body.status.as_deref() {
        Some(raw) => Some(parse_or_422!(ProductStatus::from_storage_str(raw))),
        None => None,
    };
    let command = UpdateProductSkuCommand {
        tenant_id: subject.tenant_id,
        expected_version,
        sku_id,
        name: body.name,
        title: body.title,
        sale_price_minor,
        list_price_minor,
        currency_code: body.currency_code,
        fulfillment_type,
        inventory_tracking,
        status,
        attribute_value_ids: body.attribute_value_ids,
        // `None` is the load-bearing choice, not a convenient default: this route has no metadata
        // field (closed `UpdateSkuRequest`), so a price-only edit made over HTTP must leave whatever
        // the owning capability stored untouched. Sending `Some({})` here would let a seller's edit
        // erase another capability's metadata as a side effect.
        metadata: None,
    };
    match command.validate() {
        Ok(()) => {}
        Err(error) => return validation_response(error.message()),
    }
    match state.store.update_sku(command).await {
        Ok(GuardedWrite::Applied(data)) => {
            // The row's version *after* the write, which is what the next `If-Match` must name.
            let version = data.version;
            success_resource_with_etag(map_sku(data), version)
        }
        Ok(GuardedWrite::StaleVersion(stale)) => stale_version_response("sku", stale),
        Err(error) => catalog_error_response("failed to update sku", error),
    }
}

async fn backend_delete_sku(
    headers: axum::http::HeaderMap,
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Path(sku_id): Path<String>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };

    // Read before the body is used for anything, so a request that did not say which version it is
    // editing is refused with `428` rather than being allowed to reach a write.
    let expected_version = match expected_version_from_if_match(&headers) {
        Ok(version) => version,
        Err(response) => return *response,
    };
    let command = DeleteProductSkuCommand {
        tenant_id: subject.tenant_id,
        expected_version,
        sku_id,
    };
    match command.validate() {
        Ok(()) => {}
        Err(error) => return validation_response(error.message()),
    }
    match state.store.delete_sku(command).await {
        Ok(GuardedWrite::Applied(())) => success_no_content(),
        Ok(GuardedWrite::StaleVersion(stale)) => stale_version_response("sku", stale),
        Err(error) => catalog_error_response("failed to delete sku", error),
    }
}

async fn backend_list_attributes(
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Query(params): Query<AttributeQueryParams>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };
    let query = match AttributeListQuery::new(
        &subject.tenant_id,
        subject.organization_id.as_deref(),
        params.status.as_deref(),
        params.page,
        params.page_size,
    ) {
        Ok(query) => query,
        Err(error) => return validation_response(error.message()),
    };
    match state.store.list_attributes_page(query).await {
        Ok(data) => success_offset_page(
            data.items.into_iter().map(map_attribute).collect(),
            data.page,
            data.page_size,
            data.total_items,
        ),
        Err(error) => catalog_error_response("attribute list is unavailable", error),
    }
}

async fn backend_create_attribute(
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    CatalogJson(body): CatalogJson<CreateAttributeBody>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };
    let organization_id = match subject.organization_id.as_deref() {
        Some(id) => id.to_owned(),
        None => return validation_response("organization_id is required"),
    };
    let command = CreateAttributeCommand {
        tenant_id: subject.tenant_id,
        organization_id,
        attribute_no: body.attribute_no,
        name: body.name,
        values: body.values.unwrap_or_default(),
    };
    match command.validate() {
        Ok(()) => {}
        Err(error) => return validation_response(error.message()),
    }
    match state.store.create_attribute(command).await {
        Ok(data) => success_created_resource(map_attribute(data)),
        Err(error) => catalog_error_response("failed to create attribute", error),
    }
}

async fn backend_list_category_attributes(
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Query(params): Query<CategoryAttributeQueryParams>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };
    let query = match CategoryAttributeListQuery::new(
        &subject.tenant_id,
        subject.organization_id.as_deref(),
        params.category_id.as_deref(),
        params.attribute_id.as_deref(),
        params.status.as_deref(),
        params.page,
        params.page_size,
    ) {
        Ok(query) => query,
        Err(error) => return validation_response(error.message()),
    };
    match state.store.list_category_attributes_page(query).await {
        Ok(data) => success_offset_page(
            data.items.into_iter().map(map_category_attribute).collect(),
            data.page,
            data.page_size,
            data.total_items,
        ),
        Err(error) => catalog_error_response("category attribute list is unavailable", error),
    }
}

async fn backend_create_category_attribute(
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    CatalogJson(body): CatalogJson<CreateCategoryAttributeBody>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };
    let organization_id = match subject.organization_id.as_deref() {
        Some(id) => id.to_owned(),
        None => return validation_response("organization_id is required"),
    };
    let sort_order =
        parse_optional_int64_or_400!("sortOrder", body.sort_order.as_deref()).unwrap_or(0);
    let command = CreateCategoryAttributeCommand {
        tenant_id: subject.tenant_id,
        organization_id,
        category_id: body.category_id,
        attribute_id: body.attribute_id,
        role: match body.role.as_deref() {
            Some(raw) => parse_or_422!(AttributeRole::from_storage_str(raw)),
            None => AttributeRole::Parameter,
        },
        required: body.required.unwrap_or(false),
        searchable: body.searchable.unwrap_or(false),
        filterable: body.filterable.unwrap_or(false),
        comparable: body.comparable.unwrap_or(false),
        source_category_id: body.source_category_id,
        sort_order,
    };
    match command.validate() {
        Ok(()) => {}
        Err(error) => return validation_response(error.message()),
    }
    match state.store.create_category_attribute(command).await {
        Ok(data) => success_created_resource(map_category_attribute(data)),
        Err(error) => catalog_error_response("failed to create category attribute", error),
    }
}

async fn backend_update_category_attribute(
    headers: axum::http::HeaderMap,
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Path(binding_id): Path<String>,
    CatalogJson(body): CatalogJson<UpdateCategoryAttributeBody>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };

    // Read before the body is used for anything, so a request that did not say which version it is
    // editing is refused with `428` rather than being allowed to reach a write.
    let expected_version = match expected_version_from_if_match(&headers) {
        Ok(version) => version,
        Err(response) => return *response,
    };
    let command = UpdateCategoryAttributeCommand {
        tenant_id: subject.tenant_id,
        expected_version,
        binding_id,
        role: match body.role.as_deref() {
            Some(raw) => Some(parse_or_422!(AttributeRole::from_storage_str(raw))),
            None => None,
        },
        required: body.required,
        searchable: body.searchable,
        filterable: body.filterable,
        comparable: body.comparable,
        sort_order: parse_optional_int64_or_400!("sortOrder", body.sort_order.as_deref()),
        status: match body.status.as_deref() {
            Some(raw) => Some(parse_or_422!(LifecycleStatus::from_storage_str(
                "status", raw
            ))),
            None => None,
        },
    };
    match command.validate() {
        Ok(()) => {}
        Err(error) => return validation_response(error.message()),
    }
    match state.store.update_category_attribute(command).await {
        Ok(GuardedWrite::Applied(data)) => {
            // The row's version *after* the write, which is what the next `If-Match` must name.
            let version = data.version;
            success_resource_with_etag(map_category_attribute(data), version)
        }
        Ok(GuardedWrite::StaleVersion(stale)) => {
            stale_version_response("category attribute", stale)
        }
        Err(error) => catalog_error_response("failed to update category attribute", error),
    }
}

async fn backend_delete_category_attribute(
    headers: axum::http::HeaderMap,
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Path(binding_id): Path<String>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };

    // Read before the body is used for anything, so a request that did not say which version it is
    // editing is refused with `428` rather than being allowed to reach a write.
    let expected_version = match expected_version_from_if_match(&headers) {
        Ok(version) => version,
        Err(response) => return *response,
    };
    let command = DeleteCategoryAttributeCommand {
        tenant_id: subject.tenant_id,
        expected_version,
        binding_id,
    };
    match command.validate() {
        Ok(()) => {}
        Err(error) => return validation_response(error.message()),
    }
    match state.store.delete_category_attribute(command).await {
        Ok(GuardedWrite::Applied(())) => success_no_content(),
        Ok(GuardedWrite::StaleVersion(stale)) => {
            stale_version_response("category attribute", stale)
        }
        Err(error) => catalog_error_response("failed to delete category attribute", error),
    }
}

async fn backend_list_price_lists(
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Query(params): Query<PriceListQueryParams>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };
    let query = match PriceListListQuery::new(
        &subject.tenant_id,
        subject.organization_id.as_deref(),
        params.currency_code.as_deref(),
        params.market_code.as_deref(),
        params.status.as_deref(),
        params.page,
        params.page_size,
    ) {
        Ok(query) => query,
        Err(error) => return validation_response(error.message()),
    };
    match state.store.list_price_lists_page(query).await {
        Ok(data) => success_offset_page(
            data.items.into_iter().map(map_price_list).collect(),
            data.page,
            data.page_size,
            data.total_items,
        ),
        Err(error) => catalog_error_response("price list is unavailable", error),
    }
}

async fn backend_create_price_list(
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    CatalogJson(body): CatalogJson<CreatePriceListBody>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };
    let organization_id = match subject.organization_id.as_deref() {
        Some(id) => id.to_owned(),
        None => return validation_response("organization_id is required"),
    };
    let command = CreatePriceListCommand {
        tenant_id: subject.tenant_id,
        organization_id,
        price_list_no: body.price_list_no,
        currency_code: body.currency_code,
        market_code: body.market_code,
    };
    match command.validate() {
        Ok(()) => {}
        Err(error) => return validation_response(error.message()),
    }
    match state.store.create_price_list(command).await {
        Ok(data) => success_created_resource(map_price_list(data)),
        Err(error) => catalog_error_response("failed to create price list", error),
    }
}

async fn backend_update_price_list(
    headers: axum::http::HeaderMap,
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Path(price_list_id): Path<String>,
    CatalogJson(body): CatalogJson<UpdatePriceListBody>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };

    // Read before the body is used for anything, so a request that did not say which version it is
    // editing is refused with `428` rather than being allowed to reach a write.
    let expected_version = match expected_version_from_if_match(&headers) {
        Ok(version) => version,
        Err(response) => return *response,
    };
    let status = match body.status.as_deref() {
        Some(raw) => Some(parse_or_422!(LifecycleStatus::from_storage_str(
            "status", raw
        ))),
        None => None,
    };
    let command = UpdatePriceListCommand {
        tenant_id: subject.tenant_id,
        expected_version,
        price_list_id,
        status,
        starts_at: body.starts_at,
        ends_at: body.ends_at,
    };
    match command.validate() {
        Ok(()) => {}
        Err(error) => return validation_response(error.message()),
    }
    match state.store.update_price_list(command).await {
        Ok(GuardedWrite::Applied(data)) => {
            // The row's version *after* the write, which is what the next `If-Match` must name.
            let version = data.version;
            success_resource_with_etag(map_price_list(data), version)
        }
        Ok(GuardedWrite::StaleVersion(stale)) => stale_version_response("price list", stale),
        Err(error) => catalog_error_response("failed to update price list", error),
    }
}

// --------------------------------------------------------------------------- media
//
// The media collection is addressed by a `(ownerType, ownerId)` pair instead of by a nested path
// (`/products/{id}/media`). The baseline lets one attachment hang off four different tables — `spu`,
// `sku`, `category`, `attribute_value` — so a nested path would need four routes whose bodies, DTOs,
// and read models were otherwise identical, and the fifth owner would mean a fifth copy. The pair is
// also what the `commerce_product_media` unique keys are built from
// (`uk_commerce_product_media_slot`), so the flat collection is closer to the storage identity than
// the nested spelling would be.
//
// `ownerType` carries the **storage** vocabulary (`spu`, not `product`). This is the same rule the
// other enumerated fields already follow: `product_type`, `fulfillment_type`, and `inventory_tracking`
// publish their storage spellings, and the anti-corruption translation is reserved for the nouns in
// paths and field names. The two vocabularies are close enough to be worth stating: the HTTP surface
// calls a `spu` a *product*, but `ownerType=spu` is what this filter accepts.
//
// Both enumerated filters are parsed here rather than passed through. The repository narrows with
// `($3::TEXT IS NULL OR owner_type = $3)`, so an unrecognised value would answer with an empty page —
// and for a media library an empty page is indistinguishable from "this product has no images",
// which is the most expensive wrong answer the endpoint can give. Refusing it as a `422` that names
// the permitted set costs one parse.

async fn backend_list_media(
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Query(params): Query<MediaQueryParams>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };
    let owner_type = match params.owner_type.as_deref() {
        Some(raw) => {
            Some(parse_or_422!(MediaOwnerType::from_storage_str("ownerType", raw)).as_storage_str())
        }
        None => None,
    };
    let media_role = match params.media_role.as_deref() {
        Some(raw) => {
            Some(parse_or_422!(MediaRole::from_storage_str("mediaRole", raw)).as_storage_str())
        }
        None => None,
    };
    let query = match MediaListQuery::new(
        &subject.tenant_id,
        subject.organization_id.as_deref(),
        owner_type,
        params.owner_id.as_deref(),
        media_role,
        params.status.as_deref(),
        params.page,
        params.page_size,
    ) {
        Ok(query) => query,
        Err(error) => return validation_response(error.message()),
    };
    match state.store.list_media_page(query).await {
        Ok(data) => success_offset_page(
            data.items.into_iter().map(map_media).collect(),
            data.page,
            data.page_size,
            data.total_items,
        ),
        Err(error) => catalog_error_response("media list is unavailable", error),
    }
}

/// Attaches one media resource to one owner.
///
/// `resource` is the `MediaResource` document itself, not a flattened set of fields. That is what
/// lets the domain validate it against the authored schema and derive `mediaResourceId` from its
/// `id`, so the reference and the snapshot are written from one input and cannot describe different
/// files.
async fn backend_create_media(
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    CatalogJson(body): CatalogJson<CreateMediaBody>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };
    let organization_id = match subject.organization_id.as_deref() {
        Some(id) => id.to_owned(),
        None => return validation_response("organization_id is required"),
    };
    let owner_type = parse_or_422!(MediaOwnerType::from_storage_str(
        "ownerType",
        &body.owner_type
    ));
    let media_role = parse_or_422!(MediaRole::from_storage_str("mediaRole", &body.media_role));
    let sort_order =
        parse_optional_int64_or_400!("sortOrder", body.sort_order.as_deref()).unwrap_or(0);
    let command = CreateMediaCommand {
        tenant_id: subject.tenant_id,
        organization_id,
        owner_type,
        owner_id: body.owner_id,
        media_role,
        resource_snapshot: body.resource,
        alt_text: body.alt_text,
        sort_order,
    };
    match command.validate() {
        Ok(()) => {}
        Err(error) => return validation_response(error.message()),
    }
    match state.store.create_media(command).await {
        Ok(data) => success_created_resource(map_media(data)),
        Err(error) => catalog_error_response("failed to create media", error),
    }
}

/// Updates one attachment in place.
///
/// The owner is absent from the body on purpose: an attachment's owner is fixed at creation, and the
/// role/owner-kind pair is re-checked against the row's **stored** owner kind by the repository while
/// it holds the row lock. A caller that could name the owner here could name one that disagrees with
/// the row, and the cross CHECK would then fire on a pair the caller never actually sent.
async fn backend_update_media(
    headers: axum::http::HeaderMap,
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Path(media_id): Path<String>,
    CatalogJson(body): CatalogJson<UpdateMediaBody>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };

    // Read before the body is used for anything, so a request that did not say which version it is
    // editing is refused with `428` rather than being allowed to reach a write.
    let expected_version = match expected_version_from_if_match(&headers) {
        Ok(version) => version,
        Err(response) => return *response,
    };
    let media_role = match body.media_role.as_deref() {
        Some(raw) => Some(parse_or_422!(MediaRole::from_storage_str("mediaRole", raw))),
        None => None,
    };
    let status = match body.status.as_deref() {
        Some(raw) => Some(parse_or_422!(LifecycleStatus::from_storage_str(
            "status", raw
        ))),
        None => None,
    };
    let command = UpdateMediaCommand {
        tenant_id: subject.tenant_id,
        expected_version,
        media_id,
        media_role,
        resource_snapshot: body.resource,
        alt_text: body.alt_text,
        sort_order: parse_optional_int64_or_400!("sortOrder", body.sort_order.as_deref()),
        status,
    };
    match command.validate() {
        Ok(()) => {}
        Err(error) => return validation_response(error.message()),
    }
    match state.store.update_media(command).await {
        Ok(GuardedWrite::Applied(data)) => {
            // The row's version *after* the write, which is what the next `If-Match` must name.
            let version = data.version;
            success_resource_with_etag(map_media(data), version)
        }
        Ok(GuardedWrite::StaleVersion(stale)) => stale_version_response("media", stale),
        Err(error) => catalog_error_response("failed to update media", error),
    }
}

/// Retires one attachment.
///
/// This is a soft delete: the row keeps its `deleted_at`, which is what frees the unique slot
/// (`uk_commerce_product_media_slot` is partial on `deleted_at IS NULL`) so a replacement can take the
/// same role and order without disturbing history.
async fn backend_delete_media(
    headers: axum::http::HeaderMap,
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Path(media_id): Path<String>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };

    // Read before the body is used for anything, so a request that did not say which version it is
    // editing is refused with `428` rather than being allowed to reach a write.
    let expected_version = match expected_version_from_if_match(&headers) {
        Ok(version) => version,
        Err(response) => return *response,
    };
    let command = DeleteMediaCommand {
        tenant_id: subject.tenant_id,
        expected_version,
        media_id,
    };
    match command.validate() {
        Ok(()) => {}
        Err(error) => return validation_response(error.message()),
    }
    match state.store.delete_media(command).await {
        Ok(GuardedWrite::Applied(())) => success_no_content(),
        Ok(GuardedWrite::StaleVersion(stale)) => stale_version_response("media", stale),
        Err(error) => catalog_error_response("failed to delete media", error),
    }
}

#[cfg(test)]
mod request_body_tests {
    //! The request boundary is worth pinning because two of its properties are invisible in a
    //! signature: the DTO closes itself with `deny_unknown_fields`, and a rejected body is answered
    //! through the platform problem envelope rather than by the extractor's own plain-text response.
    //! Driving a real route proves both, and proves them before any store is involved — the handler
    //! below answers without one, so nothing in these assertions can be attributed to SQL.

    use axum::body::Body;
    use axum::http::{header, Request, StatusCode};
    use axum::response::Response;
    use axum::routing::post;
    use axum::Router;
    use tower::ServiceExt;

    use super::{success_no_content, CatalogJson, CreateCategoryBody};

    async fn probe(CatalogJson(_body): CatalogJson<CreateCategoryBody>) -> Response {
        success_no_content()
    }

    async fn post_category(payload: &str) -> (StatusCode, String, String) {
        let request = Request::builder()
            .method("POST")
            .uri("/probe")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(payload.to_owned()))
            .expect("the probe request must build");
        let response = Router::new()
            .route("/probe", post(probe))
            .oneshot(request)
            .await
            .expect("the probe router must answer");
        let status = response.status();
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .expect("the response body must be readable");
        (
            status,
            content_type,
            String::from_utf8_lossy(&bytes).into_owned(),
        )
    }

    #[tokio::test]
    async fn a_documented_body_reaches_the_handler() {
        let (status, _, body) = post_category(r#"{"categoryNo":"CAT-1","name":"Apparel"}"#).await;
        assert_eq!(status, StatusCode::NO_CONTENT, "unexpected body: {body}");
    }

    #[tokio::test]
    async fn an_undeclared_field_is_refused_with_the_declared_problem_envelope() {
        let (status, content_type, body) =
            post_category(r#"{"categoryNo":"CAT-1","name":"Apparel","colour":"red"}"#).await;

        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "an undeclared field must not be silently ignored: {body}"
        );
        assert!(
            content_type.starts_with("application/problem+json"),
            "a rejected body must use the documented problem media type, got `{content_type}`"
        );
        let problem: serde_json::Value =
            serde_json::from_str(&body).expect("the problem body must be JSON");
        assert_eq!(problem["status"], 400);
        assert_eq!(problem["code"], 40001);
        assert!(
            problem["traceId"].as_str().is_some_and(|id| !id.is_empty()),
            "the envelope must carry a correlation id, got `{body}`"
        );
    }

    #[tokio::test]
    async fn a_json_number_where_the_contract_declares_a_string_is_refused() {
        // `sortOrder` is an int64-string (API_SPEC section 13.6), so a JSON number is a malformed
        // body rather than a value to coerce: coercion is how a browser's rounded id reaches SQL.
        let (status, _, body) =
            post_category(r#"{"categoryNo":"CAT-1","name":"Apparel","sortOrder":5}"#).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "unexpected body: {body}");
    }

    #[tokio::test]
    async fn a_missing_required_field_is_refused() {
        let (status, _, body) = post_category(r#"{"name":"Apparel"}"#).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "unexpected body: {body}");
    }

    // The extractor only sees the JSON type; whether the string is a legal decimal is decided by the
    // handler that consumes it, which is where these two helpers are called.
    #[test]
    fn int64_strings_are_parsed_at_the_request_boundary() {
        assert_eq!(super::parse_int64("sortOrder", "12"), Ok(12));
        assert_eq!(super::parse_int64("sortOrder", "-12"), Ok(-12));
        assert!(super::parse_int64("sortOrder", "many").is_err());
        assert!(super::parse_int64("sortOrder", "1.5").is_err());
        assert!(super::parse_int64("sortOrder", "").is_err());
        assert_eq!(super::parse_optional_int64("sortOrder", None), Ok(None));
        assert_eq!(
            super::parse_optional_int64("sortOrder", Some("7")),
            Ok(Some(7))
        );
        assert!(super::parse_optional_int64("sortOrder", Some("seven")).is_err());
    }
}
