//! SDKWork v3 response helpers shared by merchandise HTTP adapters.

use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequest, Request};
use axum::http::{header, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use sdkwork_merchandise_service::StaleVersion;
use sdkwork_utils_rust::http_api::{
    PageInfo, PageMode, SdkWorkApiResponse, SdkWorkCommandData, SdkWorkPageData,
    SdkWorkResourceData,
};
use sdkwork_utils_rust::{SdkWorkProblemDetail, SdkWorkResultCode};
use sdkwork_web_core::{
    problem_response, ProblemCorrelation, WebFrameworkError, WebFrameworkErrorKind,
};
use serde::Serialize;
use serde_json::{json, Value};

pub fn resolve_trace_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn success_item<T: Serialize>(trace_id: impl Into<String>, item: T) -> Json<Value> {
    Json(
        serde_json::to_value(SdkWorkApiResponse::success(
            SdkWorkResourceData { item },
            trace_id,
        ))
        .unwrap_or_else(|_| json!({ "code": 0, "data": { "item": null }, "traceId": "" })),
    )
}

pub fn success_page<T: Serialize>(trace_id: impl Into<String>, items: Vec<T>) -> Json<Value> {
    let item_count = items.len();
    Json(
        serde_json::to_value(SdkWorkApiResponse::success(
            SdkWorkPageData {
                items,
                page_info: PageInfo {
                    mode: PageMode::Offset,
                    page: Some(1),
                    page_size: Some(item_count as i32),
                    total_items: Some(item_count.to_string()),
                    total_pages: Some(1),
                    next_cursor: None,
                    has_more: Some(false),
                },
            },
            trace_id,
        ))
        .unwrap_or_else(|_| {
            json!({ "code": 0, "data": { "items": [], "pageInfo": { "mode": "offset" } }, "traceId": "" })
        }),
    )
}

pub fn success_offset_page<T: Serialize>(
    items: Vec<T>,
    page: i64,
    page_size: i64,
    total_items: i64,
) -> Response {
    let page_info = offset_page_info(page, page_size, total_items);
    Json(
        serde_json::to_value(SdkWorkApiResponse::success(
            SdkWorkPageData { items, page_info },
            resolve_trace_id(),
        ))
        .unwrap_or_else(|_| {
            json!({ "code": 0, "data": { "items": [], "pageInfo": { "mode": "offset" } }, "traceId": "" })
        }),
    )
    .into_response()
}

fn offset_page_info(page: i64, page_size: i64, total_items: i64) -> PageInfo {
    let total_pages = if total_items == 0 {
        0
    } else {
        (total_items + page_size - 1) / page_size
    };
    PageInfo {
        mode: PageMode::Offset,
        page: i32::try_from(page).ok(),
        page_size: i32::try_from(page_size).ok(),
        total_items: Some(total_items.to_string()),
        total_pages: i32::try_from(total_pages).ok(),
        next_cursor: None,
        has_more: Some(page < total_pages),
    }
}

pub fn success_command(trace_id: impl Into<String>) -> Json<Value> {
    Json(
        serde_json::to_value(SdkWorkApiResponse::success(
            SdkWorkCommandData::accepted(),
            trace_id,
        ))
        .unwrap_or_else(|_| json!({ "code": 0, "data": { "accepted": true }, "traceId": "" })),
    )
}

pub fn success_resource<T: Serialize>(item: T) -> Response {
    success_item(resolve_trace_id(), item).into_response()
}

pub fn success_created_resource<T: Serialize>(item: T) -> Response {
    (StatusCode::CREATED, success_item(resolve_trace_id(), item)).into_response()
}

/// Quotes a row version as an HTTP entity-tag (`RFC 9110` section 8.8.3).
///
/// The quotes are part of the syntax, not decoration: an entity-tag is `"<opaque>"`, and a bare
/// number is not a valid tag. Emitting the quotes here means the parser on the way back in has one
/// documented shape to accept, and a client that round-trips the header verbatim is correct by
/// construction.
pub fn entity_tag(version: i64) -> String {
    format!("\"{version}\"")
}

/// A single-resource success response that also publishes the row version as `ETag`.
///
/// The `version` field in the body is what a client usually reads, and this header is the same
/// number in the form `If-Match` accepts. Both are emitted because the pair is what makes
/// optimistic concurrency usable without a second round trip: whatever a client last saw can be
/// handed straight back as the precondition.
pub fn success_resource_with_etag<T: Serialize>(item: T, version: i64) -> Response {
    let mut response = success_resource(item);
    insert_entity_tag(&mut response, version);
    response
}

pub fn success_created_resource_with_etag<T: Serialize>(item: T, version: i64) -> Response {
    let mut response = success_created_resource(item);
    insert_entity_tag(&mut response, version);
    response
}

fn insert_entity_tag(response: &mut Response, version: i64) {
    if let Ok(value) = HeaderValue::from_str(&entity_tag(version)) {
        response.headers_mut().insert(header::ETAG, value);
    }
}

pub fn success_no_content() -> Response {
    StatusCode::NO_CONTENT.into_response()
}

pub fn success_list<T: Serialize>(items: Vec<T>) -> Response {
    success_page(resolve_trace_id(), items).into_response()
}

pub fn success_accepted() -> Response {
    success_command(resolve_trace_id()).into_response()
}

fn problem_for(kind: WebFrameworkErrorKind, message: impl Into<String>) -> Response {
    let trace_id = resolve_trace_id();
    problem_response(
        &WebFrameworkError {
            kind,
            message: message.into(),
            retry_after_seconds: None,
            auth_profile: None,
            failed_stage: None,
            reason: None,
        },
        ProblemCorrelation::from(Some(trace_id.as_str())),
    )
}

pub fn unauthorized_response(message: impl Into<String>) -> Response {
    problem_for(WebFrameworkErrorKind::MissingCredentials, message)
}

pub fn validation_response(message: impl Into<String>) -> Response {
    problem_for(WebFrameworkErrorKind::BadRequest, message)
}

/// JSON body extractor that answers a rejected body with the platform problem envelope.
///
/// Bare `axum::Json` rejects a body the DTO cannot absorb with a `422` whose payload is plain text,
/// and it does so before the handler runs. That response matches no schema the contract declares,
/// carries no `traceId`, and cannot be correlated with the request — while the write bodies now
/// declare `additionalProperties: false`, so an unknown field reaches exactly this path. Parsing
/// through this extractor maps every rejection into the `400` `ProblemDetail` envelope those same
/// operations already declare, which is also where a domain-level validation failure is reported:
/// one shape for "your request was not accepted".
pub struct CatalogJson<T>(pub T);

impl<S, T> FromRequest<S> for CatalogJson<T>
where
    Json<T>: FromRequest<S, Rejection = JsonRejection>,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(request, state).await {
            Ok(Json(value)) => Ok(Self(value)),
            Err(rejection) => Err(validation_response(rejection.body_text())),
        }
    }
}

pub fn not_found_response(message: impl Into<String>) -> Response {
    problem_for(WebFrameworkErrorKind::NotFound, message)
}

/// Builds the problem response for a platform result code the shared error bridge cannot express.
///
/// `API_SPEC` section 17 makes two outcomes normative for `If-Match` optimistic concurrency: a
/// missing required precondition answers `42801 PRECONDITION_REQUIRED` (`428`), and a precondition
/// that fails to match answers `41201 PRECONDITION_FAILED` (`412`). Both codes are registered in
/// `SdkWorkResultCode`, but `WebFrameworkErrorKind` has no variant for either, and
/// `problem_response` derives *both* the status and the code from that kind alone — so no
/// `WebFrameworkError` value can produce them.
///
/// The payload is therefore built from the platform's own `SdkWorkProblemDetail` rather than
/// hand-rolled JSON: the response *shape* stays owned by `sdkwork-utils-rust`, and the only local
/// part is the missing mapping from a registered result code to a response. `catalog-precondition-
/// bridge-closure.test.mjs` pins this output against `problem_response` field by field, so the two
/// cannot drift into disagreeing about what a problem body looks like.
fn precondition_problem(result_code: SdkWorkResultCode, message: impl Into<String>) -> Response {
    let trace_id = resolve_trace_id();
    let correlation = ProblemCorrelation::from(Some(trace_id.as_str()));
    let mut detail = SdkWorkProblemDetail::platform_enriched(
        result_code,
        message.into(),
        trace_id.clone(),
        correlation.routing(),
    );
    detail.status = result_code.http_status_code();
    let status = StatusCode::from_u16(result_code.http_status_code())
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let mut response = (
        status,
        [(header::CONTENT_TYPE, "application/problem+json")],
        Json(serde_json::to_value(detail).unwrap_or_else(|_| json!({}))),
    )
        .into_response();
    if let Ok(value) = HeaderValue::from_str(&trace_id) {
        response.headers_mut().insert(
            HeaderName::from_static(sdkwork_web_core::constants::SDKWORK_TRACE_ID_HEADER_LOWER),
            value,
        );
    }
    response
}

/// `42801` — the operation requires an `If-Match` precondition and the request carried none.
///
/// `API_SPEC` section 17 separates this from a *failed* precondition on purpose: "you did not say
/// which version you are editing" is a different caller mistake from "you are editing a version
/// that has moved", and only the second is worth re-reading the resource for.
pub fn precondition_required_response(message: impl Into<String>) -> Response {
    precondition_problem(SdkWorkResultCode::PreconditionRequired, message)
}

/// `41201` — the `If-Match` precondition did not match the row's current version.
pub fn precondition_failed_response(message: impl Into<String>) -> Response {
    precondition_problem(SdkWorkResultCode::PreconditionFailed, message)
}

/// The `412` a guarded write answers when the row moved past the version the caller read.
///
/// The message names **both** versions on purpose. A `412` that only says "your version is stale"
/// leaves the caller to re-read and diff a resource it may be about to overwrite anyway; naming the
/// version it expected and the version the row carries turns the failure into the retry, and tells an
/// operator reading a log which of two writers lost, without a second query.
///
/// `expected` and `actual` are quoted as entity-tags because that is the form `If-Match` accepts,
/// so the fix is legible in the message itself: send `If-Match: "<actual>"`.
pub fn stale_version_response(subject: &str, stale: StaleVersion) -> Response {
    precondition_failed_response(format!(
        "the {subject} has version {} but this request named {}: nothing was written. Re-read the \
         {subject} and retry with `If-Match: \"{}\"`",
        stale.actual, stale.expected, stale.actual,
    ))
}

/// Reads `If-Match` and parses it into the row version the caller believes it is editing.
///
/// Every mutating operation other than a create declares `If-Match` as required, so this returns a
/// response to send rather than an error to wrap:
///
/// - **absent** answers `42801` (`428`), the platform code for "a required precondition header is
///   missing" (`API_SPEC` section 17). It is deliberately not `400`: the request is well formed, the
///   caller simply has not said which version it is editing, and a client can act on the difference.
/// - **present but unparseable** answers `400` through the same envelope a rejected body uses, so an
///   operator sees one shape for "your request was not accepted".
///
/// Two values are refused rather than accepted, both on purpose:
///
/// - `If-Match: *` means "the resource exists", which would let a caller opt out of the precondition
///   this operation declares as required. An operation that requires a version cannot also accept
///   "any version"; a caller that genuinely wants last-write-wins should not be able to get it by
///   accident.
/// - A weak entity-tag (`W/"3"`) is refused because `If-Match` compares with the *strong* comparison
///   function (`RFC 9110` section 13.1.1): a weak tag can never match a versioned row, so accepting
///   it would only turn a typo into a mysterious `412` later.
///
/// The bare `3` form is accepted alongside the standard `"3"`, because the value is a decimal
/// version rather than opaque content and the quoted form is easy to lose in a hand-written request.
///
/// The failure arm is boxed rather than inline: an `axum::Response` is 128 bytes and this
/// function hands one back on a path every successful request takes and discards, so leaving it
/// inline would charge the common case for the uncommon one.
pub fn expected_version_from_if_match(
    headers: &axum::http::HeaderMap,
) -> Result<i64, Box<Response>> {
    let Some(value) = headers.get(header::IF_MATCH) else {
        return Err(Box::new(precondition_required_response(
            "If-Match is required on this operation: send the version you are editing as `If-Match: \"<version>\"` (the ETag of the last read, or the `version` field of the resource)",
        )));
    };
    let raw = match value.to_str() {
        Ok(raw) => raw.trim(),
        Err(_) => {
            return Err(Box::new(validation_response(
                "If-Match must be an ASCII entity-tag such as \"3\"",
            )))
        }
    };
    let digits = if let Some(weak) = raw.strip_prefix("W/") {
        return Err(Box::new(validation_response(format!(
            "If-Match does not accept the weak entity-tag {weak}: If-Match uses the strong comparison function"
        ))));
    } else if raw == "*" {
        return Err(Box::new(validation_response(
            "If-Match does not accept `*` on this operation: it requires the concrete version you are editing",
        )));
    } else if let Some(inner) = raw
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
    {
        inner
    } else {
        raw
    };
    match digits.parse::<i64>() {
        Ok(version) if version >= 0 => Ok(version),
        _ => Err(Box::new(validation_response(format!(
            "If-Match must be a row version such as \"3\"; received `{raw}`"
        )))),
    }
}

/// Maps a repository failure onto the status the operation actually declares.
///
/// The repository raises typed failures, and each one answers a different question the caller can
/// act on: a `validation` failure names a field to fix, a `not-found` says the row is gone, a
/// `conflict` says a uniqueness rule the caller can choose around. Collapsing all three into one
/// "dependency unavailable" (`503`) told the caller to retry a request that would never succeed —
/// while the same operation's contract declares `400`, `404`, and `409` for exactly these cases.
///
/// Anything that is not one of those three is a genuine infrastructure failure and keeps the
/// unavailable status: a connection loss is worth retrying, and pretending otherwise would invite a
/// caller to treat a database outage as a bad request.
pub fn catalog_error_response(
    context: &str,
    error: sdkwork_contract_service::CommerceServiceError,
) -> Response {
    use sdkwork_contract_service::CommerceServiceErrorKind;

    let kind = match error.kind() {
        CommerceServiceErrorKind::Validation => WebFrameworkErrorKind::BadRequest,
        CommerceServiceErrorKind::Unauthorized => WebFrameworkErrorKind::Forbidden,
        CommerceServiceErrorKind::NotFound => WebFrameworkErrorKind::NotFound,
        CommerceServiceErrorKind::Conflict
        | CommerceServiceErrorKind::Locked
        | CommerceServiceErrorKind::InvalidState
        | CommerceServiceErrorKind::InsufficientBalance => WebFrameworkErrorKind::Conflict,
        _ => WebFrameworkErrorKind::DependencyUnavailable,
    };

    problem_for(kind, format!("{context}: {}", error.message()))
}

#[cfg(test)]
mod tests {
    use super::offset_page_info;

    #[test]
    fn offset_page_info_reports_real_boundaries() {
        let value = serde_json::to_value(offset_page_info(2, 20, 41)).unwrap();
        assert_eq!(value["mode"], "offset");
        assert_eq!(value["page"], 2);
        assert_eq!(value["pageSize"], 20);
        assert_eq!(value["totalItems"], "41");
        assert_eq!(value["totalPages"], 3);
        assert_eq!(value["hasMore"], true);
    }

    #[test]
    fn offset_page_info_handles_empty_results() {
        let value = serde_json::to_value(offset_page_info(1, 20, 0)).unwrap();
        assert_eq!(value["totalPages"], 0);
        assert_eq!(value["hasMore"], false);
    }
}
