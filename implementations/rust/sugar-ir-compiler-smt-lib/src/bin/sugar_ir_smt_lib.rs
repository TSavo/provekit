// SPDX-License-Identifier: Apache-2.0
//
// Standalone JSON-RPC subprocess binary for the bundled SMT-LIB v2.6
// compiler. Speaks the protocol defined in
// protocol/specs/2026-04-30-ir-compiler-protocol.md.
//
// Read one JSON-RPC request per stdin line, write one response per
// stdout line. Logging on stderr.

use std::io::{self, BufRead, Write};

use serde_json::{json, Value as Json};

use sugar_ir_compiler::{IrCompiler, PROTOCOL_VERSION};
use sugar_ir_compiler_smt_lib::SmtLibCompiler;

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let compiler = SmtLibCompiler::new();

    let mut out = stdout.lock();
    let mut in_ = stdin.lock();
    let mut buf = String::new();

    loop {
        buf.clear();
        let n = match in_.read_line(&mut buf) {
            Ok(n) => n,
            Err(e) => {
                eprintln!("stdin read error: {e}");
                std::process::exit(1);
            }
        };
        if n == 0 {
            break;
        }
        let trimmed = buf.trim();
        if trimmed.is_empty() {
            continue;
        }

        let req: Json = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                let resp = error_response(json!(null), -32700, &format!("parse error: {e}"), None);
                emit_line(&mut out, &resp);
                continue;
            }
        };

        let id = req.get("id").cloned().unwrap_or(Json::Null);
        let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = req.get("params").cloned().unwrap_or(Json::Null);

        let resp = match method {
            "sugar.ir.handshake" => {
                let caps = compiler.capabilities();
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": caps,
                })
            }
            "sugar.ir.compile" => {
                let dialect = params
                    .get("target_dialect")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let ir = match params.get("ir_json") {
                    Some(v) => v.clone(),
                    None => {
                        let r = error_response(id.clone(), -32602, "missing param: ir_json", None);
                        emit_line(&mut out, &r);
                        continue;
                    }
                };
                if dialect.is_empty() {
                    let r =
                        error_response(id.clone(), -32602, "missing param: target_dialect", None);
                    emit_line(&mut out, &r);
                    continue;
                }
                match compiler.compile(&ir, dialect) {
                    Ok(c) => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": c,
                    }),
                    Err(e) => error_response(
                        id,
                        e.code() as i64,
                        e.symbolic(),
                        Some(json!(e.to_string())),
                    ),
                }
            }
            "sugar.ir.shutdown" => {
                let r = json!({"jsonrpc": "2.0", "id": id, "result": {}});
                emit_line(&mut out, &r);
                break;
            }
            other => error_response(id, -32601, &format!("method not found: {other}"), None),
        };

        emit_line(&mut out, &resp);
    }

    let _ = PROTOCOL_VERSION; // ensure linkage
}

fn error_response(id: Json, code: i64, message: &str, data: Option<Json>) -> Json {
    let mut err = json!({"code": code, "message": message});
    if let Some(d) = data {
        err["data"] = d;
    }
    json!({"jsonrpc": "2.0", "id": id, "error": err})
}

fn emit_line(out: &mut impl Write, v: &Json) {
    let s = serde_json::to_string(v).unwrap_or_else(|_| "{}".to_string());
    let _ = writeln!(out, "{s}");
    let _ = out.flush();
}
