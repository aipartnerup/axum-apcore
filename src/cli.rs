// CLI commands for axum-apcore.
//
// Provides scan, serve, export, and tasks commands via clap.
// Equivalent to fastapi-apcore's Typer-based CLI.

use clap::{Parser, Subcommand};

use crate::engine::tasks::TaskManager;
use crate::errors::AxumApcoreError;
use crate::output::AxumRegistryWriter;
use crate::scanner::get_scanner;

/// axum-apcore CLI — scan routes, serve MCP, and export tools.
#[derive(Parser, Debug)]
#[command(name = "axum-apcore", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Scan Axum routes and generate module definitions.
    Scan {
        /// Scanner source: "native" or "openapi".
        #[arg(long, default_value = "native")]
        source: String,

        /// Output format: "registry", "yaml", or "http-proxy".
        #[arg(long, default_value = "registry")]
        output: String,

        /// Directory for YAML output files.
        #[arg(long)]
        dir: Option<String>,

        /// Dry run — scan but don't write.
        #[arg(long)]
        dry_run: bool,

        /// Include filter regex.
        #[arg(long)]
        include: Option<String>,

        /// Exclude filter regex.
        #[arg(long)]
        exclude: Option<String>,

        /// Verify registration after writing.
        #[arg(long)]
        verify: bool,
    },

    /// Start MCP server exposing registered modules.
    Serve {
        /// Transport: "stdio", "streamable-http", or "sse".
        #[arg(long)]
        transport: Option<String>,

        /// Host to bind to.
        #[arg(long)]
        host: Option<String>,

        /// Port to listen on.
        #[arg(long)]
        port: Option<u16>,

        /// Enable MCP explorer UI.
        #[arg(long)]
        explorer: bool,

        /// JWT secret for authentication.
        #[arg(long, env = "APCORE_JWT_SECRET")]
        jwt_secret: Option<String>,

        /// Approval mode: "auto", "deny", or "manual".
        #[arg(long, default_value = "auto")]
        approval: String,

        /// Comma-separated tags to filter exposed modules.
        #[arg(long)]
        tags: Option<String>,

        /// Module ID prefix filter.
        #[arg(long)]
        prefix: Option<String>,
    },

    /// Export modules as OpenAI-compatible tools.
    Export {
        /// Export format: "openai-tools".
        #[arg(long, default_value = "openai-tools")]
        format: String,

        /// Use strict mode for OpenAI tools.
        #[arg(long)]
        strict: bool,

        /// Embed annotations in tool definitions.
        #[arg(long)]
        embed_annotations: bool,

        /// Comma-separated tags to filter.
        #[arg(long)]
        tags: Option<String>,

        /// Module ID prefix filter.
        #[arg(long)]
        prefix: Option<String>,
    },

    /// Manage async tasks.
    Tasks {
        #[command(subcommand)]
        action: TaskCommands,
    },
}

#[derive(Subcommand, Debug)]
pub enum TaskCommands {
    /// List async tasks.
    List {
        /// Filter by status.
        #[arg(long)]
        status: Option<String>,
    },
    /// Cancel a running task.
    Cancel {
        /// Task ID to cancel.
        task_id: String,
    },
    /// Clean up old completed tasks.
    Cleanup {
        /// Maximum age in seconds.
        #[arg(long, default_value = "3600")]
        max_age: u64,
    },
}

/// Parse CLI arguments.
pub fn parse_cli() -> Cli {
    Cli::parse()
}

/// Execute a CLI command.
///
/// Call this from your application's main function after calling `parse_cli()`.
pub async fn run(cli: Cli) -> Result<(), AxumApcoreError> {
    match cli.command {
        Commands::Scan {
            source,
            output,
            dir,
            dry_run,
            include,
            exclude,
            verify,
        } => run_scan(source, output, dir, dry_run, include, exclude, verify).await,
        Commands::Serve {
            transport,
            host,
            port,
            tags,
            prefix,
            ..
        } => run_serve(transport, host, port, tags, prefix).await,
        Commands::Export {
            format,
            strict,
            embed_annotations,
            tags,
            prefix,
        } => run_export(format, strict, embed_annotations, tags, prefix),
        Commands::Tasks { action } => run_tasks(action),
    }
}

/// Execute the `scan` command.
async fn run_scan(
    source: String,
    output: String,
    dir: Option<String>,
    dry_run: bool,
    include: Option<String>,
    exclude: Option<String>,
    verify: bool,
) -> Result<(), AxumApcoreError> {
    let scanner = get_scanner(&source)?;
    let router = axum::Router::new();

    let include_ref = include.as_deref();
    let exclude_ref = exclude.as_deref();
    let modules = scanner.scan(&router, include_ref, exclude_ref).await?;

    println!("Scanned {} modules via '{}' scanner", modules.len(), source);
    for m in &modules {
        println!("  - {} ({})", m.module_id, m.description);
    }

    if dry_run {
        println!("Dry run — no output written.");
        return Ok(());
    }

    match output.as_str() {
        "registry" => {
            let writer = AxumRegistryWriter::new();
            let mut registry = apcore::Registry::new();
            let results = writer.write(&modules, &mut registry, false, verify);
            let ok_count = results.iter().filter(|r| r.verified).count();
            println!("Registered {}/{} modules", ok_count, results.len());
        }
        "yaml" => {
            let out_dir = dir.unwrap_or_else(|| "apcore_modules".into());
            let yaml_writer = apcore_toolkit::YAMLWriter;
            let results = yaml_writer
                .write(&modules, &out_dir, false, verify, None)
                .map_err(|e| AxumApcoreError::Config(format!("YAML write failed: {e}")))?;
            let ok_count = results.iter().filter(|r| r.verified).count();
            println!(
                "Wrote {}/{} YAML bindings to '{}'",
                ok_count,
                results.len(),
                out_dir
            );
        }
        other => {
            return Err(AxumApcoreError::Config(format!(
                "Unknown output format: '{}'. Available: registry, yaml",
                other
            )));
        }
    }

    Ok(())
}

/// Execute the `serve` command.
async fn run_serve(
    transport: Option<String>,
    host: Option<String>,
    port: Option<u16>,
    tags: Option<String>,
    prefix: Option<String>,
) -> Result<(), AxumApcoreError> {
    #[cfg(not(feature = "mcp"))]
    {
        let _ = (transport, host, port, tags, prefix);
        return Err(AxumApcoreError::Config(
            "MCP feature not enabled. Build with --features mcp".into(),
        ));
    }

    #[cfg(feature = "mcp")]
    {
        let settings = crate::config::get_apcore_settings();

        let transport_str = transport.as_deref().unwrap_or(&settings.serve_transport);
        let host_str = host.as_deref().unwrap_or(&settings.serve_host);
        let port_val = port.unwrap_or(settings.serve_port);

        let parsed_tags = tags.map(|t| parse_tags(&t));

        println!(
            "Starting MCP server on {}://{}:{}",
            transport_str, host_str, port_val
        );

        let config = apcore_mcp::ServeConfig {
            name: settings.server_name.clone(),
            transport: transport_str.to_string(),
            host: host_str.to_string(),
            port: port_val,
            tags: parsed_tags,
            prefix,
            ..apcore_mcp::ServeConfig::default()
        };

        let backend = apcore_mcp::BackendSource::ExtensionsDir(settings.module_dir.clone());

        apcore_mcp::serve(backend, config)
            .map_err(|e| AxumApcoreError::Config(format!("MCP server error: {e}")))?;

        Ok(())
    }
}

/// Execute the `export` command.
fn run_export(
    format: String,
    strict: bool,
    embed_annotations: bool,
    tags: Option<String>,
    prefix: Option<String>,
) -> Result<(), AxumApcoreError> {
    #[cfg(not(feature = "mcp"))]
    {
        let _ = (format, strict, embed_annotations, tags, prefix);
        return Err(AxumApcoreError::Config(
            "MCP feature not enabled. Build with --features mcp".into(),
        ));
    }

    #[cfg(feature = "mcp")]
    {
        if format != "openai-tools" {
            return Err(AxumApcoreError::Config(format!(
                "Unknown export format: '{}'. Available: openai-tools",
                format
            )));
        }

        let settings = crate::config::get_apcore_settings();
        let parsed_tags = tags.map(|t| parse_tags(&t));

        let config = apcore_mcp::OpenAIToolsConfig {
            embed_annotations,
            strict,
            tags: parsed_tags,
            prefix,
        };

        let backend = apcore_mcp::BackendSource::ExtensionsDir(settings.module_dir.clone());
        let tools = apcore_mcp::to_openai_tools(backend, config)
            .map_err(|e| AxumApcoreError::Config(format!("Export failed: {e}")))?;

        let json = serde_json::to_string_pretty(&tools).map_err(AxumApcoreError::Json)?;
        println!("{}", json);
        println!("Exported {} OpenAI tool definitions", tools.len());

        Ok(())
    }
}

/// Execute task subcommands.
fn run_tasks(action: TaskCommands) -> Result<(), AxumApcoreError> {
    let settings = crate::config::get_apcore_settings();
    let task_manager = TaskManager::from_settings(settings);

    match action {
        TaskCommands::List { status } => {
            let tasks = task_manager.list_tasks(status.as_deref());
            if tasks.is_empty() {
                println!("No tasks found.");
            } else {
                let header = format!(
                    "{:<36} {:<20} {:<10} {}",
                    "TASK ID", "MODULE", "STATUS", "CREATED"
                );
                println!("{header}");
                for task in &tasks {
                    println!(
                        "{:<36} {:<20} {:<10} {}",
                        task.task_id, task.module_id, task.status, task.created_at
                    );
                }
                println!("\nTotal: {} tasks", tasks.len());
            }
        }
        TaskCommands::Cancel { task_id } => {
            if task_manager.cancel(&task_id) {
                println!("Task '{}' cancelled.", task_id);
            } else {
                println!("Task '{}' not found or not running.", task_id);
            }
        }
        TaskCommands::Cleanup { max_age } => {
            let removed = task_manager.cleanup(max_age);
            println!("Cleaned up {} tasks (older than {}s).", removed, max_age);
        }
    }

    Ok(())
}

/// Parse comma-separated tags string into a Vec.
fn parse_tags(tags: &str) -> Vec<String> {
    tags.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn test_cli_parse_scan() {
        let cli = Cli::try_parse_from(["axum-apcore", "scan"]).unwrap();
        assert!(matches!(cli.command, Commands::Scan { .. }));
    }

    #[test]
    fn test_cli_parse_serve() {
        let cli = Cli::try_parse_from(["axum-apcore", "serve"]).unwrap();
        assert!(matches!(cli.command, Commands::Serve { .. }));
    }

    #[test]
    fn test_cli_parse_export() {
        let cli = Cli::try_parse_from(["axum-apcore", "export"]).unwrap();
        assert!(matches!(cli.command, Commands::Export { .. }));
    }

    #[test]
    fn test_cli_parse_tasks_list() {
        let cli = Cli::try_parse_from(["axum-apcore", "tasks", "list"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Tasks {
                action: TaskCommands::List { .. }
            }
        ));
    }

    #[test]
    fn test_cli_parse_tasks_cancel() {
        let cli = Cli::try_parse_from(["axum-apcore", "tasks", "cancel", "task-123"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Tasks {
                action: TaskCommands::Cancel { .. }
            }
        ));
    }

    #[test]
    fn test_cli_verify() {
        Cli::command().debug_assert();
    }

    #[test]
    fn test_parse_tags() {
        assert_eq!(parse_tags("a, b, c"), vec!["a", "b", "c"]);
        assert_eq!(parse_tags("single"), vec!["single"]);
        assert_eq!(parse_tags(""), Vec::<String>::new());
        assert_eq!(parse_tags("a,,b"), vec!["a", "b"]);
    }

    #[test]
    fn test_cli_parse_scan_with_options() {
        let cli = Cli::try_parse_from([
            "axum-apcore",
            "scan",
            "--source",
            "openapi",
            "--output",
            "yaml",
            "--dry-run",
            "--include",
            "users",
            "--verify",
        ])
        .unwrap();
        match cli.command {
            Commands::Scan {
                source,
                output,
                dry_run,
                include,
                verify,
                ..
            } => {
                assert_eq!(source, "openapi");
                assert_eq!(output, "yaml");
                assert!(dry_run);
                assert_eq!(include.unwrap(), "users");
                assert!(verify);
            }
            _ => panic!("Expected Scan command"),
        }
    }

    #[test]
    fn test_cli_parse_serve_with_options() {
        let cli = Cli::try_parse_from([
            "axum-apcore",
            "serve",
            "--transport",
            "sse",
            "--host",
            "0.0.0.0",
            "--port",
            "8080",
            "--tags",
            "users,tasks",
        ])
        .unwrap();
        match cli.command {
            Commands::Serve {
                transport,
                host,
                port,
                tags,
                ..
            } => {
                assert_eq!(transport.unwrap(), "sse");
                assert_eq!(host.unwrap(), "0.0.0.0");
                assert_eq!(port.unwrap(), 8080);
                assert_eq!(tags.unwrap(), "users,tasks");
            }
            _ => panic!("Expected Serve command"),
        }
    }
}
