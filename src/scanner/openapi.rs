// OpenAPI-based scanner for Axum apps using utoipa.
//
// Scans routes by parsing a utoipa-generated OpenAPI spec, similar to
// how fastapi-apcore's OpenAPIScanner works with FastAPI's auto-generated spec.

use std::collections::HashMap;

use async_trait::async_trait;

use apcore_toolkit::{
    deduplicate_ids, extract_input_schema, extract_output_schema, filter_modules,
    infer_annotations_from_method, ScannedModule,
};

use crate::errors::AxumApcoreError;
use crate::scanner::AxumScanner;

/// Scanner that parses a utoipa-generated OpenAPI spec.
///
/// This requires the `openapi` feature and that the Axum app uses `utoipa`
/// to generate its OpenAPI documentation.
pub struct OpenAPIScanner {
    /// Whether to simplify module IDs (strip method suffixes, extract function names).
    pub simplify_ids: bool,
}

impl OpenAPIScanner {
    pub fn new() -> Self {
        Self { simplify_ids: true }
    }

    /// Create a scanner with explicit ID simplification setting.
    pub fn with_simplify_ids(simplify_ids: bool) -> Self {
        Self { simplify_ids }
    }

    /// Scan an OpenAPI spec JSON value and produce scanned modules.
    pub fn scan_spec(
        &self,
        spec: &serde_json::Value,
        include: Option<&str>,
        exclude: Option<&str>,
    ) -> Result<Vec<ScannedModule>, AxumApcoreError> {
        let paths = spec
            .get("paths")
            .and_then(|p| p.as_object())
            .ok_or_else(|| AxumApcoreError::Scanner("No 'paths' in OpenAPI spec".into()))?;

        let mut modules = Vec::new();

        for (path, path_item) in paths {
            let path_obj = match path_item.as_object() {
                Some(obj) => obj,
                None => continue,
            };

            for (method, operation) in path_obj {
                // Skip non-method keys like "parameters" or "summary"
                if !is_http_method(method) {
                    continue;
                }

                let module = self.operation_to_module(path, method, operation, spec);
                modules.push(module);
            }
        }

        let modules = deduplicate_ids(modules);
        let modules = filter_modules(&modules, include, exclude)?;

        Ok(modules)
    }

    /// Convert an OpenAPI operation to a ScannedModule.
    fn operation_to_module(
        &self,
        path: &str,
        method: &str,
        operation: &serde_json::Value,
        spec: &serde_json::Value,
    ) -> ScannedModule {
        let operation_id = operation
            .get("operationId")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let tags: Vec<String> = operation
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();

        let tag = tags.first().cloned().unwrap_or_else(|| "default".into());

        let func_name = if self.simplify_ids {
            strip_method_suffix(operation_id, method)
        } else {
            operation_id.to_string()
        };

        let module_id = format!("{}.{}.{}", tag, func_name, method.to_lowercase());

        let description = operation
            .get("summary")
            .or_else(|| operation.get("description"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let input_schema = extract_input_schema(operation, Some(spec));
        let output_schema = extract_output_schema(operation, Some(spec));
        let annotations = infer_annotations_from_method(method);

        let mut metadata = HashMap::new();
        metadata.insert(
            "http_method".into(),
            serde_json::Value::String(method.to_uppercase()),
        );
        metadata.insert(
            "url_path".into(),
            serde_json::Value::String(path.to_string()),
        );

        let documentation = operation
            .get("description")
            .and_then(|v| v.as_str())
            .map(String::from);

        ScannedModule {
            module_id,
            description,
            input_schema,
            output_schema,
            tags,
            target: format!("axum::{}", operation_id),
            version: "1.0.0".into(),
            annotations: Some(annotations),
            documentation,
            examples: vec![],
            metadata,
            warnings: vec![],
        }
    }
}

impl Default for OpenAPIScanner {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AxumScanner for OpenAPIScanner {
    async fn scan(
        &self,
        _app: &axum::Router,
        _include: Option<&str>,
        _exclude: Option<&str>,
    ) -> Result<Vec<ScannedModule>, AxumApcoreError> {
        // The OpenAPI spec must be provided externally; scanning from Router
        // is not supported because utoipa generates the spec at compile time.
        // Users should call `scan_spec()` directly with their generated spec.
        Err(AxumApcoreError::Scanner(
            "OpenAPIScanner requires a spec. Use scan_spec() directly with your \
             utoipa-generated OpenAPI JSON."
                .into(),
        ))
    }

    fn source_name(&self) -> &str {
        "openapi"
    }
}

/// Check if a string is a standard HTTP method.
fn is_http_method(s: &str) -> bool {
    matches!(
        s.to_lowercase().as_str(),
        "get" | "post" | "put" | "delete" | "patch" | "head" | "options" | "trace"
    )
}

/// Strip a method suffix from an operationId.
///
/// E.g., "get_user_get" with method "get" -> "get_user".
fn strip_method_suffix(operation_id: &str, method: &str) -> String {
    let suffix = format!("_{}", method.to_lowercase());
    if operation_id.ends_with(&suffix) {
        operation_id[..operation_id.len() - suffix.len()].to_string()
    } else {
        operation_id.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_spec() -> serde_json::Value {
        json!({
            "openapi": "3.1.0",
            "info": {"title": "Test API", "version": "1.0.0"},
            "paths": {
                "/api/users/{id}": {
                    "get": {
                        "operationId": "get_user_get",
                        "summary": "Get a user by ID",
                        "tags": ["users"],
                        "parameters": [{
                            "name": "id",
                            "in": "path",
                            "required": true,
                            "schema": {"type": "string"}
                        }],
                        "responses": {
                            "200": {
                                "description": "Success",
                                "content": {
                                    "application/json": {
                                        "schema": {
                                            "type": "object",
                                            "properties": {
                                                "name": {"type": "string"}
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
                "/api/tasks": {
                    "post": {
                        "operationId": "create_task_post",
                        "summary": "Create a task",
                        "tags": ["tasks"],
                        "requestBody": {
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "title": {"type": "string"}
                                        },
                                        "required": ["title"]
                                    }
                                }
                            }
                        },
                        "responses": {
                            "201": {
                                "description": "Created",
                                "content": {
                                    "application/json": {
                                        "schema": {
                                            "type": "object",
                                            "properties": {
                                                "id": {"type": "string"},
                                                "title": {"type": "string"}
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        })
    }

    #[test]
    fn test_scan_spec() {
        let scanner = OpenAPIScanner::new();
        let spec = sample_spec();
        let modules = scanner.scan_spec(&spec, None, None).unwrap();

        assert_eq!(modules.len(), 2);

        let get_mod = modules
            .iter()
            .find(|m| m.module_id.contains("get_user"))
            .unwrap();
        assert_eq!(get_mod.module_id, "users.get_user.get");
        assert_eq!(get_mod.description, "Get a user by ID");
        assert!(get_mod.annotations.as_ref().unwrap().readonly);

        let post_mod = modules
            .iter()
            .find(|m| m.module_id.contains("create_task"))
            .unwrap();
        assert_eq!(post_mod.module_id, "tasks.create_task.post");
    }

    #[test]
    fn test_scan_spec_with_filter() {
        let scanner = OpenAPIScanner::new();
        let spec = sample_spec();
        let modules = scanner.scan_spec(&spec, Some("users"), None).unwrap();
        assert_eq!(modules.len(), 1);
        assert!(modules[0].module_id.contains("users"));
    }

    #[test]
    fn test_strip_method_suffix() {
        assert_eq!(strip_method_suffix("get_user_get", "get"), "get_user");
        assert_eq!(
            strip_method_suffix("create_task_post", "post"),
            "create_task"
        );
        assert_eq!(strip_method_suffix("get_user", "get"), "get_user");
    }

    #[test]
    fn test_is_http_method() {
        assert!(is_http_method("get"));
        assert!(is_http_method("POST"));
        assert!(is_http_method("delete"));
        assert!(!is_http_method("parameters"));
        assert!(!is_http_method("summary"));
    }

    #[test]
    fn test_scan_spec_no_simplify() {
        let scanner = OpenAPIScanner::with_simplify_ids(false);
        let spec = sample_spec();
        let modules = scanner.scan_spec(&spec, None, None).unwrap();

        let get_mod = modules
            .iter()
            .find(|m| m.module_id.contains("get_user"))
            .unwrap();
        assert_eq!(get_mod.module_id, "users.get_user_get.get");
    }

    #[test]
    fn test_scan_spec_empty() {
        let scanner = OpenAPIScanner::new();
        let spec = json!({"openapi": "3.1.0", "paths": {}});
        let modules = scanner.scan_spec(&spec, None, None).unwrap();
        assert!(modules.is_empty());
    }

    #[test]
    fn test_scan_spec_no_paths() {
        let scanner = OpenAPIScanner::new();
        let spec = json!({"openapi": "3.1.0"});
        let result = scanner.scan_spec(&spec, None, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_metadata_stored() {
        let scanner = OpenAPIScanner::new();
        let spec = sample_spec();
        let modules = scanner.scan_spec(&spec, None, None).unwrap();
        let m = &modules[0];
        assert!(m.metadata.contains_key("http_method"));
        assert!(m.metadata.contains_key("url_path"));
    }
}
