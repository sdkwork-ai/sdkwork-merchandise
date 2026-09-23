use axum::Router;
use sdkwork_merchandise_service_host::MerchandiseServiceHost;
use sdkwork_merchandise_web_support::build_backend_catalog_router;
use std::sync::Arc;

use crate::web_bootstrap::wrap_router_with_web_framework_from_env;

pub fn build_merchandise_backend_router(host: Arc<MerchandiseServiceHost>) -> Router {
    // This crate mounts HTTP and nothing else. The store arrives already constructed, as the
    // service-owned port, so nothing here names a pool, a driver, or a repository: a route crate
    // that could name a repository would be free to bypass the port, and the port would stop being
    // the seam it exists to be.
    //
    // The PostgreSQL-only invariant (DATABASE_SPEC: authoritative-server) is enforced one layer
    // down, in the host constructor, which fails startup over any other backend — so a wired host
    // always has a store to hand over, and this entry point can keep the infallible signature its
    // generated caller expects.
    build_backend_catalog_router(host.catalog_repository())
}

pub async fn build_merchandise_backend_router_with_framework(
    host: Arc<MerchandiseServiceHost>,
) -> Router {
    wrap_router_with_web_framework_from_env(build_merchandise_backend_router(host)).await
}
