//! Test-only environment helpers.
//!
//! Environment mutation is process-global, so tests that read configuration must serialise against
//! each other. `cargo test` runs test functions on multiple threads inside one process, so a
//! mutation without this lock can leak into an unrelated test's view of the environment.

use std::sync::{Mutex, MutexGuard};

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Serialise every test that mutates or reads the process environment.
pub fn env_lock() -> MutexGuard<'static, ()> {
    ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Run `test` with `key` set to `value`, restoring the previous value afterwards.
pub fn with_env(key: &str, value: Option<&str>, test: impl FnOnce()) {
    let previous = std::env::var(key).ok();
    match value {
        Some(value) => std::env::set_var(key, value),
        None => std::env::remove_var(key),
    }
    test();
    match previous {
        Some(previous) => std::env::set_var(key, previous),
        None => std::env::remove_var(key),
    }
}
