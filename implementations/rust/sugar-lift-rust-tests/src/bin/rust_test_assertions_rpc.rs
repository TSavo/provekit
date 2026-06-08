// SPDX-License-Identifier: Apache-2.0
//
// RPC entrypoint for the Rust test-assertion consistency lifter.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use sugar_ir_symbolic::serialize::marshal_declarations;
use sugar_lift_rust_tests::lift_file;
use serde_json::{json, Value};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const SURFACE: &str = "rust-test-assertions";
const KIT_DECLARATION_RPC_METHOD: &str = "provekit.plugin.kit_declaration";

fn initialize_result() -> Value {
    json!({
        "name": "provekit-lift-rust-tests-rpc",
        "version": VERSION,
        "protocol_version": "pep/1.7.0",
        "capabilities": {
            "authoring_surfaces": [SURFACE],
            "ir_version": "v1.1.0",
            "emits_signed_mementos": false,
        },
    })
}

fn kit_declaration_result() -> Value {
    json!({
        "kit": {
            "id": SURFACE,
            "language": "rust",
            "version": VERSION,
        },
        "rpc": {
            "methods": [
                {"name": "initialize", "required": true},
                {"name": KIT_DECLARATION_RPC_METHOD, "required": true},
                {"name": "lift", "required": true},
                {"name": "shutdown", "required": false},
            ]
        },
        "proofResolution": {"strategy": "cargo"},
        "effectKinds": [],
        "effectLeaves": [],
        "guardPredicates": [],
        "controlCarriers": [],
        "residueCategories": [],
    })
}

fn lift(params: &Value) -> Value {
    let workspace_root = params
        .get("workspace_root")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let requested: Vec<String> = match params.get("source_paths").and_then(Value::as_array) {
        Some(arr) if !arr.is_empty() => arr
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        _ => vec![".".to_string()],
    };

    let mut rel_paths = Vec::new();
    for entry in &requested {
        let abs = workspace_root.join(entry);
        if abs.is_dir() {
            for rel in enumerate_rs_files(&abs) {
                let joined = if entry == "." {
                    rel
                } else {
                    format!("{}/{}", entry.trim_end_matches('/'), rel)
                };
                rel_paths.push(joined);
            }
        } else {
            rel_paths.push(entry.clone());
        }
    }
    rel_paths.sort();
    rel_paths.dedup();

    let mut entries = Vec::new();
    let mut diagnostics = Vec::new();
    for rel in &rel_paths {
        let abs = workspace_root.join(rel);
        let bytes = match std::fs::read(&abs) {
            Ok(bytes) => bytes,
            Err(e) => {
                diagnostics.push(json!({
                    "kind": "lift-gap",
                    "path": rel,
                    "reason": format!("read: {e}"),
                }));
                continue;
            }
        };
        let src = match std::str::from_utf8(&bytes) {
            Ok(src) => src,
            Err(_) => {
                diagnostics.push(json!({
                    "kind": "lift-gap",
                    "path": rel,
                    "reason": "non-utf8 source",
                }));
                continue;
            }
        };
        let file = match syn::parse_file(src) {
            Ok(file) => file,
            Err(e) => {
                diagnostics.push(json!({
                    "kind": "lift-gap",
                    "path": rel,
                    "reason": format!("parse: {e}"),
                }));
                continue;
            }
        };
        let out = lift_file(&file, rel);
        let marshalled = marshal_declarations(&out.decls);
        let parsed: Value = serde_json::from_str(&marshalled).unwrap_or_else(|_| json!([]));
        if let Some(arr) = parsed.as_array() {
            entries.extend(arr.iter().cloned());
        }
        for w in &out.warnings {
            diagnostics.push(json!({
                "kind": "lift-gap",
                "path": w.source_path,
                "item": w.item_name,
                "reason": w.reason,
            }));
        }
    }

    json!({
        "kind": "ir-document",
        "ir": entries,
        "diagnostics": diagnostics,
        "refusals": [],
    })
}

const IGNORED_DIRS: &[&str] = &[
    "target",
    ".git",
    "node_modules",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    ".venv",
    "venv",
];

fn enumerate_rs_files(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            !(entry.file_type().is_dir() && IGNORED_DIRS.contains(&name.as_ref()))
        })
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
            if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    out.sort();
    out
}

fn send(obj: &Value) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{}", serde_json::to_string(obj).unwrap_or_default());
    let _ = out.flush();
}

fn err_reply(id: &Value, msg: String) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32603, "message": msg}})
}

fn handle(id: &Value, method: &str, params: &Value) -> Value {
    match method {
        "initialize" => json!({"jsonrpc": "2.0", "id": id, "result": initialize_result()}),
        KIT_DECLARATION_RPC_METHOD => {
            json!({"jsonrpc": "2.0", "id": id, "result": kit_declaration_result()})
        }
        "lift" => json!({"jsonrpc": "2.0", "id": id, "result": lift(params)}),
        "shutdown" => json!({"jsonrpc": "2.0", "id": id, "result": Value::Null}),
        other => err_reply(id, format!("unknown method: {other}")),
    }
}

fn main() {
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let req: Value = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                send(
                    &json!({"jsonrpc": "2.0", "id": Value::Null, "error": {"code": -32700, "message": format!("parse error: {e}")}}),
                );
                continue;
            }
        };
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        let method = req.get("method").and_then(Value::as_str).unwrap_or("");
        let params = req.get("params").cloned().unwrap_or(Value::Null);
        let reply = handle(&id, method, &params);
        send(&reply);
        if method == "shutdown" {
            break;
        }
    }
}
