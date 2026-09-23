use sdkwork_contract_service::CommerceServiceError;

pub fn require_non_empty(field: &str, value: &str) -> Result<(), CommerceServiceError> {
    if value.trim().is_empty() {
        return Err(CommerceServiceError::validation(format!(
            "{field} is required"
        )));
    }

    Ok(())
}

/// `ck_commerce_product_attribute_value_display_length`.
///
/// The baseline constrains `display_value` with `char_length(...) BETWEEN 1 AND 200`, so this bound
/// is the schema's, not a policy choice: raising it here without raising the CHECK only moves the
/// failure back into the database.
pub const ATTRIBUTE_VALUE_MAX_CHARS: usize = 200;

/// `ck_commerce_product_category_name_length`.
pub const CATEGORY_NAME_MAX_CHARS: usize = 200;

/// `ck_commerce_product_attribute_name_length`.
pub const ATTRIBUTE_NAME_MAX_CHARS: usize = 100;

/// `ck_commerce_product_spu_name_length`.
///
/// `commerce_product_spu.name` is the default-locale display name and the repository derives it
/// from `title`, so the bound applies to the request field that feeds it.
pub const SPU_TITLE_MAX_CHARS: usize = 300;

/// `ck_commerce_price_list_name_length`.
///
/// `commerce_price_list.name` is derived from `price_list_no`, so the bound applies to that field
/// rather than to a `name` the caller cannot send.
pub const PRICE_LIST_NO_MAX_CHARS: usize = 200;

/// `ck_commerce_product_sku_variant_signature`.
///
/// The repository builds the signature from the SKU's sales axes and this is the length the baseline
/// admits. A combination that does not fit is refused with a `422` naming the limit rather than
/// reaching PostgreSQL as a `23514` after the axis rows were already written.
pub const SKU_VARIANT_SIGNATURE_MAX_CHARS: usize = 500;

/// `ck_commerce_product_sku_sale_price` / `_list_price`.
///
/// Monetary amounts travel as integer **minor units** (`API_SPEC` section 13.2.1), so the sign is
/// the only range question a price can raise: the baseline admits `>= 0` and the wire pattern
/// `^[0-9]+$` admits no minus. Checking here keeps a negative amount a `422` naming the field
/// instead of a `23514` raised mid-transaction inside PostgreSQL, and keeps a non-HTTP caller from
/// reaching SQL with a value the schema would reject.
pub fn require_non_negative_minor(field: &str, value: i64) -> Result<(), CommerceServiceError> {
    if value < 0 {
        return Err(CommerceServiceError::validation(format!(
            "{field} must not be negative: the wire carries an amount in the currency's minor unit"
        )));
    }

    Ok(())
}

/// Rejects a text value longer than the baseline's `char_length` bound.
///
/// `char_length` counts characters, not bytes, so the comparison is on `chars()` — a 200-character
/// CJK value is 600 bytes and the schema still accepts it.
pub fn require_within_chars(
    field: &str,
    value: &str,
    max: usize,
) -> Result<(), CommerceServiceError> {
    if value.chars().count() > max {
        return Err(CommerceServiceError::validation(format!(
            "{field} must be at most {max} characters"
        )));
    }

    Ok(())
}

/// Rejects a negative value for a column the baseline pins with `>= 0`.
///
/// `commerce_product_media.sort_order` carries `ck_commerce_product_media_sort_order`. Checking it
/// here keeps the failure a `422` naming the field instead of a `23514` raised mid-transaction.
pub fn require_non_negative(field: &str, value: i64) -> Result<(), CommerceServiceError> {
    if value < 0 {
        return Err(CommerceServiceError::validation(format!(
            "{field} must not be negative"
        )));
    }

    Ok(())
}

/// Rejects a collection that names the same id twice.
///
/// `commerce_product_sku_attribute` carries
/// `uk_commerce_product_sku_attribute_axis (tenant_id, sku_id, attribute_id) WHERE deleted_at IS
/// NULL`: one value per sales axis. Two entries for the same axis would violate that index — but
/// only after the attribute each value belongs to has been resolved, so the collision surfaces as a
/// `23505` on a column the caller never sent. Checking the submitted set here turns it into a named
/// `422`, and leaves the index as the last line of defence rather than the first.
pub fn require_distinct(field: &str, values: &[String]) -> Result<(), CommerceServiceError> {
    let mut seen = std::collections::BTreeSet::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(CommerceServiceError::validation(format!(
                "{field} must not contain an empty entry"
            )));
        }
        if !seen.insert(trimmed) {
            return Err(CommerceServiceError::validation(format!(
                "{field} contains `{trimmed}` more than once: one value per sales axis"
            )));
        }
    }

    Ok(())
}

/// The `MediaResource` keys that must be present for a reference to be persistable.
///
/// They are a named constant rather than three inline string comparisons because
/// `tests/static/catalog-media-contract-closure.test.mjs` compares this list, element by element,
/// with `MediaResource.required` in the authored OpenAPI document. `kind` and `source` are required
/// by `MEDIA_RESOURCE_SPEC` section 3 for every `MediaResource`; `id` is required by
/// `commerce_product_media.media_resource_id`, which is `BIGINT NOT NULL` and is derived from it.
pub const MEDIA_RESOURCE_REQUIRED_KEYS: [&str; 3] = ["id", "kind", "source"];

/// The remaining `MediaResource` keys this capability accepts, and the only ones it accepts.
///
/// The list is closed on purpose: the documented schema declares `additionalProperties: false`, so
/// accepting an undeclared key here would make that claim false while the SDK generator still
/// refused to emit it. `poster`, `thumbnails`, and `variants` are carried as opaque objects and
/// arrays rather than as recursive `MediaResource` values, which `MEDIA_RESOURCE_SPEC` section 4
/// explicitly permits and which keeps the snapshot a flat read-model projection.
pub const MEDIA_RESOURCE_OPTIONAL_KEYS: [&str; 19] = [
    "url",
    "publicUrl",
    "uri",
    "objectBlobId",
    "fileName",
    "mimeType",
    "sizeBytes",
    "checksum",
    "width",
    "height",
    "durationSeconds",
    "altText",
    "title",
    "poster",
    "thumbnails",
    "variants",
    "access",
    "ai",
    "metadata",
];

/// `MediaSource::Drive`. A Drive-backed resource is identified by `drive://spaces/{spaceId}/nodes/
/// {nodeId}` and its `id` is the Drive node id, so a snapshot that claims `drive` without a `uri`
/// cannot be resolved to a file at all.
const MEDIA_SOURCE_DRIVE: &str = "drive";
const MEDIA_URI_DRIVE_PREFIX: &str = "drive://";

/// Validates a `MediaResource` snapshot and returns the id it identifies.
///
/// The id is what `commerce_product_media.media_resource_id` stores, so deriving it here — in the
/// crate that owns the write model — is what keeps the reference and the snapshot from disagreeing:
/// there is exactly one function that reads `id`, and the repository calls it to fill the column.
///
/// The snapshot is a **read-model projection**, not the system of record
/// (`MEDIA_RESOURCE_SPEC` section 5). Nothing here mints an id, fetches metadata, or dereferences a
/// URL: the caller supplies what it already read from Drive.
pub fn media_resource_identity(
    field: &str,
    snapshot: &serde_json::Value,
) -> Result<i64, CommerceServiceError> {
    let object = snapshot.as_object().ok_or_else(|| {
        CommerceServiceError::validation(format!("{field} must be a MediaResource object"))
    })?;

    for key in object.keys() {
        if MEDIA_RESOURCE_REQUIRED_KEYS.contains(&key.as_str()) {
            continue;
        }
        if MEDIA_RESOURCE_OPTIONAL_KEYS.contains(&key.as_str()) {
            continue;
        }
        return Err(CommerceServiceError::validation(format!(
            "{field} carries `{key}`, which is not a MediaResource field: the declared schema is \
             closed, so an undeclared key would be dropped rather than stored"
        )));
    }

    for key in MEDIA_RESOURCE_REQUIRED_KEYS {
        if !object.contains_key(key) {
            return Err(CommerceServiceError::validation(format!(
                "{field}.{key} is required by MediaResource"
            )));
        }
    }

    let raw_id = media_resource_text(field, object, "id")?;
    let id = raw_id.parse::<i64>().map_err(|_| {
        CommerceServiceError::validation(format!(
            "{field}.id must be a decimal int64 string, got `{raw_id}`"
        ))
    })?;
    if id <= 0 {
        return Err(CommerceServiceError::validation(format!(
            "{field}.id must be a positive snowflake id, got `{raw_id}`"
        )));
    }

    let _kind = media_resource_text(field, object, "kind")?;
    let source = media_resource_text(field, object, "source")?;
    if source == MEDIA_SOURCE_DRIVE {
        let uri = media_resource_text(field, object, "uri").map_err(|_| {
            CommerceServiceError::validation(format!(
                "{field}.uri is required when source is `drive`: the identity of a Drive-backed \
                 resource is `{MEDIA_URI_DRIVE_PREFIX}spaces/{{spaceId}}/nodes/{{nodeId}}`"
            ))
        })?;
        if !uri.starts_with(MEDIA_URI_DRIVE_PREFIX) {
            return Err(CommerceServiceError::validation(format!(
                "{field}.uri must start with `{MEDIA_URI_DRIVE_PREFIX}` when source is `drive`, \
                 got `{uri}`"
            )));
        }
    }

    Ok(id)
}

/// Reads one non-empty string field of a `MediaResource` snapshot.
fn media_resource_text<'a>(
    field: &str,
    object: &'a serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<&'a str, CommerceServiceError> {
    object
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            CommerceServiceError::validation(format!("{field}.{key} must be a non-empty string"))
        })
}
