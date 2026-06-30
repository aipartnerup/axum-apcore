// ACL integration test for axum-apcore.
//
// Verifies that axum-apcore actually loads an apcore ACL from `acl_path` and
// that apcore's `acl_check` pipeline step enforces it on `call()`. The ACL
// caller for a top-level request is `@external` (apcore assigns it when no
// inter-module caller is on the call chain), so the fixture rules target
// `@external` — NOT the request's `RequestIdentity`. HTTP user-level authz is a
// separate concern handled at the Axum middleware / JWT layer.
//
// NOTE: this is a DEDICATED test binary. The executor and settings are global
// `OnceLock` singletons initialized once per binary, and the ACL is loaded from
// `APCORE_ACL_PATH` at executor init. The whole scenario therefore lives in a
// single test function: the env var is set and the fixture written BEFORE the
// first `AxumApcore` is constructed (which triggers executor init), and there
// is no second test in this binary to race the one-time initialization.

use std::sync::Arc;

use serde_json::{json, Value};

use axum_apcore::errors::AxumApcoreError;
use axum_apcore::scanner::native::{register_route, RouteMetadata};
use axum_apcore::{ApcoreSettings, AxumApcore, Context, ErrorCode, ModuleError};

fn make_route(handler: &str, path: &str) -> RouteMetadata {
    RouteMetadata {
        method: "GET".into(),
        path: path.into(),
        handler_name: handler.into(),
        description: format!("ACL test handler {handler}"),
        tags: vec!["aclt".into()],
        input_schema: json!({"type": "object"}),
        output_schema: json!({"type": "object", "properties": {"ok": {"type": "boolean"}}}),
        documentation: None,
    }
}

async fn ok_handler(_input: Value, _ctx: &Context<Value>) -> Result<Value, ModuleError> {
    Ok(json!({"ok": true}))
}

/// Write an ACL that denies the `@external` caller from reaching the blocked
/// module while leaving everything else allowed (`default_effect: allow`).
fn write_acl(path: &std::path::Path) {
    let yaml = "default_effect: allow\n\
                rules:\n\
                \x20 - callers: [\"@external\"]\n\
                \x20   targets: [\"aclt.acl_blocked_op.get\"]\n\
                \x20   effect: deny\n\
                \x20   description: \"External callers cannot reach the blocked module.\"\n";
    std::fs::write(path, yaml).expect("write acl fixture");
}

#[tokio::test]
async fn test_acl_loaded_and_enforced() {
    // Hold the temp dir for the whole test so the fixture file outlives the
    // one-time executor initialization that loads it.
    let tmp = tempfile::tempdir().expect("create temp dir");
    let acl_path = tmp.path().join("global_acl.yaml");
    write_acl(&acl_path);

    // Must be set before the first `AxumApcore` (and thus executor) init, so
    // `get_executor()` loads this ACL. Safe on edition 2021.
    std::env::set_var("APCORE_ACL_PATH", &acl_path);

    register_route(make_route("acl_blocked_op", "/api/acl_blocked"));
    register_route(make_route("acl_open_op", "/api/acl_open"));

    let settings = ApcoreSettings {
        auto_discover: false,
        acl_path: Some(acl_path.to_string_lossy().into_owned()),
        ..ApcoreSettings::default()
    };
    let apcore = AxumApcore::with_settings(settings);
    apcore.register_handler(
        "axum::acl_blocked_op",
        Arc::new(|input, ctx| Box::pin(ok_handler(input, ctx))),
    );
    apcore.register_handler(
        "axum::acl_open_op",
        Arc::new(|input, ctx| Box::pin(ok_handler(input, ctx))),
    );

    let router = axum::Router::new();
    apcore.init_app(&router).await.expect("init_app failed");

    // Blocked module: ACL denies @external -> ACLDenied surfaces as Execution.
    let denied = apcore
        .call_anonymous("aclt.acl_blocked_op.get", json!({}))
        .await;
    match denied {
        Err(AxumApcoreError::Execution(ModuleError { code, .. })) => {
            assert_eq!(
                code,
                ErrorCode::ACLDenied,
                "blocked module must be denied with ACLDenied"
            );
        }
        other => panic!("expected ACLDenied for blocked module, got: {other:?}"),
    }

    // Allowed module: default_effect allow -> call succeeds.
    let allowed = apcore
        .call_anonymous("aclt.acl_open_op.get", json!({}))
        .await
        .expect("open module must be allowed by ACL");
    assert_eq!(allowed["ok"], true);
}
