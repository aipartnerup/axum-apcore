# axum-apcore

![Rust](https://img.shields.io/badge/rust-1.75+-orange.svg)
![License](https://img.shields.io/badge/license-Apache%202.0-green.svg)

> **Expose Axum routes as AI-perceivable modules.**

Axum integration for the [apcore](https://github.com/aiperceivable/apcore-rust) AI-Perceivable Core ecosystem.

**axum-apcore** automatically scans your Axum routes and exposes them as apcore modules — with full execution, context mapping, MCP serving, and OpenAI tool export. Define your routes once, and both code and AI can discover, understand, and invoke them through enforced schemas and behavioral annotations.

## Features

- **Route scanning** — Discover Axum routes via the native metadata registry or OpenAPI (utoipa) specs
- **`ap_handler!` macro** — Register route metadata at compile time with zero boilerplate
- **`ApContext` extractor** — Extract apcore `Context` (identity, trace context) from Axum requests via `FromRequestParts`
- **Module execution** — Call any registered module programmatically with `call()`, `stream()`, or `cancellable_call()`
- **Async task management** — Submit background tasks with status tracking, cancellation, and cleanup
- **MCP server** — Serve registered modules as MCP tools (stdio, streamable-http, SSE transports)
- **OpenAI export** — Export modules as OpenAI-compatible tool definitions
- **YAML bindings** — Auto-discover modules from YAML binding files
- **CLI** — Scan, serve, export, and manage tasks from the command line
- **Configuration** — `APCORE_*` environment variables for all settings
- **Observability** — Tracing middleware, metrics collection, and structured logging via `tracing`

## API Overview

**Core**

| Type | Description |
|------|-------------|
| `AxumApcore` | Unified entry point — init, scan, register, call, stream, export |
| `ApcoreSettings` | Configuration from `APCORE_*` env vars with validation |
| `ApContext` | Axum extractor for apcore `Context<Value>` |
| `RequestIdentity` | Identity struct for auth middleware to inject into request extensions |
| `AxumContextFactory` | Creates apcore contexts from Axum request parts |

**Scanning**

| Type | Description |
|------|-------------|
| `NativeAxumScanner` | Scans routes from the compile-time metadata registry |
| `OpenAPIScanner` | Scans routes from a utoipa-generated OpenAPI spec (requires `openapi` feature) |
| `ap_handler!` | Macro for declarative route metadata registration |

**Engine**

| Type | Description |
|------|-------------|
| `AxumRegistryWriter` | Registers scanned modules into apcore's Registry |
| `AxumDiscoverer` | Discovers modules from YAML binding files |
| `AxumModuleValidator` | Validates module IDs against apcore constraints |
| `TaskManager` | Async task submission, tracking, and cancellation |

## Requirements

- Rust >= 1.75
- Tokio async runtime
- Axum 0.8+

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
axum-apcore = "0.1"
axum = { version = "0.8", features = ["json"] }
tokio = { version = "1", features = ["full"] }
serde_json = "1"
tracing-subscriber = "0.3"
```

### Feature flags

| Feature | Description | Dependencies |
|---------|-------------|-------------|
| `cli` | CLI commands (scan, serve, export, tasks) | `clap` |
| `mcp` | MCP server and OpenAI tools export | `apcore-mcp` |
| `openapi` | OpenAPI spec scanning via utoipa | `utoipa` |
| `all` | Enable all optional features | — |

```toml
axum-apcore = { version = "0.1", features = ["all"] }
```

## Quick Start

### Register routes and call modules

```rust
use axum::{routing::get, Router, Json};
use axum_apcore::{ap_handler, AxumApcore, ApContext};
use serde_json::{json, Value};
use std::sync::Arc;

// Define a handler
async fn get_user(
    ApContext(ctx): ApContext,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Json<Value> {
    Json(json!({"id": id, "name": "Alice", "trace_id": ctx.trace_id}))
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // Register route metadata
    ap_handler! {
        method: "GET",
        path: "/api/users/:id",
        handler: get_user,
        description: "Get a user by ID",
        tags: ["users"],
        input_schema: json!({"type": "object", "properties": {"id": {"type": "string"}}}),
        output_schema: json!({"type": "object", "properties": {"name": {"type": "string"}}}),
    }

    // Initialize
    let apcore = Arc::new(AxumApcore::new());
    let router = Router::new()
        .route("/api/users/{id}", get(get_user))
        .with_state(apcore.clone());

    apcore.init_app(&router).await.unwrap();

    // List discovered modules
    println!("{:?}", apcore.list_modules());
    // => ["users.get_user.get"]

    // Start the server
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, router).await.unwrap();
}
```

### Register executable handlers

```rust
use axum_apcore::{AxumApcore, Context, ModuleError};
use serde_json::{json, Value};
use std::sync::Arc;

async fn get_user_handler(input: Value, _ctx: &Context<Value>) -> Result<Value, ModuleError> {
    let id = input["id"].as_str().unwrap_or("unknown");
    Ok(json!({"id": id, "name": "Alice"}))
}

#[tokio::main]
async fn main() {
    let apcore = AxumApcore::new();

    // Register a callable handler
    apcore.register_handler(
        "axum::get_user",
        Arc::new(|input, ctx| Box::pin(get_user_handler(input, ctx))),
    );

    // After init_app(), call modules programmatically
    let result = apcore
        .call_anonymous("users.get_user.get", json!({"id": "1"}))
        .await
        .unwrap();

    println!("{result}"); // {"id":"1","name":"Alice"}
}
```

### OpenAPI scanning

```rust
use axum_apcore::OpenAPIScanner;
use serde_json::json;

let scanner = OpenAPIScanner::new();
let spec = json!({
    "openapi": "3.1.0",
    "info": {"title": "My API", "version": "1.0.0"},
    "paths": {
        "/users/{id}": {
            "get": {
                "operationId": "get_user_get",
                "summary": "Get user",
                "tags": ["users"],
                "responses": {"200": {"description": "OK"}}
            }
        }
    }
});

let modules = scanner.scan_spec(&spec, None, None).unwrap();
println!("{}", modules[0].module_id); // "users.get_user.get"
```

### Cancellable execution with timeout

```rust
use std::time::Duration;

let result = apcore
    .cancellable_call("users.get_user.get", json!({"id": "1"}), None, Duration::from_secs(5))
    .await;
```

### Background tasks

```rust
// Submit a task
let task_id = apcore.submit_task("users.get_user.get", json!({"id": "1"})).unwrap();

// Check status
let info = apcore.get_task_status(&task_id);

// Get result when complete
let result = apcore.get_task_result(&task_id);

// Cancel a running task
apcore.cancel_task(&task_id);
```

## Configuration

All settings are read from environment variables with the `APCORE_` prefix:

| Variable | Default | Description |
|----------|---------|-------------|
| `APCORE_MODULE_DIR` | `apcore_modules` | Directory for YAML binding files |
| `APCORE_AUTO_DISCOVER` | `true` | Auto-discover modules on startup |
| `APCORE_BINDING_PATTERN` | `*.binding.yaml` | Glob pattern for binding files |
| `APCORE_SCANNER_SOURCE` | `native` | Scanner: `native` or `openapi` |
| `APCORE_SERVE_TRANSPORT` | `streamable-http` | MCP transport: `stdio`, `streamable-http`, `sse` |
| `APCORE_SERVE_HOST` | `127.0.0.1` | MCP server host |
| `APCORE_SERVE_PORT` | `9090` | MCP server port |
| `APCORE_SERVER_NAME` | `axum-apcore` | MCP server name |
| `APCORE_JWT_SECRET` | — | JWT secret for MCP auth |
| `APCORE_TRACING` | `false` | Enable tracing middleware |
| `APCORE_METRICS` | `false` | Enable metrics collection |
| `APCORE_TASK_MAX_CONCURRENT` | `10` | Max concurrent background tasks |
| `APCORE_TASK_MAX_TASKS` | `100` | Max total tasks in queue |

## Examples

The `examples/` directory contains runnable demos. Run any example with:

```bash
cargo run --example basic
cargo run --example handler_registration
cargo run --example async_tasks
cargo run --example openapi_scanner --features openapi
cargo run --example mcp_server --features mcp
```

| Example | Description |
|---------|-------------|
| `basic` | Full Axum app with `ap_handler!`, `ApContext` extractor, and server startup |
| `handler_registration` | Register executable handlers, call modules with `call()` and `call_anonymous()` |
| `async_tasks` | Submit background tasks, poll status, cancel, and list tasks |
| `openapi_scanner` | Scan a utoipa-generated OpenAPI spec with include/exclude filters |
| `mcp_server` | Create an MCP server and export OpenAI-compatible tool definitions |

## Tests

Run all tests (unit + integration):

```bash
cargo test --all-features
```

Run only unit tests:

```bash
cargo test --all-features --lib
```

Run only integration tests:

```bash
cargo test --all-features --test integration_test
```

Run a specific test by name:

```bash
cargo test test_e2e_register_scan_call
```

Run with output visible:

```bash
cargo test -- --nocapture
```

## Development

### Prerequisites

Install Rust via [rustup](https://rustup.rs):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Clone and build

```bash
git clone https://github.com/aiperceivable/axum-apcore.git
cd axum-apcore
cargo build --all-features
```

### Quality gates

All of these must pass before committing:

```bash
cargo fmt --all -- --check          # Formatting
cargo clippy --all-targets --all-features -- -D warnings  # Lint
cargo build --all-features          # Full build
cargo test --all-features           # Tests (unit + integration)
cargo build --examples              # Example build
```

### Lint and format

```bash
cargo fmt           # Auto-format code
cargo clippy        # Lint with suggestions
```

### Build documentation

```bash
cargo doc --all-features --open
```

## Architecture

axum-apcore follows the same module structure as [fastapi-apcore](https://github.com/aiperceivable/fastapi-apcore):

| Rust Module | FastAPI Module | Purpose |
|-------------|----------------|---------|
| `client` | `client.py` | `AxumApcore` unified entry point |
| `config` | `engine/config.py` | `ApcoreSettings` from `APCORE_*` env vars |
| `context` | `engine/context.py` | `ApContext` extractor + `AxumContextFactory` |
| `scanner/native` | `scanners/native.py` | Route metadata registry scanning |
| `scanner/openapi` | `scanners/openapi.py` | utoipa OpenAPI spec scanning |
| `engine/registry` | `engine/registry.py` | Singleton Registry/Executor management |
| `engine/extensions` | `engine/extensions.py` | Discoverer + ModuleValidator |
| `engine/observability` | `engine/observability.py` | Tracing/metrics/logging setup |
| `engine/tasks` | `engine/tasks.py` | Async task management |
| `output/registry_writer` | `output/registry_writer.py` | ScannedModule → Registry registration |
| `cli` | `cli.py` | clap CLI (scan/serve/export/tasks) |

## License

Apache-2.0

## Links

- **Core SDK**: [aiperceivable/apcore-rust](https://github.com/aiperceivable/apcore-rust)
- **Toolkit**: [aiperceivable/apcore-toolkit-rust](https://github.com/aiperceivable/apcore-toolkit-rust)
- **Reference**: [aiperceivable/fastapi-apcore](https://github.com/aiperceivable/fastapi-apcore)
- **Website**: [aiperceivable.com](https://aiperceivable.com)
- **Issues**: [GitHub Issues](https://github.com/aiperceivable/axum-apcore/issues)
