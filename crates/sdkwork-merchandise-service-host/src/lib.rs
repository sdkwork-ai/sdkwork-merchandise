//! Merchandise service composition host.
//!
//! This crate is the composition root for the Merchandise capability. It owns three things that must
//! exist exactly once per process:
//!
//! 1. the authoritative database pool,
//! 2. the Snowflake identity every repository mints primary keys from, and
//! 3. the catalog repository implementation bound to the port the service crate declares.
//!
//! All three are decided here and injected downward. A repository that opened its own pool would
//! defeat the module's connection budget; a repository that built its own generator would hand a
//! second writer the same node id, and Snowflake ids would collide silently.
//!
//! # Why the repository is constructed here
//!
//! `CatalogRepositoryPort` is declared by `sdkwork-merchandise-service` and implemented by
//! `sdkwork-merchandise-repository-sqlx`. Someone has to name both, and the layering rules put that
//! someone here: a route crate must not depend on a concrete repository, a service crate must not
//! either, and the repository crate cannot construct itself for a caller it does not know. A host
//! that merely forwarded a pool downward would leave the binding to the transport layer, which is
//! how the HTTP adapter came to own the persistence contract.
//!
//! Constructing it here also moves one invariant to startup. The catalog store needs the
//! authoritative PostgreSQL pool, so a host built over any other backend now fails in its
//! constructor instead of panicking deep inside a request path.

mod identity;
mod runtime_env;
#[cfg(test)]
pub(crate) mod test_support;

use std::sync::Arc;

use sdkwork_database_id::IdGenerator;
use sdkwork_database_sqlx::{process_shared_database_pool, DatabasePool};
use sdkwork_merchandise_database_host::{
    bootstrap_merchandise_database, bootstrap_merchandise_database_from_env,
};
use sdkwork_merchandise_repository_sqlx::PostgresCommerceCatalogStore;
use sdkwork_merchandise_service::CatalogRepositoryPort;

pub use identity::{
    identity_posture, shared_identity, IdentityPosture, MerchandiseIdentity,
    MERCHANDISE_IDENTITY_DATABASE_SERVICE, MERCHANDISE_IDENTITY_SERVICE_NAME,
    MERCHANDISE_SNOWFLAKE_NODE_ID_ENV,
};
pub use runtime_env::{
    merchandise_environment, merchandise_environment_name, merchandise_is_production_like,
    MERCHANDISE_ENVIRONMENT_KEYS,
};

pub struct MerchandiseServiceHost {
    database_pool: DatabasePool,
    identity: Arc<MerchandiseIdentity>,
    id_generator: Arc<dyn IdGenerator>,
    catalog_repository: Arc<dyn CatalogRepositoryPort>,
}

impl MerchandiseServiceHost {
    pub async fn new() -> Self {
        Self::from_env()
            .await
            .expect("merchandise service host bootstrap failed")
    }

    pub async fn from_env() -> Result<Self, String> {
        if let Some(database_pool) = process_shared_database_pool() {
            return Self::from_pool(database_pool).await;
        }

        let database = bootstrap_merchandise_database_from_env().await?;
        Self::from_bootstrapped_pool(database.pool().clone()).await
    }

    pub async fn from_pool(pool: DatabasePool) -> Result<Self, String> {
        let database = bootstrap_merchandise_database(pool).await?;
        Self::from_bootstrapped_pool(database.pool().clone()).await
    }

    /// Acquire the process Snowflake identity against an already-bootstrapped pool.
    ///
    /// The identity is built from this pool rather than from a fresh connection so the node-registry
    /// lease shares the module's authority and survives for the process lifetime.
    async fn from_bootstrapped_pool(database_pool: DatabasePool) -> Result<Self, String> {
        let identity = Arc::new(MerchandiseIdentity::from_pool(&database_pool).await?);
        // One identity, two handles: the concrete handle keeps the node lease alive and answers
        // diagnostics, the trait object is what the repository layer is wired against.
        let id_generator: Arc<dyn IdGenerator> = identity.clone();
        // The catalog store is built from this process's single pool and single generator, so it
        // cannot mint ids under a second node id or open a second connection budget. It is stored
        // as the port, never as the concrete type: callers receive exactly what they may depend on.
        let postgres_pool = database_pool.as_postgres().ok_or_else(|| {
            "the merchandise catalog store requires an authoritative PostgreSQL pool".to_owned()
        })?;
        let catalog_repository: Arc<dyn CatalogRepositoryPort> = Arc::new(
            PostgresCommerceCatalogStore::new(postgres_pool.clone(), Arc::clone(&id_generator)),
        );
        Ok(Self {
            database_pool,
            identity,
            id_generator,
            catalog_repository,
        })
    }

    pub fn database_pool(&self) -> &DatabasePool {
        &self.database_pool
    }

    /// The injectable Snowflake port the repository layer consumes.
    ///
    /// One process, one node id: this returns the same generator to every caller, so a later
    /// repository cannot be wired to a divergent sequence.
    pub fn id_generator(&self) -> Arc<dyn IdGenerator> {
        Arc::clone(&self.id_generator)
    }

    /// The catalog repository implementation, handed out as the service-owned port.
    ///
    /// This is the single place where the port and its implementation are known to be the same
    /// thing. Route code receives `Arc<dyn CatalogRepositoryPort>` and can swap in a test double
    /// without the catalog routes changing, because they never named the concrete store.
    pub fn catalog_repository(&self) -> Arc<dyn CatalogRepositoryPort> {
        Arc::clone(&self.catalog_repository)
    }

    /// The process identity, for diagnostics and node-id assertions.
    pub fn identity(&self) -> Arc<MerchandiseIdentity> {
        Arc::clone(&self.identity)
    }
}

pub fn default_seed_locale() -> &'static str {
    "zh-CN"
}

pub fn default_seed_profile() -> &'static str {
    "standard"
}
