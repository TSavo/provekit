// SPDX-License-Identifier: Apache-2.0
//
// provekit-lsp-rust — NDJSON LSP plugin for Rust.
//
// Thin shim around provekit-lift's adapter stack. Speaks the per-language
// plugin protocol used by every kit's LSP plugin:
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
// Usage: provekit-lsp-rust (reads from stdin, writes to stdout)

use std::io::{BufRead, Write};

use provekit_ir_symbolic::serialize::marshal_declarations;
use provekit_lift::{
    adapter_contracts, adapter_proptest, adapter_rust_tests,
};

fn main() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

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
        let method = req
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        match method {
            "initialize" => {
                let resp = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "name": "provekit-lsp-rust",
                        "version": "0.1.0",
                        "capabilities": ["parse"]
                    }
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
                let source = params
                    .get("source")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let resp = handle_parse(id.clone(), source, path);
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

/// Parse Rust `source` with syn, run the lift adapters, and return a
/// JSON-RPC result object containing `declarations` and `warnings`.
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

    let decls_value: serde_json::Value = serde_json::from_str(&decls_json_str)
        .unwrap_or(serde_json::Value::Array(vec![]));

    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "declarations": decls_value,
            "warnings": warnings
        }
    })
}
