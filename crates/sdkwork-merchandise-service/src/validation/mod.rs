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
