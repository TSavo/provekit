// ProvekIt Language Server Protocol implementation.
//
// A language-agnostic LSP coordinator. Reads `.provekit/config.toml` to discover
// language plugins. Routes each source file to the correct parser (built-in or
// external RPC plugin). Delegates verification to a configurable JSON-RPC backend.
//
// ## Modes of operation
//
// ### Per-plugin subprocess mode (default)
//
// Each language is handled by a per-kit plugin binary that speaks the
// `provekit-lsp-plugin/1` NDJSON protocol (initialize/parse/shutdown).
// The plugin returns `{annotations: [...]}` for each file.  Diagnostics
// come from the local `JsonRpcBackend` (e.g., `provekit verify`).
//
// Usage: provekit-lsp [--config <path>]
//
// To add a new language, create a binary that speaks `provekit-lsp-plugin/1`:
//   1. Receives `initialize` -> responds with name/version
//   2. Receives `parse` with {uri, text} -> responds with {annotations: [...]}
//   3. Receives `shutdown` -> exits
//
// Then add to `.provekit/config.toml`:
//   [[language]]
//   name = "mylang"
//   extensions = [".mylang"]
//   plugin = "provekit-lsp-mylang"
//
// ### Daemon-client mode (opt-in)
//
// When a daemon socket path is supplied (via `--daemon-socket <path>` CLI flag
// or `server.daemon_socket` in config.toml), `did_open` / `did_change` events
// are forwarded to `provekit-linkerd` as `parseFile` JSON-RPC calls instead of
// the per-plugin subprocess path.  The daemon owns the cross-kit cache; the LSP
// server is a thin adapter that converts `LinterError` diagnostics returned by
// the daemon to LSP `Diagnostic` objects and publishes them via
// `client.publish_diagnostics`.
//
// Per-plugin mode and daemon-client mode are mutually exclusive per-file: when
// daemon mode is active the per-plugin subprocess path is bypassed.  Per-plugin
// plugins for non-rust kits still work in daemon mode for their own files once
// those kits gain daemon support; for now the daemon handles `rust` kit only.
//
// Usage: provekit-lsp --daemon-socket /run/user/1000/provekit/linkerd-<cid>.sock
//
// The daemon is the `provekit-linkerd` binary (LSP+linker step 2).  All five
// JSON-RPC methods (parseFile, getDiagnostics, projectStatus, flushCache,
// shutdown) are defined in `protocol/specs/2026-05-04-linker-daemon-protocol.md`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

mod backend;
mod config;
mod parser;
mod plugin;

use backend::JsonRpcBackend;
use config::LspConfig;
use parser::{Annotation, AnnotationKind, SourceAnnotations};
use plugin::LanguagePlugin;

/// Per-language plugin handle (built-in or external RPC).
#[derive(Debug)]
enum LanguageHandle {
    BuiltinRust,
    External(Arc<std::sync::Mutex<LanguagePlugin>>),
}

// ---------------------------------------------------------------------------
// Daemon-client mode: wire types
// ---------------------------------------------------------------------------

/// A single diagnostic entry from the daemon's `parseFile` response.
///
/// Wire shape emitted by `provekit-linkerd` methods.rs:
/// ```json
/// {
///   "kind":              "linker-error",
///   "errorKind":         "unresolved-symbol" | "unprovable-obligation",
///   "targetSymbol":      "<string>",
///   "sourceContractCid": "<string>",
///   "reason":            "<string>",
///   "file":              "<string | null>"
/// }
/// ```
///
/// Note: no per-line locus data in this daemon MVP.  Diagnostics are attached
/// at (0,0) until `LinkerError` gains locus propagation upstream.
#[derive(Debug, serde::Deserialize)]
struct DaemonDiagnostic {
    /// Discriminator for the linker-error category; maps to LSP severity.
    #[serde(rename = "errorKind", default)]
    error_kind: String,
    /// The unresolved or obligation-violating symbol name.
    #[serde(rename = "targetSymbol", default)]
    target_symbol: String,
    /// Human-readable explanation from the linker.
    #[serde(default)]
    reason: String,
}

/// Convert a single daemon `DaemonDiagnostic` into an LSP `Diagnostic`.
///
/// Range is (0,0)..(0,1) — whole-file marker — because the daemon MVP does
/// not yet propagate call-site locus from `LinkerCallEdge` into `LinkerError`.
/// Once that propagation is added upstream, this function should read locus
/// fields and produce a precise range.
fn daemon_diag_to_lsp(d: &DaemonDiagnostic) -> Diagnostic {
    let range = Range {
        start: Position { line: 0, character: 0 },
        end: Position { line: 0, character: 1 },
    };
    let severity = match d.error_kind.as_str() {
        "unprovable-obligation" => Some(DiagnosticSeverity::ERROR),
        "unresolved-symbol" => Some(DiagnosticSeverity::WARNING),
        _ => Some(DiagnosticSeverity::INFORMATION),
    };
    let message = match d.error_kind.as_str() {
        "unprovable-obligation" => format!(
            "cannot verify {}'s precondition; postcondition at call site does not establish it ({})",
            d.target_symbol, d.reason
        ),
        "unresolved-symbol" => format!(
            "cannot resolve {} against any kit in the project ({})",
            d.target_symbol, d.reason
        ),
        _ => d.reason.clone(),
    };
    Diagnostic {
        range,
        severity,
        code: Some(NumberOrString::String(format!("provekit:{}", d.error_kind))),
        source: Some("provekit".to_string()),
        message,
        ..Default::default()
    }
}

#[derive(Debug)]
struct ProvekitLanguageServer {
    client: Client,
    /// The JSON-RPC verification backend.  `Some` in per-plugin mode; `None`
    /// in daemon-client mode (the daemon handles analysis; the backend is not
    /// needed and is not spawned).
    backend: Option<Arc<Mutex<JsonRpcBackend>>>,
    config: LspConfig,
    documents: Arc<Mutex<HashMap<Url, SourceAnnotations>>>,
    plugins: Arc<Mutex<HashMap<String, LanguageHandle>>>,
    project_root: PathBuf,
    /// Path to the provekit-linkerd Unix domain socket, if daemon-client mode
    /// is active.  `None` means per-plugin subprocess mode (the default).
    daemon_socket: Option<PathBuf>,
    /// Lazy-connected daemon stream, protected by a mutex so multiple async
    /// tasks can share the single persistent connection.  `None` until the
    /// first `did_open` / `did_change` event in daemon mode.
    daemon_stream: Arc<Mutex<Option<std::os::unix::net::UnixStream>>>,
}

#[tower_lsp::async_trait]
impl LanguageServer for ProvekitLanguageServer {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        // Determine project root from workspace folders or root_uri
        let root = params
            .root_uri
            .as_ref()
            .map(|u| PathBuf::from(u.path()))
            .or_else(|| {
                params.workspace_folders.as_ref().and_then(|folders| {
                    folders.first().map(|f| PathBuf::from(f.uri.path()))
                })
            })
            .unwrap_or_else(|| PathBuf::from("."));

        // Initialize plugins from config
        self.init_plugins(&root).await;

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                diagnostic_provider: Some(DiagnosticServerCapabilities::Options(
                    DiagnosticOptions {
                        identifier: Some("provekit".to_string()),
                        inter_file_dependencies: true,
                        workspace_diagnostics: false,
                        work_done_progress_options: WorkDoneProgressOptions::default(),
                    },
                )),
                code_lens_provider: Some(CodeLensOptions {
                    resolve_provider: Some(false),
                }),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                ..ServerCapabilities::default()
            },
            ..InitializeResult::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "ProvekIt LSP server initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        // Shut down all external plugins
        let mut plugins = self.plugins.lock().await;
        for (_name, handle) in plugins.drain() {
            if let LanguageHandle::External(plugin) = handle {
                let _ = tokio::task::spawn_blocking(move || {
                    if let Ok(mut p) = plugin.lock() {
                        let _ = p.shutdown();
                    }
                });
            }
        }
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        let lang_id = params.text_document.language_id;
        self.update_document(uri, text, lang_id).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let lang_id = self
            .documents
            .lock()
            .await
            .get(&uri)
            .map(|_| String::new())
            .unwrap_or_default();
        // Full sync: take the last content change
        if let Some(change) = params.content_changes.last() {
            self.update_document(uri, change.text.clone(), lang_id).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        {
            let mut docs = self.documents.lock().await;
            docs.remove(&uri);
        }
        // Clear any published diagnostics for this file so the editor pane
        // goes clean.  This applies to both per-plugin and daemon-client mode.
        self.client
            .publish_diagnostics(uri, vec![], None)
            .await;
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let docs = self.documents.lock().await;
        let annotations = match docs.get(&uri) {
            Some(a) => a,
            None => return Ok(None),
        };

        for ann in &annotations.annotations {
            if is_in_range(position, ann.range) {
                return Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format_hover(ann),
                    }),
                    range: Some(ann.range),
                }));
            }
        }

        Ok(None)
    }

    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        let uri = params.text_document.uri;
        let docs = self.documents.lock().await;
        let annotations = match docs.get(&uri) {
            Some(a) => a,
            None => return Ok(None),
        };

        let mut lenses = Vec::new();
        for ann in &annotations.annotations {
            if let Some(cid) = &ann.target_cid {
                lenses.push(CodeLens {
                    range: ann.range,
                    command: Some(Command {
                        title: format!("🔍 Verify: {}", cid),
                        command: "provekit.verify".to_string(),
                        arguments: Some(vec![
                            serde_json::json!(ann.function_name),
                            serde_json::json!(cid),
                        ]),
                    }),
                    data: None,
                });
            }
        }

        Ok(Some(lenses))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;
        let range = params.range;

        let docs = self.documents.lock().await;
        let annotations = match docs.get(&uri) {
            Some(a) => a,
            None => return Ok(None),
        };

        let mut actions = Vec::new();
        for ann in &annotations.annotations {
            if overlaps_range(range, ann.range) {
                if let Some(cid) = &ann.target_cid {
                    actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                        title: format!("Re-verify against {}", cid),
                        kind: Some(CodeActionKind::QUICKFIX),
                        diagnostics: None,
                        edit: None,
                        command: Some(Command {
                            title: "Re-verify".to_string(),
                            command: "provekit.reverify".to_string(),
                            arguments: Some(vec![
                                serde_json::json!(ann.function_name),
                                serde_json::json!(cid),
                            ]),
                        }),
                        is_preferred: Some(false),
                        ..CodeAction::default()
                    }));
                }
            }
        }

        Ok(Some(actions))
    }
}

impl ProvekitLanguageServer {
    async fn init_plugins(&self, project_root: &std::path::Path) {
        let mut plugins = self.plugins.lock().await;
        for lang in &self.config.language {
            if lang.parser.as_deref() == Some("builtin:rust") || lang.parser.as_deref() == Some("builtin") {
                plugins.insert(lang.name.clone(), LanguageHandle::BuiltinRust);
                continue;
            }
            if let Some(plugin_name) = &lang.plugin {
                match plugin::load_plugin(project_root, lang) {
                    Ok(p) => {
                        plugins.insert(
                            lang.name.clone(),
                            LanguageHandle::External(Arc::new(std::sync::Mutex::new(p))),
                        );
                    }
                    Err(e) => {
                        self.client
                            .log_message(
                                MessageType::WARNING,
                                format!(
                                    "Failed to load language plugin `{}`: {}",
                                    plugin_name, e
                                ),
                            )
                            .await;
                    }
                }
            }
        }
    }

    async fn update_document(&self, uri: Url, text: String, _lang_id: String) {
        // --- Daemon-client mode: route through provekit-linkerd ---
        if let Some(sock_path) = &self.daemon_socket {
            self.daemon_routed_parse(uri, text, sock_path.clone()).await;
            return;
        }

        // --- Per-plugin subprocess mode (default) ---

        // Determine language from file extension
        let path = PathBuf::from(uri.path());
        let lang_config = self.config.for_path(&path);

        let annotations = match lang_config {
            Some(cfg) => {
                let plugins = self.plugins.lock().await;
                match plugins.get(&cfg.name) {
                    Some(LanguageHandle::BuiltinRust) => parser::parse_rust_source(&text),
                    Some(LanguageHandle::External(plugin)) => {
                        let plugin = plugin.clone();
                        let uri_str = uri.to_string();
                        // Run blocking plugin call in spawn_blocking
                        match tokio::task::spawn_blocking(move || {
                            let mut p = plugin.lock().unwrap();
                            p.parse(&uri_str, &text)
                        })
                        .await
                        {
                            Ok(Ok(anns)) => anns,
                            Ok(Err(e)) => {
                                self.client
                                    .log_message(MessageType::ERROR, format!("Plugin parse error: {}", e))
                                    .await;
                                SourceAnnotations { annotations: Vec::new() }
                            }
                            Err(e) => {
                                self.client
                                    .log_message(MessageType::ERROR, format!("Plugin task panicked: {}", e))
                                    .await;
                                SourceAnnotations { annotations: Vec::new() }
                            }
                        }
                    }
                    None => {
                        self.client
                            .log_message(
                                MessageType::WARNING,
                                format!("No plugin loaded for language `{}`", cfg.name),
                            )
                            .await;
                        SourceAnnotations { annotations: Vec::new() }
                    }
                }
            }
            None => {
                // Unknown file type — try built-in Rust as fallback, or skip
                if uri.path().ends_with(".rs") {
                    parser::parse_rust_source(&text)
                } else {
                    SourceAnnotations { annotations: Vec::new() }
                }
            }
        };

        // Store parsed annotations
        {
            let mut docs = self.documents.lock().await;
            docs.insert(uri.clone(), annotations.clone());
        }

        // Queue verification for annotations with target CIDs (per-plugin mode only).
        if let Some(backend) = &self.backend {
            for ann in &annotations.annotations {
                if let Some(cid) = &ann.target_cid {
                    let backend = backend.clone();
                    let client = self.client.clone();
                    let uri_clone = uri.clone();
                    let function_name = ann.function_name.clone();
                    let cid = cid.clone();
                    let range = ann.range;

                    tokio::spawn(async move {
                        let mut backend = backend.lock().await;
                        match backend.verify(&function_name, &cid).await {
                            Ok(result) => {
                                let diagnostics = build_diagnostics(&result, range);
                                client
                                    .publish_diagnostics(uri_clone, diagnostics, None)
                                    .await;
                            }
                            Err(e) => {
                                client
                                    .log_message(
                                        MessageType::ERROR,
                                        format!("Verification failed: {}", e),
                                    )
                                    .await;
                            }
                        }
                    });
                }
            }
        }
    }

    /// Forward an open/change event to the provekit-linkerd daemon via
    /// `parseFile` JSON-RPC, convert the returned diagnostics, and publish
    /// them.  Lazily connects to the daemon socket on first call.
    ///
    /// Uses `tokio::task::spawn_blocking` because `daemon_client` operations
    /// (`connect_or_spawn`, `send_parse_file`) are synchronous std I/O.
    async fn daemon_routed_parse(
        &self,
        uri: Url,
        text: String,
        sock_path: PathBuf,
    ) {
        let daemon_stream = self.daemon_stream.clone();
        let client = self.client.clone();
        let file_path = uri.path().to_string();

        let result = tokio::task::spawn_blocking(move || {
            use provekit_lsp_rust::daemon_client;

            let mut guard = daemon_stream.blocking_lock();

            // Lazy connect / spawn.
            if guard.is_none() {
                match daemon_client::connect_or_spawn(&sock_path, "provekit-lsp") {
                    Ok(stream) => {
                        *guard = Some(stream);
                    }
                    Err(e) => {
                        return Err(format!(
                            "daemon-client: failed to connect to {}: {}",
                            sock_path.display(), e
                        ));
                    }
                }
            }

            let stream = guard.as_mut().unwrap();
            daemon_client::send_parse_file(stream, "rust", &file_path, &text, 1)
                .map_err(|e| {
                    // Connection may have dropped; clear so we reconnect next time.
                    format!("daemon-client send_parse_file failed: {e}")
                })
        })
        .await;

        match result {
            Ok(Ok(raw_diags)) => {
                // Deserialize daemon JSON -> DaemonDiagnostic -> LSP Diagnostic.
                let diagnostics: Vec<Diagnostic> = raw_diags
                    .iter()
                    .filter_map(|v| {
                        serde_json::from_value::<DaemonDiagnostic>(v.clone()).ok()
                    })
                    .map(|d| daemon_diag_to_lsp(&d))
                    .collect();

                client.publish_diagnostics(uri, diagnostics, None).await;
            }
            Ok(Err(e)) => {
                // Clear the stale stream so the next call reconnects.
                {
                    let mut guard = self.daemon_stream.lock().await;
                    *guard = None;
                }
                client
                    .log_message(MessageType::WARNING, format!("provekit daemon: {}", e))
                    .await;
                // Publish empty diagnostics to clear any stale markers.
                client.publish_diagnostics(uri, vec![], None).await;
            }
            Err(join_err) => {
                client
                    .log_message(
                        MessageType::ERROR,
                        format!("provekit daemon task panicked: {}", join_err),
                    )
                    .await;
            }
        }
    }
}

fn is_in_range(position: Position, range: Range) -> bool {
    (position.line > range.start.line
        || (position.line == range.start.line && position.character >= range.start.character))
        && (position.line < range.end.line
            || (position.line == range.end.line && position.character <= range.end.character))
}

fn overlaps_range(a: Range, b: Range) -> bool {
    a.start.line <= b.end.line && a.end.line >= b.start.line
}

fn format_hover(ann: &Annotation) -> String {
    match &ann.kind {
        AnnotationKind::Implement { target_cid } => {
            format!(
                "## ProvekIt Contract\n\n**Function:** `{}`\n**Kind:** implement\n**Target CID:** `{}`\n\nThis function is bound to the contract at the given CID. The framework will verify that the function body satisfies the contract's postcondition.",
                ann.function_name, target_cid
            )
        }
        AnnotationKind::Contract => {
            format!(
                "## ProvekIt Contract\n\n**Function:** `{}`\n**Kind:** contract\n\nThis function declares its own contract via `#[provekit::contract]`.",
                ann.function_name
            )
        }
        AnnotationKind::Verify => {
            format!(
                "## ProvekIt Verify\n\n**Function:** `{}`\n**Kind:** verify\n\nThis function is marked for verification against its contract.",
                ann.function_name
            )
        }
    }
}

fn build_diagnostics(result: &backend::VerifyResult, range: Range) -> Vec<Diagnostic> {
    match result.status.as_str() {
        "verified" => vec![Diagnostic {
            range,
            severity: Some(DiagnosticSeverity::HINT),
            code: Some(NumberOrString::String("provekit.verified".to_string())),
            source: Some("provekit".to_string()),
            message: format!(
                "✅ Bridge verified: {} domain transfers",
                result.transfers.len()
            ),
            related_information: None,
            code_description: None,
            data: None,
            tags: None,
        }],
        "violation" => vec![Diagnostic {
            range,
            severity: Some(DiagnosticSeverity::ERROR),
            code: Some(NumberOrString::String("provekit.violation".to_string())),
            source: Some("provekit".to_string()),
            message: format!(
                "❌ Contract violation: {}",
                result.error.as_deref().unwrap_or("unknown")
            ),
            related_information: None,
            code_description: None,
            data: None,
            tags: None,
        }],
        _ => vec![Diagnostic {
            range,
            severity: Some(DiagnosticSeverity::WARNING),
            code: Some(NumberOrString::String("provekit.unknown".to_string())),
            source: Some("provekit".to_string()),
            message: format!("⚠️ Unknown verification status: {}", result.status),
            related_information: None,
            code_description: None,
            data: None,
            tags: None,
        }],
    }
}

#[tokio::main]
async fn main() {
    let mut config_path = ".provekit/config.toml".to_string();
    // CLI flag `--daemon-socket <path>` overrides config.server.daemon_socket.
    let mut daemon_socket_cli: Option<String> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" => {
                if let Some(path) = args.next() {
                    config_path = path;
                }
            }
            "--daemon-socket" => {
                if let Some(path) = args.next() {
                    daemon_socket_cli = Some(path);
                }
            }
            _ => {}
        }
    }

    // Read config
    let config = config::load_config(&config_path).unwrap_or_default();

    // Resolve daemon socket: CLI flag wins over config file entry.
    let daemon_socket: Option<PathBuf> = daemon_socket_cli
        .as_deref()
        .or(config.server.daemon_socket.as_deref())
        .map(PathBuf::from);

    let backend_path = config.server.backend.clone();

    // Spawn backend in per-plugin mode.  In daemon-client mode, the daemon
    // handles all analysis so no backend binary is needed.
    let backend: Option<Arc<Mutex<JsonRpcBackend>>> = if daemon_socket.is_some() {
        eprintln!(
            "provekit-lsp: daemon-client mode active (socket: {})",
            daemon_socket.as_ref().unwrap().display()
        );
        None
    } else {
        match JsonRpcBackend::spawn(&backend_path, &config.server.backend_args).await {
            Ok(b) => Some(Arc::new(Mutex::new(b))),
            Err(e) => {
                eprintln!("Failed to spawn backend '{}': {}", backend_path, e);
                std::process::exit(1);
            }
        }
    };

    // Start LSP
    let (stdin, stdout) = (tokio::io::stdin(), tokio::io::stdout());
    let (service, socket) = LspService::new(|client| ProvekitLanguageServer {
        client,
        backend,
        config,
        documents: Arc::new(Mutex::new(HashMap::new())),
        plugins: Arc::new(Mutex::new(HashMap::new())),
        project_root: PathBuf::from("."),
        daemon_socket,
        daemon_stream: Arc::new(Mutex::new(None)),
    });

    Server::new(stdin, stdout, socket).serve(service).await;
}
