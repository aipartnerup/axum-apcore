// Thread-safe singleton management for apcore Registry and Executor.
//
// Mirrors the pattern from fastapi-apcore's engine/registry.py.

use std::sync::{Arc, Mutex, OnceLock};

use apcore::{Config, Executor, Registry};

use crate::config::get_apcore_settings;

/// Global singleton for the apcore Registry.
static REGISTRY: OnceLock<Arc<Mutex<Registry>>> = OnceLock::new();

/// Global singleton for the apcore Executor.
///
/// Uses `tokio::sync::Mutex` because the executor's async methods (`call`,
/// `stream`) require holding the lock across `.await` points. A
/// `std::sync::Mutex` would block the entire thread in that scenario.
static EXECUTOR: OnceLock<Arc<tokio::sync::Mutex<Executor>>> = OnceLock::new();

/// Get or initialize the global Registry singleton.
pub fn get_registry() -> Arc<Mutex<Registry>> {
    REGISTRY
        .get_or_init(|| {
            tracing::debug!("Initializing apcore Registry");
            Arc::new(Mutex::new(Registry::new()))
        })
        .clone()
}

/// Get or initialize the global Executor singleton.
///
/// Returns a `tokio::sync::Mutex`-wrapped executor to allow safe usage
/// across `.await` points in `call()`, `stream()`, and `cancellable_call()`.
pub fn get_executor() -> Arc<tokio::sync::Mutex<Executor>> {
    EXECUTOR
        .get_or_init(|| {
            tracing::debug!("Initializing apcore Executor");
            let registry = Registry::new();
            let config = build_config();
            Arc::new(tokio::sync::Mutex::new(Executor::new(registry, config)))
        })
        .clone()
}

/// Build an apcore Config from settings.
fn build_config() -> Config {
    let settings = get_apcore_settings();
    Config {
        enable_tracing: settings.tracing,
        enable_metrics: settings.metrics,
        modules_path: Some(settings.module_dir.clone()),
        ..Config::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_registry_returns_same_instance() {
        let r1 = get_registry();
        let r2 = get_registry();
        assert!(Arc::ptr_eq(&r1, &r2));
    }
}
