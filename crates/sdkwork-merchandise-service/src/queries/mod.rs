use sdkwork_contract_service::CommerceServiceError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CategoryListQuery {
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub parent_id: Option<String>,
    pub status: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CategoryRetrieveQuery {
    pub tenant_id: String,
    pub category_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttributeListQuery {
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub status: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

/// Lists price lists.
///
/// `currency_code` and `market_code` exist because a price list is only meaningful inside one
/// currency and (optionally) one market, so both are filters rather than free text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceListListQuery {
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub currency_code: Option<String>,
    pub market_code: Option<String>,
    pub status: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

/// Lists category attribute bindings.
///
/// Every filter is optional and each one narrows the template: `category_id` picks a category's
/// template, `attribute_id` asks where one attribute is used, and `status` separates the live
/// template from bindings retired without losing their history.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CategoryAttributeListQuery {
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub category_id: Option<String>,
    pub attribute_id: Option<String>,
    pub status: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

/// Lists product SPUs.
///
/// `q` is free-text search. It is matched against the SPU's own identifier and its titles, because
/// those are the fields an operator types when looking for a product; it never reaches SQL as
/// text — the statement binds it as a parameter and builds the `ILIKE` pattern inside the driver.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductSpuListQuery {
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub q: Option<String>,
    pub category_id: Option<String>,
    pub product_type: Option<String>,
    pub status: Option<String>,
    pub sort: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductSpuRetrieveQuery {
    pub tenant_id: String,
    pub spu_id: String,
}

/// Lists SKUs.
///
/// `attribute_value_id` narrows the page to the SKUs that sit on one sales-axis value — the
/// buyer-facing question "which bodies does this trim come in". It is a filter rather than part of
/// the address because a value is shared by every SKU that carries it, and it is the read path
/// `idx_commerce_product_sku_attribute_tenant_value` exists for.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductSkuListQuery {
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub spu_id: Option<String>,
    pub attribute_value_id: Option<String>,
    pub status: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductSkuRetrieveQuery {
    pub tenant_id: String,
    pub sku_id: String,
}

/// Lists media attachments.
///
/// `owner_type` and `owner_id` are filters that only mean something together — an id alone is
/// ambiguous across the four owner tables — so the repository narrows on the pair and a caller that
/// supplies one without the other gets the whole tenant's attachments rather than a wrong set.
/// That is deliberate: the pair is an optimisation of the address, not an access rule.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaListQuery {
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub owner_type: Option<String>,
    pub owner_id: Option<String>,
    pub media_role: Option<String>,
    pub status: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

macro_rules! impl_query_new {
    ($query:ident, required: [$($req:ident),*], optional: [$($opt:ident),*]) => {
        impl $query {
            pub fn new(
                $($req: &str,)*
                $($opt: Option<&str>,)*
            ) -> Result<Self, CommerceServiceError> {
                Ok(Self {
                    $($req: required_text(stringify!($req), $req)?,)*
                    $($opt: optional_text($opt),)*
                })
            }
        }
    };
    ($query:ident, required: [$($req:ident),*], optional: [$($opt:ident),*], page: [$page:ident, $page_size:ident]) => {
        impl $query {
            #[allow(clippy::too_many_arguments)]
            pub fn new(
                $($req: &str,)*
                $($opt: Option<&str>,)*
                $page: Option<i64>,
                $page_size: Option<i64>,
            ) -> Result<Self, CommerceServiceError> {
                if let Some(page) = $page {
                    if page < 1 {
                        return Err(CommerceServiceError::validation(
                            "page must be greater than or equal to 1",
                        ));
                    }
                }
                if let Some(page_size) = $page_size {
                    if !(1..=200).contains(&page_size) {
                        return Err(CommerceServiceError::validation(
                            "page_size must be between 1 and 200",
                        ));
                    }
                }
                Ok(Self {
                    $($req: required_text(stringify!($req), $req)?,)*
                    $($opt: optional_text($opt),)*
                    $page,
                    $page_size,
                })
            }
        }
    };
}

impl_query_new!(CategoryListQuery, required: [tenant_id], optional: [organization_id, parent_id, status], page: [page, page_size]);
impl_query_new!(CategoryRetrieveQuery, required: [tenant_id, category_id], optional: []);
impl_query_new!(AttributeListQuery, required: [tenant_id], optional: [organization_id, status], page: [page, page_size]);
impl_query_new!(PriceListListQuery, required: [tenant_id], optional: [organization_id, currency_code, market_code, status], page: [page, page_size]);
impl_query_new!(CategoryAttributeListQuery, required: [tenant_id], optional: [organization_id, category_id, attribute_id, status], page: [page, page_size]);
impl_query_new!(ProductSpuListQuery, required: [tenant_id], optional: [organization_id, q, category_id, product_type, status, sort], page: [page, page_size]);
impl_query_new!(ProductSpuRetrieveQuery, required: [tenant_id, spu_id], optional: []);
impl_query_new!(ProductSkuListQuery, required: [tenant_id], optional: [organization_id, spu_id, attribute_value_id, status], page: [page, page_size]);
impl_query_new!(ProductSkuRetrieveQuery, required: [tenant_id, sku_id], optional: []);
impl_query_new!(MediaListQuery, required: [tenant_id], optional: [organization_id, owner_type, owner_id, media_role, status], page: [page, page_size]);

fn required_text(field_name: &str, value: &str) -> Result<String, CommerceServiceError> {
    crate::validation::require_non_empty(field_name, value)?;
    Ok(value.trim().to_string())
}

fn optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::ProductSkuListQuery;

    #[test]
    fn product_sku_page_size_is_bounded() {
        let error = ProductSkuListQuery::new(
            "tenant-1",
            None,
            Some("product-1"),
            None,
            Some("active"),
            Some(1),
            Some(201),
        )
        .unwrap_err();

        assert_eq!(error.message(), "page_size must be between 1 and 200");
    }
}
