# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.1.0] - 2026-03-20

Initial release. Axum integration for the apcore AI-Perceivable Core ecosystem,
feature-aligned with [fastapi-apcore](https://github.com/aipartnerup/fastapi-apcore).

### Added

#### Core
- **`AxumApcore`** — Unified entry point: init, scan, register, call, stream, export
- **`ApcoreSettings`** — Configuration from `APCORE_*` environment variables with validation
- **`ap_handler!` macro** — Declarative route metadata registration at compile time
- **`AxumApcoreError`** — `thiserror`-based error enum with `IntoResponse` for Axum handlers

#### Context Extraction
- **`ApContext`** — Axum `FromRequestParts` extractor for apcore `Context<Value>`
- **`RequestIdentity`** — Identity struct for auth middleware to inject into request extensions
- **`AxumContextFactory`** — Creates apcore contexts from Axum request parts with W3C `traceparent` support

#### Scanning
- **`NativeAxumScanner`** — Scans routes from the compile-time metadata registry (`RouteMetadata`)
- **`OpenAPIScanner`** — Scans routes from utoipa-generated OpenAPI specs (feature = `openapi`)
- **`AxumScanner` trait** — Extensible scanner interface with include/exclude regex filters
- **`get_scanner()`** — Factory function for scanner selection by source name

#### Execution
- **`call()`** — Execute a module by ID with optional context
- **`call_anonymous()`** — Execute with a default anonymous identity
- **`stream()`** — Execute with streaming output (vec-wrapped)
- **`cancellable_call()`** — Execute with timeout and cooperative cancellation via `CancelToken`
- **`register_handler()`** — Register callable handler functions for target strings
- Executor uses `tokio::sync::Mutex` for safe async lock holding

#### Task Management
- **`TaskManager`** — Async task submission with concurrency and total limits
- **`submit_task()`** — Background execution via `tokio::spawn`
- **`get_task_status()` / `get_task_result()`** — Poll task lifecycle
- **`cancel_task()`** — Cancel running tasks via `CancelToken`
- **`list_tasks()`** — List tasks with optional status filter
- **`cleanup()`** — Remove completed/failed/cancelled tasks by age

#### Engine
- **`get_registry()` / `get_executor()`** — Thread-safe singleton management via `OnceLock`
- **`AxumRegistryWriter`** — Registers scanned modules into both query registry and executor registry
- **`AxumDiscoverer`** — Discovers modules from YAML binding files
- **`AxumModuleValidator`** — Validates module IDs (length, reserved words, segment format)
- **`setup_observability()`** — Configures tracing, metrics, and error history from settings

#### MCP & Export (feature = `mcp`)
- **`create_mcp_server()`** — Create an MCP server from the registry (stdio, streamable-http, SSE)
- **`to_openai_tools()`** — Export modules as OpenAI-compatible tool definitions

#### CLI (feature = `cli`)
- **`scan`** — Scan routes and output to registry or YAML
- **`serve`** — Start an MCP server exposing registered modules
- **`export`** — Export modules as OpenAI tool definitions
- **`tasks`** — List, cancel, and clean up async tasks

#### Examples
- `basic` — Full Axum app with `ap_handler!`, `ApContext`, and server startup
- `handler_registration` — Register handlers, call with `call()` and `call_anonymous()`
- `async_tasks` — Submit, poll, cancel, and list background tasks
- `openapi_scanner` — Scan OpenAPI specs with include/exclude filters
- `mcp_server` — Create MCP server and export OpenAI tools

#### Tests
- 67 unit tests across all modules
- 10 integration tests covering end-to-end flow, task management, context extraction, and scanner filters
- All tests pass with `cargo test --all-features`
