// SPDX-License-Identifier: Apache-2.0
//
// provekit-realize-c: PEP 1.7.0 JSON-RPC stdio sidecar for C lower/realize.
//
// Reads newline-delimited JSON-RPC 2.0 requests from stdin, writes responses
// to stdout. Parallel to `provekit-realize-rust --rpc` from PR #773.
//
// Supported methods:
//   provekit.plugin.describe  -- returns the c-canonical sugar plugin memento
//   provekit.plugin.invoke    -- lowers one binding to C source
//   provekit.plugin.shutdown  -- graceful exit
//
// Usage:
//   provekit-realize-c --rpc

use std::io::{self, BufRead, Write};

use provekit_realize_c_core::{emit, SUGAR_PLUGIN_CID, sugar_content_json_from_embedded};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let rpc = args.iter().any(|a| a == "--rpc");
    if !rpc {
        eprintln!("Usage: provekit-realize-c --rpc");
        std::process::exit(1);
    }

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    for line_result in stdin.lock().lines() {
        let line = match line_result {
            Ok(l) => l,
            Err(_) => break,
        };
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }
        handle_line(&line, &mut out);
    }
}

fn handle_line(line: &str, out: &mut impl Write) {
    let req: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            let err = serde_json::json!({
                "jsonrpc": "2.0",
                "id": null,
                "error": {"code": -32700, "message": format!("parse error: {e}")}
            });
            writeln!(out, "{}", err).ok();
            return;
        }
    };

    let id = req.get("id").cloned().unwrap_or(serde_json::Value::Null);
    let method = req.get("method").and_then(|v| v.as_str()).unwrap_or("");

    let response = match method {
        "provekit.plugin.describe" => {
            let result = describe_result();
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": serde_json::from_str::<serde_json::Value>(&result).unwrap()
            })
        }
        "provekit.plugin.invoke" => {
            match handle_invoke(&req) {
                Ok(result_str) => {
                    serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": serde_json::from_str::<serde_json::Value>(&result_str).unwrap()
                    })
                }
                Err(msg) => {
                    serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {"code": -32000, "message": msg}
                    })
                }
            }
        }
        "provekit.plugin.shutdown" => {
            let resp = serde_json::json!({"jsonrpc": "2.0", "id": id, "result": null});
            writeln!(out, "{}", resp).ok();
            std::process::exit(0);
        }
        other => {
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {"code": -32601, "message": format!("method not found: {other}")}
            })
        }
    };

    writeln!(out, "{}", response).ok();
}

/// Handle `provekit.plugin.invoke`.
///
/// Params:
///   function      - snake_case function name
///   params        - JSON array of parameter name strings
///   param_types   - JSON array of source-language type strings
///   return_type   - source-language return type string
///   concept_name  - canonical concept name for this binding
///
/// Returns JSON: {"source": "...", "is_stub": bool}
fn handle_invoke(req: &serde_json::Value) -> Result<String, String> {
    let params = req
        .get("params")
        .ok_or_else(|| "missing params".to_string())?;

    let function = params
        .get("function")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing params.function".to_string())?;

    let return_type = params
        .get("return_type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing params.return_type".to_string())?;

    let concept_name = params
        .get("concept_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing params.concept_name".to_string())?;

    let param_names: Vec<String> = params
        .get("params")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let param_types: Vec<String> = params
        .get("param_types")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let r = emit(function, &param_names, &param_types, return_type, concept_name);

    let result = format!(
        "{{\"source\":{},\"is_stub\":{}}}",
        serde_json::to_string(&r.source).map_err(|e| e.to_string())?,
        if r.is_stub { "true" } else { "false" }
    );
    Ok(result)
}

/// Build the `provekit.plugin.describe` result.
///
/// Returns the c-canonical sugar plugin memento. The CID matches
/// `menagerie/c-language-signature/specs/sugar/c-canonical.json`.
fn describe_result() -> String {
    let content = sugar_content_json_from_embedded();
    format!(
        r#"{{"envelope":{{"declaredAt":"2026-05-13T00:00:00.000Z","signature":"ed25519:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA","signer":"ed25519:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="}},"header":{{"cid":"{cid}","content":{content},"critical":false,"kind":"sugar","protocol_versions":["pep/1.7.0"],"provenance_cid":"blake3-512:0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000","schemaVersion":"1","version":"1.0.0"}},"metadata":{{"note":"Canonical C comment sugar dict for ProvekIt contract clause rendering.","source_url":"menagerie/c-language-signature/specs/sugar/c-canonical.json"}}}}"#,
        cid = SUGAR_PLUGIN_CID,
        content = content,
    )
}
