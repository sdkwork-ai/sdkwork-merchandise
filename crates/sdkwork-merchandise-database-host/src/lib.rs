use std::path::PathBuf;
use std::sync::Arc;

use sdkwork_database_config::DatabaseConfig;
use sdkwork_database_lifecycle::{lifecycle_options_from_env, LifecycleOrchestrator};
use sdkwork_database_spi::{
    DatabaseAssetProvider, DatabaseManifest, DefaultDatabaseModule, SpiError,
};
use sdkwork_database_sqlx::{create_pool_from_config, DatabasePool};

pub struct MerchandiseDatabaseHost {
    pool: DatabasePool,
    module: Arc<DefaultDatabaseModule>,
}

impl MerchandiseDatabaseHost {
    pub fn pool(&self) -> &DatabasePool {
        &self.pool
    }

    pub fn module(&self) -> Arc<DefaultDatabaseModule> {
        self.module.clone()
    }
}

/// Returns the merchandise [`DefaultDatabaseModule`] loaded from the
/// merchandise repository's `database/` directory.
///
/// # Convention
///
/// Each `*-database-host` crate exports this function so that federated hosts
/// (e.g. CloudRouter) can register the module in a `DatabaseModuleRegistry`
/// and run init + migrate + seed through
/// `RegistryLifecycleOrchestrator::bootstrap_all` — the same
/// convention-over-configuration entry point used by payment/order/membership.
pub fn database_module() -> Result<DefaultDatabaseModule, SpiError> {
    DefaultDatabaseModule::from_app_root(resolve_app_root())
}

pub async fn bootstrap_merchandise_database(
    pool: DatabasePool,
) -> Result<MerchandiseDatabaseHost, String> {
    let module = Arc::new(
        database_module()
            .map_err(|error| format!("load merchandise database module failed: {error}"))?,
    );
    let manifest = DatabaseManifest::from_file(module.manifest_path())
        .map_err(|error| format!("read merchandise database manifest failed: {error}"))?;
    let options = lifecycle_options_from_env("MERCHANDISE", &manifest);
    let orchestrator = LifecycleOrchestrator::new(pool.clone(), module.clone())
        .with_applied_by("sdkwork-merchandise");

    orchestrator
        .init()
        .await
        .map_err(|error| format!("merchandise database init failed: {error}"))?;

    if options.auto_migrate {
        orchestrator
            .migrate()
            .await
            .map_err(|error| format!("merchandise database migrate failed: {error}"))?;
    }

    Ok(MerchandiseDatabaseHost { pool, module })
}

pub async fn bootstrap_merchandise_database_from_env() -> Result<MerchandiseDatabaseHost, String> {
    let _ = dotenvy::dotenv();
    let config = DatabaseConfig::from_env("MERCHANDISE")
        .map_err(|error| format!("read merchandise database config failed: {error}"))?;
    let pool = create_pool_from_config(config)
        .await
        .map_err(|error| format!("create merchandise database pool failed: {error}"))?;
    bootstrap_merchandise_database(pool).await
}

fn resolve_app_root() -> PathBuf {
    std::env::var("SDKWORK_MERCHANDISE_APP_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .canonicalize()
                .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
        })
}
