//! OpenAPI scanner example — scan a utoipa-generated OpenAPI spec.
//!
//! Demonstrates:
//! 1. Building an OpenAPI spec (simulating utoipa output)
//! 2. Scanning with `OpenAPIScanner::scan_spec()`
//! 3. Include/exclude filtering
//! 4. Registering scanned modules into the registry

use serde_json::json;

use axum_apcore::OpenAPIScanner;

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // Simulate a utoipa-generated OpenAPI spec
    let spec = json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Pet Store API",
            "version": "1.0.0"
        },
        "paths": {
            "/api/pets": {
                "get": {
                    "operationId": "list_pets_get",
                    "summary": "List all pets",
                    "tags": ["pets"],
                    "parameters": [{
                        "name": "limit",
                        "in": "query",
                        "schema": {"type": "integer"}
                    }],
                    "responses": {
                        "200": {
                            "description": "A list of pets",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "array",
                                        "items": {
                                            "type": "object",
                                            "properties": {
                                                "id": {"type": "integer"},
                                                "name": {"type": "string"},
                                                "species": {"type": "string"}
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
                "post": {
                    "operationId": "create_pet_post",
                    "summary": "Create a pet",
                    "tags": ["pets"],
                    "requestBody": {
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "properties": {
                                        "name": {"type": "string"},
                                        "species": {"type": "string"}
                                    },
                                    "required": ["name", "species"]
                                }
                            }
                        }
                    },
                    "responses": {
                        "201": {
                            "description": "Pet created",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "id": {"type": "integer"},
                                            "name": {"type": "string"}
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/api/pets/{id}": {
                "get": {
                    "operationId": "get_pet_get",
                    "summary": "Get a pet by ID",
                    "description": "Returns a single pet by its unique identifier.",
                    "tags": ["pets"],
                    "parameters": [{
                        "name": "id",
                        "in": "path",
                        "required": true,
                        "schema": {"type": "integer"}
                    }],
                    "responses": {
                        "200": {
                            "description": "A pet",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "id": {"type": "integer"},
                                            "name": {"type": "string"},
                                            "species": {"type": "string"}
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
                "delete": {
                    "operationId": "delete_pet_delete",
                    "summary": "Delete a pet",
                    "tags": ["pets"],
                    "parameters": [{
                        "name": "id",
                        "in": "path",
                        "required": true,
                        "schema": {"type": "integer"}
                    }],
                    "responses": {
                        "204": {"description": "Pet deleted"}
                    }
                }
            },
            "/api/owners": {
                "get": {
                    "operationId": "list_owners_get",
                    "summary": "List all owners",
                    "tags": ["owners"],
                    "responses": {
                        "200": {
                            "description": "A list of owners",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "array",
                                        "items": {
                                            "type": "object",
                                            "properties": {
                                                "id": {"type": "integer"},
                                                "name": {"type": "string"}
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    // --- Scan all routes ---
    println!("=== Scan all routes ===");
    let scanner = OpenAPIScanner::new();
    let modules = scanner.scan_spec(&spec, None, None).unwrap();

    for m in &modules {
        let annotations = m.annotations.as_ref().unwrap();
        println!(
            "  {} | {} | readonly={} destructive={}",
            m.module_id, m.description, annotations.readonly, annotations.destructive
        );
    }
    println!("Total: {} modules\n", modules.len());

    // --- Scan with include filter ---
    println!("=== Filter: include 'pets' ===");
    let filtered = scanner.scan_spec(&spec, Some("pets"), None).unwrap();
    for m in &filtered {
        println!("  {}", m.module_id);
    }
    println!("Matched: {}\n", filtered.len());

    // --- Scan with exclude filter ---
    println!("=== Filter: exclude 'delete' ===");
    let filtered = scanner.scan_spec(&spec, None, Some("delete")).unwrap();
    for m in &filtered {
        println!("  {}", m.module_id);
    }
    println!("Matched: {}\n", filtered.len());

    // --- Without ID simplification ---
    println!("=== Without ID simplification ===");
    let scanner_raw = OpenAPIScanner::with_simplify_ids(false);
    let modules_raw = scanner_raw.scan_spec(&spec, None, None).unwrap();
    for m in &modules_raw {
        println!("  {} -> target: {}", m.module_id, m.target);
    }

    // --- Show metadata ---
    println!("\n=== Module metadata ===");
    let m = &modules[0];
    println!("Module: {}", m.module_id);
    println!(
        "  Input schema: {}",
        serde_json::to_string(&m.input_schema).unwrap()
    );
    println!(
        "  Output schema: {}",
        serde_json::to_string(&m.output_schema).unwrap()
    );
    println!(
        "  Annotations: {}",
        serde_json::to_string_pretty(m.annotations.as_ref().unwrap()).unwrap()
    );
}
