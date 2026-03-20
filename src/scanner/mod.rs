// Scanner module — discover Axum routes as apcore modules.
//
// Provides a native scanner for direct Router introspection and
// an optional OpenAPI scanner for utoipa-based apps.

pub mod native;

#[cfg(feature = "openapi")]
pub mod openapi;

use apcore_toolkit::ScannedModule;
use async_trait::async_trait;

/// Trait for Axum route scanners.
///
/// This is a specialized wrapper around `apcore_toolkit::Scanner<axum::Router>`
/// that adds include/exclude filter support.
#[async_trait]
pub trait AxumScanner: Send + Sync {
    /// Scan routes and return module definitions.
    async fn scan(
        &self,
        app: &axum::Router,
        include: Option<&str>,
        exclude: Option<&str>,
    ) -> Result<Vec<ScannedModule>, crate::errors::AxumApcoreError>;

    /// Human-readable scanner name.
    fn source_name(&self) -> &str;
}

/// Factory function to get a scanner by source name.
pub fn get_scanner(source: &str) -> Result<Box<dyn AxumScanner>, crate::errors::AxumApcoreError> {
    match source {
        "native" => Ok(Box::new(native::NativeAxumScanner::new())),
        #[cfg(feature = "openapi")]
        "openapi" => Ok(Box::new(openapi::OpenAPIScanner::new())),
        other => Err(crate::errors::AxumApcoreError::Scanner(format!(
            "Unknown scanner source: '{}'. Available: native{}",
            other,
            if cfg!(feature = "openapi") {
                ", openapi"
            } else {
                ""
            }
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_scanner_native() {
        let scanner = get_scanner("native").unwrap();
        assert_eq!(scanner.source_name(), "native");
    }

    #[test]
    fn test_get_scanner_unknown() {
        let result = get_scanner("unknown");
        assert!(result.is_err());
    }
}
