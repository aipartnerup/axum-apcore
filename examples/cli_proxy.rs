// Example: HTTP proxy CLI using apcore-cli integration.
//
// Scans Axum routes and generates a CLI where each command forwards
// requests to the running API via HTTP proxy.
//
// Usage:
//   cargo run --example cli_proxy --features cli -- list
//   cargo run --example cli_proxy --features cli -- describe users.get_user.get
//   cargo run --example cli_proxy --features cli -- completion bash

use axum::{routing::get, Json, Router};
use serde_json::{json, Value};

use axum_apcore::scanner::native::{register_route, RouteMetadata};
use axum_apcore::{AxumApcore, CreateCliConfig};

async fn get_user(Json(input): Json<Value>) -> Json<Value> {
    let id = input
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    Json(json!({"user_id": id, "name": "Alice"}))
}

async fn list_users() -> Json<Value> {
    Json(json!({"users": [{"id": "1", "name": "Alice"}, {"id": "2", "name": "Bob"}]}))
}

#[tokio::main]
async fn main() {
    // Register route metadata for the native scanner
    register_route(RouteMetadata {
        method: "GET".into(),
        path: "/api/users/:id".into(),
        handler_name: "get_user".into(),
        description: "Get a user by ID".into(),
        tags: vec!["users".into()],
        input_schema: json!({"type": "object", "properties": {"id": {"type": "string"}}}),
        output_schema: json!({"type": "object", "properties": {"user_id": {"type": "string"}, "name": {"type": "string"}}}),
        documentation: None,
    });

    register_route(RouteMetadata {
        method: "GET".into(),
        path: "/api/users".into(),
        handler_name: "list_users".into(),
        description: "List all users".into(),
        tags: vec!["users".into()],
        input_schema: json!({"type": "object"}),
        output_schema: json!({"type": "object"}),
        documentation: None,
    });

    let router = Router::new()
        .route("/api/users/:id", get(get_user))
        .route("/api/users", get(list_users));

    let apcore = AxumApcore::new();

    let config = CreateCliConfig {
        prog_name: "myapp-cli".into(),
        base_url: "http://localhost:3000".into(),
        ..Default::default()
    };

    let mut cmd = apcore
        .create_cli(&router, config)
        .await
        .expect("Failed to create CLI");

    // In a real app, you'd parse args and dispatch.
    // Here we just print the help to show the generated commands.
    let _ = cmd.print_help();
}
