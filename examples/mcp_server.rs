//! MCP server example — expose registered modules as MCP tools.
//!
//! Demonstrates:
//! 1. Registering route metadata and handlers
//! 2. Creating an MCP server from the registry
//! 3. Exporting modules as OpenAI-compatible tool definitions
//!
//! Requires the `mcp` feature: `cargo run --example mcp_server --features mcp`

use axum::Router;
use serde_json::{json, Value};
use std::sync::Arc;

use axum_apcore::{ap_handler, AxumApcore, Context, ModuleError};

// ---------------------------------------------------------------------------
// Handler functions
// ---------------------------------------------------------------------------

async fn get_weather(input: Value, _ctx: &Context<Value>) -> Result<Value, ModuleError> {
    let city = input["city"].as_str().unwrap_or("unknown");
    Ok(json!({
        "city": city,
        "temperature": 22,
        "unit": "celsius",
        "conditions": "sunny",
    }))
}

async fn translate_text(input: Value, _ctx: &Context<Value>) -> Result<Value, ModuleError> {
    let text = input["text"].as_str().unwrap_or("");
    let target = input["target_language"].as_str().unwrap_or("en");
    Ok(json!({
        "original": text,
        "translated": format!("[{target}] {text}"),
        "target_language": target,
    }))
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // Register route metadata
    ap_handler! {
        method: "GET",
        path: "/api/weather/:city",
        handler: get_weather,
        description: "Get current weather for a city",
        tags: ["weather"],
        input_schema: json!({
            "type": "object",
            "properties": {
                "city": {"type": "string", "description": "City name"}
            },
            "required": ["city"]
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "city": {"type": "string"},
                "temperature": {"type": "number"},
                "unit": {"type": "string"},
                "conditions": {"type": "string"}
            }
        }),
    }

    ap_handler! {
        method: "POST",
        path: "/api/translate",
        handler: translate_text,
        description: "Translate text to a target language",
        tags: ["translate"],
        input_schema: json!({
            "type": "object",
            "properties": {
                "text": {"type": "string", "description": "Text to translate"},
                "target_language": {"type": "string", "description": "Target language code"}
            },
            "required": ["text", "target_language"]
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "original": {"type": "string"},
                "translated": {"type": "string"},
                "target_language": {"type": "string"}
            }
        }),
    }

    // Create AxumApcore and register handlers
    let settings = axum_apcore::ApcoreSettings {
        serve_transport: "streamable-http".into(),
        serve_host: "127.0.0.1".into(),
        serve_port: 9090,
        server_name: "mcp-example".into(),
        ..axum_apcore::ApcoreSettings::default()
    };
    let apcore = Arc::new(AxumApcore::with_settings(settings));

    apcore.register_handler(
        "axum::get_weather",
        Arc::new(|input, ctx| Box::pin(get_weather(input, ctx))),
    );
    apcore.register_handler(
        "axum::translate_text",
        Arc::new(|input, ctx| Box::pin(translate_text(input, ctx))),
    );

    let router = Router::new();
    apcore.init_app(&router).await.expect("Failed to init");

    // List registered modules
    println!("=== Registered modules ===");
    for m in apcore.list_modules() {
        println!("  - {m}");
    }

    // Export as OpenAI tools
    #[cfg(feature = "mcp")]
    {
        println!("\n=== OpenAI tool definitions ===");
        match apcore.to_openai_tools(false, false, None, None) {
            Ok(tools) => {
                for tool in &tools {
                    println!("{}", serde_json::to_string_pretty(tool).unwrap());
                }
                println!("\nExported {} tools", tools.len());
            }
            Err(e) => println!("Export error: {e}"),
        }

        // Create MCP server (print config, don't actually start)
        println!("\n=== MCP server config ===");
        match apcore.create_mcp_server() {
            Ok(server) => {
                println!("MCP server created: {:?}", server.config());
                println!("To start: call server.run().await");
            }
            Err(e) => println!("MCP server error: {e}"),
        }
    }

    #[cfg(not(feature = "mcp"))]
    {
        println!("\n[!] MCP feature not enabled.");
        println!("    Run with: cargo run --example mcp_server --features mcp");
    }
}
