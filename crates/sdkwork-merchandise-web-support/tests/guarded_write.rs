//! Drives a full write route into a port double — no database involved.
//!
//! The router holds `Arc<dyn CatalogRepositoryPort>`, so the composition root decides what
//! arrives; this test decides it arrives from memory. The double keeps every method except
//! `update_category` loud, so an accidental second store call fails the test instead of
//! succeeding silently. This lives in `tests/` rather than `src/`: `src/` must contain exactly
//! one `impl CatalogRepositoryPort` — the repository crate's binding — which the port-ownership
//! gate enforces, and a test double is not that binding.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use sdkwork_contract_service::CommerceServiceError;
use sdkwork_iam_context_service::{AuthLevel, DeploymentMode, Environment, IamAppContext};
use sdkwork_merchandise_service::CatalogRepositoryPort;
use sdkwork_merchandise_service::{
    ArchiveSpuCommand, AttributeListQuery, AttributeRecord, AttributeRetrieveQuery,
    AttributeValueListQuery, AttributeValueRecord, BrandListQuery, BrandRecord, BrandRetrieveQuery,
    CatalogOffsetPage, CategoryAttributeListQuery, CategoryAttributeRecord, CategoryListQuery,
    CategoryRecord, CategoryRetrieveQuery, CommerceCatalogFuture, CreateAttributeCommand,
    CreateAttributeValueCommand, CreateBrandCommand, CreateCategoryAttributeCommand,
    CreateCategoryCommand, CreateMediaCommand, CreatePriceListCommand, CreatePriceListItemCommand,
    CreateProductSkuCommand, CreateProductSpuCommand, DeleteAttributeValueCommand,
    DeleteBrandCommand, DeleteCategoryAttributeCommand, DeleteCategoryCommand, DeleteMediaCommand,
    DeletePriceListItemCommand, DeleteProductSkuCommand, DeleteProductSpuCommand, GuardedWrite,
    MediaListQuery, MediaRecord, PriceListItemListQuery, PriceListItemRecord, PriceListListQuery,
    PriceListRecord, PriceListRetrieveQuery, ProductAttributesRetrieveQuery,
    ProductAttributesWrite, ProductSkuListQuery, ProductSkuRetrieveQuery, ProductSpuListQuery,
    ProductSpuRetrieveQuery, PublishSpuCommand, ReplaceProductAttributesCommand, SkuRecord,
    SpuAttributeRecord, SpuRecord, UpdateAttributeCommand, UpdateAttributeValueCommand,
    UpdateBrandCommand, UpdateCategoryAttributeCommand, UpdateCategoryCommand, UpdateMediaCommand,
    UpdatePriceListCommand, UpdatePriceListItemCommand, UpdateProductSkuCommand,
    UpdateProductSpuCommand,
};
use sdkwork_merchandise_web_support::build_backend_catalog_router;
use tower::ServiceExt;

/// A port double whose `update_category` applies and every other method is unreachable.
struct UpdateCategoryOnly;

fn category_record() -> CategoryRecord {
    CategoryRecord {
        id: 1000,
        tenant_id: 1,
        organization_id: 1,
        category_no: "cat-1".to_owned(),
        parent_id: None,
        back_parent_id: None,
        path: "/1000/".to_owned(),
        depth: 0,
        is_leaf: true,
        name: "Apparel".to_owned(),
        sort_order: 0,
        status: "active".to_owned(),
        version: 7,
        created_at: "2026-01-01T00:00:00Z".to_owned(),
        updated_at: "2026-01-01T00:00:00Z".to_owned(),
    }
}

macro_rules! unreachable {
    () => {
        Box::pin(async { Err(CommerceServiceError::storage("not reachable")) })
    };
}

impl CatalogRepositoryPort for UpdateCategoryOnly {
    fn list_categories<'a>(
        &'a self,
        _: CategoryListQuery,
    ) -> CommerceCatalogFuture<'a, Vec<CategoryRecord>> {
        unreachable!()
    }
    fn list_categories_page<'a>(
        &'a self,
        _: CategoryListQuery,
    ) -> CommerceCatalogFuture<'a, CatalogOffsetPage<CategoryRecord>> {
        unreachable!()
    }
    fn retrieve_category<'a>(
        &'a self,
        _: CategoryRetrieveQuery,
    ) -> CommerceCatalogFuture<'a, Option<CategoryRecord>> {
        unreachable!()
    }
    fn create_category<'a>(
        &'a self,
        _: CreateCategoryCommand,
    ) -> CommerceCatalogFuture<'a, CategoryRecord> {
        unreachable!()
    }
    fn update_category<'a>(
        &'a self,
        command: UpdateCategoryCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<CategoryRecord>> {
        // The actor must arrive threaded from the IAM context; the double refuses a write without
        // one, which is what makes the audit-actor wiring observable here.
        let mut record = category_record();
        record.version = command.expected_version + 1;
        Box::pin(async move {
            assert_eq!(command.actor, "user-1", "the write must carry the actor");
            Ok(GuardedWrite::Applied(record))
        })
    }
    fn delete_category<'a>(
        &'a self,
        _: DeleteCategoryCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<()>> {
        unreachable!()
    }
    fn list_attributes<'a>(
        &'a self,
        _: AttributeListQuery,
    ) -> CommerceCatalogFuture<'a, Vec<AttributeRecord>> {
        unreachable!()
    }
    fn list_attributes_page<'a>(
        &'a self,
        _: AttributeListQuery,
    ) -> CommerceCatalogFuture<'a, CatalogOffsetPage<AttributeRecord>> {
        unreachable!()
    }
    fn retrieve_attribute<'a>(
        &'a self,
        _: AttributeRetrieveQuery,
    ) -> CommerceCatalogFuture<'a, Option<AttributeRecord>> {
        unreachable!()
    }
    fn create_attribute<'a>(
        &'a self,
        _: CreateAttributeCommand,
    ) -> CommerceCatalogFuture<'a, AttributeRecord> {
        unreachable!()
    }
    fn update_attribute<'a>(
        &'a self,
        _: UpdateAttributeCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<AttributeRecord>> {
        unreachable!()
    }
    fn list_attribute_values_page<'a>(
        &'a self,
        _: AttributeValueListQuery,
    ) -> CommerceCatalogFuture<'a, CatalogOffsetPage<AttributeValueRecord>> {
        unreachable!()
    }
    fn create_attribute_value<'a>(
        &'a self,
        _: CreateAttributeValueCommand,
    ) -> CommerceCatalogFuture<'a, AttributeValueRecord> {
        unreachable!()
    }
    fn update_attribute_value<'a>(
        &'a self,
        _: UpdateAttributeValueCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<AttributeValueRecord>> {
        unreachable!()
    }
    fn delete_attribute_value<'a>(
        &'a self,
        _: DeleteAttributeValueCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<()>> {
        unreachable!()
    }
    fn list_category_attributes<'a>(
        &'a self,
        _: CategoryAttributeListQuery,
    ) -> CommerceCatalogFuture<'a, Vec<CategoryAttributeRecord>> {
        unreachable!()
    }
    fn list_category_attributes_page<'a>(
        &'a self,
        _: CategoryAttributeListQuery,
    ) -> CommerceCatalogFuture<'a, CatalogOffsetPage<CategoryAttributeRecord>> {
        unreachable!()
    }
    fn create_category_attribute<'a>(
        &'a self,
        _: CreateCategoryAttributeCommand,
    ) -> CommerceCatalogFuture<'a, CategoryAttributeRecord> {
        unreachable!()
    }
    fn update_category_attribute<'a>(
        &'a self,
        _: UpdateCategoryAttributeCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<CategoryAttributeRecord>> {
        unreachable!()
    }
    fn delete_category_attribute<'a>(
        &'a self,
        _: DeleteCategoryAttributeCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<()>> {
        unreachable!()
    }
    fn list_spus<'a>(
        &'a self,
        _: ProductSpuListQuery,
    ) -> CommerceCatalogFuture<'a, Vec<SpuRecord>> {
        unreachable!()
    }
    fn list_spus_page<'a>(
        &'a self,
        _: ProductSpuListQuery,
    ) -> CommerceCatalogFuture<'a, CatalogOffsetPage<SpuRecord>> {
        unreachable!()
    }
    fn retrieve_spu<'a>(
        &'a self,
        _: ProductSpuRetrieveQuery,
    ) -> CommerceCatalogFuture<'a, Option<SpuRecord>> {
        unreachable!()
    }
    fn create_spu<'a>(
        &'a self,
        _: CreateProductSpuCommand,
    ) -> CommerceCatalogFuture<'a, SpuRecord> {
        unreachable!()
    }
    fn update_spu<'a>(
        &'a self,
        _: UpdateProductSpuCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<SpuRecord>> {
        unreachable!()
    }
    fn publish_spu<'a>(
        &'a self,
        _: PublishSpuCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<SpuRecord>> {
        unreachable!()
    }
    fn archive_spu<'a>(
        &'a self,
        _: ArchiveSpuCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<SpuRecord>> {
        unreachable!()
    }
    fn delete_spu<'a>(
        &'a self,
        _: DeleteProductSpuCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<()>> {
        unreachable!()
    }
    fn list_spu_attributes<'a>(
        &'a self,
        _: ProductAttributesRetrieveQuery,
    ) -> CommerceCatalogFuture<'a, Vec<SpuAttributeRecord>> {
        unreachable!()
    }
    fn replace_spu_attributes<'a>(
        &'a self,
        _: ReplaceProductAttributesCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<ProductAttributesWrite>> {
        unreachable!()
    }
    fn list_skus<'a>(
        &'a self,
        _: ProductSkuListQuery,
    ) -> CommerceCatalogFuture<'a, Vec<SkuRecord>> {
        unreachable!()
    }
    fn list_skus_page<'a>(
        &'a self,
        _: ProductSkuListQuery,
    ) -> CommerceCatalogFuture<'a, CatalogOffsetPage<SkuRecord>> {
        unreachable!()
    }
    fn retrieve_sku<'a>(
        &'a self,
        _: ProductSkuRetrieveQuery,
    ) -> CommerceCatalogFuture<'a, Option<SkuRecord>> {
        unreachable!()
    }
    fn create_sku<'a>(
        &'a self,
        _: CreateProductSkuCommand,
    ) -> CommerceCatalogFuture<'a, SkuRecord> {
        unreachable!()
    }
    fn update_sku<'a>(
        &'a self,
        _: UpdateProductSkuCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<SkuRecord>> {
        unreachable!()
    }
    fn delete_sku<'a>(
        &'a self,
        _: DeleteProductSkuCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<()>> {
        unreachable!()
    }
    fn list_price_lists<'a>(
        &'a self,
        _: PriceListListQuery,
    ) -> CommerceCatalogFuture<'a, Vec<PriceListRecord>> {
        unreachable!()
    }
    fn list_price_lists_page<'a>(
        &'a self,
        _: PriceListListQuery,
    ) -> CommerceCatalogFuture<'a, CatalogOffsetPage<PriceListRecord>> {
        unreachable!()
    }
    fn retrieve_price_list<'a>(
        &'a self,
        _: PriceListRetrieveQuery,
    ) -> CommerceCatalogFuture<'a, Option<PriceListRecord>> {
        unreachable!()
    }
    fn create_price_list<'a>(
        &'a self,
        _: CreatePriceListCommand,
    ) -> CommerceCatalogFuture<'a, PriceListRecord> {
        unreachable!()
    }
    fn update_price_list<'a>(
        &'a self,
        _: UpdatePriceListCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<PriceListRecord>> {
        unreachable!()
    }
    fn list_price_list_items<'a>(
        &'a self,
        _: PriceListItemListQuery,
    ) -> CommerceCatalogFuture<'a, Vec<PriceListItemRecord>> {
        unreachable!()
    }
    fn list_price_list_items_page<'a>(
        &'a self,
        _: PriceListItemListQuery,
    ) -> CommerceCatalogFuture<'a, CatalogOffsetPage<PriceListItemRecord>> {
        unreachable!()
    }
    fn create_price_list_item<'a>(
        &'a self,
        _: CreatePriceListItemCommand,
    ) -> CommerceCatalogFuture<'a, PriceListItemRecord> {
        unreachable!()
    }
    fn update_price_list_item<'a>(
        &'a self,
        _: UpdatePriceListItemCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<PriceListItemRecord>> {
        unreachable!()
    }
    fn delete_price_list_item<'a>(
        &'a self,
        _: DeletePriceListItemCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<()>> {
        unreachable!()
    }
    fn list_media<'a>(&'a self, _: MediaListQuery) -> CommerceCatalogFuture<'a, Vec<MediaRecord>> {
        unreachable!()
    }
    fn list_media_page<'a>(
        &'a self,
        _: MediaListQuery,
    ) -> CommerceCatalogFuture<'a, CatalogOffsetPage<MediaRecord>> {
        unreachable!()
    }
    fn create_media<'a>(&'a self, _: CreateMediaCommand) -> CommerceCatalogFuture<'a, MediaRecord> {
        unreachable!()
    }
    fn update_media<'a>(
        &'a self,
        _: UpdateMediaCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<MediaRecord>> {
        unreachable!()
    }
    fn delete_media<'a>(
        &'a self,
        _: DeleteMediaCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<()>> {
        unreachable!()
    }
    fn list_brands<'a>(&'a self, _: BrandListQuery) -> CommerceCatalogFuture<'a, Vec<BrandRecord>> {
        unreachable!()
    }
    fn list_brands_page<'a>(
        &'a self,
        _: BrandListQuery,
    ) -> CommerceCatalogFuture<'a, CatalogOffsetPage<BrandRecord>> {
        unreachable!()
    }
    fn retrieve_brand<'a>(
        &'a self,
        _: BrandRetrieveQuery,
    ) -> CommerceCatalogFuture<'a, Option<BrandRecord>> {
        unreachable!()
    }
    fn create_brand<'a>(&'a self, _: CreateBrandCommand) -> CommerceCatalogFuture<'a, BrandRecord> {
        unreachable!()
    }
    fn update_brand<'a>(
        &'a self,
        _: UpdateBrandCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<BrandRecord>> {
        unreachable!()
    }
    fn delete_brand<'a>(
        &'a self,
        _: DeleteBrandCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<()>> {
        unreachable!()
    }
}

/// A context `app_runtime_subject_from_iam` accepts: non-empty tenant and user id.
fn iam_probe_context() -> IamAppContext {
    IamAppContext::new(
        "100001",
        Some("1"),
        "user-1",
        "session-1",
        "sdkwork-merchandise-test",
        Environment::Dev,
        DeploymentMode::Saas,
        AuthLevel::Password,
        vec![],
        vec![],
    )
}

async fn patch_category(if_match: Option<&str>) -> (StatusCode, String, String) {
    let mut request = Request::builder()
        .method("PATCH")
        .uri("/backend/v3/api/catalog/categories/1000")
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(version) = if_match {
        request = request.header(header::IF_MATCH, version);
    }
    let request = request
        .body(Body::from(r#"{"name":"Apparel renamed"}"#))
        .expect("the request must build");
    let router = build_backend_catalog_router(std::sync::Arc::new(UpdateCategoryOnly))
        .layer(axum::Extension(iam_probe_context()));
    let response = router
        .oneshot(request)
        .await
        .expect("the router must answer");
    let status = response.status();
    let etag = response
        .headers()
        .get(header::ETAG)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("the response body must be readable");
    (status, etag, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test]
async fn a_guarded_write_round_trips_the_version() {
    let (status, etag, body) = patch_category(Some("\"6\"")).await;

    assert_eq!(status, StatusCode::OK, "unexpected body: {body}");
    assert_eq!(etag, "\"7\"", "the ETag must carry the post-write version");
    let json: serde_json::Value = serde_json::from_str(&body).expect("the body must be JSON");
    assert_eq!(json["code"], 0);
    assert_eq!(json["data"]["item"]["version"], "7");
}

#[tokio::test]
async fn a_write_without_the_precondition_is_refused_before_the_store() {
    let (status, _, body) = patch_category(None).await;

    assert_eq!(status, StatusCode::PRECONDITION_REQUIRED);
    let problem: serde_json::Value =
        serde_json::from_str(&body).expect("the problem body must be JSON");
    assert_eq!(problem["code"], 42801);
}
