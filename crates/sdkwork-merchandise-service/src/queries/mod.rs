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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceListListQuery {
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub status: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CategoryAttributeListQuery {
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub category_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductSpuListQuery {
    pub tenant_id: String,
    pub organization_id: Option<String>,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductSkuListQuery {
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub spu_id: Option<String>,
    pub status: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductSkuRetrieveQuery {
    pub tenant_id: String,
    pub sku_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SkuPriceRetrieveQuery {
    pub tenant_id: String,
    pub sku_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CartRetrieveQuery {
    pub tenant_id: String,
    pub owner_user_id: String,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AddressListQuery {
    pub tenant_id: String,
    pub owner_user_id: String,
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
impl_query_new!(PriceListListQuery, required: [tenant_id], optional: [organization_id, status]);
impl_query_new!(CategoryAttributeListQuery, required: [tenant_id], optional: [organization_id, category_id]);
impl_query_new!(ProductSpuListQuery, required: [tenant_id], optional: [organization_id, category_id, product_type, status, sort], page: [page, page_size]);
impl_query_new!(ProductSpuRetrieveQuery, required: [tenant_id, spu_id], optional: []);
impl_query_new!(ProductSkuListQuery, required: [tenant_id], optional: [organization_id, spu_id, status], page: [page, page_size]);
impl_query_new!(ProductSkuRetrieveQuery, required: [tenant_id, sku_id], optional: []);
impl_query_new!(SkuPriceRetrieveQuery, required: [tenant_id, sku_id], optional: []);
impl_query_new!(CartRetrieveQuery, required: [tenant_id, owner_user_id], optional: [], page: [page, page_size]);
impl_query_new!(AddressListQuery, required: [tenant_id, owner_user_id], optional: [], page: [page, page_size]);

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
            Some("active"),
            Some(1),
            Some(201),
        )
        .unwrap_err();

        assert_eq!(error.message(), "page_size must be between 1 and 200");
    }
}
