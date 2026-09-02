use sdkwork_contract_service::CommerceServiceError;
use sdkwork_merchandise_service::{
    AddCartItemCommand, AddressListQuery, AddressRecord, ArchiveSpuCommand, AttributeListQuery,
    AttributeRecord, CartItemRecord, CartRetrieveQuery, CategoryListQuery, CategoryRecord,
    CategoryRetrieveQuery, CreateAddressCommand, CreateAttributeCommand, CreateCategoryCommand,
    CreatePriceListCommand, CreateProductSkuCommand, CreateProductSpuCommand, DeleteAddressCommand,
    DeleteCategoryCommand, DeleteProductSkuCommand, DeleteProductSpuCommand, PriceListListQuery,
    PriceListRecord, ProductSkuListQuery, ProductSkuRetrieveQuery, ProductSpuListQuery,
    ProductSpuRetrieveQuery, PublishSpuCommand, RemoveCartItemCommand, SetDefaultAddressCommand,
    SkuRecord, SpuRecord, UpdateAddressCommand, UpdateCartItemCommand, UpdateCategoryCommand,
    UpdatePriceListCommand, UpdateProductSkuCommand, UpdateProductSpuCommand,
};
use sqlx::{PgPool, Row};

#[derive(Debug, Clone)]
pub struct PostgresCommerceCatalogStore {
    pool: PgPool,
}

impl PostgresCommerceCatalogStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn list_categories(
        &self,
        query: &CategoryListQuery,
    ) -> Result<Vec<CategoryRecord>, CommerceServiceError> {
        let limit = query.page_size.unwrap_or(20).min(200);
        let offset = (query.page.unwrap_or(1) - 1).max(0) * limit;
        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, organization_id, category_no, parent_id, path, level_no,
                   name, sort_order, status, created_at, updated_at
            FROM commerce_product_category
            WHERE tenant_id = CAST($1 AS TEXT)
              AND ($2 IS NULL OR organization_id = CAST($2 AS TEXT))
              AND ($3 IS NULL OR parent_id = CAST($3 AS TEXT))
              AND ($4 IS NULL OR status = $4)
            ORDER BY sort_order ASC, created_at ASC
            LIMIT $5 OFFSET $6
            "#,
        )
        .bind(&query.tenant_id)
        .bind(query.organization_id.as_deref())
        .bind(query.parent_id.as_deref())
        .bind(query.status.as_deref())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| store_error("failed to list categories", e))?;

        Ok(rows.iter().map(map_category_row).collect())
    }

    pub async fn count_categories(
        &self,
        query: &CategoryListQuery,
    ) -> Result<i64, CommerceServiceError> {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM commerce_product_category
            WHERE tenant_id = CAST($1 AS TEXT)
              AND ($2 IS NULL OR organization_id = CAST($2 AS TEXT))
              AND ($3 IS NULL OR parent_id = CAST($3 AS TEXT))
              AND ($4 IS NULL OR status = $4)
            "#,
        )
        .bind(&query.tenant_id)
        .bind(query.organization_id.as_deref())
        .bind(query.parent_id.as_deref())
        .bind(query.status.as_deref())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| store_error("failed to count categories", e))
    }

    pub async fn retrieve_category(
        &self,
        query: &CategoryRetrieveQuery,
    ) -> Result<Option<CategoryRecord>, CommerceServiceError> {
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, organization_id, category_no, parent_id, path, level_no,
                   name, sort_order, status, created_at, updated_at
            FROM commerce_product_category
            WHERE tenant_id = CAST($1 AS TEXT) AND id = CAST($2 AS TEXT)
            LIMIT 1
            "#,
        )
        .bind(&query.tenant_id)
        .bind(&query.category_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| store_error("failed to retrieve category", e))?;

        Ok(row.map(|r| map_category_row(&r)))
    }

    pub async fn create_category(
        &self,
        command: &CreateCategoryCommand,
    ) -> Result<CategoryRecord, CommerceServiceError> {
        let id = uuid_v7();
        let now = now_iso8601();
        let path =
            build_category_path(&self.pool, &command.tenant_id, command.parent_id.as_deref())
                .await?;
        let level_no = path.matches('/').count() as i64;

        let row = sqlx::query(
            r#"
            INSERT INTO commerce_product_category
                (id, tenant_id, organization_id, category_no, parent_id, path, level_no, name, sort_order, status, created_at, updated_at)
            VALUES (CAST($1 AS TEXT), CAST($2 AS TEXT), $3, CAST($4 AS TEXT), $5, $6, $7, CAST($8 AS TEXT), $9, 'active', $10, $10)
            RETURNING id, tenant_id, organization_id, category_no, parent_id, path, level_no, name, sort_order, status, created_at, updated_at
            "#,
        )
        .bind(&id)
        .bind(&command.tenant_id)
        .bind(&command.organization_id)
        .bind(&command.category_no)
        .bind(command.parent_id.as_deref())
        .bind(&path)
        .bind(level_no)
        .bind(&command.name)
        .bind(command.sort_order)
        .bind(&now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| store_error("failed to create category", e))?;

        Ok(map_category_row(&row))
    }

    pub async fn update_category(
        &self,
        command: &UpdateCategoryCommand,
    ) -> Result<CategoryRecord, CommerceServiceError> {
        let now = now_iso8601();
        let row = sqlx::query(
            r#"
            UPDATE commerce_product_category
            SET parent_id = COALESCE($3, parent_id),
                name = COALESCE($4, name),
                sort_order = COALESCE($5, sort_order),
                status = COALESCE($6, status),
                updated_at = $7
            WHERE tenant_id = CAST($1 AS TEXT) AND id = CAST($2 AS TEXT)
            RETURNING id, tenant_id, organization_id, category_no, parent_id, path, level_no, name, sort_order, status, created_at, updated_at
            "#,
        )
        .bind(&command.tenant_id)
        .bind(&command.category_id)
        .bind(command.parent_id.as_deref())
        .bind(command.name.as_deref())
        .bind(command.sort_order)
        .bind(command.status.as_deref())
        .bind(&now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| store_error("failed to update category", e))?;

        Ok(map_category_row(&row))
    }

    pub async fn delete_category(
        &self,
        command: &DeleteCategoryCommand,
    ) -> Result<(), CommerceServiceError> {
        sqlx::query(
            r#"
            UPDATE commerce_product_category
            SET status = 'deleted', updated_at = $3
            WHERE tenant_id = CAST($1 AS TEXT) AND id = CAST($2 AS TEXT)
            "#,
        )
        .bind(&command.tenant_id)
        .bind(&command.category_id)
        .bind(now_iso8601())
        .execute(&self.pool)
        .await
        .map_err(|e| store_error("failed to delete category", e))?;

        Ok(())
    }

    pub async fn list_attributes(
        &self,
        query: &AttributeListQuery,
    ) -> Result<Vec<AttributeRecord>, CommerceServiceError> {
        let limit = query.page_size.unwrap_or(20).min(200);
        let offset = (query.page.unwrap_or(1) - 1).max(0) * limit;
        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, organization_id, attribute_no, name, value_type, scope, status, sort_order, created_at, updated_at
            FROM commerce_product_attribute
            WHERE tenant_id = CAST($1 AS TEXT)
              AND ($2 IS NULL OR organization_id = CAST($2 AS TEXT))
              AND ($3 IS NULL OR status = $3)
            ORDER BY sort_order ASC, created_at ASC
            LIMIT $4 OFFSET $5
            "#,
        )
        .bind(&query.tenant_id)
        .bind(query.organization_id.as_deref())
        .bind(query.status.as_deref())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| store_error("failed to list attributes", e))?;

        Ok(rows.iter().map(map_attribute_row).collect())
    }

    pub async fn count_attributes(
        &self,
        query: &AttributeListQuery,
    ) -> Result<i64, CommerceServiceError> {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM commerce_product_attribute
            WHERE tenant_id = CAST($1 AS TEXT)
              AND ($2 IS NULL OR organization_id = CAST($2 AS TEXT))
              AND ($3 IS NULL OR status = $3)
            "#,
        )
        .bind(&query.tenant_id)
        .bind(query.organization_id.as_deref())
        .bind(query.status.as_deref())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| store_error("failed to count attributes", e))
    }

    pub async fn create_attribute(
        &self,
        command: &CreateAttributeCommand,
    ) -> Result<AttributeRecord, CommerceServiceError> {
        let id = uuid_v7();
        let now = now_iso8601();

        let row = sqlx::query(
            r#"
            INSERT INTO commerce_product_attribute
                (id, tenant_id, organization_id, attribute_no, name, value_type, scope, status, sort_order, created_at, updated_at)
            VALUES (CAST($1 AS TEXT), CAST($2 AS TEXT), $3, CAST($4 AS TEXT), $5, 'enum', 'product', 'active', 0, $6, $6)
            RETURNING id, tenant_id, organization_id, attribute_no, name, value_type, scope, status, sort_order, created_at, updated_at
            "#,
        )
        .bind(&id)
        .bind(&command.tenant_id)
        .bind(&command.organization_id)
        .bind(&command.attribute_no)
        .bind(&command.name)
        .bind(&now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| store_error("failed to create attribute", e))?;

        let attribute = map_attribute_row(&row);

        for (idx, value) in command.values.iter().enumerate() {
            let value_id = uuid_v7();
            sqlx::query(
                r#"
                INSERT INTO commerce_product_attribute_value
                    (id, tenant_id, organization_id, attribute_id, value_code, display_value, sort_order, status, created_at, updated_at)
                VALUES (CAST($1 AS TEXT), CAST($2 AS TEXT), $3, CAST($4 AS TEXT), CAST($5 AS TEXT), CAST($6 AS TEXT), $7, 'active', $8, $8)
                ON CONFLICT (tenant_id, attribute_id, value_code) DO NOTHING
                "#,
            )
            .bind(&value_id)
            .bind(&command.tenant_id)
            .bind(&command.organization_id)
            .bind(&attribute.id)
            .bind(value)
            .bind(value)
            .bind(idx as i64)
            .bind(&now)
            .execute(&self.pool)
            .await
            .map_err(|e| store_error("failed to create attribute value", e))?;
        }

        Ok(attribute)
    }

    pub async fn list_price_lists(
        &self,
        query: &PriceListListQuery,
    ) -> Result<Vec<PriceListRecord>, CommerceServiceError> {
        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, organization_id, price_list_no, currency_code, market_code, status, starts_at, ends_at, created_at, updated_at
            FROM commerce_price_list
            WHERE tenant_id = CAST($1 AS TEXT)
              AND ($2 IS NULL OR organization_id = CAST($2 AS TEXT))
              AND ($3 IS NULL OR status = $3)
            ORDER BY created_at DESC
            "#,
        )
        .bind(&query.tenant_id)
        .bind(query.organization_id.as_deref())
        .bind(query.status.as_deref())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| store_error("failed to list price lists", e))?;

        Ok(rows.iter().map(map_price_list_row).collect())
    }

    pub async fn create_price_list(
        &self,
        command: &CreatePriceListCommand,
    ) -> Result<PriceListRecord, CommerceServiceError> {
        let id = uuid_v7();
        let now = now_iso8601();

        let row = sqlx::query(
            r#"
            INSERT INTO commerce_price_list
                (id, tenant_id, organization_id, price_list_no, currency_code, market_code, status, created_at, updated_at)
            VALUES (CAST($1 AS TEXT), CAST($2 AS TEXT), $3, CAST($4 AS TEXT), CAST($5 AS TEXT), $6, 'active', $7, $7)
            RETURNING id, tenant_id, organization_id, price_list_no, currency_code, market_code, status, starts_at, ends_at, created_at, updated_at
            "#,
        )
        .bind(&id)
        .bind(&command.tenant_id)
        .bind(&command.organization_id)
        .bind(&command.price_list_no)
        .bind(&command.currency_code)
        .bind(command.market_code.as_deref())
        .bind(&now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| store_error("failed to create price list", e))?;

        Ok(map_price_list_row(&row))
    }

    pub async fn update_price_list(
        &self,
        command: &UpdatePriceListCommand,
    ) -> Result<PriceListRecord, CommerceServiceError> {
        let now = now_iso8601();
        let row = sqlx::query(
            r#"
            UPDATE commerce_price_list
            SET status = COALESCE($3, status),
                starts_at = COALESCE($4, starts_at),
                ends_at = COALESCE($5, ends_at),
                updated_at = $6
            WHERE tenant_id = CAST($1 AS TEXT) AND id = CAST($2 AS TEXT)
            RETURNING id, tenant_id, organization_id, price_list_no, currency_code, market_code, status, starts_at, ends_at, created_at, updated_at
            "#,
        )
        .bind(&command.tenant_id)
        .bind(&command.price_list_id)
        .bind(command.status.as_deref())
        .bind(command.starts_at.as_deref())
        .bind(command.ends_at.as_deref())
        .bind(&now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| store_error("failed to update price list", e))?;

        Ok(map_price_list_row(&row))
    }

    pub async fn list_spus(
        &self,
        query: &ProductSpuListQuery,
    ) -> Result<Vec<SpuRecord>, CommerceServiceError> {
        let limit = query.page_size.unwrap_or(20).min(200);
        let offset = (query.page.unwrap_or(1) - 1).max(0) * limit;

        let sql = spu_list_sql(query.sort.as_deref());

        let rows = sqlx::query(sql)
            .bind(&query.tenant_id)
            .bind(query.organization_id.as_deref())
            .bind(query.category_id.as_deref())
            .bind(query.product_type.as_deref())
            .bind(query.status.as_deref())
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| store_error("failed to list spus", e))?;

        Ok(rows.iter().map(map_spu_row).collect())
    }

    pub async fn count_spus(
        &self,
        query: &ProductSpuListQuery,
    ) -> Result<i64, CommerceServiceError> {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM commerce_product_spu
            WHERE tenant_id = CAST($1 AS TEXT)
              AND ($2 IS NULL OR organization_id = CAST($2 AS TEXT))
              AND ($3 IS NULL OR category_id = CAST($3 AS TEXT))
              AND ($4 IS NULL OR product_type = $4)
              AND ($5 IS NULL OR status = $5)
            "#,
        )
        .bind(&query.tenant_id)
        .bind(query.organization_id.as_deref())
        .bind(query.category_id.as_deref())
        .bind(query.product_type.as_deref())
        .bind(query.status.as_deref())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| store_error("failed to count spus", e))
    }

    pub async fn retrieve_spu(
        &self,
        query: &ProductSpuRetrieveQuery,
    ) -> Result<Option<SpuRecord>, CommerceServiceError> {
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, organization_id, spu_no, title, subtitle, description, product_type,
                   category_id, status, published_at, visible_surfaces, created_at, updated_at
            FROM commerce_product_spu
            WHERE tenant_id = CAST($1 AS TEXT) AND id = CAST($2 AS TEXT)
            LIMIT 1
            "#,
        )
        .bind(&query.tenant_id)
        .bind(&query.spu_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| store_error("failed to retrieve spu", e))?;

        Ok(row.map(|r| map_spu_row(&r)))
    }

    pub async fn create_spu(
        &self,
        command: &CreateProductSpuCommand,
    ) -> Result<SpuRecord, CommerceServiceError> {
        let id = uuid_v7();
        let now = now_iso8601();

        let row = sqlx::query(
            r#"
            INSERT INTO commerce_product_spu
                (id, tenant_id, organization_id, spu_no, title, subtitle, description, product_type, category_id, status, visible_surfaces, created_at, updated_at)
            VALUES (CAST($1 AS TEXT), CAST($2 AS TEXT), $3, CAST($4 AS TEXT), CAST($5 AS TEXT), $6, $7, CAST($8 AS TEXT), $9, 'draft', CAST($10 AS TEXT), $11, $11)
            RETURNING id, tenant_id, organization_id, spu_no, title, subtitle, description, product_type, category_id, status, published_at, visible_surfaces, created_at, updated_at
            "#,
        )
        .bind(&id)
        .bind(&command.tenant_id)
        .bind(&command.organization_id)
        .bind(&command.spu_no)
        .bind(&command.title)
        .bind(command.subtitle.as_deref())
        .bind(command.description.as_deref())
        .bind(&command.product_type)
        .bind(command.category_id.as_deref())
        .bind(&command.visible_surfaces)
        .bind(&now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| store_error("failed to create spu", e))?;

        Ok(map_spu_row(&row))
    }

    pub async fn update_spu(
        &self,
        command: &UpdateProductSpuCommand,
    ) -> Result<SpuRecord, CommerceServiceError> {
        let now = now_iso8601();
        let row = sqlx::query(
            r#"
            UPDATE commerce_product_spu
            SET title = COALESCE($3, title),
                subtitle = COALESCE($4, subtitle),
                description = COALESCE($5, description),
                category_id = COALESCE($6, category_id),
                visible_surfaces = COALESCE($7, visible_surfaces),
                updated_at = $8
            WHERE tenant_id = CAST($1 AS TEXT) AND id = CAST($2 AS TEXT)
            RETURNING id, tenant_id, organization_id, spu_no, title, subtitle, description, product_type, category_id, status, published_at, visible_surfaces, created_at, updated_at
            "#,
        )
        .bind(&command.tenant_id)
        .bind(&command.spu_id)
        .bind(command.title.as_deref())
        .bind(command.subtitle.as_deref())
        .bind(command.description.as_deref())
        .bind(command.category_id.as_deref())
        .bind(command.visible_surfaces.as_deref())
        .bind(&now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| store_error("failed to update spu", e))?;

        Ok(map_spu_row(&row))
    }

    pub async fn delete_spu(
        &self,
        command: &DeleteProductSpuCommand,
    ) -> Result<(), CommerceServiceError> {
        sqlx::query(
            r#"
            UPDATE commerce_product_spu
            SET status = 'deleted', updated_at = $3
            WHERE tenant_id = CAST($1 AS TEXT) AND id = CAST($2 AS TEXT)
            "#,
        )
        .bind(&command.tenant_id)
        .bind(&command.spu_id)
        .bind(now_iso8601())
        .execute(&self.pool)
        .await
        .map_err(|e| store_error("failed to delete spu", e))?;

        Ok(())
    }

    pub async fn publish_spu(
        &self,
        command: &PublishSpuCommand,
    ) -> Result<SpuRecord, CommerceServiceError> {
        let now = now_iso8601();
        let row = sqlx::query(
            r#"
            UPDATE commerce_product_spu
            SET status = 'active', published_at = $3, updated_at = $3
            WHERE tenant_id = CAST($1 AS TEXT) AND id = CAST($2 AS TEXT)
            RETURNING id, tenant_id, organization_id, spu_no, title, subtitle, description, product_type, category_id, status, published_at, visible_surfaces, created_at, updated_at
            "#,
        )
        .bind(&command.tenant_id)
        .bind(&command.spu_id)
        .bind(&now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| store_error("failed to publish spu", e))?;

        Ok(map_spu_row(&row))
    }

    pub async fn archive_spu(
        &self,
        command: &ArchiveSpuCommand,
    ) -> Result<SpuRecord, CommerceServiceError> {
        let now = now_iso8601();
        let row = sqlx::query(
            r#"
            UPDATE commerce_product_spu
            SET status = 'archived', updated_at = $3
            WHERE tenant_id = CAST($1 AS TEXT) AND id = CAST($2 AS TEXT)
            RETURNING id, tenant_id, organization_id, spu_no, title, subtitle, description, product_type, category_id, status, published_at, visible_surfaces, created_at, updated_at
            "#,
        )
        .bind(&command.tenant_id)
        .bind(&command.spu_id)
        .bind(&now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| store_error("failed to archive spu", e))?;

        Ok(map_spu_row(&row))
    }

    pub async fn list_skus(
        &self,
        query: &ProductSkuListQuery,
    ) -> Result<Vec<SkuRecord>, CommerceServiceError> {
        let limit = query.page_size.unwrap_or(20).min(200);
        let offset = (query.page.unwrap_or(1) - 1).max(0) * limit;

        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, organization_id, spu_id, sku_no, name, title, price_amount,
                   original_price_amount, currency_code, fulfillment_type, inventory_tracking,
                   status, published_at, spec_json, created_at, updated_at
            FROM commerce_product_sku
            WHERE tenant_id = CAST($1 AS TEXT)
              AND ($2 IS NULL OR organization_id = CAST($2 AS TEXT))
              AND ($3 IS NULL OR spu_id = CAST($3 AS TEXT))
              AND ($4 IS NULL OR status = $4)
            ORDER BY created_at DESC
            LIMIT $5 OFFSET $6
            "#,
        )
        .bind(&query.tenant_id)
        .bind(query.organization_id.as_deref())
        .bind(query.spu_id.as_deref())
        .bind(query.status.as_deref())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| store_error("failed to list skus", e))?;

        Ok(rows.iter().map(map_sku_row).collect())
    }

    pub async fn count_skus(
        &self,
        query: &ProductSkuListQuery,
    ) -> Result<i64, CommerceServiceError> {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM commerce_product_sku
            WHERE tenant_id = CAST($1 AS TEXT)
              AND ($2 IS NULL OR organization_id = CAST($2 AS TEXT))
              AND ($3 IS NULL OR spu_id = CAST($3 AS TEXT))
              AND ($4 IS NULL OR status = $4)
            "#,
        )
        .bind(&query.tenant_id)
        .bind(query.organization_id.as_deref())
        .bind(query.spu_id.as_deref())
        .bind(query.status.as_deref())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| store_error("failed to count skus", e))
    }

    pub async fn retrieve_sku(
        &self,
        query: &ProductSkuRetrieveQuery,
    ) -> Result<Option<SkuRecord>, CommerceServiceError> {
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, organization_id, spu_id, sku_no, name, title, price_amount,
                   original_price_amount, currency_code, fulfillment_type, inventory_tracking,
                   status, published_at, spec_json, created_at, updated_at
            FROM commerce_product_sku
            WHERE tenant_id = CAST($1 AS TEXT) AND id = CAST($2 AS TEXT)
            LIMIT 1
            "#,
        )
        .bind(&query.tenant_id)
        .bind(&query.sku_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| store_error("failed to retrieve sku", e))?;

        Ok(row.map(|r| map_sku_row(&r)))
    }

    pub async fn create_sku(
        &self,
        command: &CreateProductSkuCommand,
    ) -> Result<SkuRecord, CommerceServiceError> {
        let id = uuid_v7();
        let now = now_iso8601();

        let row = sqlx::query(
            r#"
            INSERT INTO commerce_product_sku
                (id, tenant_id, organization_id, spu_id, sku_no, name, title, price_amount, original_price_amount, currency_code, fulfillment_type, inventory_tracking, status, created_at, updated_at)
            VALUES (CAST($1 AS TEXT), CAST($2 AS TEXT), $3, CAST($4 AS TEXT), CAST($5 AS TEXT), CAST($6 AS TEXT), CAST($7 AS TEXT), CAST($8 AS TEXT), $9, CAST($10 AS TEXT), CAST($11 AS TEXT), CAST($12 AS TEXT), 'draft', $13, $13)
            RETURNING id, tenant_id, organization_id, spu_id, sku_no, name, title, price_amount, original_price_amount, currency_code, fulfillment_type, inventory_tracking, status, published_at, spec_json, created_at, updated_at
            "#,
        )
        .bind(&id)
        .bind(&command.tenant_id)
        .bind(&command.organization_id)
        .bind(&command.spu_id)
        .bind(&command.sku_no)
        .bind(&command.name)
        .bind(&command.title)
        .bind(command.price_amount.as_str())
        .bind(command.original_price_amount.as_ref().map(|m: &sdkwork_contract_service::CommerceMoney| m.as_str()))
        .bind(&command.currency_code)
        .bind(&command.fulfillment_type)
        .bind(&command.inventory_tracking)
        .bind(&now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| store_error("failed to create sku", e))?;

        Ok(map_sku_row(&row))
    }

    pub async fn update_sku(
        &self,
        command: &UpdateProductSkuCommand,
    ) -> Result<SkuRecord, CommerceServiceError> {
        let now = now_iso8601();
        let row = sqlx::query(
            r#"
            UPDATE commerce_product_sku
            SET name = COALESCE($3, name),
                title = COALESCE($4, title),
                price_amount = COALESCE($5, price_amount),
                original_price_amount = COALESCE($6, original_price_amount),
                currency_code = COALESCE($7, currency_code),
                fulfillment_type = COALESCE($8, fulfillment_type),
                inventory_tracking = COALESCE($9, inventory_tracking),
                status = COALESCE($10, status),
                updated_at = $11
            WHERE tenant_id = CAST($1 AS TEXT) AND id = CAST($2 AS TEXT)
            RETURNING id, tenant_id, organization_id, spu_id, sku_no, name, title, price_amount, original_price_amount, currency_code, fulfillment_type, inventory_tracking, status, published_at, spec_json, created_at, updated_at
            "#,
        )
        .bind(&command.tenant_id)
        .bind(&command.sku_id)
        .bind(command.name.as_deref())
        .bind(command.title.as_deref())
        .bind(command.price_amount.as_ref().map(|m: &sdkwork_contract_service::CommerceMoney| m.as_str()))
        .bind(command.original_price_amount.as_ref().map(|m: &sdkwork_contract_service::CommerceMoney| m.as_str()))
        .bind(command.currency_code.as_deref())
        .bind(command.fulfillment_type.as_deref())
        .bind(command.inventory_tracking.as_deref())
        .bind(command.status.as_deref())
        .bind(&now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| store_error("failed to update sku", e))?;

        Ok(map_sku_row(&row))
    }

    pub async fn delete_sku(
        &self,
        command: &DeleteProductSkuCommand,
    ) -> Result<(), CommerceServiceError> {
        sqlx::query(
            r#"
            UPDATE commerce_product_sku
            SET status = 'deleted', updated_at = $3
            WHERE tenant_id = CAST($1 AS TEXT) AND id = CAST($2 AS TEXT)
            "#,
        )
        .bind(&command.tenant_id)
        .bind(&command.sku_id)
        .bind(now_iso8601())
        .execute(&self.pool)
        .await
        .map_err(|e| store_error("failed to delete sku", e))?;

        Ok(())
    }

    pub async fn list_cart_items(
        &self,
        query: &CartRetrieveQuery,
    ) -> Result<Vec<CartItemRecord>, CommerceServiceError> {
        let limit = query.page_size.unwrap_or(20).min(200);
        let offset = (query.page.unwrap_or(1) - 1).max(0) * limit;
        let rows = sqlx::query(
            r#"
            SELECT ci.id, ci.tenant_id, c.owner_user_id, ci.sku_id, ci.quantity, ci.created_at, ci.updated_at
            FROM commerce_cart_item ci
            JOIN commerce_cart c ON c.tenant_id = ci.tenant_id AND c.id = ci.cart_id
            WHERE ci.tenant_id = CAST($1 AS TEXT) AND c.owner_user_id = CAST($2 AS TEXT) AND c.status = 'active'
            ORDER BY ci.created_at ASC
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(&query.tenant_id)
        .bind(&query.owner_user_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| store_error("failed to list cart items", e))?;

        Ok(rows.iter().map(map_cart_item_row).collect())
    }

    pub async fn count_cart_items(
        &self,
        query: &CartRetrieveQuery,
    ) -> Result<i64, CommerceServiceError> {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM commerce_cart_item ci
            JOIN commerce_cart c ON c.tenant_id = ci.tenant_id AND c.id = ci.cart_id
            WHERE ci.tenant_id = CAST($1 AS TEXT)
              AND c.owner_user_id = CAST($2 AS TEXT)
              AND c.status = 'active'
            "#,
        )
        .bind(&query.tenant_id)
        .bind(&query.owner_user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| store_error("failed to count cart items", e))
    }

    pub async fn add_cart_item(
        &self,
        command: &AddCartItemCommand,
    ) -> Result<CartItemRecord, CommerceServiceError> {
        let id = uuid_v7();
        let now = now_iso8601();

        let cart_id =
            resolve_or_create_cart(&self.pool, &command.tenant_id, &command.owner_user_id).await?;

        let row = sqlx::query(
            r#"
            INSERT INTO commerce_cart_item
                (id, tenant_id, cart_id, sku_id, quantity, selected_options_hash, selected_options_json, created_at, updated_at)
            VALUES (CAST($1 AS TEXT), CAST($2 AS TEXT), CAST($3 AS TEXT), CAST($4 AS TEXT), $5, '', '{}', $6, $6)
            ON CONFLICT (tenant_id, cart_id, sku_id, selected_options_hash) DO UPDATE SET
                quantity = commerce_cart_item.quantity + EXCLUDED.quantity,
                updated_at = EXCLUDED.updated_at
            RETURNING id, tenant_id, CAST($7 AS TEXT) AS owner_user_id, sku_id, quantity, created_at, updated_at
            "#,
        )
        .bind(&id)
        .bind(&command.tenant_id)
        .bind(&cart_id)
        .bind(&command.sku_id)
        .bind(command.quantity as i64)
        .bind(&now)
        .bind(&command.owner_user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| store_error("failed to add cart item", e))?;

        Ok(map_cart_item_row(&row))
    }

    pub async fn update_cart_item(
        &self,
        command: &UpdateCartItemCommand,
    ) -> Result<CartItemRecord, CommerceServiceError> {
        let now = now_iso8601();
        let row = sqlx::query(
            r#"
            UPDATE commerce_cart_item
            SET quantity = $4, updated_at = $5
            WHERE tenant_id = CAST($1 AS TEXT) AND id = CAST($2 AS TEXT)
            RETURNING id, tenant_id, CAST($3 AS TEXT) AS owner_user_id, sku_id, quantity, created_at, updated_at
            "#,
        )
        .bind(&command.tenant_id)
        .bind(&command.cart_item_id)
        .bind(&command.owner_user_id)
        .bind(command.quantity as i64)
        .bind(&now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| store_error("failed to update cart item", e))?;

        Ok(map_cart_item_row(&row))
    }

    pub async fn remove_cart_item(
        &self,
        command: &RemoveCartItemCommand,
    ) -> Result<(), CommerceServiceError> {
        sqlx::query(
            r#"
            DELETE FROM commerce_cart_item
            WHERE tenant_id = CAST($1 AS TEXT) AND id = CAST($2 AS TEXT)
            "#,
        )
        .bind(&command.tenant_id)
        .bind(&command.cart_item_id)
        .execute(&self.pool)
        .await
        .map_err(|e| store_error("failed to remove cart item", e))?;

        Ok(())
    }

    pub async fn list_addresses(
        &self,
        query: &AddressListQuery,
    ) -> Result<Vec<AddressRecord>, CommerceServiceError> {
        let limit = query.page_size.unwrap_or(20).min(200);
        let offset = (query.page.unwrap_or(1) - 1).max(0) * limit;
        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, owner_user_id, receiver_name, receiver_phone,
                   country_code, province, city, detail_address, is_default, status, created_at, updated_at
            FROM commerce_user_address
            WHERE tenant_id = CAST($1 AS TEXT) AND owner_user_id = CAST($2 AS TEXT) AND status = 'active'
            ORDER BY is_default DESC, created_at ASC
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(&query.tenant_id)
        .bind(&query.owner_user_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| store_error("failed to list addresses", e))?;

        Ok(rows.iter().map(map_address_row).collect())
    }

    pub async fn count_addresses(
        &self,
        query: &AddressListQuery,
    ) -> Result<i64, CommerceServiceError> {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM commerce_user_address
            WHERE tenant_id = CAST($1 AS TEXT)
              AND owner_user_id = CAST($2 AS TEXT)
              AND status = 'active'
            "#,
        )
        .bind(&query.tenant_id)
        .bind(&query.owner_user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| store_error("failed to count addresses", e))
    }

    pub async fn create_address(
        &self,
        command: &CreateAddressCommand,
    ) -> Result<AddressRecord, CommerceServiceError> {
        let id = uuid_v7();
        let now = now_iso8601();

        if command.is_default {
            sqlx::query(
                r#"
                UPDATE commerce_user_address
                SET is_default = FALSE, updated_at = $3
                WHERE tenant_id = CAST($1 AS TEXT) AND owner_user_id = CAST($2 AS TEXT) AND is_default = TRUE
                "#,
            )
            .bind(&command.tenant_id)
            .bind(&command.owner_user_id)
            .bind(&now)
            .execute(&self.pool)
            .await
            .map_err(|e| store_error("failed to reset default address", e))?;
        }

        let row = sqlx::query(
            r#"
            INSERT INTO commerce_user_address
                (id, tenant_id, owner_user_id, receiver_name, receiver_phone, country_code, province, city, detail_address, is_default, status, created_at, updated_at)
            VALUES (CAST($1 AS TEXT), CAST($2 AS TEXT), CAST($3 AS TEXT), CAST($4 AS TEXT), CAST($5 AS TEXT), CAST($6 AS TEXT), CAST($7 AS TEXT), CAST($8 AS TEXT), CAST($9 AS TEXT), $10, 'active', $11, $11)
            RETURNING id, tenant_id, owner_user_id, receiver_name, receiver_phone, country_code, province, city, detail_address, is_default, status, created_at, updated_at
            "#,
        )
        .bind(&id)
        .bind(&command.tenant_id)
        .bind(&command.owner_user_id)
        .bind(&command.receiver_name)
        .bind(&command.receiver_phone)
        .bind(&command.country_code)
        .bind(&command.province)
        .bind(&command.city)
        .bind(&command.detail_address)
        .bind(command.is_default)
        .bind(&now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| store_error("failed to create address", e))?;

        Ok(map_address_row(&row))
    }

    pub async fn update_address(
        &self,
        command: &UpdateAddressCommand,
    ) -> Result<AddressRecord, CommerceServiceError> {
        let now = now_iso8601();
        let row = sqlx::query(
            r#"
            UPDATE commerce_user_address
            SET receiver_name = COALESCE($4, receiver_name),
                receiver_phone = COALESCE($5, receiver_phone),
                province = COALESCE($6, province),
                city = COALESCE($7, city),
                detail_address = COALESCE($8, detail_address),
                updated_at = $9
            WHERE tenant_id = CAST($1 AS TEXT) AND owner_user_id = CAST($2 AS TEXT) AND id = CAST($3 AS TEXT)
            RETURNING id, tenant_id, owner_user_id, receiver_name, receiver_phone, country_code, province, city, detail_address, is_default, status, created_at, updated_at
            "#,
        )
        .bind(&command.tenant_id)
        .bind(&command.owner_user_id)
        .bind(&command.address_id)
        .bind(command.receiver_name.as_deref())
        .bind(command.receiver_phone.as_deref())
        .bind(command.province.as_deref())
        .bind(command.city.as_deref())
        .bind(command.detail_address.as_deref())
        .bind(&now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| store_error("failed to update address", e))?;

        Ok(map_address_row(&row))
    }

    pub async fn delete_address(
        &self,
        command: &DeleteAddressCommand,
    ) -> Result<(), CommerceServiceError> {
        sqlx::query(
            r#"
            UPDATE commerce_user_address
            SET status = 'deleted', updated_at = $4
            WHERE tenant_id = CAST($1 AS TEXT) AND owner_user_id = CAST($2 AS TEXT) AND id = CAST($3 AS TEXT)
            "#,
        )
        .bind(&command.tenant_id)
        .bind(&command.owner_user_id)
        .bind(&command.address_id)
        .bind(now_iso8601())
        .execute(&self.pool)
        .await
        .map_err(|e| store_error("failed to delete address", e))?;

        Ok(())
    }

    pub async fn set_default_address(
        &self,
        command: &SetDefaultAddressCommand,
    ) -> Result<AddressRecord, CommerceServiceError> {
        let now = now_iso8601();

        sqlx::query(
            r#"
            UPDATE commerce_user_address
            SET is_default = FALSE, updated_at = $3
            WHERE tenant_id = CAST($1 AS TEXT) AND owner_user_id = CAST($2 AS TEXT) AND is_default = TRUE
            "#,
        )
        .bind(&command.tenant_id)
        .bind(&command.owner_user_id)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| store_error("failed to reset default address", e))?;

        let row = sqlx::query(
            r#"
            UPDATE commerce_user_address
            SET is_default = TRUE, updated_at = $4
            WHERE tenant_id = CAST($1 AS TEXT) AND owner_user_id = CAST($2 AS TEXT) AND id = CAST($3 AS TEXT)
            RETURNING id, tenant_id, owner_user_id, receiver_name, receiver_phone, country_code, province, city, detail_address, is_default, status, created_at, updated_at
            "#,
        )
        .bind(&command.tenant_id)
        .bind(&command.owner_user_id)
        .bind(&command.address_id)
        .bind(&now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| store_error("failed to set default address", e))?;

        Ok(map_address_row(&row))
    }
}

fn map_category_row(row: &sqlx::postgres::PgRow) -> CategoryRecord {
    CategoryRecord {
        id: string_cell(row, "id"),
        tenant_id: string_cell(row, "tenant_id"),
        organization_id: optional_string_cell(row, "organization_id"),
        category_no: string_cell(row, "category_no"),
        parent_id: optional_string_cell(row, "parent_id"),
        path: string_cell(row, "path"),
        level_no: integer_cell(row, "level_no"),
        name: string_cell(row, "name"),
        sort_order: integer_cell(row, "sort_order"),
        status: string_cell(row, "status"),
        created_at: string_cell(row, "created_at"),
        updated_at: string_cell(row, "updated_at"),
    }
}

fn map_attribute_row(row: &sqlx::postgres::PgRow) -> AttributeRecord {
    AttributeRecord {
        id: string_cell(row, "id"),
        tenant_id: string_cell(row, "tenant_id"),
        organization_id: optional_string_cell(row, "organization_id"),
        attribute_no: string_cell(row, "attribute_no"),
        name: string_cell(row, "name"),
        value_type: string_cell(row, "value_type"),
        scope: string_cell(row, "scope"),
        status: string_cell(row, "status"),
        sort_order: integer_cell(row, "sort_order"),
        created_at: string_cell(row, "created_at"),
        updated_at: string_cell(row, "updated_at"),
    }
}

fn map_price_list_row(row: &sqlx::postgres::PgRow) -> PriceListRecord {
    PriceListRecord {
        id: string_cell(row, "id"),
        tenant_id: string_cell(row, "tenant_id"),
        organization_id: optional_string_cell(row, "organization_id"),
        price_list_no: string_cell(row, "price_list_no"),
        currency_code: string_cell(row, "currency_code"),
        market_code: optional_string_cell(row, "market_code"),
        status: string_cell(row, "status"),
        starts_at: optional_string_cell(row, "starts_at"),
        ends_at: optional_string_cell(row, "ends_at"),
        created_at: string_cell(row, "created_at"),
        updated_at: string_cell(row, "updated_at"),
    }
}

/// Selects a fixed `&'static str` SQL statement for listing SPUs by sort key.
///
/// Only whitelisted sort keys select a nontrivial statement (price ordering
/// adds a `LEFT JOIN commerce_product_sku` and an aggregate `MIN(price)` sort);
/// all other values fall back to `created_at DESC`. Each statement is a compiled
/// static literal, so arbitrary user input is never interpolated into SQL.
fn spu_list_sql(sort: Option<&str>) -> &'static str {
    const SPU_LIST_SQL_DEFAULT: &str = r#"
            SELECT id, tenant_id, organization_id, spu_no, title, subtitle, description, product_type,
                   category_id, status, published_at, visible_surfaces, created_at, updated_at
            FROM commerce_product_spu
            WHERE tenant_id = CAST($1 AS TEXT)
              AND ($2 IS NULL OR organization_id = CAST($2 AS TEXT))
              AND ($3 IS NULL OR category_id = CAST($3 AS TEXT))
              AND ($4 IS NULL OR product_type = $4)
              AND ($5 IS NULL OR status = $5)
            ORDER BY created_at DESC
            LIMIT $6 OFFSET $7
            "#;
    const SPU_LIST_SQL_PRICE_ASC: &str = r#"
            SELECT id, tenant_id, organization_id, spu_no, title, subtitle, description, product_type,
                   category_id, status, published_at, visible_surfaces, created_at, updated_at
            FROM commerce_product_spu
            LEFT JOIN commerce_product_sku sku
              ON sku.spu_id = commerce_product_spu.id
             AND sku.tenant_id = commerce_product_spu.tenant_id
             AND sku.status = 'active'
            WHERE tenant_id = CAST($1 AS TEXT)
              AND ($2 IS NULL OR organization_id = CAST($2 AS TEXT))
              AND ($3 IS NULL OR category_id = CAST($3 AS TEXT))
              AND ($4 IS NULL OR product_type = $4)
              AND ($5 IS NULL OR status = $5)
            GROUP BY commerce_product_spu.id
            ORDER BY MIN(CAST(sku.price_amount AS NUMERIC)) ASC
            LIMIT $6 OFFSET $7
            "#;
    const SPU_LIST_SQL_PRICE_DESC: &str = r#"
            SELECT id, tenant_id, organization_id, spu_no, title, subtitle, description, product_type,
                   category_id, status, published_at, visible_surfaces, created_at, updated_at
            FROM commerce_product_spu
            LEFT JOIN commerce_product_sku sku
              ON sku.spu_id = commerce_product_spu.id
             AND sku.tenant_id = commerce_product_spu.tenant_id
             AND sku.status = 'active'
            WHERE tenant_id = CAST($1 AS TEXT)
              AND ($2 IS NULL OR organization_id = CAST($2 AS TEXT))
              AND ($3 IS NULL OR category_id = CAST($3 AS TEXT))
              AND ($4 IS NULL OR product_type = $4)
              AND ($5 IS NULL OR status = $5)
            GROUP BY commerce_product_spu.id
            ORDER BY MIN(CAST(sku.price_amount AS NUMERIC)) DESC
            LIMIT $6 OFFSET $7
            "#;
    match sort {
        Some("price-asc") => SPU_LIST_SQL_PRICE_ASC,
        Some("price-desc") => SPU_LIST_SQL_PRICE_DESC,
        _ => SPU_LIST_SQL_DEFAULT,
    }
}

fn map_spu_row(row: &sqlx::postgres::PgRow) -> SpuRecord {
    SpuRecord {
        id: string_cell(row, "id"),
        tenant_id: string_cell(row, "tenant_id"),
        organization_id: optional_string_cell(row, "organization_id"),
        spu_no: string_cell(row, "spu_no"),
        title: string_cell(row, "title"),
        subtitle: optional_string_cell(row, "subtitle"),
        description: optional_string_cell(row, "description"),
        product_type: string_cell(row, "product_type"),
        category_id: optional_string_cell(row, "category_id"),
        status: string_cell(row, "status"),
        published_at: optional_string_cell(row, "published_at"),
        visible_surfaces: string_cell(row, "visible_surfaces"),
        created_at: string_cell(row, "created_at"),
        updated_at: string_cell(row, "updated_at"),
    }
}

pub(crate) fn map_sku_row(row: &sqlx::postgres::PgRow) -> SkuRecord {
    SkuRecord {
        id: string_cell(row, "id"),
        tenant_id: string_cell(row, "tenant_id"),
        organization_id: optional_string_cell(row, "organization_id"),
        spu_id: string_cell(row, "spu_id"),
        sku_no: string_cell(row, "sku_no"),
        name: string_cell(row, "name"),
        title: string_cell(row, "title"),
        price_amount: string_cell(row, "price_amount"),
        original_price_amount: optional_string_cell(row, "original_price_amount"),
        currency_code: string_cell(row, "currency_code"),
        fulfillment_type: string_cell(row, "fulfillment_type"),
        inventory_tracking: string_cell(row, "inventory_tracking"),
        status: string_cell(row, "status"),
        published_at: optional_string_cell(row, "published_at"),
        spec_json: optional_string_cell(row, "spec_json"),
        created_at: string_cell(row, "created_at"),
        updated_at: string_cell(row, "updated_at"),
    }
}

fn map_cart_item_row(row: &sqlx::postgres::PgRow) -> CartItemRecord {
    CartItemRecord {
        id: string_cell(row, "id"),
        tenant_id: string_cell(row, "tenant_id"),
        owner_user_id: string_cell(row, "owner_user_id"),
        sku_id: string_cell(row, "sku_id"),
        quantity: integer_cell(row, "quantity"),
        created_at: string_cell(row, "created_at"),
        updated_at: string_cell(row, "updated_at"),
    }
}

fn map_address_row(row: &sqlx::postgres::PgRow) -> AddressRecord {
    AddressRecord {
        id: string_cell(row, "id"),
        tenant_id: string_cell(row, "tenant_id"),
        owner_user_id: string_cell(row, "owner_user_id"),
        receiver_name: string_cell(row, "receiver_name"),
        receiver_phone: string_cell(row, "receiver_phone"),
        country_code: string_cell(row, "country_code"),
        province: string_cell(row, "province"),
        city: string_cell(row, "city"),
        detail_address: string_cell(row, "detail_address"),
        is_default: bool_cell(row, "is_default"),
        status: string_cell(row, "status"),
        created_at: string_cell(row, "created_at"),
        updated_at: string_cell(row, "updated_at"),
    }
}

fn string_cell(row: &sqlx::postgres::PgRow, column: &str) -> String {
    row.try_get::<String, _>(column).unwrap_or_default()
}

fn optional_string_cell(row: &sqlx::postgres::PgRow, column: &str) -> Option<String> {
    row.try_get::<Option<String>, _>(column).ok().flatten()
}

fn integer_cell(row: &sqlx::postgres::PgRow, column: &str) -> i64 {
    row.try_get::<i64, _>(column).unwrap_or(0)
}

fn bool_cell(row: &sqlx::postgres::PgRow, column: &str) -> bool {
    row.try_get::<bool, _>(column)
        .or_else(|_| row.try_get::<i32, _>(column).map(|v| v > 0))
        .unwrap_or(false)
}

fn store_error(context: &str, error: sqlx::Error) -> CommerceServiceError {
    CommerceServiceError::storage(format!("{context}: {error}"))
}

fn uuid_v7() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let millis = now.as_millis() as u64;
    let nanos = now.subsec_nanos();
    format!(
        "{:08x}-{:04x}-7{:03x}-{:04x}-{:012x}",
        (millis >> 16) as u32,
        (millis & 0xFFFF) as u16,
        (nanos >> 20) & 0xFFF,
        (nanos >> 4) & 0xFFFF,
        ((nanos as u64) << 32) | (millis & 0xFFFFFFFF)
    )
}

fn now_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let nanos = now.subsec_nanos();
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    let mut y = 1970;
    let mut remaining_days = days;
    loop {
        let days_in_year = if is_leap_year(y) { 366 } else { 365 };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        y += 1;
    }
    let mut m = 1u32;
    loop {
        let days_in_month = days_in_month(y, m);
        if remaining_days < days_in_month as u64 {
            break;
        }
        remaining_days -= days_in_month as u64;
        m += 1;
    }
    let d = remaining_days + 1;

    format!(
        "{y:04}-{m:02}-{d:02}T{hours:02}:{minutes:02}:{seconds:02}.{:03}Z",
        nanos / 1_000_000
    )
}

fn is_leap_year(year: u64) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

fn days_in_month(year: u64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

async fn build_category_path(
    pool: &PgPool,
    tenant_id: &str,
    parent_id: Option<&str>,
) -> Result<String, CommerceServiceError> {
    match parent_id {
        None => Ok("/".to_string()),
        Some(pid) => {
            let row = sqlx::query(
                r#"SELECT path FROM commerce_product_category WHERE tenant_id = CAST($1 AS TEXT) AND id = CAST($2 AS TEXT) LIMIT 1"#,
            )
            .bind(tenant_id)
            .bind(pid)
            .fetch_optional(pool)
            .await
            .map_err(|e| store_error("failed to resolve parent category path", e))?;

            match row {
                Some(r) => {
                    let parent_path: String = r.try_get("path").unwrap_or_default();
                    Ok(format!("{parent_path}{pid}/"))
                }
                None => Ok("/".to_string()),
            }
        }
    }
}

async fn resolve_or_create_cart(
    pool: &PgPool,
    tenant_id: &str,
    owner_user_id: &str,
) -> Result<String, CommerceServiceError> {
    let existing = sqlx::query(
        r#"SELECT id FROM commerce_cart WHERE tenant_id = CAST($1 AS TEXT) AND owner_user_id = CAST($2 AS TEXT) AND status = 'active' LIMIT 1"#,
    )
    .bind(tenant_id)
    .bind(owner_user_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| store_error("failed to resolve cart", e))?;

    if let Some(row) = existing {
        return Ok(row.try_get::<String, _>("id").unwrap_or_default());
    }

    let id = uuid_v7();
    let now = now_iso8601();
    sqlx::query(
        r#"
        INSERT INTO commerce_cart (id, tenant_id, owner_user_id, status, created_at, updated_at)
        VALUES (CAST($1 AS TEXT), CAST($2 AS TEXT), CAST($3 AS TEXT), 'active', $4, $4)
        "#,
    )
    .bind(&id)
    .bind(tenant_id)
    .bind(owner_user_id)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(|e| store_error("failed to create cart", e))?;

    Ok(id)
}
