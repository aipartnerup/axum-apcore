//! ACL demo — enforce apcore Access Control Lists on Axum routes.
//!
//! The same `orders.delete` (admins only) / `orders.list` (public read)
//! contract used across the apcore framework integrations.
//!
//! Run it:
//!
//! ```bash
//! APCORE_ACL_PATH=examples/acl_demo/acl.yaml cargo run --example acl_demo
//!
//! curl -X DELETE localhost:3000/orders/1                     # 403 (anonymous)
//! curl -X DELETE localhost:3000/orders/1 -H 'X-Roles: user'  # 403 (not admin)
//! curl -X DELETE localhost:3000/orders/1 -H 'X-Roles: admin' # 200
//! curl localhost:3000/orders                                 # 200 (public read)
//! ```

use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::Request;
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{delete, get};
use axum::{Json, Router};
use serde_json::{json, Value};

use axum_apcore::errors::AxumApcoreError;
use axum_apcore::{ApContext, AxumApcore, Context, ModuleError, RequestIdentity, ScannedModule};

// --- apcore module handlers ------------------------------------------------

async fn orders_delete(input: Value, _ctx: &Context<Value>) -> Result<Value, ModuleError> {
    Ok(json!({ "deleted": input["order_id"] }))
}

async fn orders_list(_input: Value, _ctx: &Context<Value>) -> Result<Value, ModuleError> {
    Ok(json!({ "orders": [{ "id": 1 }, { "id": 2 }] }))
}

/// Register the two ACL-protected `orders.*` modules with explicit IDs.
async fn build_apcore() -> Arc<AxumApcore> {
    let apcore = Arc::new(AxumApcore::new());

    apcore.register_handler(
        "axum::orders_delete",
        Arc::new(|input, ctx| Box::pin(orders_delete(input, ctx))),
    );
    apcore.register_handler(
        "axum::orders_list",
        Arc::new(|input, ctx| Box::pin(orders_list(input, ctx))),
    );

    let delete_module = ScannedModule::new(
        "orders.delete".to_string(),
        "Delete an order (admins only)".to_string(),
        json!({
            "type": "object",
            "properties": { "order_id": { "type": "integer" } },
            "required": ["order_id"]
        }),
        json!({ "type": "object", "properties": { "deleted": { "type": "integer" } } }),
        vec!["orders".to_string()],
        "axum::orders_delete".to_string(),
    );
    let list_module = ScannedModule::new(
        "orders.list".to_string(),
        "List orders (public read)".to_string(),
        json!({ "type": "object" }),
        json!({ "type": "object" }),
        vec!["orders".to_string()],
        "axum::orders_list".to_string(),
    );

    apcore
        .register_modules(&[delete_module, list_module])
        .await
        .expect("failed to register orders modules");

    apcore
}

// --- HTTP layer ------------------------------------------------------------

/// Demo auth shortcut: turn a comma-separated `X-Roles` header into a
/// `RequestIdentity` in the request extensions, which `ApContext` (and thus the
/// ACL check) then reads. Real apps populate this from a JWT/session guard.
async fn inject_identity(mut req: Request<Body>, next: Next) -> Response {
    if let Some(header) = req.headers().get("x-roles").and_then(|h| h.to_str().ok()) {
        let roles: Vec<String> = header
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if !roles.is_empty() {
            req.extensions_mut().insert(RequestIdentity {
                id: "u1".to_string(),
                identity_type: "user".to_string(),
                roles,
                attrs: Default::default(),
            });
        }
    }
    next.run(req).await
}

async fn delete_order_route(
    State(apcore): State<Arc<AxumApcore>>,
    ApContext(ctx): ApContext,
    Path(order_id): Path<i64>,
) -> Result<Json<Value>, AxumApcoreError> {
    let out = apcore
        .call("orders.delete", json!({ "order_id": order_id }), Some(&ctx))
        .await?;
    Ok(Json(out))
}

async fn list_orders_route(
    State(apcore): State<Arc<AxumApcore>>,
    ApContext(ctx): ApContext,
) -> Result<Json<Value>, AxumApcoreError> {
    let out = apcore.call("orders.list", json!({}), Some(&ctx)).await?;
    Ok(Json(out))
}

fn app(apcore: Arc<AxumApcore>) -> Router {
    Router::new()
        .route("/orders/{order_id}", delete(delete_order_route))
        .route("/orders", get(list_orders_route))
        .layer(middleware::from_fn(inject_identity))
        .with_state(apcore)
}

#[tokio::main]
async fn main() {
    let apcore = build_apcore().await;
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .expect("failed to bind port 3000");
    println!("axum-apcore ACL demo listening on http://localhost:3000");
    axum::serve(listener, app(apcore))
        .await
        .expect("server error");
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum_apcore::Identity;

    fn ctx_with_roles(roles: Vec<String>) -> Context<Value> {
        let anonymous = roles.is_empty();
        Context::new(Identity::new(
            if anonymous { "anonymous" } else { "u1" }.to_string(),
            if anonymous { "anonymous" } else { "user" }.to_string(),
            roles,
            Default::default(),
        ))
    }

    #[tokio::test]
    async fn test_acl_demo_orders_contract() {
        std::env::set_var(
            "APCORE_ACL_PATH",
            concat!(env!("CARGO_MANIFEST_DIR"), "/examples/acl_demo/acl.yaml"),
        );
        let apcore = build_apcore().await;

        let admin = ctx_with_roles(vec!["admin".to_string()]);
        let user = ctx_with_roles(vec!["user".to_string()]);
        let anon = ctx_with_roles(vec![]);

        // admin may delete
        let ok = apcore
            .call("orders.delete", json!({ "order_id": 1 }), Some(&admin))
            .await;
        assert!(ok.is_ok(), "admin delete should succeed: {ok:?}");
        assert_eq!(ok.unwrap()["deleted"], json!(1));

        // anonymous + non-admin denied
        assert!(apcore
            .call("orders.delete", json!({ "order_id": 1 }), Some(&anon))
            .await
            .is_err());
        assert!(apcore
            .call("orders.delete", json!({ "order_id": 1 }), Some(&user))
            .await
            .is_err());

        // anyone may read the public list
        assert!(apcore
            .call("orders.list", json!({}), Some(&anon))
            .await
            .is_ok());
    }
}
