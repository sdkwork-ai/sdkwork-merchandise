//! Localization: translation upserts and the locale read override.
//!
//! # The two-carrier rule
//!
//! The baseline splits localization so that base tables carry the default
//! language (`zh-CN`, the seed manifest's `defaultLocale` and
//! `fallbackLocale`) and `*_translation` tables carry the rest. A write with no
//! locale — or with the default locale — therefore behaves exactly as it did
//! before this module existed: the text lands on the base row. A write with any
//! other locale lands on the translation row and leaves the base row alone, so
//! two locales genuinely coexist instead of the last writer erasing the first.
//!
//! # Reading with a locale
//!
//! The store's per-entity locale attach functions load the translation rows for
//! a page in one statement and override the read model's text fields in place.
//! A field with no translation for the requested locale falls back to the base
//! row — the same rule the seed manifest's `fallbackLocale: zh-CN` states for
//! seed data, and the reason a caller that asks for `de-DE` against a zh-only
//! catalog still gets a readable page.

use std::collections::HashMap;
use std::sync::Arc;

use sqlx::{PgPool, Postgres, Row, Transaction};

use sdkwork_contract_service::CommerceServiceError;
use sdkwork_database_id::IdGenerator;

use crate::postgres_catalog::{next_snowflake, store_error};

pub(crate) const DEFAULT_LOCALE: &str = "zh-CN";

/// `true` when the locale names the default language, so the text belongs on
/// the base row rather than in a translation table.
pub(crate) fn is_default_locale(locale: Option<&str>) -> bool {
    match locale {
        None => true,
        Some(raw) => raw.trim().eq_ignore_ascii_case(DEFAULT_LOCALE),
    }
}

macro_rules! upsert_translation_statement {
    ($const_name:ident, $table:literal, $parent_column:literal) => {
        const $const_name: &str = concat!(
            "INSERT INTO ",
            $table,
            " (id, tenant_id, organization_id, ",
            $parent_column,
            ", locale, field_name, value)
     VALUES ($1, $2, $3, $4, $5, $6, $7)
     ON CONFLICT (",
            $parent_column,
            ", locale, field_name)
     DO UPDATE SET value = EXCLUDED.value, version = version + 1, updated_at = NOW()"
        );
    };
}

upsert_translation_statement!(
    UPSERT_CATEGORY_TRANSLATION_SQL,
    "commerce_product_category_translation",
    "category_id"
);
upsert_translation_statement!(
    UPSERT_ATTRIBUTE_TRANSLATION_SQL,
    "commerce_product_attribute_translation",
    "attribute_id"
);
upsert_translation_statement!(
    UPSERT_ATTRIBUTE_VALUE_TRANSLATION_SQL,
    "commerce_product_attribute_value_translation",
    "attribute_value_id"
);
upsert_translation_statement!(
    UPSERT_SPU_TRANSLATION_SQL,
    "commerce_product_spu_translation",
    "spu_id"
);
upsert_translation_statement!(
    UPSERT_SKU_TRANSLATION_SQL,
    "commerce_product_sku_translation",
    "sku_id"
);
upsert_translation_statement!(
    UPSERT_BRAND_TRANSLATION_SQL,
    "commerce_product_brand_translation",
    "brand_id"
);

/// One text field destined for a translation row.
pub(crate) struct TranslationField<'a> {
    pub field_name: &'a str,
    pub value: &'a str,
}

/// Writes one translation row (or updates it) inside a write transaction.
pub(crate) async fn upsert_translation(
    ids: &Arc<dyn IdGenerator>,
    transaction: &mut Transaction<'_, Postgres>,
    tenant_id: i64,
    organization_id: i64,
    statement: &'static str,
    parent_id: i64,
    locale: &str,
    field: &TranslationField<'_>,
) -> Result<(), CommerceServiceError> {
    sqlx::query(statement)
        .bind(next_snowflake(ids)?)
        .bind(tenant_id)
        .bind(organization_id)
        .bind(parent_id)
        .bind(locale)
        .bind(field.field_name)
        .bind(field.value)
        .execute(&mut **transaction)
        .await
        .map_err(|error| store_error("failed to write a translation row", error))?;
    Ok(())
}

/// One translation row read back for a page of parents.
pub(crate) struct LocaleOverride {
    pub parent_id: i64,
    pub field_name: String,
    pub value: String,
}

macro_rules! load_overrides_statement {
    ($const_name:ident, $table:literal, $parent_column:literal) => {
        pub(crate) const $const_name: &str = concat!(
            "SELECT ",
            $parent_column,
            " AS parent_id, field_name, value FROM ",
            $table,
            " WHERE tenant_id = $1 AND locale = $2 AND ",
            $parent_column,
            " = ANY($3)"
        );
    };
}

load_overrides_statement!(
    LOAD_CATEGORY_OVERRIDES_SQL,
    "commerce_product_category_translation",
    "category_id"
);
load_overrides_statement!(
    LOAD_ATTRIBUTE_OVERRIDES_SQL,
    "commerce_product_attribute_translation",
    "attribute_id"
);
load_overrides_statement!(
    LOAD_ATTRIBUTE_VALUE_OVERRIDES_SQL,
    "commerce_product_attribute_value_translation",
    "attribute_value_id"
);
load_overrides_statement!(
    LOAD_SPU_OVERRIDES_SQL,
    "commerce_product_spu_translation",
    "spu_id"
);
load_overrides_statement!(
    LOAD_SKU_OVERRIDES_SQL,
    "commerce_product_sku_translation",
    "sku_id"
);
load_overrides_statement!(
    LOAD_BRAND_OVERRIDES_SQL,
    "commerce_product_brand_translation",
    "brand_id"
);

/// Loads every translation row of one table for a page of parents in one statement.
pub(crate) async fn load_locale_overrides(
    pool: &PgPool,
    statement: &'static str,
    tenant_id: i64,
    locale: &str,
    parent_ids: &[i64],
) -> Result<HashMap<i64, Vec<LocaleOverride>>, CommerceServiceError> {
    let rows = sqlx::query(statement)
        .bind(tenant_id)
        .bind(locale)
        .bind(parent_ids)
        .fetch_all(pool)
        .await
        .map_err(|error| store_error("failed to load locale overrides", error))?;

    let mut grouped: HashMap<i64, Vec<LocaleOverride>> = HashMap::new();
    for row in rows.iter() {
        let parent_id: i64 = row
            .try_get("parent_id")
            .map_err(|error| store_error("failed to read a translation parent id", error))?;
        let field_name: String = row
            .try_get("field_name")
            .map_err(|error| store_error("failed to read a translation field name", error))?;
        let value: String = row
            .try_get("value")
            .map_err(|error| store_error("failed to read a translation value", error))?;
        grouped.entry(parent_id).or_default().push(LocaleOverride {
            parent_id,
            field_name,
            value,
        });
    }
    Ok(grouped)
}

/// Groups overrides for one parent id into `(field_name, value)` pairs.
pub(crate) fn overrides_for<'a>(
    grouped: &'a HashMap<i64, Vec<LocaleOverride>>,
    parent_id: i64,
) -> impl Iterator<Item = (&'a str, &'a str)> {
    grouped
        .get(&parent_id)
        .into_iter()
        .flatten()
        .map(|o| (o.field_name.as_str(), o.value.as_str()))
}
