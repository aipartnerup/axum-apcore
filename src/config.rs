// Configuration system for axum-apcore.
//
// All settings are read from environment variables with the APCORE_ prefix.
// Mirrors the configuration pattern from fastapi-apcore.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// Global singleton for settings.
static SETTINGS: OnceLock<ApcoreSettings> = OnceLock::new();

/// Configuration for axum-apcore, populated from APCORE_* environment variables.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApcoreSettings {
    // -- Core --
    /// Directory for YAML module binding files.
    pub module_dir: PathBuf,
    /// Whether to auto-discover modules on startup.
    pub auto_discover: bool,
    /// Glob pattern for binding files (e.g., "*.binding.yaml").
    pub binding_pattern: String,
    /// Comma-separated list of Rust module paths to scan for `@module` functions.
    pub module_packages: Vec<String>,

    // -- MCP Server --
    /// MCP transport: "stdio", "streamable-http", or "sse".
    pub serve_transport: String,
    /// Host to bind the MCP server to.
    pub serve_host: String,
    /// Port for the MCP server.
    pub serve_port: u16,
    /// Server name advertised in MCP.
    pub server_name: String,
    /// URL prefix for the MCP explorer UI.
    pub explorer_prefix: String,
    /// Whether to enable the MCP explorer UI.
    pub explorer_enabled: bool,

    // -- Security --
    /// JWT secret for authenticating MCP requests.
    pub jwt_secret: Option<String>,
    /// JWT algorithm (e.g., "HS256").
    pub jwt_algorithm: String,
    /// Path to an ACL YAML file.
    pub acl_path: Option<String>,

    // -- Observability --
    /// Enable tracing (JSON or dict config).
    pub tracing: bool,
    /// Enable metrics collection.
    pub metrics: bool,
    /// Enable structured logging middleware.
    pub observability_logging: bool,
    /// Whether to start an embedded MCP server automatically.
    pub embedded_server: bool,

    // -- Tasks --
    /// Maximum concurrent async tasks.
    pub task_max_concurrent: usize,
    /// Maximum total tasks in the queue.
    pub task_max_tasks: usize,
    /// Cleanup age for completed tasks (seconds).
    pub task_cleanup_age: u64,

    // -- Scanner --
    /// Default scanner source: "native" or "openapi".
    pub scanner_source: String,

    // -- Advanced --
    /// Whether to enable hot-reload for module bindings.
    pub hot_reload: bool,
    /// Custom output formatter dotted path.
    pub output_formatter: Option<String>,
    /// Arbitrary extra settings.
    pub extra: HashMap<String, serde_json::Value>,
}

impl Default for ApcoreSettings {
    fn default() -> Self {
        Self {
            module_dir: PathBuf::from("apcore_modules"),
            auto_discover: true,
            binding_pattern: "*.binding.yaml".into(),
            module_packages: vec![],
            serve_transport: "streamable-http".into(),
            serve_host: "127.0.0.1".into(),
            serve_port: 9090,
            server_name: "axum-apcore".into(),
            explorer_prefix: "/explorer".into(),
            explorer_enabled: true,
            jwt_secret: None,
            jwt_algorithm: "HS256".into(),
            acl_path: None,
            tracing: false,
            metrics: false,
            observability_logging: false,
            embedded_server: false,
            task_max_concurrent: 10,
            task_max_tasks: 100,
            task_cleanup_age: 3600,
            scanner_source: "native".into(),
            hot_reload: false,
            output_formatter: None,
            extra: HashMap::new(),
        }
    }
}

impl ApcoreSettings {
    /// Load settings from environment variables with the APCORE_ prefix.
    pub fn from_env() -> Self {
        let mut settings = Self::default();

        if let Ok(v) = std::env::var("APCORE_MODULE_DIR") {
            settings.module_dir = PathBuf::from(v);
        }
        if let Ok(v) = std::env::var("APCORE_AUTO_DISCOVER") {
            settings.auto_discover = parse_bool(&v);
        }
        if let Ok(v) = std::env::var("APCORE_BINDING_PATTERN") {
            settings.binding_pattern = v;
        }
        if let Ok(v) = std::env::var("APCORE_MODULE_PACKAGES") {
            settings.module_packages = v.split(',').map(|s| s.trim().to_string()).collect();
        }

        // MCP Server
        if let Ok(v) = std::env::var("APCORE_SERVE_TRANSPORT") {
            settings.serve_transport = v;
        }
        if let Ok(v) = std::env::var("APCORE_SERVE_HOST") {
            settings.serve_host = v;
        }
        if let Ok(v) = std::env::var("APCORE_SERVE_PORT") {
            if let Ok(port) = v.parse() {
                settings.serve_port = port;
            }
        }
        if let Ok(v) = std::env::var("APCORE_SERVER_NAME") {
            settings.server_name = v;
        }
        if let Ok(v) = std::env::var("APCORE_EXPLORER_PREFIX") {
            settings.explorer_prefix = v;
        }
        if let Ok(v) = std::env::var("APCORE_EXPLORER_ENABLED") {
            settings.explorer_enabled = parse_bool(&v);
        }

        // Security
        settings.jwt_secret = std::env::var("APCORE_JWT_SECRET").ok();
        if let Ok(v) = std::env::var("APCORE_JWT_ALGORITHM") {
            settings.jwt_algorithm = v;
        }
        settings.acl_path = std::env::var("APCORE_ACL_PATH").ok();

        // Observability
        if let Ok(v) = std::env::var("APCORE_TRACING") {
            settings.tracing = parse_bool(&v);
        }
        if let Ok(v) = std::env::var("APCORE_METRICS") {
            settings.metrics = parse_bool(&v);
        }
        if let Ok(v) = std::env::var("APCORE_OBSERVABILITY_LOGGING") {
            settings.observability_logging = parse_bool(&v);
        }
        if let Ok(v) = std::env::var("APCORE_EMBEDDED_SERVER") {
            settings.embedded_server = parse_bool(&v);
        }

        // Tasks
        if let Ok(v) = std::env::var("APCORE_TASK_MAX_CONCURRENT") {
            if let Ok(n) = v.parse() {
                settings.task_max_concurrent = n;
            }
        }
        if let Ok(v) = std::env::var("APCORE_TASK_MAX_TASKS") {
            if let Ok(n) = v.parse() {
                settings.task_max_tasks = n;
            }
        }
        if let Ok(v) = std::env::var("APCORE_TASK_CLEANUP_AGE") {
            if let Ok(n) = v.parse() {
                settings.task_cleanup_age = n;
            }
        }

        // Scanner
        if let Ok(v) = std::env::var("APCORE_SCANNER_SOURCE") {
            settings.scanner_source = v;
        }

        // Advanced
        if let Ok(v) = std::env::var("APCORE_HOT_RELOAD") {
            settings.hot_reload = parse_bool(&v);
        }
        settings.output_formatter = std::env::var("APCORE_OUTPUT_FORMATTER").ok();

        settings
    }

    /// Validate settings and return errors if any.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        let valid_transports = ["stdio", "streamable-http", "sse"];
        if !valid_transports.contains(&self.serve_transport.as_str()) {
            errors.push(format!(
                "Invalid APCORE_SERVE_TRANSPORT: '{}'. Must be one of: {}",
                self.serve_transport,
                valid_transports.join(", ")
            ));
        }

        let valid_sources = ["native", "openapi"];
        if !valid_sources.contains(&self.scanner_source.as_str()) {
            errors.push(format!(
                "Invalid APCORE_SCANNER_SOURCE: '{}'. Must be one of: {}",
                self.scanner_source,
                valid_sources.join(", ")
            ));
        }

        if self.serve_port == 0 {
            errors.push("APCORE_SERVE_PORT must be > 0".into());
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// Get or initialize the global settings singleton.
pub fn get_apcore_settings() -> &'static ApcoreSettings {
    SETTINGS.get_or_init(|| {
        let settings = ApcoreSettings::from_env();
        if let Err(errors) = settings.validate() {
            tracing::warn!("ApcoreSettings validation warnings: {:?}", errors);
        }
        settings
    })
}

fn parse_bool(s: &str) -> bool {
    matches!(s.to_lowercase().as_str(), "1" | "true" | "yes" | "on")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings() {
        let s = ApcoreSettings::default();
        assert!(s.auto_discover);
        assert_eq!(s.serve_port, 9090);
        assert_eq!(s.serve_transport, "streamable-http");
        assert_eq!(s.scanner_source, "native");
    }

    #[test]
    fn test_validate_valid() {
        let s = ApcoreSettings::default();
        assert!(s.validate().is_ok());
    }

    #[test]
    fn test_validate_invalid_transport() {
        let s = ApcoreSettings {
            serve_transport: "invalid".into(),
            ..ApcoreSettings::default()
        };
        let err = s.validate().unwrap_err();
        assert!(err[0].contains("APCORE_SERVE_TRANSPORT"));
    }

    #[test]
    fn test_validate_invalid_scanner() {
        let s = ApcoreSettings {
            scanner_source: "bad".into(),
            ..ApcoreSettings::default()
        };
        let err = s.validate().unwrap_err();
        assert!(err[0].contains("APCORE_SCANNER_SOURCE"));
    }

    #[test]
    fn test_parse_bool() {
        assert!(parse_bool("true"));
        assert!(parse_bool("1"));
        assert!(parse_bool("yes"));
        assert!(parse_bool("on"));
        assert!(parse_bool("TRUE"));
        assert!(!parse_bool("false"));
        assert!(!parse_bool("0"));
        assert!(!parse_bool(""));
    }
}
