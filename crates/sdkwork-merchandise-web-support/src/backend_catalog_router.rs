//! Merchandise routes published through the SDKWork Shop backend authority.

use std::sync::Arc;

use axum::extract::{Extension, Path, Query, State};
use axum::response::Response;
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use sdkwork_contract_service::CommerceMoney;
use sdkwork_iam_context_service::IamAppContext;
use sdkwork_merchandise_repository_sqlx::PostgresCommerceCatalogStore;
use sdkwork_merchandise_service::{
    ArchiveSpuCommand, AttributeListQuery, CategoryAttributeListQuery, CategoryListQuery,
    CreateAttributeCommand, CreateCategoryAttributeCommand, CreateCategoryCommand,
    CreatePriceListCommand, CreateProductSkuCommand, CreateProductSpuCommand,
    DeleteCategoryAttributeCommand, DeleteCategoryCommand, DeleteProductSkuCommand,
    DeleteProductSpuCommand, PriceListListQuery, ProductSkuListQuery, ProductSpuListQuery,
    ProductSpuRetrieveQuery, PublishSpuCommand, UpdateCategoryAttributeCommand,
    UpdateCategoryCommand, UpdatePriceListCommand, UpdateProductSkuCommand,
    UpdateProductSpuCommand,
};
use sqlx::PgPool;

use super::{
    catalog_system_response, map_attribute, map_category, map_category_attribute, map_price_list,
    map_sku, map_spu, not_found_response, success_created_resource, success_list,
    success_no_content, success_offset_page, success_resource, unauthorized_response,
    validation_response, AttributeQueryParams, CatalogState, CategoryAttributeQueryParams,
    CategoryQueryParams, CommerceCatalogStore, CreateAttributeBody, CreateCategoryAttributeBody,
    CreateCategoryBody, CreatePriceListBody, CreateSkuBody, CreateSpuBody, PriceListQueryParams,
    SkuListQueryParams, SpuListQueryParams, UpdateCategoryAttributeBody, UpdateCategoryBody,
    UpdatePriceListBody, UpdateSkuBody, UpdateSpuBody,
};
use crate::subject::app_runtime_subject_from_extension;

pub fn backend_catalog_router_with_postgres_pool(pool: PgPool) -> Router {
    build_backend_catalog_router(Arc::new(PostgresCommerceCatalogStore::new(pool)))
}

pub fn build_backend_catalog_router(store: Arc<dyn CommerceCatalogStore>) -> Router {
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
            "/backend/v3/api/catalog/spus",
            get(backend_list_spus).post(backend_create_spu),
        )
        .route(
            "/backend/v3/api/catalog/spus/{spuId}",
            patch(backend_update_spu),
        )
        .route(
            "/backend/v3/api/catalog/spus/{spuId}/publish",
            post(backend_publish_spu),
        )
        .route(
            "/backend/v3/api/catalog/spus/{spuId}/archive",
            post(backend_archive_spu),
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
        subject
            .organization_id
            .as_deref()
            .or(params.organization_id.as_deref()),
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
        Err(error) => catalog_system_response("category list is unavailable", error),
    }
}

async fn backend_create_category(
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Json(body): Json<CreateCategoryBody>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };
    let organization_id = match subject.organization_id.as_deref() {
        Some(id) => id.to_owned(),
        None => return validation_response("organization_id is required"),
    };
    let command = CreateCategoryCommand {
        tenant_id: subject.tenant_id,
        organization_id,
        category_no: body.category_no,
        parent_id: body.parent_id,
        name: body.name,
        sort_order: body.sort_order.unwrap_or(0),
    };
    match command.validate() {
        Ok(()) => {}
        Err(error) => return validation_response(error.message()),
    }
    match state.store.create_category(command).await {
        Ok(data) => success_created_resource(map_category(data)),
        Err(error) => catalog_system_response("failed to create category", error),
    }
}

async fn backend_update_category(
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Path(category_id): Path<String>,
    Json(body): Json<UpdateCategoryBody>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };
    let command = UpdateCategoryCommand {
        tenant_id: subject.tenant_id,
        category_id,
        parent_id: body.parent_id,
        name: body.name,
        sort_order: body.sort_order,
        status: body.status,
    };
    match command.validate() {
        Ok(()) => {}
        Err(error) => return validation_response(error.message()),
    }
    match state.store.update_category(command).await {
        Ok(data) => success_resource(map_category(data)),
        Err(error) => catalog_system_response("failed to update category", error),
    }
}

async fn backend_delete_category(
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Path(category_id): Path<String>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };
    let command = DeleteCategoryCommand {
        tenant_id: subject.tenant_id,
        category_id,
    };
    match command.validate() {
        Ok(()) => {}
        Err(error) => return validation_response(error.message()),
    }
    match state.store.delete_category(command).await {
        Ok(()) => success_no_content(),
        Err(error) => catalog_system_response("failed to delete category", error),
    }
}

async fn backend_list_products(
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Query(params): Query<SpuListQueryParams>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };
    let query = match ProductSpuListQuery::new(
        &subject.tenant_id,
        subject
            .organization_id
            .as_deref()
            .or(params.organization_id.as_deref()),
        params.category_id.as_deref(),
        params.product_type.as_deref(),
        params.status.as_deref(),
        None,
        params.page,
        params.page_size,
    ) {
        Ok(query) => query,
        Err(error) => return validation_response(error.message()),
    };
    match state.store.list_spus_page(query).await {
        Ok(data) => success_offset_page(
            data.items.into_iter().map(map_spu).collect(),
            data.page,
            data.page_size,
            data.total_items,
        ),
        Err(error) => catalog_system_response("product list is unavailable", error),
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
        Ok(Some(data)) => success_resource(map_spu(data)),
        Ok(None) => not_found_response("product was not found"),
        Err(error) => catalog_system_response("product read model is unavailable", error),
    }
}

async fn backend_create_product(
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Json(body): Json<CreateSpuBody>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };
    let organization_id = match subject.organization_id.as_deref() {
        Some(id) => id.to_owned(),
        None => return validation_response("organization_id is required"),
    };
    let command = CreateProductSpuCommand {
        tenant_id: subject.tenant_id,
        organization_id,
        spu_no: body.spu_no,
        title: body.title,
        subtitle: body.subtitle,
        description: body.description,
        product_type: body.product_type,
        category_id: body.category_id,
        visible_surfaces: body.visible_surfaces.unwrap_or_else(|| "all".to_owned()),
    };
    match command.validate() {
        Ok(()) => {}
        Err(error) => return validation_response(error.message()),
    }
    match state.store.create_spu(command).await {
        Ok(data) => success_created_resource(map_spu(data)),
        Err(error) => catalog_system_response("failed to create product", error),
    }
}

async fn backend_update_product(
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Path(product_id): Path<String>,
    Json(body): Json<UpdateSpuBody>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };
    let command = UpdateProductSpuCommand {
        tenant_id: subject.tenant_id,
        spu_id: product_id,
        title: body.title,
        subtitle: body.subtitle,
        description: body.description,
        category_id: body.category_id,
        visible_surfaces: body.visible_surfaces,
    };
    match command.validate() {
        Ok(()) => {}
        Err(error) => return validation_response(error.message()),
    }
    match state.store.update_spu(command).await {
        Ok(data) => success_resource(map_spu(data)),
        Err(error) => catalog_system_response("failed to update product", error),
    }
}

async fn backend_delete_product(
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Path(product_id): Path<String>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };
    let command = DeleteProductSpuCommand {
        tenant_id: subject.tenant_id,
        spu_id: product_id,
    };
    match command.validate() {
        Ok(()) => {}
        Err(error) => return validation_response(error.message()),
    }
    match state.store.delete_spu(command).await {
        Ok(()) => success_no_content(),
        Err(error) => catalog_system_response("failed to delete product", error),
    }
}

async fn backend_list_spus(
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Query(params): Query<SpuListQueryParams>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };
    let query = match ProductSpuListQuery::new(
        &subject.tenant_id,
        subject
            .organization_id
            .as_deref()
            .or(params.organization_id.as_deref()),
        params.category_id.as_deref(),
        params.product_type.as_deref(),
        params.status.as_deref(),
        None,
        params.page,
        params.page_size,
    ) {
        Ok(query) => query,
        Err(error) => return validation_response(error.message()),
    };
    match state.store.list_spus_page(query).await {
        Ok(data) => success_offset_page(
            data.items.into_iter().map(map_spu).collect(),
            data.page,
            data.page_size,
            data.total_items,
        ),
        Err(error) => catalog_system_response("spu list is unavailable", error),
    }
}

async fn backend_create_spu(
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Json(body): Json<CreateSpuBody>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };
    let organization_id = match subject.organization_id.as_deref() {
        Some(id) => id.to_owned(),
        None => return validation_response("organization_id is required"),
    };
    let command = CreateProductSpuCommand {
        tenant_id: subject.tenant_id,
        organization_id,
        spu_no: body.spu_no,
        title: body.title,
        subtitle: body.subtitle,
        description: body.description,
        product_type: body.product_type,
        category_id: body.category_id,
        visible_surfaces: body.visible_surfaces.unwrap_or_else(|| "all".to_owned()),
    };
    match command.validate() {
        Ok(()) => {}
        Err(error) => return validation_response(error.message()),
    }
    match state.store.create_spu(command).await {
        Ok(data) => success_created_resource(map_spu(data)),
        Err(error) => catalog_system_response("failed to create spu", error),
    }
}

async fn backend_update_spu(
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Path(spu_id): Path<String>,
    Json(body): Json<UpdateSpuBody>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };
    let command = UpdateProductSpuCommand {
        tenant_id: subject.tenant_id,
        spu_id,
        title: body.title,
        subtitle: body.subtitle,
        description: body.description,
        category_id: body.category_id,
        visible_surfaces: body.visible_surfaces,
    };
    match command.validate() {
        Ok(()) => {}
        Err(error) => return validation_response(error.message()),
    }
    match state.store.update_spu(command).await {
        Ok(data) => success_resource(map_spu(data)),
        Err(error) => catalog_system_response("failed to update spu", error),
    }
}

async fn backend_publish_spu(
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Path(spu_id): Path<String>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };
    let command = PublishSpuCommand {
        tenant_id: subject.tenant_id,
        spu_id,
    };
    match command.validate() {
        Ok(()) => {}
        Err(error) => return validation_response(error.message()),
    }
    match state.store.publish_spu(command).await {
        Ok(data) => success_resource(map_spu(data)),
        Err(error) => catalog_system_response("failed to publish spu", error),
    }
}

async fn backend_archive_spu(
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Path(spu_id): Path<String>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };
    let command = ArchiveSpuCommand {
        tenant_id: subject.tenant_id,
        spu_id,
    };
    match command.validate() {
        Ok(()) => {}
        Err(error) => return validation_response(error.message()),
    }
    match state.store.archive_spu(command).await {
        Ok(data) => success_resource(map_spu(data)),
        Err(error) => catalog_system_response("failed to archive spu", error),
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
        subject
            .organization_id
            .as_deref()
            .or(params.organization_id.as_deref()),
        params.spu_id.as_deref(),
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
        Err(error) => catalog_system_response("sku list is unavailable", error),
    }
}

async fn backend_create_sku(
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Json(body): Json<CreateSkuBody>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };
    let organization_id = match subject.organization_id.as_deref() {
        Some(id) => id.to_owned(),
        None => return validation_response("organization_id is required"),
    };
    let price_amount = match CommerceMoney::new(&body.price_amount) {
        Ok(money) => money,
        Err(error) => return validation_response(error),
    };
    let original_price_amount = match body.original_price_amount {
        Some(ref amount) => match CommerceMoney::new(amount) {
            Ok(money) => Some(money),
            Err(error) => return validation_response(error),
        },
        None => None,
    };
    let command = CreateProductSkuCommand {
        tenant_id: subject.tenant_id,
        organization_id,
        spu_id: body.spu_id,
        sku_no: body.sku_no,
        name: body.name,
        title: body.title,
        price_amount,
        original_price_amount,
        currency_code: body.currency_code,
        fulfillment_type: body.fulfillment_type,
        inventory_tracking: body.inventory_tracking,
    };
    match command.validate() {
        Ok(()) => {}
        Err(error) => return validation_response(error.message()),
    }
    match state.store.create_sku(command).await {
        Ok(data) => success_created_resource(map_sku(data)),
        Err(error) => catalog_system_response("failed to create sku", error),
    }
}

async fn backend_update_sku(
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Path(sku_id): Path<String>,
    Json(body): Json<UpdateSkuBody>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };
    let price_amount = match body.price_amount {
        Some(ref amount) => match CommerceMoney::new(amount) {
            Ok(money) => Some(money),
            Err(error) => return validation_response(error),
        },
        None => None,
    };
    let original_price_amount = match body.original_price_amount {
        Some(ref amount) => match CommerceMoney::new(amount) {
            Ok(money) => Some(money),
            Err(error) => return validation_response(error),
        },
        None => None,
    };
    let command = UpdateProductSkuCommand {
        tenant_id: subject.tenant_id,
        sku_id,
        name: body.name,
        title: body.title,
        price_amount,
        original_price_amount,
        currency_code: body.currency_code,
        fulfillment_type: body.fulfillment_type,
        inventory_tracking: body.inventory_tracking,
        status: body.status,
    };
    match command.validate() {
        Ok(()) => {}
        Err(error) => return validation_response(error.message()),
    }
    match state.store.update_sku(command).await {
        Ok(data) => success_resource(map_sku(data)),
        Err(error) => catalog_system_response("failed to update sku", error),
    }
}

async fn backend_delete_sku(
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Path(sku_id): Path<String>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };
    let command = DeleteProductSkuCommand {
        tenant_id: subject.tenant_id,
        sku_id,
    };
    match command.validate() {
        Ok(()) => {}
        Err(error) => return validation_response(error.message()),
    }
    match state.store.delete_sku(command).await {
        Ok(()) => success_no_content(),
        Err(error) => catalog_system_response("failed to delete sku", error),
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
        subject
            .organization_id
            .as_deref()
            .or(params.organization_id.as_deref()),
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
        Err(error) => catalog_system_response("attribute list is unavailable", error),
    }
}

async fn backend_create_attribute(
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Json(body): Json<CreateAttributeBody>,
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
        Err(error) => catalog_system_response("failed to create attribute", error),
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
        subject
            .organization_id
            .as_deref()
            .or(params.organization_id.as_deref()),
        params.category_id.as_deref(),
    ) {
        Ok(query) => query,
        Err(error) => return validation_response(error.message()),
    };
    match state.store.list_category_attributes(query).await {
        Ok(data) => success_list(data.into_iter().map(map_category_attribute).collect()),
        Err(error) => catalog_system_response("category attribute list is unavailable", error),
    }
}

async fn backend_create_category_attribute(
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Json(body): Json<CreateCategoryAttributeBody>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };
    let organization_id = match subject.organization_id.as_deref() {
        Some(id) => id.to_owned(),
        None => return validation_response("organization_id is required"),
    };
    let command = CreateCategoryAttributeCommand {
        tenant_id: subject.tenant_id,
        organization_id,
        category_id: body.category_id,
        attribute_id: body.attribute_id,
        required: body.required.unwrap_or(false),
        searchable: body.searchable.unwrap_or(false),
        filterable: body.filterable.unwrap_or(false),
        sort_order: body.sort_order.unwrap_or(0),
    };
    match command.validate() {
        Ok(()) => {}
        Err(error) => return validation_response(error.message()),
    }
    match state.store.create_category_attribute(command).await {
        Ok(data) => success_created_resource(map_category_attribute(data)),
        Err(error) => catalog_system_response("failed to create category attribute", error),
    }
}

async fn backend_update_category_attribute(
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Path(binding_id): Path<String>,
    Json(body): Json<UpdateCategoryAttributeBody>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };
    let command = UpdateCategoryAttributeCommand {
        tenant_id: subject.tenant_id,
        binding_id,
        required: body.required,
        searchable: body.searchable,
        filterable: body.filterable,
        sort_order: body.sort_order,
    };
    match command.validate() {
        Ok(()) => {}
        Err(error) => return validation_response(error.message()),
    }
    match state.store.update_category_attribute(command).await {
        Ok(data) => success_resource(map_category_attribute(data)),
        Err(error) => catalog_system_response("failed to update category attribute", error),
    }
}

async fn backend_delete_category_attribute(
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Path(binding_id): Path<String>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };
    let command = DeleteCategoryAttributeCommand {
        tenant_id: subject.tenant_id,
        binding_id,
    };
    match command.validate() {
        Ok(()) => {}
        Err(error) => return validation_response(error.message()),
    }
    match state.store.delete_category_attribute(command).await {
        Ok(()) => success_no_content(),
        Err(error) => catalog_system_response("failed to delete category attribute", error),
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
        subject
            .organization_id
            .as_deref()
            .or(params.organization_id.as_deref()),
        params.status.as_deref(),
    ) {
        Ok(query) => query,
        Err(error) => return validation_response(error.message()),
    };
    match state.store.list_price_lists(query).await {
        Ok(data) => success_list(data.into_iter().map(map_price_list).collect()),
        Err(error) => catalog_system_response("price list is unavailable", error),
    }
}

async fn backend_create_price_list(
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Json(body): Json<CreatePriceListBody>,
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
        Err(error) => catalog_system_response("failed to create price list", error),
    }
}

async fn backend_update_price_list(
    State(state): State<CatalogState>,
    runtime_context: Option<Extension<IamAppContext>>,
    Path(price_list_id): Path<String>,
    Json(body): Json<UpdatePriceListBody>,
) -> Response {
    let subject = match app_runtime_subject_from_extension(runtime_context) {
        Ok(subject) => subject,
        Err(message) => return unauthorized_response(message),
    };
    let command = UpdatePriceListCommand {
        tenant_id: subject.tenant_id,
        price_list_id,
        status: body.status,
        starts_at: body.starts_at,
        ends_at: body.ends_at,
    };
    match command.validate() {
        Ok(()) => {}
        Err(error) => return validation_response(error.message()),
    }
    match state.store.update_price_list(command).await {
        Ok(data) => success_resource(map_price_list(data)),
        Err(error) => catalog_system_response("failed to update price list", error),
    }
}
