//! API assembly for sdkwork-merchandise.
//! Application bootstrap lives in `bootstrap.rs`; route inventory is in `assembly-manifest.json`.
use sdkwork_database_sqlx::DatabasePool;
use sdkwork_web_bootstrap::WebModule;
// SDKWORK-ASSEMBLY-LIB-CUSTOM

mod bootstrap;
mod generated;

pub use bootstrap::{
    assemble_api_router, web_module_with_context, ApiAssembly, ApiAssemblyContext,
};

pub async fn assemble_api_router_from_env() -> Result<ApiAssembly, String> {
    let host = std::sync::Arc::new(
        sdkwork_merchandise_service_host::MerchandiseServiceHost::from_env().await?,
    );
    let readiness_check = std::sync::Arc::new(
        sdkwork_web_bootstrap::DatabasePoolReadinessCheck::new(host.database_pool().clone()),
    );
    assemble_api_router(ApiAssemblyContext {
        host,
        domain_context_injectors: Vec::new(),
        readiness_check,
    })
    .await
}

/// Builds the complete Merchandise contribution against the host process pool.
pub async fn assemble_api_router_with_pool(
    pool: sdkwork_database_sqlx::DatabasePool,
) -> Result<ApiAssembly, String> {
    let host = std::sync::Arc::new(
        sdkwork_merchandise_service_host::MerchandiseServiceHost::from_pool(pool).await?,
    );
    let readiness_check = std::sync::Arc::new(
        sdkwork_web_bootstrap::DatabasePoolReadinessCheck::new(host.database_pool().clone()),
    );
    assemble_api_router(ApiAssemblyContext {
        host,
        domain_context_injectors: Vec::new(),
        readiness_check,
    })
    .await
}

pub fn assembly_route_count() -> usize {
    generated::ROUTE_CRATE_COUNT
}

/// Canonical Web Module definition for this application
/// (API_ASSEMBLY_SPEC §4.1.1): the complete HTTP surface — every route,
/// manifest, and OpenAPI document of this owner — as one installable module.
pub async fn web_module() -> Result<WebModule, String> {
    Ok(WebModule::from_contribution(
        assemble_api_router_from_env().await?,
    ))
}

/// Same as [`web_module`] but composed on a process-shared database pool
/// (platform gateways, API_ASSEMBLY_SPEC §4.1.1).
pub async fn web_module_with_pool(pool: DatabasePool) -> Result<WebModule, String> {
    Ok(WebModule::from_contribution(
        assemble_api_router_with_pool(pool).await?,
    ))
}
