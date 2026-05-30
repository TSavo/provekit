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
//   {"jsonrpc":"2.0","id":3,"method":"analyzeDocument","params":{"file":"...","text":"..."}}
//   {"jsonrpc":"2.0","id":4,"method":"shutdown"}
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

use provekit_ir_symbolic::serialize::marshal_declarations;
use provekit_lift::{
    adapter_contracts, adapter_creusot, adapter_flux, adapter_kani, adapter_proptest,
    adapter_prusti, adapter_quickcheck, adapter_rust_tests, adapter_verus,
};
use provekit_lsp_rust::forward_propagator::ForwardPropagator;

const KIT_ID: &str = "rust";
const SHARED_LSP_PROTOCOL_VERSION: &str = "provekit-lsp-shared/1";
const SHARED_LSP_PROTOCOL_CATALOG_CID: &str = "blake3-512:0e3905c2a7a098cd538b9669428a7dffd2b84ba8ccf8fde3724fe2ab61fd3fbc1e1a616a6b20b6817464cdc50c466b5497d4ac2e2dc34c3c15f05535b463643c";

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
                    "result": {
                        "name": "provekit-lsp-rust",
                        "version": "0.1.0",
                        "protocol_version": SHARED_LSP_PROTOCOL_VERSION,
                        "kit_id": KIT_ID,
                        "protocol_catalog_cid": SHARED_LSP_PROTOCOL_CATALOG_CID,
                        "capabilities": {
                            "source_surfaces": ["rust-source"],
                            "entry_kinds": ["bind-lift-entry"],
                            "diagnostic_codes": [
                                "provekit.lsp.parse_error",
                                "provekit.lsp.implication_failed"
                            ],
                            "status_kinds": ["materialize", "emit", "check", "prove"]
                        }
                    }
                });
                let _ = writeln!(stdout, "{resp}");
                let _ = stdout.flush();
            }
            "analyzeDocument" => {
                let params = req.get("params").cloned().unwrap_or_default();
                let resp = handle_analyze_document(id.clone(), params);
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

fn handle_analyze_document(
    id: serde_json::Value,
    params: serde_json::Value,
) -> serde_json::Value {
    let file = params
        .get("file")
        .or_else(|| params.get("path"))
        .and_then(|v| v.as_str())
        .unwrap_or("source.rs");
    let uri = params
        .get("uri")
        .and_then(|v| v.as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("file://{file}"));
    let source = params
        .get("text")
        .or_else(|| params.get("source"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let parse_resp = handle_parse(serde_json::Value::Null, source, file);
    let result = match parse_resp.get("result") {
        Some(result) => result,
        None => {
            let message = parse_resp
                .get("error")
                .and_then(|err| err.get("message"))
                .and_then(|message| message.as_str())
                .unwrap_or("rust source parse failed");
            return analyze_document_response(
                id,
                &uri,
                file,
                source,
                Vec::new(),
                vec![parse_error_diagnostic(message)],
                Vec::new(),
            );
        }
    };

    let entries = result
        .get("declarations")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .cloned()
        .map(|entry| {
            serde_json::json!({
                "kind": "bind-lift-entry",
                "entry": entry,
                "range": whole_document_range(source)
            })
        })
        .collect();

    let diagnostics = result
        .get("diagnostics")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .map(shared_diagnostic_from_lsp_diagnostic)
        .collect();

    analyze_document_response(id, &uri, file, source, entries, diagnostics, Vec::new())
}

fn analyze_document_response(
    id: serde_json::Value,
    uri: &str,
    file: &str,
    source: &str,
    entries: Vec<serde_json::Value>,
    diagnostics: Vec<serde_json::Value>,
    statuses: Vec<serde_json::Value>,
) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "kind": "lsp-document-analysis",
            "schema_version": "1",
            "kit_id": KIT_ID,
            "uri": uri,
            "file": file,
            "document_cid": blake3_512_cid(source.as_bytes()),
            "protocol_catalog_cid": SHARED_LSP_PROTOCOL_CATALOG_CID,
            "entries": entries,
            "diagnostics": diagnostics,
            "statuses": statuses,
            "project": null
        }
    })
}

fn parse_error_diagnostic(message: &str) -> serde_json::Value {
    serde_json::json!({
        "code": "provekit.lsp.parse_error",
        "message": message,
        "severity": "error",
        "range": first_byte_range(),
        "producer": "kit",
        "kit_id": KIT_ID,
        "protocol_catalog_cid": SHARED_LSP_PROTOCOL_CATALOG_CID
    })
}

fn shared_diagnostic_from_lsp_diagnostic(diagnostic: &serde_json::Value) -> serde_json::Value {
    let code = diagnostic
        .get("data")
        .and_then(|data| data.get("kind"))
        .and_then(|kind| kind.as_str())
        .or_else(|| diagnostic.get("code").and_then(|code| code.as_str()))
        .unwrap_or("provekit.lsp.lift_gap");
    let message = diagnostic
        .get("message")
        .and_then(|message| message.as_str())
        .unwrap_or("ProvekIt diagnostic");
    let severity = diagnostic
        .get("severity")
        .and_then(|severity| severity.as_u64())
        .map(shared_severity)
        .unwrap_or("information");
    let range = diagnostic
        .get("range")
        .map(lsp_range_to_source_range)
        .unwrap_or_else(first_byte_range);

    let mut shared = serde_json::json!({
        "code": code,
        "message": message,
        "severity": severity,
        "range": range,
        "producer": "forward-propagation",
        "kit_id": KIT_ID,
        "protocol_catalog_cid": SHARED_LSP_PROTOCOL_CATALOG_CID
    });
    if let Some(data) = diagnostic.get("data") {
        shared["data"] = data.clone();
    }
    shared
}

fn shared_severity(severity: u64) -> &'static str {
    match severity {
        1 => "error",
        2 => "warning",
        3 => "information",
        4 => "hint",
        _ => "information",
    }
}

fn lsp_range_to_source_range(range: &serde_json::Value) -> serde_json::Value {
    let start = range.get("start").unwrap_or(&serde_json::Value::Null);
    let end = range.get("end").unwrap_or(&serde_json::Value::Null);
    let start_line = start.get("line").and_then(|v| v.as_u64()).unwrap_or(0) + 1;
    let start_col = start
        .get("character")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let end_line = end.get("line").and_then(|v| v.as_u64()).unwrap_or(0) + 1;
    let end_col = end
        .get("character")
        .and_then(|v| v.as_u64())
        .unwrap_or(start_col);
    serde_json::json!({
        "start_line": start_line,
        "start_col": start_col,
        "end_line": end_line,
        "end_col": end_col
    })
}

fn whole_document_range(source: &str) -> serde_json::Value {
    let mut line = 1u64;
    let mut col = 0u64;
    for ch in source.chars() {
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    serde_json::json!({
        "start_line": 1,
        "start_col": 0,
        "end_line": line,
        "end_col": col
    })
}

fn first_byte_range() -> serde_json::Value {
    serde_json::json!({
        "start_line": 1,
        "start_col": 0,
        "end_line": 1,
        "end_col": 0
    })
}

fn blake3_512_cid(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut hasher = blake3::Hasher::new();
    hasher.update(bytes);
    let mut output = [0u8; 64];
    hasher.finalize_xof().fill(&mut output);

    let mut cid = String::from("blake3-512:");
    for byte in output {
        write!(&mut cid, "{byte:02x}").expect("write to string");
    }
    cid
}

/// Default mode: parse Rust `source` with syn, run the lift adapters, and
/// return a JSON-RPC result object containing `declarations` and `warnings`.
fn handle_parse(id: serde_json::Value, source: &str, path: &str) -> serde_json::Value {
    let file = match syn::parse_str::<syn::File>(source) {
        Ok(f) => f,
        Err(e) => {
            return serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32603,
                    "message": format!("syn parse error: {e}")
                }
            });
        }
    };

    let mut decls = Vec::new();
    let mut warnings: Vec<serde_json::Value> = Vec::new();

    // Run adapters in the same order as provekit-lift dispatcher.

    let p_out = adapter_proptest::lift_file(&file, path);
    decls.extend(p_out.decls);
    for w in &p_out.warnings {
        warnings.push(serde_json::json!({
            "adapter": "proptest",
            "path": w.source_path,
            "item": w.item_name,
            "reason": w.reason
        }));
    }

    let c_out = adapter_contracts::lift_file(&file, path);
    decls.extend(c_out.decls);
    for w in &c_out.warnings {
        warnings.push(serde_json::json!({
            "adapter": "contracts",
            "path": w.source_path,
            "item": w.item_name,
            "reason": w.reason
        }));
    }

    let k_out = adapter_kani::lift_file(&file, path);
    decls.extend(k_out.decls);
    for w in &k_out.warnings {
        warnings.push(serde_json::json!({
            "adapter": "kani",
            "path": w.source_path,
            "item": w.item_name,
            "reason": w.reason
        }));
    }

    let pr_out = adapter_prusti::lift_file(&file, path);
    decls.extend(pr_out.decls);
    for w in &pr_out.warnings {
        warnings.push(serde_json::json!({
            "adapter": "prusti",
            "path": w.source_path,
            "item": w.item_name,
            "reason": w.reason
        }));
    }

    let cr_out = adapter_creusot::lift_file(&file, path);
    decls.extend(cr_out.decls);
    for w in &cr_out.warnings {
        warnings.push(serde_json::json!({
            "adapter": "creusot",
            "path": w.source_path,
            "item": w.item_name,
            "reason": w.reason
        }));
    }

    let fl_out = adapter_flux::lift_file(&file, path);
    decls.extend(fl_out.decls);
    for w in &fl_out.warnings {
        warnings.push(serde_json::json!({
            "adapter": "flux",
            "path": w.source_path,
            "item": w.item_name,
            "reason": w.reason
        }));
    }

    let qc_out = adapter_quickcheck::lift_file(&file, path);
    decls.extend(qc_out.decls);
    for w in &qc_out.warnings {
        warnings.push(serde_json::json!({
            "adapter": "quickcheck",
            "path": w.source_path,
            "item": w.item_name,
            "reason": w.reason
        }));
    }

    let ve_out = adapter_verus::lift_file(&file, path);
    decls.extend(ve_out.decls);
    for w in &ve_out.warnings {
        warnings.push(serde_json::json!({
            "adapter": "verus",
            "path": w.source_path,
            "item": w.item_name,
            "reason": w.reason
        }));
    }

    // rust-tests: Layer 2 first, then Layer 0 skipping claimed tests.
    let l2_out = adapter_rust_tests::lift_file_layer2(&file, path);
    let claimed = l2_out.claimed_tests.clone();
    decls.extend(l2_out.decls);
    for w in &l2_out.warnings {
        warnings.push(serde_json::json!({
            "adapter": "rust-tests-layer2",
            "path": w.source_path,
            "item": w.item_name,
            "reason": w.reason
        }));
    }

    let rt_out = adapter_rust_tests::lift_file_with_skip(&file, path, &claimed);
    decls.extend(rt_out.decls);
    for w in &rt_out.warnings {
        warnings.push(serde_json::json!({
            "adapter": "rust-tests",
            "path": w.source_path,
            "item": w.item_name,
            "reason": w.reason
        }));
    }

    // Marshal declarations to kit-shape JSON array, then parse it back so
    // it embeds as JSON (not a string) in the response envelope.
    let decls_json_str = if decls.is_empty() {
        "[]".to_string()
    } else {
        marshal_declarations(&decls)
    };

    let decls_value: serde_json::Value =
        serde_json::from_str(&decls_json_str).unwrap_or(serde_json::Value::Array(vec![]));

    let floor_stmts = ForwardPropagator::lower_floor_source(source);
    let diagnostics: Vec<serde_json::Value> = ForwardPropagator::floor_v1_seed_index()
        .emit_diagnostics(&floor_stmts)
        .into_iter()
        .map(|diagnostic| diagnostic.to_lsp_json())
        .collect();

    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "declarations": decls_value,
            "warnings": warnings,
            "diagnostics": diagnostics
        }
    })
}
