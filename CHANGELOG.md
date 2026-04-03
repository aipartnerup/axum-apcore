# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---
## [0.2.0] - 2026-04-01

### Added

#### apcore-cli Integration (feature = `cli`)
- **`create_cli()`** — New method on `AxumApcore` that scans Axum routes, registers them as HTTP proxy modules via `HTTPProxyRegistryWriter`, and builds a grouped clap `Command` using apcore-cli's `GroupedModuleGroup`. Mirrors fastapi-apcore's `create_cli()` pattern.
- **`CreateCliConfig`** — Configuration struct for `create_cli()` with prog_name, base_url, auth_header_factory, timeout, scan_source, include/exclude filters, help_text_max_length, docs_url, and verbose_help.
- **`list` command** — List available modules in the registry, delegated to `apcore_cli::cmd_list`. Supports `--tag` filtering and `--format` (table/json).
- **`describe` command** — Show schema and annotations for a module, delegated to `apcore_cli::cmd_describe`.
- **`completion` command** — Generate shell completion scripts (bash, zsh, fish, elvish, powershell), delegated to `apcore_cli::cmd_completion`.
- **`man` command** — Generate roff man pages for any command, delegated to `apcore_cli::cmd_man` and `apcore_cli::build_program_man_page`.
- **`init module` command** — Scaffold new apcore module files (decorator, convention, or binding style), delegated to `apcore_cli::handle_init`.
- **`cli_proxy` example** — New example demonstrating HTTP proxy CLI generation with `create_cli()`.

#### Re-exports
- **`HTTPProxyRegistryWriter`** — Re-exported from apcore-toolkit when `cli` feature is enabled.
- **`CreateCliConfig`** — Re-exported at crate root when `cli` feature is enabled.

### Changed

#### Dependency Upgrades
- **`apcore`** 0.14 → 0.15 — `Context.identity` changed from `Identity` to `Option<Identity>`; all production code and tests updated.
- **`apcore-toolkit`** 0.3 → 0.4 — Adds `DisplayResolver`, `SyntaxVerifier`, and `HTTPProxyRegistryWriter` (http-proxy feature).
- **`apcore-mcp`** 0.10 → 0.12 — Adds MCP Explorer, error formatter integration, identity propagation, and display overlays.

#### CLI Feature Expansion
- The `cli` feature now includes `apcore-cli` (0.5), `clap_complete`, and enables `apcore-toolkit/http-proxy` for HTTP proxy module support.
- CLI description updated from "scan routes, serve MCP, and export tools" to "scan routes, serve MCP, export tools, and manage modules".

### Tests
- 79 unit tests + 10 integration tests (89 total), all passing with `cargo test --all-features`
- Added 6 new CLI tests: `test_build_registry_provider_empty`, `test_run_list_empty_registry`, `test_run_completion_bash`, `test_run_completion_invalid_shell`, `test_run_man_program_page`, `test_run_man_unknown_command`
- Added 6 CLI parsing tests: list, list_with_tags, describe, completion, man, init_module

---

## [0.1.1] - 2026-03-22

### Changed
- Rebrand: aipartnerup → aiperceivable


## [0.1.0] - 2026-03-20

Initial release. Axum integration for the apcore AI-Perceivable Core ecosystem,
feature-aligned with [fastapi-apcore](https://github.com/aiperceivable/fastapi-apcore).

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
