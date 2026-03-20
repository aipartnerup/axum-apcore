// Axum-specific registry writer.
//
// Extends apcore-toolkit's RegistryWriter with Axum handler resolution.

use std::collections::HashMap;
use std::sync::Arc;

use apcore::Registry;
use apcore_toolkit::{HandlerFactory, HandlerFn, RegistryWriter, ScannedModule, WriteResult};

/// Registry writer that resolves Axum handler targets to executable functions.
///
/// Wraps `apcore_toolkit::RegistryWriter` with a handler factory that
/// looks up handlers from the native route metadata registry.
pub struct AxumRegistryWriter {
    inner: RegistryWriter,
}

impl AxumRegistryWriter {
    /// Create a writer with passthrough handlers (schema-only registration).
    pub fn new() -> Self {
        Self {
            inner: RegistryWriter::new(),
        }
    }

    /// Create a writer with a custom handler factory.
    pub fn with_handler_factory(factory: HandlerFactory) -> Self {
        Self {
            inner: RegistryWriter::with_handler_factory(factory),
        }
    }

    /// Create a writer that resolves handlers from a lookup map.
    ///
    /// The map keys are target strings (e.g., "axum::get_user") and values
    /// are the async handler functions.
    pub fn with_handler_map(handlers: HashMap<String, HandlerFn>) -> Self {
        let handlers = Arc::new(handlers);
        let factory: HandlerFactory = Arc::new(move |target: &str| handlers.get(target).cloned());
        Self {
            inner: RegistryWriter::with_handler_factory(factory),
        }
    }

    /// Register scanned modules into the registry.
    pub fn write(
        &self,
        modules: &[ScannedModule],
        registry: &mut Registry,
        dry_run: bool,
        verify: bool,
    ) -> Vec<WriteResult> {
        self.inner.write(modules, registry, dry_run, verify, None)
    }
}

impl Default for AxumRegistryWriter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_module() -> ScannedModule {
        ScannedModule::new(
            "users.get_user.get".into(),
            "Get a user by ID".into(),
            json!({"type": "object"}),
            json!({"type": "object"}),
            vec!["users".into()],
            "axum::get_user".into(),
        )
    }

    #[test]
    fn test_write_registers_module() {
        let writer = AxumRegistryWriter::new();
        let mut registry = Registry::new();
        let modules = vec![sample_module()];
        let results = writer.write(&modules, &mut registry, false, false);
        assert_eq!(results.len(), 1);
        assert!(registry.has("users.get_user.get"));
    }

    #[test]
    fn test_write_dry_run() {
        let writer = AxumRegistryWriter::new();
        let mut registry = Registry::new();
        let modules = vec![sample_module()];
        let results = writer.write(&modules, &mut registry, true, false);
        assert_eq!(results.len(), 1);
        assert!(!registry.has("users.get_user.get"));
    }

    #[test]
    fn test_write_with_verify() {
        let writer = AxumRegistryWriter::new();
        let mut registry = Registry::new();
        let modules = vec![sample_module()];
        let results = writer.write(&modules, &mut registry, false, true);
        assert_eq!(results.len(), 1);
        assert!(results[0].verified);
    }

    #[test]
    fn test_write_empty_list() {
        let writer = AxumRegistryWriter::new();
        let mut registry = Registry::new();
        let results = writer.write(&[], &mut registry, false, false);
        assert!(results.is_empty());
    }

    #[test]
    fn test_write_multiple_modules() {
        let writer = AxumRegistryWriter::new();
        let mut registry = Registry::new();
        let modules = vec![
            ScannedModule::new(
                "mod.a".into(),
                "A".into(),
                json!({"type": "object"}),
                json!({"type": "object"}),
                vec![],
                "axum::a".into(),
            ),
            ScannedModule::new(
                "mod.b".into(),
                "B".into(),
                json!({"type": "object"}),
                json!({"type": "object"}),
                vec![],
                "axum::b".into(),
            ),
        ];
        let results = writer.write(&modules, &mut registry, false, false);
        assert_eq!(results.len(), 2);
        assert!(registry.has("mod.a"));
        assert!(registry.has("mod.b"));
    }
}
