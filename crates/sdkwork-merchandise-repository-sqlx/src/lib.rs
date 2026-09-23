//! PostgreSQL persistence for the merchandise catalog.
//!
//! One product model only: SPU/SKU master data defined by
//! `commerce_product_*`, `commerce_price_list*`, and `commerce_currency`.
//! Inventory quantities, carts, and buyer addresses are owned by other
//! capabilities and are deliberately absent here.

pub mod postgres_catalog;

pub use postgres_catalog::{unimplemented_operation, PostgresCommerceCatalogStore};
