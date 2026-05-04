use std::io::{BufRead, BufReader, Write};

use serde::Deserialize;
use serde_json::{json, Value};

mod ir_builder;
mod openapi;
mod protobuf;
mod types;

use types::{Declaration, Diagnostics};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if !args.contains(&"--rpc".to_string()) {
        eprintln!("usage: provekit-lift-openapi --rpc");
        std::process::exit(1);
    }
    run_rpc();
}

fn run_rpc() {
    let stdin = std::io::stdin();
    let reader = BufReader::new(stdin);
    let mut stdout = std::io::stdout();

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => {
                eprintln!("rpc: read error");
                break;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        let req: RpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("rpc: parse: {e}");
                continue;
            }
        };
        let resp = dispatch(req.method, req.id, req.params);
        let resp_line = serde_json::to_string(&resp).unwrap_or_default();
        writeln!(stdout, "{resp_line}").ok();
        stdout.flush().ok();
    }
}

#[derive(Debug, Deserialize)]
struct RpcRequest {
    method: String,
    id: Value,
    #[serde(default)]
    params: Value,
}

fn dispatch(method: String, id: Value, params: Value) -> Value {
    match method.as_str() {
        "initialize" => handle_initialize(id, params),
        "lift" => handle_lift(id, params),
        "shutdown" => handle_shutdown(id),
        _ => error_response(id, -32601, format!("METHOD_NOT_FOUND: {method}")),
    }
}

fn handle_initialize(id: Value, _params: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "name": "openapi",
            "version": "1.0.0",
            "protocol_version": "provekit-lift/1",
            "capabilities": {
                "authoring_surfaces": ["openapi", "swagger", "protobuf"],
                "ir_version": "v1.1.0",
                "emits_signed_mementos": false
            }
        }
    })
}

fn handle_lift(id: Value, params: Value) -> Value {
    let surface = params
        .get("surface")
        .and_then(|v| v.as_str())
        .unwrap_or("openapi");

    if !["openapi", "swagger", "protobuf"].contains(&surface) {
        return error_response(
            id,
            1003,
            format!("SURFACE_NOT_SUPPORTED: {surface}"),
        );
    }

    let source_paths: Vec<String> = params
        .get("source_paths")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    if source_paths.is_empty() {
        return error_response(id, -32602, "source_paths is empty".to_string());
    }

    let workspace_root = params
        .get("workspace_root")
        .and_then(|v| v.as_str())
        .unwrap_or(".");

    let mut diagnostics = Diagnostics::default();
    let mut declarations: Vec<Declaration> = Vec::new();

    for rel_path in &source_paths {
        let abs_path = if std::path::Path::new(rel_path).is_absolute() {
            std::path::PathBuf::from(rel_path)
        } else {
            std::path::Path::new(workspace_root).join(rel_path)
        };

        let result = match surface {
            "openapi" | "swagger" => openapi::lift_spec(&abs_path, &mut diagnostics),
            "protobuf" => protobuf::lift_proto(&abs_path, &mut diagnostics),
            _ => unreachable!(),
        };

        match result {
            Ok(mut decls) => declarations.append(&mut decls),
            Err(e) => {
                diagnostics.push(format!(
                    "failed to lift {rel_path}: {e}"
                ));
            }
        }
    }

    let ir: Vec<Value> = declarations.iter().map(|d| d.to_json()).collect();

    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "kind": "ir-document",
            "ir": ir,
            "diagnostics": diagnostics.messages
        }
    })
}

fn handle_shutdown(id: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": null
    })
}

fn error_response(id: Value, code: i64, message: String) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
}
