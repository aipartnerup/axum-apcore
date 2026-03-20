// Basic example: Axum app with apcore module scanning.
//
// Demonstrates:
// 1. Registering route metadata via `ap_handler!`
// 2. Initializing AxumApcore
// 3. Using the ApContext extractor in handlers
// 4. Executing modules programmatically

use axum::{extract::State, routing::get, Json, Router};
use serde_json::{json, Value};
use std::sync::Arc;

use axum_apcore::{ap_handler, ApContext, AxumApcore};

// -- Handlers --

async fn get_user(
    State(_apcore): State<Arc<AxumApcore>>,
    ApContext(ctx): ApContext,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Json<Value> {
    tracing::info!(trace_id = %ctx.trace_id, user_id = %id, "Handling get_user");
    Json(json!({
        "id": id,
        "name": "Alice",
        "trace_id": ctx.trace_id,
    }))
}

async fn list_users() -> Json<Value> {
    Json(json!({
        "users": [
            {"id": "1", "name": "Alice"},
            {"id": "2", "name": "Bob"},
        ]
    }))
}

async fn create_user(Json(body): Json<Value>) -> Json<Value> {
    Json(json!({
        "id": "3",
        "name": body.get("name").and_then(|v| v.as_str()).unwrap_or("unknown"),
        "created": true,
    }))
}

async fn health() -> Json<Value> {
    Json(json!({"status": "ok"}))
}

// -- App setup --

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // Register route metadata for scanning
    ap_handler! {
        method: "GET",
        path: "/api/users/:id",
        handler: get_user,
        description: "Get a user by ID",
        tags: ["users"],
        input_schema: json!({"type": "object", "properties": {"id": {"type": "string"}}, "required": ["id"]}),
        output_schema: json!({"type": "object", "properties": {"id": {"type": "string"}, "name": {"type": "string"}}}),
    }

    ap_handler! {
        method: "GET",
        path: "/api/users",
        handler: list_users,
        description: "List all users",
        tags: ["users"],
        input_schema: json!({"type": "object"}),
        output_schema: json!({"type": "object", "properties": {"users": {"type": "array"}}}),
    }

    ap_handler! {
        method: "POST",
        path: "/api/users",
        handler: create_user,
        description: "Create a new user",
        tags: ["users"],
        input_schema: json!({"type": "object", "properties": {"name": {"type": "string"}}, "required": ["name"]}),
        output_schema: json!({"type": "object", "properties": {"id": {"type": "string"}, "created": {"type": "boolean"}}}),
    }

    // Create the AxumApcore instance
    let apcore = Arc::new(AxumApcore::new());

    // Build the Axum router
    let router = Router::new()
        .route("/api/users/{id}", get(get_user))
        .route("/api/users", get(list_users).post(create_user))
        .route("/health", get(health))
        .with_state(apcore.clone());

    // Initialize apcore (scan and register modules)
    apcore
        .init_app(&router)
        .await
        .expect("Failed to init apcore");

    // List discovered modules
    let modules = apcore.list_modules();
    tracing::info!(?modules, "Registered modules");

    // Start the server
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    tracing::info!("Listening on http://0.0.0.0:3000");
    axum::serve(listener, router).await.unwrap();
}
