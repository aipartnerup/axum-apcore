// Integration tests for axum-apcore.
//
// Tests the end-to-end flow: metadata registration → scanning → module
// registration → execution, plus task management and context extraction.
//
// NOTE: Tests share global singletons (ROUTE_REGISTRY, REGISTRY, EXECUTOR).
// Each test uses unique handler names to avoid interference.

use axum::http::Request;
use axum::Router;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

use axum_apcore::scanner::native::{register_route, RouteMetadata};
use axum_apcore::{
    ApcoreSettings, AxumApcore, Context, Identity, ModuleError, NativeAxumScanner, RequestIdentity,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn test_settings() -> ApcoreSettings {
    ApcoreSettings {
        auto_discover: false,
        ..ApcoreSettings::default()
    }
}

fn make_route(method: &str, path: &str, handler: &str, desc: &str) -> RouteMetadata {
    RouteMetadata {
        method: method.into(),
        path: path.into(),
        handler_name: handler.into(),
        description: desc.into(),
        tags: vec!["inttest".into()],
        input_schema: json!({"type": "object", "properties": {"id": {"type": "string"}}}),
        output_schema: json!({"type": "object", "properties": {"result": {"type": "string"}}}),
        documentation: None,
    }
}

async fn echo_handler(input: Value, _ctx: &Context<Value>) -> Result<Value, ModuleError> {
    Ok(json!({"echo": input}))
}

async fn ctx_echo_handler(input: Value, ctx: &Context<Value>) -> Result<Value, ModuleError> {
    Ok(json!({
        "caller": ctx.identity.id,
        "input": input,
    }))
}

async fn slow_handler(input: Value, ctx: &Context<Value>) -> Result<Value, ModuleError> {
    let ms = input["ms"].as_u64().unwrap_or(500);
    for _ in 0..ms / 10 {
        if let Some(token) = &ctx.cancel_token {
            if token.is_cancelled() {
                return Err(ModuleError::new(
                    axum_apcore::ErrorCode::ExecutionCancelled,
                    "cancelled".to_string(),
                ));
            }
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Ok(json!({"slept_ms": ms}))
}

// ---------------------------------------------------------------------------
// End-to-end: register → scan → call
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_e2e_register_scan_call() {
    register_route(make_route(
        "GET",
        "/api/e2e_items/:id",
        "e2e_get_item",
        "Get item (e2e)",
    ));

    let apcore = AxumApcore::with_settings(test_settings());
    apcore.register_handler(
        "axum::e2e_get_item",
        Arc::new(|input, ctx| Box::pin(echo_handler(input, ctx))),
    );

    let router = Router::new();
    apcore.init_app(&router).await.unwrap();

    let modules = apcore.list_modules();
    assert!(
        modules.iter().any(|m| m.contains("e2e_get_item")),
        "Expected e2e_get_item module, got: {modules:?}"
    );

    let result = apcore
        .call_anonymous("inttest.e2e_get_item.get", json!({"id": "42"}))
        .await
        .unwrap();

    assert_eq!(result["echo"]["id"], "42");
}

// ---------------------------------------------------------------------------
// Call with explicit context
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_call_with_identity_context() {
    register_route(make_route(
        "GET",
        "/api/ctx_test",
        "ctx_test_handler",
        "Context test",
    ));

    let apcore = AxumApcore::with_settings(test_settings());
    apcore.register_handler(
        "axum::ctx_test_handler",
        Arc::new(|input, ctx| Box::pin(ctx_echo_handler(input, ctx))),
    );

    let router = Router::new();
    apcore.init_app(&router).await.unwrap();

    let ctx = Context::new(Identity {
        id: "admin-ctx".into(),
        identity_type: "admin".into(),
        roles: vec!["admin".into()],
        attrs: Default::default(),
    });

    let result = apcore
        .call(
            "inttest.ctx_test_handler.get",
            json!({"key": "val"}),
            Some(&ctx),
        )
        .await
        .unwrap();

    assert_eq!(result["caller"], "admin-ctx");
    assert_eq!(result["input"]["key"], "val");
}

// ---------------------------------------------------------------------------
// Cancellable call — timeout
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_cancellable_call_timeout() {
    register_route(make_route(
        "POST",
        "/api/slow_cancel",
        "slow_cancel_op",
        "Slow cancel op",
    ));

    let apcore = AxumApcore::with_settings(test_settings());
    apcore.register_handler(
        "axum::slow_cancel_op",
        Arc::new(|input, ctx| Box::pin(slow_handler(input, ctx))),
    );

    let router = Router::new();
    apcore.init_app(&router).await.unwrap();

    let result = apcore
        .cancellable_call(
            "inttest.slow_cancel_op.post",
            json!({"ms": 5000}),
            None,
            Duration::from_millis(100),
        )
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("timed out"),
        "Expected timeout error, got: {err}"
    );
}

// ---------------------------------------------------------------------------
// Task management: submit → complete → result
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_task_submit_complete_and_result() {
    register_route(make_route(
        "POST",
        "/api/task_echo",
        "task_echo_op",
        "Task echo",
    ));

    let apcore = AxumApcore::with_settings(test_settings());
    apcore.register_handler(
        "axum::task_echo_op",
        Arc::new(|input, ctx| Box::pin(echo_handler(input, ctx))),
    );

    let router = Router::new();
    apcore.init_app(&router).await.unwrap();

    let task_id = apcore
        .submit_task("inttest.task_echo_op.post", json!({"data": "hello"}))
        .unwrap();

    // Poll until complete (max 2s)
    let mut completed = false;
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let Some(info) = apcore.get_task_status(&task_id) {
            if info.status != "Running" {
                completed = true;
                break;
            }
        }
    }
    assert!(completed, "Task did not complete within 2s");

    let result = apcore.get_task_result(&task_id);
    assert!(result.is_some(), "Task result should be available");
    assert_eq!(result.unwrap()["echo"]["data"], "hello");
}

// ---------------------------------------------------------------------------
// Task management: cancel
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_task_cancel() {
    register_route(make_route(
        "POST",
        "/api/task_slow",
        "task_slow_op",
        "Slow task",
    ));

    let apcore = AxumApcore::with_settings(test_settings());
    apcore.register_handler(
        "axum::task_slow_op",
        Arc::new(|input, ctx| Box::pin(slow_handler(input, ctx))),
    );

    let router = Router::new();
    apcore.init_app(&router).await.unwrap();

    let task_id = apcore
        .submit_task("inttest.task_slow_op.post", json!({"ms": 5000}))
        .unwrap();

    // Give it time to start
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        apcore.cancel_task(&task_id),
        "Should be able to cancel a running task"
    );

    let info = apcore.get_task_status(&task_id).unwrap();
    assert_eq!(info.status, "Cancelled");
}

// ---------------------------------------------------------------------------
// Scanner: include/exclude filters (no global state)
// ---------------------------------------------------------------------------

#[test]
fn test_scanner_include_exclude() {
    let scanner = NativeAxumScanner::new();
    let routes = vec![
        make_route("GET", "/api/users/:id", "scan_get_user", "Get user"),
        make_route("POST", "/api/tasks", "scan_create_task", "Create task"),
        make_route(
            "DELETE",
            "/api/users/:id",
            "scan_delete_user",
            "Delete user",
        ),
    ];

    // Include only users
    let filtered = scanner.scan_routes(&routes, Some("user"), None).unwrap();
    assert_eq!(filtered.len(), 2);

    // Exclude delete
    let filtered = scanner.scan_routes(&routes, None, Some("delete")).unwrap();
    assert_eq!(filtered.len(), 2);

    // Include users + exclude delete
    let filtered = scanner
        .scan_routes(&routes, Some("user"), Some("delete"))
        .unwrap();
    assert_eq!(filtered.len(), 1);
    assert!(filtered[0].module_id.contains("scan_get_user"));
}

// ---------------------------------------------------------------------------
// Context extraction from request parts
// ---------------------------------------------------------------------------

#[test]
fn test_context_factory_with_request_identity() {
    let factory = axum_apcore::AxumContextFactory;

    let mut req = Request::builder().body(()).unwrap();
    req.extensions_mut().insert(RequestIdentity {
        id: "svc-1".into(),
        identity_type: "service".into(),
        roles: vec!["reader".into(), "writer".into()],
        attrs: Default::default(),
    });
    let (parts, _) = req.into_parts();

    let ctx = factory.create_from_parts(&parts).unwrap();
    assert_eq!(ctx.identity.id, "svc-1");
    assert_eq!(ctx.identity.identity_type, "service");
    assert_eq!(ctx.identity.roles.len(), 2);
}

#[test]
fn test_context_factory_anonymous_fallback() {
    let factory = axum_apcore::AxumContextFactory;

    let req = Request::builder().body(()).unwrap();
    let (parts, _) = req.into_parts();

    let ctx = factory.create_from_parts(&parts).unwrap();
    assert_eq!(ctx.identity.id, "anonymous");
}

// ---------------------------------------------------------------------------
// OpenAPI scanner (feature-gated)
// ---------------------------------------------------------------------------

#[cfg(feature = "openapi")]
#[test]
fn test_openapi_scanner_end_to_end() {
    let scanner = axum_apcore::OpenAPIScanner::new();
    let spec = json!({
        "openapi": "3.1.0",
        "info": {"title": "Test", "version": "1.0.0"},
        "paths": {
            "/items": {
                "get": {
                    "operationId": "list_items_get",
                    "summary": "List items",
                    "tags": ["items"],
                    "responses": {"200": {"description": "OK"}}
                }
            }
        }
    });

    let modules = scanner.scan_spec(&spec, None, None).unwrap();
    assert_eq!(modules.len(), 1);
    assert_eq!(modules[0].module_id, "items.list_items.get");
    assert!(modules[0].annotations.as_ref().unwrap().readonly);
}

// ---------------------------------------------------------------------------
// Settings validation
// ---------------------------------------------------------------------------

#[test]
fn test_settings_validation_catches_invalid() {
    let settings = ApcoreSettings {
        serve_transport: "grpc".into(),
        scanner_source: "magic".into(),
        serve_port: 0,
        ..ApcoreSettings::default()
    };
    let errors = settings.validate().unwrap_err();
    assert_eq!(errors.len(), 3);
}
