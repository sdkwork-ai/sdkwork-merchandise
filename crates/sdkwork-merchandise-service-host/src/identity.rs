//! Snowflake identity bootstrap for the Merchandise composition host.
//!
//! Every Merchandise table uses a platform Snowflake `BIGINT` primary key
//! (`DATABASE_SPEC.md` §6.1, `SUBJECT_ID_SPEC`). Those keys cannot be invented per call site: a
//! Snowflake id encodes a node id plus a per-node sequence, so two generators sharing a node id
//! silently produce duplicate keys. This module is the single place that acquires the process
//! identity, and the only place a generator is constructed.
//!
//! # Why the allocator, not a local generator
//!
//! In production-like environments the node id comes from the platform node registry through
//! [`SnowflakeNodeAllocator::allocate_process_generator`], which
//!
//! - leases a node id that is unique across every running process,
//! - returns the **same** generator to every caller in this process, so every repository composed
//!   into this host shares one node id and one sequence state, and
//! - rejects a second generator bound to a different database authority.
//!
//! A statically configured node id is therefore refused in production-like environments: it cannot
//! be collision-free once more than one instance runs. Development and test runs use a static node
//! id so local boots do not depend on the node registry table existing.
//!
//! # Why the id is injected, not global
//!
//! `sdkwork-merchandise-repository-sqlx` takes `Arc<dyn IdGenerator>` as an explicit dependency.
//! A process-global generator would make two hosts in one test process share a sequence and would
//! hide the dependency that decides a row's primary key.

use std::sync::Arc;

use sdkwork_database_id::{
    IdGenError, IdGenerator, NodeAllocatorConfig, NodeLease, SnowflakeIdGenerator,
    SnowflakeNodeAllocator,
};
use sdkwork_database_sqlx::DatabasePool;

use crate::runtime_env::{merchandise_environment_name, merchandise_is_production_like};

/// Static node id escape hatch, honoured only outside production-like environments.
pub const MERCHANDISE_SNOWFLAKE_NODE_ID_ENV: &str = "SDKWORK_MERCHANDISE_SNOWFLAKE_NODE_ID";

/// Logical service name registered in the platform node registry.
pub const MERCHANDISE_IDENTITY_SERVICE_NAME: &str = "sdkwork-merchandise";

/// Database module whose connection profile serves the node registry.
///
/// The lease lives in a platform-owned registry table, so it uses the same database authority as
/// the application's own tables. Connection identity and pool policy always resolve from
/// `SDKWORK_DATABASE_*`.
pub const MERCHANDISE_IDENTITY_DATABASE_SERVICE: &str = "MERCHANDISE";

/// Node id used when no lease is required and none was configured.
const DEVELOPMENT_NODE_ID: u16 = 1;

/// How this process must obtain its Snowflake node id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityPosture {
    /// Lease a node id from the platform registry. Required when running more than one instance.
    Leased,
    /// Use a static node id with no lease. Development and test only.
    Static,
}

/// Decide the identity posture from the resolved environment and the configured static node id.
///
/// Split out from [`MerchandiseIdentity::from_pool`] so the fail-closed rule is testable without an
/// environment mutation or a database.
///
/// # Errors
///
/// Returns an error when a static node id is configured in a production-like environment:
/// honouring it would hand every replica the same node id.
pub fn identity_posture(
    production_like: bool,
    configured_node_id: Option<&str>,
) -> Result<IdentityPosture, String> {
    if !production_like {
        return Ok(IdentityPosture::Static);
    }
    match configured_node_id {
        Some(value) => Err(format!(
            "static {MERCHANDISE_SNOWFLAKE_NODE_ID_ENV}={value} is forbidden in production-like \
             environments; remove it so the node id is leased from the platform node registry"
        )),
        None => Ok(IdentityPosture::Leased),
    }
}

/// The Merchandise process identity: one Snowflake generator plus the lease that keeps its node id
/// reserved.
///
/// Cloneable and cheap — the generator is internally shared, and clones share the same node id and
/// sequence state. Hold a clone for as long as the process generates ids; dropping the last clone
/// releases the lease so another instance can claim the node id.
#[derive(Clone)]
pub struct MerchandiseIdentity {
    generator: SnowflakeIdGenerator,
    lease: Option<NodeLease>,
}

impl std::fmt::Debug for MerchandiseIdentity {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MerchandiseIdentity")
            .field("node_id", &self.generator.node_id())
            .field("database_backed", &self.lease.is_some())
            .finish()
    }
}

impl MerchandiseIdentity {
    /// Acquire the process identity against an existing database pool.
    ///
    /// Pass the pool the application already owns: the node registry is a platform table served by
    /// the same authority as the Merchandise tables, and reusing the pool keeps the lease alive for
    /// the process lifetime instead of stranding a second connection pool.
    ///
    /// # Errors
    ///
    /// Fails when a production-like environment cannot obtain a lease, and when a static node id is
    /// configured in a production-like environment. Both are startup failures by design: continuing
    /// would produce colliding ids.
    pub async fn from_pool(pool: &DatabasePool) -> Result<Self, String> {
        let posture = identity_posture(
            merchandise_is_production_like(),
            std::env::var(MERCHANDISE_SNOWFLAKE_NODE_ID_ENV)
                .ok()
                .as_deref(),
        )?;

        match posture {
            IdentityPosture::Static => Self::development_identity(),
            IdentityPosture::Leased => Self::leased_identity(pool).await,
        }
    }

    /// Lease a node id from the platform registry.
    async fn leased_identity(pool: &DatabasePool) -> Result<Self, String> {
        let config = NodeAllocatorConfig::from_service_name(MERCHANDISE_IDENTITY_SERVICE_NAME);
        let (generator, lease) = SnowflakeNodeAllocator::allocate_process_generator(pool, &config)
            .await
            .map_err(|error| {
                format!("allocate Merchandise Snowflake node lease failed: {error}")
            })?;

        tracing::info!(
            node_id = lease.node_id(),
            service = MERCHANDISE_IDENTITY_SERVICE_NAME,
            database_service = MERCHANDISE_IDENTITY_DATABASE_SERVICE,
            "merchandise acquired Snowflake node lease"
        );

        Ok(Self {
            generator,
            lease: Some(lease),
        })
    }

    /// Identity for development and test runs: a static node id, no lease, no dependency on the
    /// node registry table.
    ///
    /// # Errors
    ///
    /// Fails when the configured node id is malformed or exceeds the platform node-id range.
    pub fn development_identity() -> Result<Self, String> {
        let node_id = match std::env::var(MERCHANDISE_SNOWFLAKE_NODE_ID_ENV) {
            Ok(value) => value
                .trim()
                .parse::<u16>()
                .map_err(|error| format!("invalid {MERCHANDISE_SNOWFLAKE_NODE_ID_ENV}: {error}"))?,
            Err(_) => DEVELOPMENT_NODE_ID,
        };
        let generator = SnowflakeIdGenerator::new(node_id)
            .map_err(|error| format!("create dev Snowflake generator failed: {error}"))?;

        tracing::warn!(
            node_id,
            environment = merchandise_environment_name(),
            "merchandise using a static Snowflake node id; this is only valid outside \
             production-like environments"
        );

        Ok(Self {
            generator,
            lease: None,
        })
    }

    /// Build an identity from an explicit generator, for tests that need deterministic ids.
    #[must_use]
    pub fn from_generator(generator: SnowflakeIdGenerator) -> Self {
        Self {
            generator,
            lease: None,
        }
    }

    /// The shared generator. Reach for this when a repository needs to produce ids itself; prefer
    /// [`Self::next_id`] for a single id.
    #[must_use]
    pub fn generator(&self) -> &SnowflakeIdGenerator {
        &self.generator
    }

    /// The node id this process generates ids under.
    #[must_use]
    pub fn node_id(&self) -> u16 {
        self.generator.node_id()
    }

    /// Whether the node id is held under a platform lease rather than a static configuration.
    #[must_use]
    pub fn is_database_backed(&self) -> bool {
        self.lease.is_some()
    }

    /// Generate one id.
    ///
    /// # Errors
    ///
    /// Fails when the sequence is exhausted for the current millisecond and the clock has not
    /// advanced — a genuine capacity boundary rather than a transient error.
    pub fn next_id(&self) -> Result<i64, String> {
        self.generator
            .generate()
            .map_err(|error| format!("snowflake generation failed: {error}"))
    }
}

impl IdGenerator for MerchandiseIdentity {
    fn next_id(&self) -> Result<String, IdGenError> {
        IdGenerator::next_id(&self.generator)
    }

    fn label(&self) -> &str {
        "snowflake"
    }
}

/// Wrap an identity in the shared handle a composition root stores.
#[must_use]
pub fn shared_identity(identity: MerchandiseIdentity) -> Arc<MerchandiseIdentity> {
    Arc::new(identity)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::test_support::{env_lock, with_env};

    #[test]
    fn production_refuses_a_static_node_id() {
        let error = identity_posture(true, Some("7")).expect_err("must fail closed");
        assert!(
            error.contains(MERCHANDISE_SNOWFLAKE_NODE_ID_ENV),
            "the error must name the offending variable: {error}"
        );
    }

    #[test]
    fn production_without_a_static_node_id_leases() {
        assert_eq!(
            identity_posture(true, None).expect("leased posture"),
            IdentityPosture::Leased
        );
    }

    #[test]
    fn development_uses_a_static_node_id_even_when_one_is_configured() {
        assert_eq!(
            identity_posture(false, Some("7")).expect("static posture"),
            IdentityPosture::Static
        );
    }

    #[test]
    fn development_identity_defaults_to_node_one_without_a_lease() {
        let _guard = env_lock();
        with_env(MERCHANDISE_SNOWFLAKE_NODE_ID_ENV, None, || {
            let identity = MerchandiseIdentity::development_identity().expect("dev identity");
            assert_eq!(identity.node_id(), DEVELOPMENT_NODE_ID);
            assert!(!identity.is_database_backed());
        });
    }

    #[test]
    fn development_identity_honours_an_explicit_node_id() {
        let _guard = env_lock();
        with_env(MERCHANDISE_SNOWFLAKE_NODE_ID_ENV, Some("42"), || {
            let identity = MerchandiseIdentity::development_identity().expect("dev identity");
            assert_eq!(identity.node_id(), 42);
        });
    }

    #[test]
    fn development_identity_rejects_a_malformed_node_id() {
        let _guard = env_lock();
        with_env(
            MERCHANDISE_SNOWFLAKE_NODE_ID_ENV,
            Some("not-a-number"),
            || {
                assert!(MerchandiseIdentity::development_identity().is_err());
            },
        );
    }

    #[test]
    fn generated_ids_are_unique_and_ordered() {
        let identity = MerchandiseIdentity::from_generator(
            SnowflakeIdGenerator::new(DEVELOPMENT_NODE_ID).expect("valid node id"),
        );
        let first = identity.next_id().expect("first id");
        let second = identity.next_id().expect("second id");
        assert!(
            second > first,
            "snowflake ids must be monotonically ordered"
        );
    }

    /// The port the repository sees must hand back the same decimal string the trait promises, and
    /// must be parseable as the BIGINT the baseline stores.
    #[test]
    fn the_injected_port_produces_a_decimal_bigint_string() {
        let identity = MerchandiseIdentity::from_generator(
            SnowflakeIdGenerator::new(DEVELOPMENT_NODE_ID).expect("valid node id"),
        );
        let port: Arc<dyn IdGenerator> = Arc::new(identity);
        assert_eq!(port.label(), "snowflake");

        let raw = port.next_id().expect("port id");
        let parsed = raw.parse::<i64>().expect("wire id must be a decimal i64");
        assert!(parsed > 0, "snowflake ids are positive");
    }

    #[test]
    fn clones_share_one_node_id() {
        let identity = MerchandiseIdentity::from_generator(
            SnowflakeIdGenerator::new(DEVELOPMENT_NODE_ID).expect("valid node id"),
        );
        let clone = identity.clone();
        assert_eq!(clone.node_id(), identity.node_id());
    }
}
