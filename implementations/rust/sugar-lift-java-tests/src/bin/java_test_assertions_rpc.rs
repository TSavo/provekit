// SPDX-License-Identifier: Apache-2.0
//
// RPC entrypoint for the Java learned assertion-vocabulary consistency lifter.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use sugar_ir_symbolic::serialize::marshal_declarations;
use sugar_lift_java_tests::{learn_vocab_for_test_source, lift_source_with_vocab};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const SURFACE: &str = "java-test-assertions";
const KIT_DECLARATION_RPC_METHOD: &str = "sugar.plugin.kit_declaration";

fn initialize_result() -> Value {
    json!({
        "name": "sugar-lift-java-tests-rpc",
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
            "language": "java",
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
        "proofResolution": {"strategy": "junit"},
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
            for rel in enumerate_java_files(&abs) {
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

    let exception_dirs = vec![workspace_root
        .join(".sugar")
        .join("vocab-exceptions")
        .to_string_lossy()
        .to_string()];
    let mut entries = Vec::new();
    let mut diagnostics = Vec::new();
    for rel in &rel_paths {
        let abs = workspace_root.join(rel);
        let src = match std::fs::read_to_string(&abs) {
            Ok(src) => src,
            Err(e) => {
                diagnostics.push(json!({
                    "kind": "lift-gap",
                    "path": rel,
                    "reason": format!("read: {e}"),
                }));
                continue;
            }
        };
        let vocab = match learn_vocab_for_test_source(&src, &workspace_root, &exception_dirs) {
            Ok(vocab) => vocab,
            Err(e) => {
                diagnostics.push(json!({
                    "kind": "lift-gap",
                    "path": rel,
                    "reason": format!("learn-vocab: {e}"),
                }));
                continue;
            }
        };
        let out = lift_source_with_vocab(&src, rel, &vocab);
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

fn enumerate_java_files(root: &Path) -> Vec<String> {
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
        if path.is_file() && path.extension().is_some_and(|ext| ext == "java") {
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
        send(&handle(&id, method, &params));
    }
}
