use axum::Extension;
use sdkwork_iam_context_service::IamAppContext;

#[derive(Debug, Clone)]
pub(crate) struct WebRuntimeSubject {
    pub tenant_id: String,
    pub organization_id: Option<String>,
    /// The authenticated principal, carried into every write command as the audit actor. The
    /// context requires it, so a subject without one cannot be constructed.
    pub user_id: String,
}

pub(crate) fn app_runtime_subject_from_extension(
    context: Option<Extension<IamAppContext>>,
) -> Result<WebRuntimeSubject, String> {
    let Some(Extension(context)) = context else {
        return Err("authenticated runtime context is required".to_owned());
    };
    app_runtime_subject_from_iam(&context)
}

pub(crate) fn app_runtime_subject_from_iam(
    context: &IamAppContext,
) -> Result<WebRuntimeSubject, String> {
    let tenant_id = required_context_text(&context.tenant_id, "tenant_id")?;
    let user_id = required_context_text(&context.user_id, "user_id")?;
    let organization_id = context
        .organization_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);

    Ok(WebRuntimeSubject {
        tenant_id,
        organization_id,
        user_id,
    })
}

fn required_context_text(value: &str, field_name: &'static str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(format!(
            "authenticated runtime context {field_name} is required"
        ));
    }
    Ok(value.to_owned())
}
