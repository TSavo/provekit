// SPDX-License-Identifier: Apache-2.0
//
// provekit-lsp-rust: NDJSON LSP plugin for Rust.
//
// ## Operating modes
//
// ### Default mode (no `--daemon-socket` flag)
//
// Speaks the per-language plugin protocol used by every kit's LSP plugin:
//
//   {"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
//   {"jsonrpc":"2.0","id":2,"method":"parse","params":{"path":"...","source":"..."}}
//   {"jsonrpc":"2.0","id":3,"method":"shutdown"}
//
// The `parse` handler parses the source with `syn`, runs all registered
// lift adapters (proptest, contracts, kani, prusti, rust-tests, etc.),
// and returns the lifted ContractDecls as a JSON array in the shape:
//
//   {"declarations": [...], "warnings": [...]}
//
// Used by tooling that consumes lifter output directly (e.g. snapshot
// pipelines, CI checks).
//
// ### Daemon-client mode (`--daemon-socket <path>`)
//
// Forwards every `parse` request to the `provekit-linkerd` daemon as a
// `parseFile` JSON-RPC (spec `2026-05-04-linker-daemon-protocol.md` R5).
// The daemon runs the lifter in a dedicated long-running process, maintains
// the cross-language contract and call-edge union in memory, and returns
// per-file linker diagnostics.
//
// The `parse` response shape changes to:
//
//   {"diagnostics": [...]}
//
// where each element is a `LinterError` memento returned by the daemon.
//
// This mode is used by editor-facing components: in particular the real LSP
// server (`provekit-lsp-server`, step 3b of the LSP+linker path) that handles
// `textDocument/didOpen` and emits `publishDiagnostics` to the editor.
//
// Usage:
//   provekit-lsp-rust                          # default mode
//   provekit-lsp-rust --daemon-socket <path>   # daemon-client mode

mod daemon_client;

use std::io::{BufRead, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use provekit_ir_symbolic::{serialize::marshal_declarations, ContractDecl};
use provekit_lift::{
    adapter_contracts, adapter_creusot, adapter_flux, adapter_kani, adapter_proptest,
    adapter_prusti, adapter_quickcheck, adapter_rust_tests, adapter_verus,
};
use provekit_lsp_rust::forward_propagator::ForwardPropagator;
use serde_json::{json, Value};

const SHARED_PROTOCOL_VERSION: &str = "provekit-lsp-shared/1";
const PROTOCOL_CATALOG_CID: &str = "blake3-512:52bdb2be4b381cec2aff95db7755c84184878b45cd91882d262114a1abd2dd513f9ef3b250fb87093316fd0fcb48e4b97e109d463e57df5bda6aac0b1c719a0f";

fn main() {
    // Parse CLI.
    let args: Vec<String> = std::env::args().collect();
    let mut daemon_socket: Option<PathBuf> = None;

    let mut i = 1usize;
    while i < args.len() {
        if args[i] == "--daemon-socket" {
            i += 1;
            if let Some(v) = args.get(i) {
                daemon_socket = Some(PathBuf::from(v));
            }
        }
        i += 1;
    }

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    // Cached daemon connection and request-id counter.
    let mut daemon_stream: Option<UnixStream> = None;
    let req_counter = AtomicU64::new(1);

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let req: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                let resp = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {"code": -32700, "message": format!("parse error: {e}")}
                });
                let _ = writeln!(stdout, "{resp}");
                let _ = stdout.flush();
                continue;
            }
        };

        let id = req.get("id").cloned().unwrap_or(serde_json::Value::Null);
        let method = req.get("method").and_then(|v| v.as_str()).unwrap_or("");

        match method {
            "initialize" => {
                let resp = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": initialize_result()
                });
                let _ = writeln!(stdout, "{resp}");
                let _ = stdout.flush();
            }
            "analyzeDocument" => {
                let params = req.get("params").cloned().unwrap_or_default();
                let path = params
                    .get("file")
                    .or_else(|| params.get("path"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("source.rs");
                let uri = params.get("uri").and_then(|v| v.as_str()).unwrap_or(path);
                let source = params
                    .get("text")
                    .or_else(|| params.get("source"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let analysis = build_document_analysis(source, path, uri);
                let resp = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": analysis.shared_result,
                });

                let _ = writeln!(stdout, "{resp}");
                let _ = stdout.flush();
            }
            "parse" => {
                let params = req.get("params").cloned().unwrap_or_default();
                let path = params
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("source.rs");
                let source = params.get("source").and_then(|v| v.as_str()).unwrap_or("");

                let resp = if let Some(ref socket_path) = daemon_socket {
                    handle_parse_daemon(
                        id.clone(),
                        source,
                        path,
                        socket_path,
                        &mut daemon_stream,
                        &req_counter,
                    )
                } else {
                    handle_parse(id.clone(), source, path)
                };

                let _ = writeln!(stdout, "{resp}");
                let _ = stdout.flush();
            }
            "shutdown" => {
                let resp = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": null
                });
                let _ = writeln!(stdout, "{resp}");
                let _ = stdout.flush();
                std::process::exit(0);
            }
            _ => {
                let resp = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32601,
                        "message": format!("unknown method: {method}")
                    }
                });
                let _ = writeln!(stdout, "{resp}");
                let _ = stdout.flush();
            }
        }
    }
}

fn initialize_result() -> Value {
    json!({
        "name": "provekit-lsp-rust",
        "version": "0.1.0",
        "protocol_version": SHARED_PROTOCOL_VERSION,
        "kit_id": "rust",
        "protocol_catalog_cid": PROTOCOL_CATALOG_CID,
        "capabilities": {
            "methods": ["initialize", "analyzeDocument", "parse", "shutdown"],
            "legacy_methods": ["parse"],
            "source_surfaces": ["rust-source"],
            "entry_kinds": [
                "bind-lift-entry",
                "library-sugar-binding-entry",
                "concept-site",
                "proof-site"
            ],
            "diagnostic_codes": [
                "provekit.lsp.parse_error",
                "provekit.lsp.lift_gap",
                "provekit.lsp.implication_failed"
            ],
            "status_kinds": ["lift", "materialize", "emit", "check", "prove"]
        }
    })
}

/// Daemon-client mode: forward `parse` to the `provekit-linkerd` daemon as a
/// `parseFile` RPC and return `{diagnostics: [...]}`.
///
/// The daemon connection is established lazily on the first `parse` call and
/// cached for the lifetime of the plugin process. If the daemon is not yet
/// running, it is spawned automatically; see `daemon_client::connect_or_spawn`.
fn handle_parse_daemon(
    id: serde_json::Value,
    source: &str,
    path: &str,
    socket_path: &PathBuf,
    stream_cache: &mut Option<UnixStream>,
    req_counter: &AtomicU64,
) -> serde_json::Value {
    // Derive a stable project CID from the socket path (deterministic per
    // project per spec §1; manifest-reading is out of scope for MVP).
    let project_cid = project_cid_from_socket(socket_path);

    // Lazy connect-or-spawn.
    let stream = match stream_cache {
        Some(ref mut s) => s,
        None => match daemon_client::connect_or_spawn(socket_path, &project_cid) {
            Ok(s) => {
                *stream_cache = Some(s);
                stream_cache.as_mut().unwrap()
            }
            Err(e) => {
                return serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32603,
                        "message": format!("daemon connect/spawn failed: {e}")
                    }
                });
            }
        },
    };

    let rpc_id = req_counter.fetch_add(1, Ordering::Relaxed);

    match daemon_client::send_parse_file(stream, "rust", path, source, rpc_id) {
        Ok(diagnostics) => {
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "diagnostics": diagnostics
                }
            })
        }
        Err(e) => {
            // On error, drop the cached stream so the next call reconnects.
            *stream_cache = None;
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32603,
                    "message": format!("daemon parseFile failed: {e}")
                }
            })
        }
    }
}

/// Derive a deterministic project CID from the daemon socket path.
///
/// This is a simple sha256-hex of the socket path string. The daemon uses a
/// proper blake3-512(JCS(manifest)) per spec §1; for the MVP, the socket path
/// already encodes the project identity because callers construct it as
/// `linkerd-<cid>.sock`. We extract the stem rather than re-hashing so that
/// two clients pointing at the same socket path share the same daemon.
fn project_cid_from_socket(socket_path: &PathBuf) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    socket_path.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// Default mode: parse Rust `source` with syn, run the lift adapters, and
/// return a JSON-RPC result object containing `declarations` and `warnings`.
fn handle_parse(id: serde_json::Value, source: &str, path: &str) -> serde_json::Value {
    let analysis = build_document_analysis(source, path, path);
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "declarations": analysis.legacy_declarations,
            "warnings": analysis.legacy_warnings,
            "diagnostics": analysis.legacy_diagnostics
        }
    })
}

struct DocumentAnalysis {
    shared_result: Value,
    legacy_declarations: Value,
    legacy_warnings: Vec<Value>,
    legacy_diagnostics: Vec<Value>,
}

#[derive(Clone, Debug)]
struct SourceRange {
    start_line: usize,
    start_col: usize,
    end_line: usize,
    end_col: usize,
}

impl SourceRange {
    fn to_json(&self) -> Value {
        json!({
            "start_line": self.start_line,
            "start_col": self.start_col,
            "end_line": self.end_line,
            "end_col": self.end_col,
        })
    }
}

fn build_document_analysis(source: &str, path: &str, uri: &str) -> DocumentAnalysis {
    let document_cid = blake3_512_of(source.as_bytes());
    let whole_file_range = whole_file_range(source);

    let file = match syn::parse_str::<syn::File>(source) {
        Ok(f) => f,
        Err(e) => {
            let diagnostic = shared_diagnostic(
                "provekit.lsp.parse_error",
                format!("Rust parse error: {e}"),
                "error",
                "kit",
                &whole_file_range,
                json!({"parser": "syn"}),
            );
            return DocumentAnalysis {
                shared_result: shared_result(
                    uri,
                    path,
                    document_cid,
                    Vec::new(),
                    vec![diagnostic],
                    kit_statuses(&whole_file_range, false),
                ),
                legacy_declarations: Value::Array(vec![]),
                legacy_warnings: vec![],
                legacy_diagnostics: vec![],
            };
        }
    };

    let function_ranges = collect_function_ranges(&file, source);
    let primary_range = function_ranges
        .first()
        .map(|(_, range)| range.clone())
        .unwrap_or_else(|| whole_file_range.clone());

    let lift = run_lift_adapters(&file, path);
    let mut entries = Vec::new();

    if let Some(decls) = lift.declarations.as_array() {
        for decl in decls {
            let source_function_name = decl
                .get("name")
                .and_then(|v| v.as_str())
                .and_then(|name| best_function_name_for_contract(name, &function_ranges))
                .or_else(|| function_ranges.first().map(|(name, _)| name.clone()))
                .unwrap_or_else(|| "source.rs".to_string());
            let range = range_for_function(&source_function_name, &function_ranges)
                .unwrap_or_else(|| primary_range.clone());
            let mut entry = decl.clone();
            if let Some(obj) = entry.as_object_mut() {
                obj.insert(
                    "source_function_name".to_string(),
                    Value::String(source_function_name),
                );
            }
            entries.push(json!({
                "kind": "bind-lift-entry",
                "entry": entry,
                "range": range.to_json(),
            }));
        }
    }

    for sugar_entry in collect_sugar_entries(&file, &function_ranges) {
        entries.push(sugar_entry);
    }

    let legacy_diagnostics: Vec<Value> = ForwardPropagator::floor_v1_seed_index()
        .emit_diagnostics(&ForwardPropagator::lower_floor_source(source))
        .into_iter()
        .map(|diagnostic| diagnostic.to_lsp_json())
        .collect();
    let mut shared_diagnostics: Vec<Value> = legacy_diagnostics
        .iter()
        .map(shared_forward_diagnostic)
        .collect();

    for warning in &lift.warnings {
        let range = warning
            .get("item")
            .and_then(|v| v.as_str())
            .and_then(|name| range_for_function(name, &function_ranges))
            .unwrap_or_else(|| primary_range.clone());
        let reason = warning
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("Rust lift adapter could not lift source site");
        shared_diagnostics.push(shared_diagnostic(
            "provekit.lsp.lift_gap",
            reason.to_string(),
            "warning",
            "kit",
            &range,
            warning.clone(),
        ));
    }

    let statuses = kit_statuses(&primary_range, !entries.is_empty());
    DocumentAnalysis {
        shared_result: shared_result(
            uri,
            path,
            document_cid,
            entries,
            shared_diagnostics,
            statuses,
        ),
        legacy_declarations: lift.declarations,
        legacy_warnings: lift.warnings,
        legacy_diagnostics,
    }
}

struct LiftAdapterResult {
    declarations: Value,
    warnings: Vec<Value>,
}

fn run_lift_adapters(file: &syn::File, path: &str) -> LiftAdapterResult {
    let mut decls: Vec<ContractDecl> = Vec::new();
    let mut warnings: Vec<Value> = Vec::new();
    macro_rules! push_adapter_warnings {
        ($adapter:expr, $adapter_warnings:expr) => {
            for w in $adapter_warnings {
                warnings.push(json!({
                    "adapter": $adapter,
                    "path": w.source_path,
                    "item": w.item_name,
                    "reason": w.reason
                }));
            }
        };
    }

    let p_out = adapter_proptest::lift_file(file, path);
    decls.extend(p_out.decls);
    push_adapter_warnings!("proptest", &p_out.warnings);

    let c_out = adapter_contracts::lift_file(file, path);
    decls.extend(c_out.decls);
    push_adapter_warnings!("contracts", &c_out.warnings);

    let k_out = adapter_kani::lift_file(file, path);
    decls.extend(k_out.decls);
    push_adapter_warnings!("kani", &k_out.warnings);

    let pr_out = adapter_prusti::lift_file(file, path);
    decls.extend(pr_out.decls);
    push_adapter_warnings!("prusti", &pr_out.warnings);

    let cr_out = adapter_creusot::lift_file(file, path);
    decls.extend(cr_out.decls);
    push_adapter_warnings!("creusot", &cr_out.warnings);

    let fl_out = adapter_flux::lift_file(file, path);
    decls.extend(fl_out.decls);
    push_adapter_warnings!("flux", &fl_out.warnings);

    let qc_out = adapter_quickcheck::lift_file(file, path);
    decls.extend(qc_out.decls);
    push_adapter_warnings!("quickcheck", &qc_out.warnings);

    let ve_out = adapter_verus::lift_file(file, path);
    decls.extend(ve_out.decls);
    push_adapter_warnings!("verus", &ve_out.warnings);

    let l2_out = adapter_rust_tests::lift_file_layer2(file, path);
    let claimed = l2_out.claimed_tests.clone();
    decls.extend(l2_out.decls);
    push_adapter_warnings!("rust-tests-layer2", &l2_out.warnings);

    let rt_out = adapter_rust_tests::lift_file_with_skip(file, path, &claimed);
    decls.extend(rt_out.decls);
    push_adapter_warnings!("rust-tests", &rt_out.warnings);

    let decls_json_str = if decls.is_empty() {
        "[]".to_string()
    } else {
        marshal_declarations(&decls)
    };
    let declarations = serde_json::from_str(&decls_json_str).unwrap_or(Value::Array(vec![]));

    LiftAdapterResult {
        declarations,
        warnings,
    }
}

fn shared_result(
    uri: &str,
    file: &str,
    document_cid: String,
    entries: Vec<Value>,
    diagnostics: Vec<Value>,
    statuses: Vec<Value>,
) -> Value {
    json!({
        "kind": "lsp-document-analysis",
        "schema_version": "1",
        "kit_id": "rust",
        "uri": uri,
        "file": file,
        "document_cid": document_cid,
        "protocol_catalog_cid": PROTOCOL_CATALOG_CID,
        "entries": entries,
        "diagnostics": diagnostics,
        "statuses": statuses,
        "project": null,
    })
}

fn collect_function_ranges(file: &syn::File, source: &str) -> Vec<(String, SourceRange)> {
    let mut names = Vec::new();
    collect_function_names(&file.items, &mut names);
    names
        .into_iter()
        .map(|name| {
            let range =
                find_function_range(source, &name).unwrap_or_else(|| whole_file_range(source));
            (name, range)
        })
        .collect()
}

fn collect_function_names(items: &[syn::Item], names: &mut Vec<String>) {
    for item in items {
        match item {
            syn::Item::Fn(item_fn) => names.push(item_fn.sig.ident.to_string()),
            syn::Item::Mod(item_mod) => {
                if let Some((_, nested)) = &item_mod.content {
                    collect_function_names(nested, names);
                }
            }
            _ => {}
        }
    }
}

fn find_function_range(source: &str, name: &str) -> Option<SourceRange> {
    let needle = format!("fn {name}");
    for (line_idx, line) in source.lines().enumerate() {
        if let Some(start_col) = line.find(&needle) {
            return Some(SourceRange {
                start_line: line_idx + 1,
                start_col,
                end_line: line_idx + 1,
                end_col: (start_col + needle.len()).min(line.len()),
            });
        }
    }
    None
}

fn whole_file_range(source: &str) -> SourceRange {
    let mut line_count = source.lines().count();
    if line_count == 0 {
        line_count = 1;
    };
    let end_col = source.lines().last().map(str::len).unwrap_or(0);
    SourceRange {
        start_line: 1,
        start_col: 0,
        end_line: line_count,
        end_col,
    }
}

fn range_for_function(
    name: &str,
    function_ranges: &[(String, SourceRange)],
) -> Option<SourceRange> {
    function_ranges
        .iter()
        .find(|(candidate, _)| candidate == name)
        .map(|(_, range)| range.clone())
}

fn best_function_name_for_contract(
    contract_name: &str,
    function_ranges: &[(String, SourceRange)],
) -> Option<String> {
    function_ranges
        .iter()
        .filter(|(name, _)| contract_name == name || contract_name.contains(name))
        .max_by_key(|(name, _)| name.len())
        .map(|(name, _)| name.clone())
}

fn collect_sugar_entries(
    file: &syn::File,
    function_ranges: &[(String, SourceRange)],
) -> Vec<Value> {
    let mut entries = Vec::new();
    collect_sugar_entries_in_items(&file.items, function_ranges, &mut entries);
    entries
}

fn collect_sugar_entries_in_items(
    items: &[syn::Item],
    function_ranges: &[(String, SourceRange)],
    entries: &mut Vec<Value>,
) {
    for item in items {
        match item {
            syn::Item::Fn(item_fn) => {
                if let Some((concept, library)) = sugar_concept_and_library(item_fn) {
                    let source_function_name = item_fn.sig.ident.to_string();
                    let range = range_for_function(&source_function_name, function_ranges)
                        .unwrap_or(SourceRange {
                            start_line: 1,
                            start_col: 0,
                            end_line: 1,
                            end_col: 0,
                        });
                    entries.push(json!({
                        "kind": "library-sugar-binding-entry",
                        "entry": {
                            "kind": "library-sugar-binding-entry",
                            "target_language": "rust",
                            "target_library_tag": library,
                            "concept_name": concept,
                            "source_function_name": source_function_name,
                            "loss_record_contribution": {
                                "form": "literal",
                                "value": {"entries": []}
                            }
                        },
                        "range": range.to_json(),
                    }));
                }
            }
            syn::Item::Mod(item_mod) => {
                if let Some((_, nested)) = &item_mod.content {
                    collect_sugar_entries_in_items(nested, function_ranges, entries);
                }
            }
            _ => {}
        }
    }
}

fn sugar_concept_and_library(item_fn: &syn::ItemFn) -> Option<(String, String)> {
    for attr in &item_fn.attrs {
        let path = attr.path();
        let segments: Vec<_> = path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect();
        if segments.len() != 2 || segments[0] != "provekit" || segments[1] != "sugar" {
            continue;
        }
        if let syn::Meta::List(list) = &attr.meta {
            let tokens = list.tokens.to_string();
            let concept = quoted_attr_value(&tokens, "concept")?;
            let library = quoted_attr_value(&tokens, "library")?;
            return Some((concept, library));
        }
    }
    None
}

fn quoted_attr_value(tokens: &str, key: &str) -> Option<String> {
    let needle = format!("{key} = \"");
    let start = tokens.find(&needle)? + needle.len();
    let rest = &tokens[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn shared_forward_diagnostic(legacy: &Value) -> Value {
    let range = legacy
        .get("range")
        .map(shared_range_from_lsp_range)
        .unwrap_or_else(|| SourceRange {
            start_line: 1,
            start_col: 0,
            end_line: 1,
            end_col: 0,
        });
    shared_diagnostic(
        "provekit.lsp.implication_failed",
        legacy
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("Current post facts do not establish the callee precondition")
            .to_string(),
        "error",
        "forward-propagation",
        &range,
        legacy.get("data").cloned().unwrap_or(Value::Null),
    )
}

fn shared_range_from_lsp_range(lsp_range: &Value) -> SourceRange {
    let start = lsp_range.get("start").unwrap_or(&Value::Null);
    let end = lsp_range.get("end").unwrap_or(&Value::Null);
    SourceRange {
        start_line: start
            .get("line")
            .and_then(|v| v.as_u64())
            .map(|line| line as usize + 1)
            .unwrap_or(1),
        start_col: start.get("character").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
        end_line: end
            .get("line")
            .and_then(|v| v.as_u64())
            .map(|line| line as usize + 1)
            .unwrap_or(1),
        end_col: end.get("character").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
    }
}

fn shared_diagnostic(
    code: &str,
    message: String,
    severity: &str,
    producer: &str,
    range: &SourceRange,
    data: Value,
) -> Value {
    json!({
        "code": code,
        "message": message,
        "severity": severity,
        "range": range.to_json(),
        "producer": producer,
        "kit_id": "rust",
        "protocol_catalog_cid": PROTOCOL_CATALOG_CID,
        "data": data,
    })
}

fn kit_statuses(range: &SourceRange, has_lifted_entries: bool) -> Vec<Value> {
    vec![
        status(
            "lift",
            if has_lifted_entries {
                "available"
            } else {
                "unavailable"
            },
            if has_lifted_entries {
                "Rust kit lifted editor document sites through the live LSP helper"
            } else {
                "Rust kit found no liftable editor document sites"
            },
            range,
        ),
        status(
            "materialize",
            "unknown",
            "Rust materialize status backend is not wired to analyzeDocument yet",
            range,
        ),
        status(
            "emit",
            "unknown",
            "provekit-emit-rust-cargo-test status RPC is not wired to analyzeDocument yet",
            range,
        ),
        status(
            "check",
            "unknown",
            "Rust kit check status backend is not wired to analyzeDocument yet",
            range,
        ),
        status(
            "prove",
            "unknown",
            "Rust prover receipt status backend is not wired to analyzeDocument yet",
            range,
        ),
    ]
}

fn status(kind: &str, state: &str, message: &str, range: &SourceRange) -> Value {
    json!({
        "kind": kind,
        "range": range.to_json(),
        "state": state,
        "producer": "rust-kit",
        "message": message,
    })
}

fn blake3_512_of(bytes: &[u8]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(bytes);
    let mut out = [0u8; 64];
    hasher.finalize_xof().fill(&mut out);
    let mut hex = String::with_capacity(out.len() * 2);
    for byte in out {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
    }
    format!("blake3-512:{hex}")
}
