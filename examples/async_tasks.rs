//! Async task management example — submit, track, cancel, and clean up tasks.
//!
//! Demonstrates:
//! 1. Submitting background tasks via `submit_task()`
//! 2. Polling task status and retrieving results
//! 3. Cancelling a running task
//! 4. Listing and cleaning up tasks

use axum::Router;
use serde_json::{json, Value};
use std::sync::Arc;

use axum_apcore::{ap_handler, AxumApcore, Context, ModuleError};

// ---------------------------------------------------------------------------
// A slow handler that simulates background work
// ---------------------------------------------------------------------------

async fn slow_process(input: Value, ctx: &Context<Value>) -> Result<Value, ModuleError> {
    let steps = input["steps"].as_u64().unwrap_or(5) as usize;

    for i in 0..steps {
        // Check for cancellation
        if let Some(token) = &ctx.cancel_token {
            if token.is_cancelled() {
                return Err(ModuleError::new(
                    axum_apcore::ErrorCode::ExecutionCancelled,
                    format!("Cancelled at step {i}/{steps}"),
                ));
            }
        }

        println!("  [slow_process] Step {}/{steps}...", i + 1);
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    Ok(json!({"completed_steps": steps}))
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // Register the slow process handler
    ap_handler! {
        method: "POST",
        path: "/api/process",
        handler: slow_process,
        description: "Run a slow background process",
        tags: ["tasks"],
        input_schema: json!({
            "type": "object",
            "properties": {"steps": {"type": "integer"}},
        }),
        output_schema: json!({
            "type": "object",
            "properties": {"completed_steps": {"type": "integer"}},
        }),
    }

    let apcore = Arc::new(AxumApcore::new());

    apcore.register_handler(
        "axum::slow_process",
        Arc::new(|input, ctx| Box::pin(slow_process(input, ctx))),
    );

    let router = Router::new();
    apcore.init_app(&router).await.expect("Failed to init");

    // --- Submit a task ---
    println!("=== Submit task (3 steps) ===");
    let task_id = apcore
        .submit_task("tasks.slow_process.post", json!({"steps": 3}))
        .expect("Failed to submit task");
    println!("Task submitted: {task_id}");

    // --- Poll for completion ---
    loop {
        tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
        if let Some(info) = apcore.get_task_status(&task_id) {
            println!("  Status: {}", info.status);
            if info.status != "Running" {
                break;
            }
        }
    }

    // --- Get result ---
    if let Some(result) = apcore.get_task_result(&task_id) {
        println!("Result: {result}");
    }

    // --- Submit and cancel ---
    println!("\n=== Submit and cancel (10 steps) ===");
    let task_id2 = apcore
        .submit_task("tasks.slow_process.post", json!({"steps": 10}))
        .expect("Failed to submit task");
    println!("Task submitted: {task_id2}");

    // Let it run for a bit, then cancel
    tokio::time::sleep(tokio::time::Duration::from_millis(250)).await;
    let cancelled = apcore.cancel_task(&task_id2);
    println!("Cancel result: {cancelled}");

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    if let Some(info) = apcore.get_task_status(&task_id2) {
        println!("Status after cancel: {}", info.status);
    }

    // --- List all tasks ---
    println!("\n=== All tasks ===");
    let tasks = apcore.list_tasks(None);
    for t in &tasks {
        println!(
            "  {} | module={} | status={} | error={:?}",
            t.task_id, t.module_id, t.status, t.error
        );
    }
}
