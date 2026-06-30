// Thread-safe singleton management for apcore Registry and Executor.
//
// Mirrors the pattern from fastapi-apcore's engine/registry.py.

use std::sync::{Arc, Mutex, OnceLock};

use apcore::{Config, Executor, Registry, ACL};

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
///
/// When `APCORE_ACL_PATH` (settings `acl_path`) points at an ACL YAML file, the
/// executor is built via `Executor::with_options` with that ACL attached, so
/// apcore's `acl_check` pipeline step enforces caller authorization on every
/// `call`/`stream`. Top-level requests are checked as the `@external` caller;
/// inter-module calls are checked under the calling module's id. Without
/// `acl_path` the executor carries no ACL and all calls are permitted, matching
/// fastapi-apcore's `acl_path`-driven wiring.
pub fn get_executor() -> Arc<Executor> {
    EXECUTOR
        .get_or_init(|| {
            tracing::debug!("Initializing apcore Executor");
            let registry = Registry::new();
            let config = build_config();
            match load_acl() {
                Some(acl) => Arc::new(Executor::with_options(
                    registry,
                    config,
                    None,
                    Some(acl),
                    None,
                )),
                None => Arc::new(Executor::new(registry, config)),
            }
        })
        .clone()
}

/// Load the ACL from `settings.acl_path`, if configured.
///
/// Returns `None` when no `acl_path` is set. A configured-but-unloadable ACL
/// (missing file, malformed rules) is logged and treated as `None` rather than
/// panicking — a broken ACL must not crash executor initialization. This is a
/// deliberate fail-open on *load* errors only; once a valid ACL is loaded its
/// own `default_effect` governs unmatched calls (default-deny per apcore).
fn load_acl() -> Option<ACL> {
    let settings = get_apcore_settings();
    let path = settings.acl_path.as_deref()?;
    match ACL::load(path) {
        Ok(acl) => {
            tracing::info!(acl_path = path, "Loaded ACL for executor");
            Some(acl)
        }
        Err(e) => {
            tracing::error!(acl_path = path, error = %e.message, "Failed to load ACL; proceeding without access control");
            None
        }
    }
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
