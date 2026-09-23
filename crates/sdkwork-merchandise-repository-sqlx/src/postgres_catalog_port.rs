//! Binds `PostgresCommerceCatalogStore` to the port the service crate owns.
//!
//! # Why the binding lives here and not in the HTTP layer
//!
//! `CatalogRepositoryPort` is declared by `sdkwork-merchandise-service` and published as a provided
//! port in that crate's `specs/component.spec.json`. A trait implementation must be written either
//! in the crate that owns the trait or in the crate that owns the type, so the two legal homes are
//! the service crate and this one. It is this one: the service crate would have to depend on
//! `sdkwork-merchandise-repository-sqlx` to name `PostgresCommerceCatalogStore`, and a service that
//! depends on a concrete repository has inverted the dependency direction the port exists to
//! establish.
//!
//! The binding previously sat in the HTTP adapter crate, which is the worst of the three: a
//! transport crate owned the persistence contract and depended on the SQL repository to name the
//! type. That is the edge this file removes.
//!
//! # This is a forwarding layer, and that is deliberate
//!
//! Every method here delegates to the inherent method of the same name in
//! [`postgres_catalog`](crate::postgres_catalog). The inherent methods carry the statements, the
//! transaction boundaries, and the row mapping; this file only adapts their signature to the
//! port's boxed future, because a native `async fn` in a trait is not dyn-compatible and the router
//! consumes the port as `Arc<dyn CatalogRepositoryPort>`.
//!
//! Two shapes are not one-line forwards:
//!
//! * `list_*_page` composes the page window with the total count. It cannot be derived from the
//!   non-page method, because the count is a second statement over the same filter.
//! * `CatalogOffsetPage::new` applies the port's pagination defaults, so no adapter chooses its
//!   own page size.
//!
//! One thing this file does **not** do is flatten the thirteen guarded writes back to their bare
//! record. `Result<GuardedWrite<T>, _>` is the port's shape for them, and a forward that unwrapped
//! `Applied` and turned `StaleVersion` into something else would be re-deciding at the transport
//! boundary a question only the repository can answer.

use sdkwork_merchandise_service::{
    ArchiveSpuCommand, AttributeListQuery, AttributeRecord, CatalogOffsetPage,
    CatalogRepositoryPort, CategoryAttributeListQuery, CategoryAttributeRecord, CategoryListQuery,
    CategoryRecord, CategoryRetrieveQuery, CommerceCatalogFuture, CreateAttributeCommand,
    CreateCategoryAttributeCommand, CreateCategoryCommand, CreateMediaCommand,
    CreatePriceListCommand, CreateProductSkuCommand, CreateProductSpuCommand,
    DeleteCategoryAttributeCommand, DeleteCategoryCommand, DeleteMediaCommand,
    DeleteProductSkuCommand, DeleteProductSpuCommand, GuardedWrite, MediaListQuery, MediaRecord,
    PriceListListQuery, PriceListRecord, ProductSkuListQuery, ProductSkuRetrieveQuery,
    ProductSpuListQuery, ProductSpuRetrieveQuery, PublishSpuCommand, SkuRecord, SpuRecord,
    UpdateCategoryAttributeCommand, UpdateCategoryCommand, UpdateMediaCommand,
    UpdatePriceListCommand, UpdateProductSkuCommand, UpdateProductSpuCommand,
};

use crate::postgres_catalog::PostgresCommerceCatalogStore;

impl CatalogRepositoryPort for PostgresCommerceCatalogStore {
    fn list_categories<'a>(
        &'a self,
        query: CategoryListQuery,
    ) -> CommerceCatalogFuture<'a, Vec<CategoryRecord>> {
        Box::pin(async move { self.list_categories(&query).await })
    }

    fn list_categories_page<'a>(
        &'a self,
        query: CategoryListQuery,
    ) -> CommerceCatalogFuture<'a, CatalogOffsetPage<CategoryRecord>> {
        Box::pin(async move {
            let items = self.list_categories(&query).await?;
            let total_items = self.count_categories(&query).await?;
            Ok(CatalogOffsetPage::new(
                items,
                query.page,
                query.page_size,
                total_items,
            ))
        })
    }

    fn retrieve_category<'a>(
        &'a self,
        query: CategoryRetrieveQuery,
    ) -> CommerceCatalogFuture<'a, Option<CategoryRecord>> {
        Box::pin(async move { self.retrieve_category(&query).await })
    }

    fn create_category<'a>(
        &'a self,
        command: CreateCategoryCommand,
    ) -> CommerceCatalogFuture<'a, CategoryRecord> {
        Box::pin(async move { self.create_category(&command).await })
    }

    fn update_category<'a>(
        &'a self,
        command: UpdateCategoryCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<CategoryRecord>> {
        Box::pin(async move { self.update_category(&command).await })
    }

    fn delete_category<'a>(
        &'a self,
        command: DeleteCategoryCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<()>> {
        Box::pin(async move { self.delete_category(&command).await })
    }

    fn list_attributes<'a>(
        &'a self,
        query: AttributeListQuery,
    ) -> CommerceCatalogFuture<'a, Vec<AttributeRecord>> {
        Box::pin(async move { self.list_attributes(&query).await })
    }

    fn list_attributes_page<'a>(
        &'a self,
        query: AttributeListQuery,
    ) -> CommerceCatalogFuture<'a, CatalogOffsetPage<AttributeRecord>> {
        Box::pin(async move {
            let items = self.list_attributes(&query).await?;
            let total_items = self.count_attributes(&query).await?;
            Ok(CatalogOffsetPage::new(
                items,
                query.page,
                query.page_size,
                total_items,
            ))
        })
    }

    fn create_attribute<'a>(
        &'a self,
        command: CreateAttributeCommand,
    ) -> CommerceCatalogFuture<'a, AttributeRecord> {
        Box::pin(async move { self.create_attribute(&command).await })
    }

    fn list_price_lists<'a>(
        &'a self,
        query: PriceListListQuery,
    ) -> CommerceCatalogFuture<'a, Vec<PriceListRecord>> {
        Box::pin(async move { self.list_price_lists(&query).await })
    }

    fn list_price_lists_page<'a>(
        &'a self,
        query: PriceListListQuery,
    ) -> CommerceCatalogFuture<'a, CatalogOffsetPage<PriceListRecord>> {
        Box::pin(async move {
            let items = self.list_price_lists(&query).await?;
            let total_items = self.count_price_lists(&query).await?;
            Ok(CatalogOffsetPage::new(
                items,
                query.page,
                query.page_size,
                total_items,
            ))
        })
    }

    fn create_price_list<'a>(
        &'a self,
        command: CreatePriceListCommand,
    ) -> CommerceCatalogFuture<'a, PriceListRecord> {
        Box::pin(async move { self.create_price_list(&command).await })
    }

    fn update_price_list<'a>(
        &'a self,
        command: UpdatePriceListCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<PriceListRecord>> {
        Box::pin(async move { self.update_price_list(&command).await })
    }

    fn list_category_attributes<'a>(
        &'a self,
        query: CategoryAttributeListQuery,
    ) -> CommerceCatalogFuture<'a, Vec<CategoryAttributeRecord>> {
        Box::pin(async move { self.list_category_attributes(&query).await })
    }

    fn list_category_attributes_page<'a>(
        &'a self,
        query: CategoryAttributeListQuery,
    ) -> CommerceCatalogFuture<'a, CatalogOffsetPage<CategoryAttributeRecord>> {
        Box::pin(async move {
            let items = self.list_category_attributes(&query).await?;
            let total_items = self.count_category_attributes(&query).await?;
            Ok(CatalogOffsetPage::new(
                items,
                query.page,
                query.page_size,
                total_items,
            ))
        })
    }

    fn create_category_attribute<'a>(
        &'a self,
        command: CreateCategoryAttributeCommand,
    ) -> CommerceCatalogFuture<'a, CategoryAttributeRecord> {
        Box::pin(async move { self.create_category_attribute(&command).await })
    }

    fn update_category_attribute<'a>(
        &'a self,
        command: UpdateCategoryAttributeCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<CategoryAttributeRecord>> {
        Box::pin(async move { self.update_category_attribute(&command).await })
    }

    fn delete_category_attribute<'a>(
        &'a self,
        command: DeleteCategoryAttributeCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<()>> {
        Box::pin(async move { self.delete_category_attribute(&command).await })
    }

    fn list_spus<'a>(
        &'a self,
        query: ProductSpuListQuery,
    ) -> CommerceCatalogFuture<'a, Vec<SpuRecord>> {
        Box::pin(async move { self.list_spus(&query).await })
    }

    fn list_spus_page<'a>(
        &'a self,
        query: ProductSpuListQuery,
    ) -> CommerceCatalogFuture<'a, CatalogOffsetPage<SpuRecord>> {
        Box::pin(async move {
            let items = self.list_spus(&query).await?;
            let total_items = self.count_spus(&query).await?;
            Ok(CatalogOffsetPage::new(
                items,
                query.page,
                query.page_size,
                total_items,
            ))
        })
    }

    fn retrieve_spu<'a>(
        &'a self,
        query: ProductSpuRetrieveQuery,
    ) -> CommerceCatalogFuture<'a, Option<SpuRecord>> {
        Box::pin(async move { self.retrieve_spu(&query).await })
    }

    fn create_spu<'a>(
        &'a self,
        command: CreateProductSpuCommand,
    ) -> CommerceCatalogFuture<'a, SpuRecord> {
        Box::pin(async move { self.create_spu(&command).await })
    }

    fn update_spu<'a>(
        &'a self,
        command: UpdateProductSpuCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<SpuRecord>> {
        Box::pin(async move { self.update_spu(&command).await })
    }

    fn publish_spu<'a>(
        &'a self,
        command: PublishSpuCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<SpuRecord>> {
        Box::pin(async move { self.publish_spu(&command).await })
    }

    fn archive_spu<'a>(
        &'a self,
        command: ArchiveSpuCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<SpuRecord>> {
        Box::pin(async move { self.archive_spu(&command).await })
    }

    fn delete_spu<'a>(
        &'a self,
        command: DeleteProductSpuCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<()>> {
        Box::pin(async move { self.delete_spu(&command).await })
    }

    fn list_skus<'a>(
        &'a self,
        query: ProductSkuListQuery,
    ) -> CommerceCatalogFuture<'a, Vec<SkuRecord>> {
        Box::pin(async move { self.list_skus(&query).await })
    }

    fn list_skus_page<'a>(
        &'a self,
        query: ProductSkuListQuery,
    ) -> CommerceCatalogFuture<'a, CatalogOffsetPage<SkuRecord>> {
        Box::pin(async move {
            let items = self.list_skus(&query).await?;
            let total_items = self.count_skus(&query).await?;
            Ok(CatalogOffsetPage::new(
                items,
                query.page,
                query.page_size,
                total_items,
            ))
        })
    }

    fn retrieve_sku<'a>(
        &'a self,
        query: ProductSkuRetrieveQuery,
    ) -> CommerceCatalogFuture<'a, Option<SkuRecord>> {
        Box::pin(async move { self.retrieve_sku(&query).await })
    }

    fn create_sku<'a>(
        &'a self,
        command: CreateProductSkuCommand,
    ) -> CommerceCatalogFuture<'a, SkuRecord> {
        Box::pin(async move { self.create_sku(&command).await })
    }

    fn update_sku<'a>(
        &'a self,
        command: UpdateProductSkuCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<SkuRecord>> {
        Box::pin(async move { self.update_sku(&command).await })
    }

    fn delete_sku<'a>(
        &'a self,
        command: DeleteProductSkuCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<()>> {
        Box::pin(async move { self.delete_sku(&command).await })
    }

    fn list_media<'a>(
        &'a self,
        query: MediaListQuery,
    ) -> CommerceCatalogFuture<'a, Vec<MediaRecord>> {
        Box::pin(async move { self.list_media(&query).await })
    }

    fn list_media_page<'a>(
        &'a self,
        query: MediaListQuery,
    ) -> CommerceCatalogFuture<'a, CatalogOffsetPage<MediaRecord>> {
        Box::pin(async move {
            let items = self.list_media(&query).await?;
            let total_items = self.count_media(&query).await?;
            Ok(CatalogOffsetPage::new(
                items,
                query.page,
                query.page_size,
                total_items,
            ))
        })
    }

    fn create_media<'a>(
        &'a self,
        command: CreateMediaCommand,
    ) -> CommerceCatalogFuture<'a, MediaRecord> {
        Box::pin(async move { self.create_media(&command).await })
    }

    fn update_media<'a>(
        &'a self,
        command: UpdateMediaCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<MediaRecord>> {
        Box::pin(async move { self.update_media(&command).await })
    }

    fn delete_media<'a>(
        &'a self,
        command: DeleteMediaCommand,
    ) -> CommerceCatalogFuture<'a, GuardedWrite<()>> {
        Box::pin(async move { self.delete_media(&command).await })
    }
}
