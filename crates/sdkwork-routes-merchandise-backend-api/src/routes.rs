use axum::Router;
use sdkwork_merchandise_service_host::MerchandiseServiceHost;
use sdkwork_merchandise_web_support::backend_catalog_router_with_postgres_pool;
use std::sync::Arc;

use crate::web_bootstrap::wrap_router_with_web_framework_from_env;

pub fn build_merchandise_backend_router(host: Arc<MerchandiseServiceHost>) -> Router {
    // The merchandise backend surface is PostgreSQL-only (DATABASE_SPEC:
    // authoritative-server). `MerchandiseServiceHost` is constructed by the
    // composition root, which fails startup when it cannot open the
    // authoritative pool, so a wired host always yields `Some` here. The
    // assembly bootstrap that calls this is generated, so this entry point
    // keeps its infallible signature and states the invariant instead of
    // returning a `Result` that could never be observed.
    let pool = host
        .database_pool()
        .as_postgres()
        .expect("merchandise backend router requires an authoritative PostgreSQL pool")
        .clone();
    // The Snowflake port comes from the host, never from a local constructor: it is the same
    // generator every other repository in this process uses, so the catalog cannot mint ids under a
    // second node id.
    backend_catalog_router_with_postgres_pool(pool, host.id_generator())
}

pub async fn build_merchandise_backend_router_with_framework(
    host: Arc<MerchandiseServiceHost>,
) -> Router {
    wrap_router_with_web_framework_from_env(build_merchandise_backend_router(host)).await
}
