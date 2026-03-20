//! Handler registration example — register executable handlers and call modules.
//!
//! Demonstrates:
//! 1. Registering route metadata via `ap_handler!`
//! 2. Registering callable handler functions via `register_handler`
//! 3. Executing modules programmatically via `call()` and `call_anonymous()`

use axum::Router;
use serde_json::{json, Value};
use std::sync::Arc;

use axum_apcore::{ap_handler, AxumApcore, Context, Identity, ModuleError};

// ---------------------------------------------------------------------------
// Handler functions
// ---------------------------------------------------------------------------

async fn get_user_handler(input: Value, _ctx: &Context<Value>) -> Result<Value, ModuleError> {
    let user_id = input["id"].as_str().unwrap_or("unknown");

    // Simulate a database lookup
    let user = match user_id {
        "1" => json!({"id": "1", "name": "Alice", "email": "alice@example.com"}),
        "2" => json!({"id": "2", "name": "Bob", "email": "bob@example.com"}),
        _ => json!({"id": user_id, "name": "Unknown", "email": "unknown@example.com"}),
    };

    Ok(user)
}

async fn create_user_handler(input: Value, _ctx: &Context<Value>) -> Result<Value, ModuleError> {
    let name = input["name"].as_str().unwrap_or("unknown");

    Ok(json!({
        "id": "3",
        "name": name,
        "created": true,
    }))
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // Step 1: Register route metadata
    ap_handler! {
        method: "GET",
        path: "/api/users/:id",
        handler: get_user,
        description: "Get a user by ID",
        tags: ["users"],
        input_schema: json!({
            "type": "object",
            "properties": {"id": {"type": "string"}},
            "required": ["id"]
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "id": {"type": "string"},
                "name": {"type": "string"},
                "email": {"type": "string"}
            }
        }),
    }

    ap_handler! {
        method: "POST",
        path: "/api/users",
        handler: create_user,
        description: "Create a new user",
        tags: ["users"],
        input_schema: json!({
            "type": "object",
            "properties": {"name": {"type": "string"}},
            "required": ["name"]
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "id": {"type": "string"},
                "name": {"type": "string"},
                "created": {"type": "boolean"}
            }
        }),
    }

    // Step 2: Create AxumApcore and register executable handlers
    let apcore = Arc::new(AxumApcore::new());

    // Map target strings to actual handler functions
    apcore.register_handler(
        "axum::get_user",
        Arc::new(|input, ctx| Box::pin(get_user_handler(input, ctx))),
    );
    apcore.register_handler(
        "axum::create_user",
        Arc::new(|input, ctx| Box::pin(create_user_handler(input, ctx))),
    );

    // Step 3: Initialize (scan + register modules)
    let router = Router::new();
    apcore
        .init_app(&router)
        .await
        .expect("Failed to init apcore");

    // Step 4: List registered modules
    let modules = apcore.list_modules();
    println!("=== Registered modules ===");
    for m in &modules {
        println!("  - {m}");
    }

    // Step 5: Execute modules via call()
    println!("\n=== call() with context ===");
    let ctx = Context::new(Identity {
        id: "admin-1".into(),
        identity_type: "admin".into(),
        roles: vec!["admin".into()],
        attrs: Default::default(),
    });

    let result = apcore
        .call("users.get_user.get", json!({"id": "1"}), Some(&ctx))
        .await
        .unwrap();
    println!("get_user(1): {result}");

    let result = apcore
        .call("users.get_user.get", json!({"id": "999"}), Some(&ctx))
        .await
        .unwrap();
    println!("get_user(999): {result}");

    // Step 6: Execute via call_anonymous()
    println!("\n=== call_anonymous() ===");
    let result = apcore
        .call_anonymous("users.create_user.post", json!({"name": "Charlie"}))
        .await
        .unwrap();
    println!("create_user: {result}");
}
