// ProvekIt Language Server Protocol implementation.
//
// A language-agnostic LSP coordinator. Reads `.provekit/config.toml` to discover
// language plugins. Routes each source file to the correct parser (built-in or
// external RPC plugin). Delegates verification to a configurable JSON-RPC backend.
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

#[derive(Debug)]
struct ProvekitLanguageServer {
    client: Client,
    backend: Arc<Mutex<JsonRpcBackend>>,
    config: LspConfig,
    documents: Arc<Mutex<HashMap<Url, SourceAnnotations>>>,
    plugins: Arc<Mutex<HashMap<String, LanguageHandle>>>,
    project_root: PathBuf,
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
        let mut docs = self.documents.lock().await;
        docs.remove(&params.text_document.uri);
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

        // Queue verification for annotations with target CIDs
        for ann in &annotations.annotations {
            if let Some(cid) = &ann.target_cid {
                let backend = self.backend.clone();
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

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" => {
                if let Some(path) = args.next() {
                    config_path = path;
                }
            }
            _ => {}
        }
    }

    // Read config
    let config = config::load_config(&config_path).unwrap_or_default();
    let backend_path = config.server.backend.clone();

    // Spawn backend
    let backend = match JsonRpcBackend::spawn(&backend_path, &config.server.backend_args).await {
        Ok(b) => Arc::new(Mutex::new(b)),
        Err(e) => {
            eprintln!("Failed to spawn backend '{}': {}", backend_path, e);
            std::process::exit(1);
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
    });

    Server::new(stdin, stdout, socket).serve(service).await;
}
