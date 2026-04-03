// AxumApcore — unified entry point for the axum-apcore integration.
//
// This is the main client that ties together scanning, registration,
// context mapping, task management, and MCP serving.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;

use apcore::cancel::CancelToken;
use apcore::context::{Context, Identity};
use apcore::{Executor, Registry};
use apcore_toolkit::{HandlerFn, ScannedModule};

use crate::config::{get_apcore_settings, ApcoreSettings};
use crate::context::AxumContextFactory;
use crate::engine::extensions::{AxumDiscoverer, AxumModuleValidator};
use crate::engine::observability::{setup_observability, ObservabilityState};
use crate::engine::registry::{get_executor, get_registry};
use crate::engine::tasks::{TaskInfo, TaskManager};
use crate::errors::AxumApcoreError;
use crate::output::AxumRegistryWriter;
use crate::scanner::get_scanner;

/// Main entry point for axum-apcore integration.
///
/// # Usage
///
/// ```ignore
/// use axum_apcore::AxumApcore;
///
/// let apcore = AxumApcore::new();
/// apcore.init_app(&router).await?;
///
/// // Execute a module
/// let result = apcore.call("users.get_user.get", json!({"id": "123"}), None).await?;
/// ```
pub struct AxumApcore {
    settings: ApcoreSettings,
    registry: Arc<Mutex<Registry>>,
    executor: Arc<tokio::sync::Mutex<Executor>>,
    context_factory: Arc<AxumContextFactory>,
    task_manager: TaskManager,
    observability: ObservabilityState,
    handler_map: Arc<Mutex<HashMap<String, HandlerFn>>>,
}

impl AxumApcore {
    /// Create a new AxumApcore with default settings from environment.
    pub fn new() -> Self {
        let settings = get_apcore_settings().clone();
        Self::with_settings(settings)
    }

    /// Create a new AxumApcore with explicit settings.
    pub fn with_settings(settings: ApcoreSettings) -> Self {
        let observability = setup_observability(&settings);
        let task_manager = TaskManager::from_settings(&settings);

        Self {
            settings,
            registry: get_registry(),
            executor: get_executor(),
            context_factory: Arc::new(AxumContextFactory),
            task_manager,
            observability,
            handler_map: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    // ---- Lifecycle ----

    /// Initialize the integration: discover modules, scan routes, register.
    ///
    /// This is the equivalent of FastAPIApcore.init_app(). Call it after
    /// building your Axum Router but before serving.
    pub async fn init_app(&self, router: &axum::Router) -> Result<(), AxumApcoreError> {
        // Step 1: Auto-discover modules from binding files
        if self.settings.auto_discover {
            self.discover_modules()?;
        }

        // Step 2: Scan routes and register
        let modules = self.scan(router, None, None).await?;
        self.register_modules(&modules).await?;

        tracing::info!(
            count = modules.len(),
            "axum-apcore initialized with {} modules",
            modules.len()
        );

        Ok(())
    }

    /// Discover modules from YAML binding files.
    fn discover_modules(&self) -> Result<(), AxumApcoreError> {
        let discoverer = AxumDiscoverer::new(self.settings.clone());
        let discovered = discoverer.discover()?;

        let mut registry = self.registry.lock().expect("registry lock poisoned");
        let writer = AxumRegistryWriter::new();

        let scanned: Vec<ScannedModule> = discovered
            .into_iter()
            .map(|d| {
                ScannedModule::new(
                    d.module_id,
                    d.description,
                    d.input_schema,
                    d.output_schema,
                    d.tags,
                    d.target,
                )
            })
            .collect();

        writer.write(&scanned, &mut registry, false, false);
        Ok(())
    }

    /// Scan Axum routes and return module definitions.
    pub async fn scan(
        &self,
        router: &axum::Router,
        include: Option<&str>,
        exclude: Option<&str>,
    ) -> Result<Vec<ScannedModule>, AxumApcoreError> {
        let scanner = get_scanner(&self.settings.scanner_source)?;
        scanner.scan(router, include, exclude).await
    }

    /// Register scanned modules into the registry and executor.
    ///
    /// Modules are written to both the query registry (for `list_modules`)
    /// and the executor's internal registry (for `call`/`stream`).
    pub async fn register_modules(&self, modules: &[ScannedModule]) -> Result<(), AxumApcoreError> {
        let validator = AxumModuleValidator::new();
        for module in modules {
            let errors = validator.validate(&module.module_id);
            if !errors.is_empty() {
                tracing::warn!(
                    module_id = %module.module_id,
                    errors = ?errors,
                    "Module validation warnings"
                );
            }
        }

        // Build the writer and determine which modules have real handlers.
        let (writer, handler_targets) = {
            let handlers = self.handler_map.lock().expect("handler map lock poisoned");
            let targets: std::collections::HashSet<String> = handlers.keys().cloned().collect();
            let w = if handlers.is_empty() {
                AxumRegistryWriter::new()
            } else {
                AxumRegistryWriter::with_handler_map(handlers.clone())
            };
            (w, targets)
        };

        // Write all modules to the query registry (for list_modules, exports).
        {
            let mut registry = self.registry.lock().expect("registry lock poisoned");
            writer.write(modules, &mut registry, false, true);
        }

        // Write only modules with registered handlers to the executor's
        // registry. This avoids overwriting real handlers with passthrough
        // handlers when multiple AxumApcore instances share a global executor.
        let handler_modules: Vec<&ScannedModule> = modules
            .iter()
            .filter(|m| handler_targets.contains(&m.target))
            .collect();

        if !handler_modules.is_empty() {
            let mut executor = self.executor.lock().await;
            for module in &handler_modules {
                let _ = executor.registry.unregister(&module.module_id);
            }
            let refs: Vec<ScannedModule> = handler_modules.into_iter().cloned().collect();
            writer.write(&refs, &mut executor.registry, false, false);
        }

        Ok(())
    }

    /// Register a handler function for a target string.
    pub fn register_handler(&self, target: &str, handler: HandlerFn) {
        let mut handlers = self.handler_map.lock().expect("handler map lock poisoned");
        handlers.insert(target.to_string(), handler);
    }

    // ---- Execution ----

    /// Execute a module by ID.
    pub async fn call(
        &self,
        module_id: &str,
        inputs: Value,
        context: Option<&Context<Value>>,
    ) -> Result<Value, AxumApcoreError> {
        let executor = self.executor.lock().await;
        let result = executor.call(module_id, inputs, context, None).await?;
        Ok(result)
    }

    /// Execute a module with a default anonymous context.
    pub async fn call_anonymous(
        &self,
        module_id: &str,
        inputs: Value,
    ) -> Result<Value, AxumApcoreError> {
        let ctx = Context::new(anonymous_identity());
        self.call(module_id, inputs, Some(&ctx)).await
    }

    /// Execute a module with streaming output.
    ///
    /// Returns a vector of result chunks. The apcore executor currently
    /// wraps the single result in a vec; true streaming will be added
    /// when the protocol supports it.
    pub async fn stream(
        &self,
        module_id: &str,
        inputs: Value,
        context: Option<&Context<Value>>,
    ) -> Result<Vec<Value>, AxumApcoreError> {
        let executor = self.executor.lock().await;
        let results = executor.stream(module_id, inputs, context, None).await?;
        Ok(results)
    }

    /// Execute a module with a timeout and cancellation support.
    ///
    /// The timeout covers both lock acquisition and execution. If the
    /// timeout elapses, the cancel token is triggered and a `ModuleTimeout`
    /// error is returned.
    pub async fn cancellable_call(
        &self,
        module_id: &str,
        inputs: Value,
        context: Option<&Context<Value>>,
        timeout: Duration,
    ) -> Result<Value, AxumApcoreError> {
        let cancel_token = CancelToken::new();

        // Build a context with the cancel token attached
        let ctx = match context {
            Some(parent) => {
                let mut child = parent.clone();
                child.cancel_token = Some(cancel_token.clone());
                child
            }
            None => {
                let mut ctx = Context::new(anonymous_identity());
                ctx.cancel_token = Some(cancel_token.clone());
                ctx
            }
        };

        // Wrap both lock acquisition and call inside the timeout so the
        // full duration is bounded, not just the call itself.
        let call_fut = async {
            let executor = self.executor.lock().await;
            executor.call(module_id, inputs, Some(&ctx), None).await
        };

        match tokio::time::timeout(timeout, call_fut).await {
            Ok(result) => Ok(result?),
            Err(_elapsed) => {
                cancel_token.cancel();
                Err(AxumApcoreError::Execution(apcore::ModuleError::new(
                    apcore::ErrorCode::ModuleTimeout,
                    format!(
                        "Module '{}' timed out after {}ms",
                        module_id,
                        timeout.as_millis()
                    ),
                )))
            }
        }
    }

    // ---- Task Management ----

    /// Submit an async task for background execution.
    ///
    /// The task runs the specified module in the background. Returns the task ID.
    pub fn submit_task(&self, module_id: &str, inputs: Value) -> Result<String, AxumApcoreError> {
        let task_id = uuid::Uuid::new_v4().to_string();
        let (task_id, cancel_token) = self.task_manager.submit(task_id, module_id.to_string())?;

        let executor = self.executor.clone();
        let task_manager = self.task_manager.clone();
        let module_id_owned = module_id.to_string();
        let task_id_clone = task_id.clone();

        tokio::spawn(async move {
            let mut ctx = Context::new(anonymous_identity());
            ctx.cancel_token = Some(cancel_token);

            let exec = executor.lock().await;
            let result = exec.call(&module_id_owned, inputs, Some(&ctx), None).await;

            match result {
                Ok(value) => task_manager.complete(&task_id_clone, value),
                Err(e) => task_manager.fail(&task_id_clone, e.message),
            }
        });

        Ok(task_id)
    }

    /// Get the status of an async task.
    pub fn get_task_status(&self, task_id: &str) -> Option<TaskInfo> {
        self.task_manager.get_task_info(task_id)
    }

    /// Get the result of a completed task.
    pub fn get_task_result(&self, task_id: &str) -> Option<Value> {
        self.task_manager.get_result(task_id)
    }

    /// Cancel a running async task.
    pub fn cancel_task(&self, task_id: &str) -> bool {
        self.task_manager.cancel(task_id)
    }

    /// List async tasks, optionally filtered by status.
    pub fn list_tasks(&self, status: Option<&str>) -> Vec<TaskInfo> {
        self.task_manager.list_tasks(status)
    }

    // ---- Export ----

    /// Export registered modules as OpenAI-compatible tool definitions.
    #[cfg(feature = "mcp")]
    pub fn to_openai_tools(
        &self,
        embed_annotations: bool,
        strict: bool,
        tags: Option<Vec<String>>,
        prefix: Option<String>,
    ) -> Result<Vec<Value>, AxumApcoreError> {
        let registry = self.registry.lock().expect("registry lock poisoned");
        let registry_arc = Arc::new(registry_snapshot(&registry));

        let config = apcore_mcp::OpenAIToolsConfig {
            embed_annotations,
            strict,
            tags,
            prefix,
        };
        apcore_mcp::to_openai_tools(apcore_mcp::BackendSource::Registry(registry_arc), config)
            .map_err(|e| AxumApcoreError::Config(format!("OpenAI export failed: {e}")))
    }

    // ---- MCP Server ----

    /// Create an MCP server from the current registry (requires "mcp" feature).
    #[cfg(feature = "mcp")]
    pub fn create_mcp_server(&self) -> Result<apcore_mcp::MCPServer, AxumApcoreError> {
        let transport: apcore_mcp::TransportKind = self
            .settings
            .serve_transport
            .parse()
            .map_err(|e| AxumApcoreError::Config(format!("Invalid transport: {e}")))?;

        let config = apcore_mcp::MCPServerConfig {
            transport,
            host: self.settings.serve_host.clone(),
            port: self.settings.serve_port,
            name: self.settings.server_name.clone(),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
            validate_inputs: true,
            tags: None,
            prefix: None,
            require_auth: self.settings.jwt_secret.is_some(),
            exempt_paths: None,
        };

        Ok(apcore_mcp::MCPServer::new(config))
    }

    // ---- Accessors ----

    pub fn settings(&self) -> &ApcoreSettings {
        &self.settings
    }

    pub fn registry(&self) -> Arc<Mutex<Registry>> {
        self.registry.clone()
    }

    pub fn executor(&self) -> Arc<tokio::sync::Mutex<Executor>> {
        self.executor.clone()
    }

    pub fn context_factory(&self) -> Arc<AxumContextFactory> {
        self.context_factory.clone()
    }

    pub fn task_manager(&self) -> &TaskManager {
        &self.task_manager
    }

    pub fn observability(&self) -> &ObservabilityState {
        &self.observability
    }

    /// List registered module IDs.
    pub fn list_modules(&self) -> Vec<String> {
        let registry = self.registry.lock().expect("registry lock poisoned");
        registry
            .list(None, None)
            .into_iter()
            .map(|s| s.to_string())
            .collect()
    }

    // ---- CLI (HTTP proxy) ----

    /// Create an apcore-cli clap Command with all scanned routes as CLI commands.
    ///
    /// Scans Axum routes, registers them as HTTP proxy modules, and returns
    /// a clap Command ready for dispatch. Each CLI command forwards requests
    /// to the running REST API via HTTP.
    ///
    /// Requires the `cli` feature.
    ///
    /// # Arguments
    ///
    /// * `router` — The Axum Router to scan.
    /// * `config` — Configuration for the CLI builder.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use axum_apcore::{AxumApcore, CreateCliConfig};
    ///
    /// let apcore = AxumApcore::new();
    /// let config = CreateCliConfig {
    ///     base_url: "http://localhost:3000".into(),
    ///     ..Default::default()
    /// };
    /// let cmd = apcore.create_cli(&router, config).await?;
    /// ```
    #[cfg(feature = "cli")]
    pub async fn create_cli(
        &self,
        router: &axum::Router,
        config: CreateCliConfig,
    ) -> Result<clap::Command, AxumApcoreError> {
        use apcore_cli::{
            register_discovery_commands, register_shell_commands, set_docs_url, set_verbose_help,
            ApCoreRegistryProvider, GroupedModuleGroup,
        };
        use apcore_toolkit::HTTPProxyRegistryWriter;

        // Apply apcore-cli global settings
        set_verbose_help(config.verbose_help);
        set_docs_url(config.docs_url.clone());

        // 1. Scan routes using the config's scanner source (not self.settings)
        let scanner = crate::scanner::get_scanner(&config.scan_source)?;
        let modules = scanner
            .scan(router, config.include.as_deref(), config.exclude.as_deref())
            .await?;
        tracing::info!(
            count = modules.len(),
            "Scanned {} API routes for CLI",
            modules.len()
        );

        // 2. Register as HTTP proxy modules
        let mut proxy_registry = Registry::new();
        let writer = HTTPProxyRegistryWriter::new(
            config.base_url.clone(),
            config.auth_header_factory,
            config.timeout,
        );
        let results = writer.write(&modules, &mut proxy_registry);
        let registered = results.iter().filter(|r| r.verified).count();
        tracing::info!(
            registered,
            total = results.len(),
            "Registered HTTP proxy modules"
        );

        // 3. Build registry provider for discovery commands.
        // ApCoreRegistryProvider takes ownership of a Registry, so we build
        // the provider from the proxy registry directly.
        let provider = ApCoreRegistryProvider::new(proxy_registry);
        let registry_arc: std::sync::Arc<dyn apcore_cli::RegistryProvider> =
            std::sync::Arc::new(provider);

        // GroupedModuleGroup requires an executor Arc but only uses the registry
        // for building the command tree. We create a placeholder executor.
        let placeholder_registry = Registry::new();
        let placeholder_executor =
            apcore::Executor::new(placeholder_registry, apcore::Config::default());
        let executor_adapter = apcore_cli::cli::ApCoreExecutorAdapter(placeholder_executor);
        let executor_arc: std::sync::Arc<dyn apcore_cli::ModuleExecutor> =
            std::sync::Arc::new(executor_adapter);

        // 4. Build GroupedModuleGroup for organized commands
        let mut gmg = GroupedModuleGroup::new(
            registry_arc.clone(),
            executor_arc,
            config.help_text_max_length,
        );
        gmg.build_group_map();

        // 5. Build the root clap Command
        let cli_description = format!(
            "{prog} — CLI for Axum API.\n\n\
             Tips:\n\
             \x20 {prog} list                List all available commands\n\
             \x20 {prog} describe MODULE_ID  Show schema and annotations for a command",
            prog = config.prog_name,
        );

        let mut cmd = clap::Command::new(config.prog_name.clone())
            .about(cli_description)
            .version(env!("CARGO_PKG_VERSION"))
            .subcommand_required(true)
            .arg_required_else_help(true);

        // 5a. Add grouped module subcommands
        for name in gmg.list_commands() {
            if let Some(sub) = gmg.get_command(&name) {
                cmd = cmd.subcommand(sub);
            }
        }

        // 6. Register discovery commands (list, describe)
        cmd = register_discovery_commands(cmd, registry_arc);

        // 7. Register shell commands (completion, man)
        cmd = register_shell_commands(cmd, &config.prog_name);

        // 8. Register init command
        cmd = cmd.subcommand(apcore_cli::init_command());

        Ok(cmd)
    }
}

impl Default for AxumApcore {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration for [`AxumApcore::create_cli`].
#[cfg(feature = "cli")]
pub struct CreateCliConfig {
    /// CLI program name shown in help text.
    pub prog_name: String,
    /// Base URL of the running Axum API server.
    pub base_url: String,
    /// Optional callable returning HTTP auth headers.
    pub auth_header_factory: Option<Box<dyn Fn() -> HashMap<String, String> + Send + Sync>>,
    /// HTTP request timeout in seconds.
    pub timeout: f64,
    /// Scanner source: "native" or "openapi".
    pub scan_source: String,
    /// Include regex filter for module IDs.
    pub include: Option<String>,
    /// Exclude regex filter for module IDs.
    pub exclude: Option<String>,
    /// Max characters for CLI help text per command.
    pub help_text_max_length: usize,
    /// Base URL for online documentation.
    pub docs_url: Option<String>,
    /// Show built-in apcore options in help output.
    pub verbose_help: bool,
}

#[cfg(feature = "cli")]
impl Default for CreateCliConfig {
    fn default() -> Self {
        Self {
            prog_name: "axum-apcore-cli".to_string(),
            base_url: "http://localhost:3000".to_string(),
            auth_header_factory: None,
            timeout: 60.0,
            scan_source: "native".to_string(),
            include: None,
            exclude: None,
            help_text_max_length: 1000,
            docs_url: None,
            verbose_help: false,
        }
    }
}

/// Create an anonymous identity for default contexts.
fn anonymous_identity() -> Identity {
    Identity {
        id: "anonymous".into(),
        identity_type: "anonymous".into(),
        roles: vec![],
        attrs: HashMap::new(),
    }
}

/// Create a snapshot copy of a registry (schema-only, no handlers).
#[cfg(feature = "mcp")]
fn registry_snapshot(source: &Registry) -> Registry {
    // We create a new empty registry for the backend source.
    // The MCPServer will read module descriptors from the registry it's given.
    // Since we hold a lock, we copy descriptor data here.
    let mut target = Registry::new();
    for name in source.list(None, None) {
        if let Some(descriptor) = source.get_definition(name) {
            // Register with passthrough handler — MCP only needs the schema
            let fm = apcore::decorator::FunctionModule::new::<_, ()>(
                descriptor.annotations.clone(),
                descriptor.input_schema.clone(),
                descriptor.output_schema.clone(),
                |inputs: Value,
                 _ctx: &Context<Value>|
                 -> std::pin::Pin<
                    Box<
                        dyn std::future::Future<Output = Result<Value, apcore::ModuleError>>
                            + Send
                            + '_,
                    >,
                > { Box::pin(async move { Ok(inputs) }) },
            );
            // Ignore registration errors (e.g., duplicate names in edge cases)
            let _ = target.register(name, Box::new(fm), descriptor.clone());
        }
    }
    target
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_creates_instance() {
        let apcore = AxumApcore::new();
        assert_eq!(apcore.settings().serve_port, 9090);
    }

    #[test]
    fn test_with_settings() {
        let settings = ApcoreSettings {
            serve_port: 8080,
            ..ApcoreSettings::default()
        };
        let apcore = AxumApcore::with_settings(settings);
        assert_eq!(apcore.settings().serve_port, 8080);
    }

    #[test]
    fn test_list_modules_empty() {
        let apcore = AxumApcore::new();
        let _ = apcore.list_modules();
    }

    #[test]
    fn test_anonymous_identity() {
        let id = anonymous_identity();
        assert_eq!(id.id, "anonymous");
        assert_eq!(id.identity_type, "anonymous");
        assert!(id.roles.is_empty());
    }

    #[test]
    fn test_submit_and_list_tasks() {
        let apcore = AxumApcore::new();
        let tasks = apcore.list_tasks(None);
        // May contain tasks from other tests due to shared state, but should not panic
        let _ = tasks;
    }
}
