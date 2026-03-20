# CLAUDE.md — axum-apcore Development & Code Quality Specification

## Project Overview

`axum-apcore` is the **Axum (Rust) integration for the apcore AI-Perceivable Core ecosystem**. It exposes Axum routes as apcore modules via automatic scanning, with full execution, context mapping, and MCP serving support.

**Reference implementation**: `../fastapi-apcore` (Python/FastAPI)
**Core SDK**: `../apcore-rust`
**Toolkit**: `../apcore-toolkit-rust`

---

## Core Principles

- Prioritize **simplicity, readability, and maintainability** above all.
- Avoid premature abstraction, optimization, or over-engineering.
- Code should be understandable in ≤10 seconds; favor straightforward over clever.
- Always follow: **Understand → Plan → Implement minimally → Test/Validate → Commit**.

---

## Rust Code Quality

### Readability

- Use precise, full-word names; standard abbreviations only when idiomatic (`buf`, `cfg`, `ctx`).
- Functions ≤50 lines, single responsibility, verb-named (`parse_request`, `build_schema`).
- Avoid obscure tricks, overly chained iterators, unnecessary macros, or excessive generics.
- Break complex logic into small, well-named helper functions.

### Types (Mandatory)

- Provide explicit types on all public items; do not rely on inference for public API surfaces.
- Prefer `struct` over raw tuples for anything with more than 2 fields.
- Use **`enum`** for exhaustive variants; avoid stringly-typed logic.
- Implement `serde::Serialize` / `serde::Deserialize` on all public data types.

### Design

- Favor **composition over inheritance**; use `trait` only for true behavioral interfaces.
- Prefer plain functions + data structs; minimize trait object (`dyn Trait`) indirection.
- No circular module dependencies.
- Keep `pub` surface minimal — default to module-private, expose only what consumers need.

### Errors & Resources

- Define domain errors with **`thiserror`**; no bare `Box<dyn Error>` in library code.
- Propagate errors with `?`; no `unwrap()` / `expect()` in library paths (tests excepted).
- Validate and sanitize all public inputs at crate boundaries.

### Async

- Runtime: **Tokio** (`features = ["full"]`).
- Traits with async methods: use **`async-trait`**.

### Logging

- Use **`tracing`** — no `println!` / `eprintln!` in production code.

### Testing

- Run with: `cargo test --all-features`
- **Unit tests**: in the same file under `#[cfg(test)] mod tests { ... }`.
- Test names: `test_<unit>_<behavior>` (e.g., `test_scan_with_include_filter`).
- Never change production code without adding or updating corresponding tests.

### Serialization

- JSON: `serde_json`. YAML: `serde_yaml`.

---

## Mandatory Quality Gates

| Command | Purpose |
|---------|---------|
| `cargo fmt --all -- --check` | Formatting |
| `cargo clippy --all-targets --all-features -- -D warnings` | Lint |
| `cargo build --all-features` | Full build |
| `cargo test --all-features` | Tests |
| `cargo build --examples` | Example build |

---

## Architecture

### Module Map (mapped from fastapi-apcore)

| Rust Module | FastAPI Module | Purpose |
|-------------|----------------|---------|
| `client` | `client.py` | `AxumApcore` unified entry point |
| `config` | `engine/config.py` | `ApcoreSettings` from APCORE_* env vars |
| `context` | `engine/context.py` | `ApContext` extractor + `AxumContextFactory` |
| `scanner/native` | `scanners/native.py` | Route metadata registry scanning |
| `scanner/openapi` | `scanners/openapi.py` | utoipa OpenAPI spec scanning |
| `engine/registry` | `engine/registry.py` | Singleton Registry/Executor management |
| `engine/extensions` | `engine/extensions.py` | Discoverer + ModuleValidator |
| `engine/observability` | `engine/observability.py` | Tracing/metrics/logging setup |
| `engine/tasks` | `engine/tasks.py` | Async task management |
| `output/registry_writer` | `output/registry_writer.py` | ScannedModule → Registry registration |
| `cli` | `cli.py` | clap CLI (scan/serve/export/tasks) |

### Key Design Decisions

1. **Native scanner uses a metadata registry** (not Router introspection) because Axum's Router is type-erased. The `ap_handler!` macro populates metadata at compile time.
2. **OpenAPI scanner** relies on external utoipa-generated specs (compile-time generation).
3. **Context extraction** uses Axum's `FromRequestParts` trait for ergonomic `ApContext` extractors.
4. **Singleton management** uses `OnceLock` for thread-safe lazy initialization.

## Dependency Management

- Evaluate necessity before adding a new dependency.
- Dev-only crates go in `[dev-dependencies]`, never `[dependencies]`.

## General Guidelines

- **English only** for all code, comments, doc comments, error messages, and commit messages.
- Fully understand surrounding code before making changes.
