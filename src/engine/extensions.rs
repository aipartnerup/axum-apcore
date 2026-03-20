// Extension adapters for apcore — Discoverer and ModuleValidator implementations.
//
// Follows the same protocol-adaptation pattern as fastapi-apcore.

use apcore::module::ModuleAnnotations;

use crate::config::ApcoreSettings;
use crate::errors::AxumApcoreError;

/// Maximum allowed module ID length.
const MAX_MODULE_ID_LENGTH: usize = 256;

/// Reserved words that cannot be used as module IDs.
const RESERVED_WORDS: &[&str] = &["__init__", "__main__", "apcore", "system"];

/// Discovers apcore modules from YAML binding files and registered packages.
///
/// Implements the apcore Discoverer protocol for Axum applications.
pub struct AxumDiscoverer {
    settings: ApcoreSettings,
}

impl AxumDiscoverer {
    pub fn new(settings: ApcoreSettings) -> Self {
        Self { settings }
    }

    /// Discover modules from binding files in the configured module directory.
    pub fn discover(&self) -> Result<Vec<DiscoveredModule>, AxumApcoreError> {
        let mut modules = Vec::new();

        let module_dir = &self.settings.module_dir;
        if !module_dir.exists() {
            tracing::debug!(
                path = %module_dir.display(),
                "Module directory does not exist, skipping discovery"
            );
            return Ok(modules);
        }

        let pattern = module_dir.join(&self.settings.binding_pattern);
        let pattern_str = pattern.to_string_lossy();

        let entries = glob_binding_files(&pattern_str);
        for path in entries {
            match load_binding_file(&path) {
                Ok(mut discovered) => modules.append(&mut discovered),
                Err(e) => {
                    tracing::warn!(path = %path, error = %e, "Failed to load binding file");
                }
            }
        }

        tracing::info!(count = modules.len(), "Discovered modules from bindings");
        Ok(modules)
    }
}

/// A module discovered from binding files.
#[derive(Debug, Clone)]
pub struct DiscoveredModule {
    pub module_id: String,
    pub target: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub output_schema: serde_json::Value,
    pub tags: Vec<String>,
    pub annotations: ModuleAnnotations,
}

/// Validates module IDs against apcore constraints.
pub struct AxumModuleValidator;

impl AxumModuleValidator {
    pub fn new() -> Self {
        Self
    }

    /// Validate a module ID. Returns a list of validation errors.
    pub fn validate(&self, module_id: &str) -> Vec<String> {
        let mut errors = Vec::new();

        if module_id.is_empty() {
            errors.push("Module ID cannot be empty".into());
            return errors;
        }

        if module_id.len() > MAX_MODULE_ID_LENGTH {
            errors.push(format!(
                "Module ID '{}' exceeds maximum length of {}",
                module_id, MAX_MODULE_ID_LENGTH
            ));
        }

        if RESERVED_WORDS.contains(&module_id) {
            errors.push(format!("Module ID '{}' is a reserved word", module_id));
        }

        // Module IDs must be dot-separated alphanumeric segments
        for segment in module_id.split('.') {
            if segment.is_empty() {
                errors.push(format!(
                    "Module ID '{}' contains empty segment (double dot)",
                    module_id
                ));
            }
        }

        errors
    }
}

impl Default for AxumModuleValidator {
    fn default() -> Self {
        Self::new()
    }
}

/// Glob for binding files matching the given pattern.
fn glob_binding_files(pattern: &str) -> Vec<String> {
    // Simple glob implementation using std::fs
    let dir = std::path::Path::new(pattern)
        .parent()
        .unwrap_or(std::path::Path::new("."));

    let extension_pattern = std::path::Path::new(pattern)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_default();

    let suffix = extension_pattern
        .strip_prefix('*')
        .unwrap_or(&extension_pattern);

    let mut results = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(suffix) {
                results.push(entry.path().to_string_lossy().to_string());
            }
        }
    }
    results
}

/// Load modules from a YAML binding file.
fn load_binding_file(path: &str) -> Result<Vec<DiscoveredModule>, AxumApcoreError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| AxumApcoreError::Config(format!("Failed to read {}: {}", path, e)))?;

    let value: serde_json::Value = serde_yaml::from_str(&content)
        .map_err(|e| AxumApcoreError::Config(format!("Failed to parse {}: {}", path, e)))?;

    let modules_value = value
        .get("modules")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            AxumApcoreError::Config(format!("No 'modules' array in binding file: {}", path))
        })?;

    let mut modules = Vec::new();
    for module_val in modules_value {
        let module_id = module_val
            .get("module_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let target = module_val
            .get("target")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let description = module_val
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let input_schema = module_val
            .get("input_schema")
            .cloned()
            .unwrap_or(serde_json::json!({"type": "object"}));

        let output_schema = module_val
            .get("output_schema")
            .cloned()
            .unwrap_or(serde_json::json!({"type": "object"}));

        let tags = module_val
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();

        modules.push(DiscoveredModule {
            module_id,
            target,
            description,
            input_schema,
            output_schema,
            tags,
            annotations: ModuleAnnotations::default(),
        });
    }

    Ok(modules)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validator_valid_id() {
        let v = AxumModuleValidator::new();
        assert!(v.validate("users.get_user.get").is_empty());
    }

    #[test]
    fn test_validator_empty_id() {
        let v = AxumModuleValidator::new();
        let errors = v.validate("");
        assert!(!errors.is_empty());
        assert!(errors[0].contains("empty"));
    }

    #[test]
    fn test_validator_reserved_word() {
        let v = AxumModuleValidator::new();
        let errors = v.validate("apcore");
        assert!(!errors.is_empty());
        assert!(errors[0].contains("reserved"));
    }

    #[test]
    fn test_validator_too_long() {
        let v = AxumModuleValidator::new();
        let long_id = "a".repeat(300);
        let errors = v.validate(&long_id);
        assert!(!errors.is_empty());
        assert!(errors[0].contains("exceeds"));
    }

    #[test]
    fn test_validator_double_dot() {
        let v = AxumModuleValidator::new();
        let errors = v.validate("users..get");
        assert!(!errors.is_empty());
        assert!(errors[0].contains("empty segment"));
    }

    #[test]
    fn test_load_binding_file_missing() {
        let result = load_binding_file("/nonexistent/file.yaml");
        assert!(result.is_err());
    }
}
