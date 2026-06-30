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
/// Stored as a bare `Arc<Executor>` (no outer lock). Since apcore 0.25 the
/// `Executor` is fully interior-mutable — `call`/`stream` take `&self` and its
/// `registry` is an interior-mutable `Arc<Registry>` — so concurrent callers
/// can share one executor without serializing through a `Mutex`.
static EXECUTOR: OnceLock<Arc<Executor>> = OnceLock::new();

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
/// Returns a shared `Arc<Executor>`. The executor is interior-mutable, so
/// `call()`, `stream()`, and `cancellable_call()` operate on `&self` without
/// any outer lock.
pub fn get_executor() -> Arc<Executor> {
    EXECUTOR
        .get_or_init(|| {
            tracing::debug!("Initializing apcore Executor");
            let registry = Registry::new();
            let config = build_config();
            Arc::new(Executor::new(registry, config))
        })
        .clone()
}

/// Build an apcore Config from settings.
///
/// Uses `Config::from_defaults()` (not `Config::default()`) so the built-in
/// namespace registry is initialized — `from_defaults` is apcore's canonical
/// constructor for user code, while bare `default()` is reserved for internal
/// test scaffolding. Tracing/metrics toggles moved under the nested
/// `observability` config in apcore 0.18.
fn build_config() -> Config {
    let settings = get_apcore_settings();
    let mut config = Config::from_defaults();
    config.modules_path = Some(settings.module_dir.clone());
    config.observability.tracing.enabled = settings.tracing;
    config.observability.metrics.enabled = settings.metrics;
    config
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
