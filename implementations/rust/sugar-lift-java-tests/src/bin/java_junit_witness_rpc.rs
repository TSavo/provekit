// SPDX-License-Identifier: Apache-2.0
//
// JUnit witness lift/resolve RPC.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use sugar_lift_java_tests as kit;

const KIT_ID: &str = "java-junit-witness";
const KIT_VERSION: &str = env!("CARGO_PKG_VERSION");
const SURFACE: &str = "java-junit-witness";
const KIT_DECLARATION_RPC_METHOD: &str = "sugar.plugin.kit_declaration";
const RESOLVE_WITNESS_RPC_METHOD: &str = "sugar.plugin.resolve_witness";

fn send(obj: &Value) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{}", serde_json::to_string(obj).unwrap_or_default());
    let _ = out.flush();
}

fn err_reply(id: &Value, msg: String) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32603, "message": msg}})
}

fn resolve_root(params: &Value) -> PathBuf {
    params
        .get("workspace_root")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn handle_lift(id: &Value, params: &Value) -> Value {
    let root = resolve_root(params);
    match kit::lift_project(&root) {
        Ok(Some(result)) => {
            let _ = kit::write_bundle_package(&root, &result.bundle_cid, &result.bundle_bytes);
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "kind": "ir-document",
                    "ir": result.ir,
                    "witness_mementos": result.mementos,
                    "implications": [],
                    "diagnostics": [],
                    "warnings": [],
                }
            })
        }
        Ok(None) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "kind": "ir-document",
                "ir": [],
                "witness_mementos": [],
                "implications": [],
                "diagnostics": [],
                "warnings": [],
            }
        }),
        Err(e) => err_reply(id, e),
    }
}

fn handle_resolve_witness(id: &Value, params: &Value) -> Value {
    let memento = params.get("memento").cloned().unwrap_or(Value::Null);
    let cid = memento
        .get("witness_cid")
        .and_then(Value::as_str)
        .or_else(|| params.get("witness_cid").and_then(Value::as_str));
    let Some(cid) = cid else {
        return err_reply(id, "resolve_witness requires a witness_cid".to_string());
    };
    let cid = cid.to_string();
    let ws = params.get("workspace_root").and_then(Value::as_str);
    let package_dir = params.get("package_dir").and_then(Value::as_str);

    if let Some(pd) = package_dir {
        let pdir = if Path::new(pd).is_absolute() {
            PathBuf::from(pd)
        } else {
            PathBuf::from(ws.unwrap_or(".")).join(pd)
        };
        let path = pdir.join(kit::cid_filename(&cid, ".witness"));
        if path.is_file() {
            if let Ok(bytes) = std::fs::read(&path) {
                return json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "witness_cid": cid,
                        "body_b64": kit::b64(&bytes),
                        "resolved_by": "package",
                    }
                });
            }
        }
    }

    let witness_kind = memento.get("witness_kind").and_then(Value::as_str);
    if let (Some(ws), Some("junit-test-witness-package")) = (ws, witness_kind) {
        let code_files = kit::memento_str_list(&memento, "code_files");
        match kit::recompute_bundle_body(Path::new(ws), &code_files, &cid) {
            Ok(bytes) => {
                return json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "witness_cid": cid,
                        "body_b64": kit::b64(&bytes),
                        "resolved_by": "recompute",
                    }
                });
            }
            Err(e) => return err_reply(id, e),
        }
    }

    err_reply(
        id,
        format!("cannot resolve witness body for {cid}: no package file and not re-runnable"),
    )
}

fn kit_declaration() -> Value {
    json!({
        "kit": {"id": KIT_ID, "language": "java", "version": KIT_VERSION},
        "rpc": {"methods": [
            {"name": "initialize", "required": true},
            {"name": KIT_DECLARATION_RPC_METHOD, "required": true},
            {"name": "lift", "required": true},
            {"name": RESOLVE_WITNESS_RPC_METHOD, "required": false},
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

fn handle(id: &Value, method: &str, params: &Value) -> Value {
    match method {
        "initialize" => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "name": "sugar-lift-java-junit-witness-rpc",
                "version": KIT_VERSION,
                "protocol_version": "pep/1.7.0",
                "capabilities": {
                    "authoring_surfaces": [SURFACE],
                    "ir_version": "v1.1.0",
                    "emits_signed_mementos": true,
                },
            }
        }),
        KIT_DECLARATION_RPC_METHOD => {
            json!({"jsonrpc": "2.0", "id": id, "result": kit_declaration()})
        }
        "lift" => handle_lift(id, params),
        RESOLVE_WITNESS_RPC_METHOD => handle_resolve_witness(id, params),
        "shutdown" => json!({"jsonrpc": "2.0", "id": id, "result": Value::Null}),
        other => err_reply(id, format!("unknown method: {other}")),
    }
}

fn main() {
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(msg): Result<Value, _> = serde_json::from_str(line) else {
            continue;
        };
        let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
        let id = msg.get("id").cloned().unwrap_or(Value::Null);
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        send(&handle(&id, method, &params));
    }
}
