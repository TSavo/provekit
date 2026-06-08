// SPDX-License-Identifier: Apache-2.0
//
// RPC entrypoint for Java JSR-380 method-contract lifting.

use std::io::{BufRead, Write};
use std::path::PathBuf;

use serde_json::{json, Value};
use sugar_lift_java_tests::lift_java_jsr380_contracts_project;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const SURFACE: &str = "java-jsr380-contracts";
const KIT_DECLARATION_RPC_METHOD: &str = "sugar.plugin.kit_declaration";

fn initialize_result() -> Value {
    json!({
        "name": "sugar-lift-java-jsr380-contracts-rpc",
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
        "kit": {"id": SURFACE, "language": "java", "version": VERSION},
        "rpc": {"methods": [
            {"name": "initialize", "required": true},
            {"name": KIT_DECLARATION_RPC_METHOD, "required": true},
            {"name": "lift", "required": true},
            {"name": "shutdown", "required": false},
        ]},
        "proofResolution": {"strategy": "junit"},
        "effectKinds": [],
        "effectLeaves": [],
        "guardPredicates": [],
        "controlCarriers": [],
        "residueCategories": [],
    })
}

fn source_paths(params: &Value) -> Vec<String> {
    match params.get("source_paths").and_then(Value::as_array) {
        Some(arr) if !arr.is_empty() => arr
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        _ => vec![".".to_string()],
    }
}

fn lift(params: &Value) -> Result<Value, String> {
    let root = params
        .get("workspace_root")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let out = lift_java_jsr380_contracts_project(&root, &source_paths(params))?;
    Ok(json!({
        "kind": "ir-document",
        "ir": out.ir,
        "diagnostics": out.diagnostics,
        "refusals": [],
    }))
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
        "lift" => lift(params)
            .map(|result| json!({"jsonrpc": "2.0", "id": id, "result": result}))
            .unwrap_or_else(|e| err_reply(id, e)),
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
