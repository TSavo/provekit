use std::io::{BufRead, BufReader, Write};

use serde::Deserialize;
use serde_json::{json, Value};

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    if args.iter().any(|arg| arg == "--rpc") {
        run_rpc();
        return;
    }

    eprintln!("usage: provekit-lift-asm-x86-64 --rpc");
    std::process::exit(1);
}

fn run_rpc() {
    let stdin = std::io::stdin();
    let reader = BufReader::new(stdin);
    let mut stdout = std::io::stdout();

    for line in reader.lines() {
        let line = match line {
            Ok(line) => line,
            Err(err) => {
                eprintln!("rpc read error: {err}");
                break;
            }
        };
        if line.trim().is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<RpcRequest>(&line) {
            Ok(request) => dispatch(request),
            Err(err) => json!({
                "jsonrpc": "2.0",
                "id": null,
                "error": {"code": -32700, "message": format!("PARSE_ERROR: {err}")}
            }),
        };

        let rendered = serde_json::to_string(&response).unwrap_or_else(|_| "{}".to_string());
        writeln!(stdout, "{rendered}").ok();
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

fn dispatch(request: RpcRequest) -> Value {
    match request.method.as_str() {
        "initialize" => handle_initialize(request.id),
        "lift" => handle_lift(request.id, request.params),
        "shutdown" => json!({"jsonrpc": "2.0", "id": request.id, "result": null}),
        other => error_response(request.id, -32601, format!("METHOD_NOT_FOUND: {other}")),
    }
}

fn handle_initialize(id: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "name": "provekit-lift-asm-x86-64",
            "version": "0.1.0",
            "protocol_version": "provekit-lift/1",
            "capabilities": {
                "authoring_surfaces": [provekit_lift_asm_x86_64::SURFACE],
                "ir_version": "v1.1.0",
                "emits_signed_mementos": false
            }
        }
    })
}

fn handle_lift(id: Value, params: Value) -> Value {
    let surface = params
        .get("surface")
        .and_then(Value::as_str)
        .unwrap_or(provekit_lift_asm_x86_64::SURFACE);
    if surface != provekit_lift_asm_x86_64::SURFACE {
        return error_response(id, 1003, format!("SURFACE_NOT_SUPPORTED: {surface}"));
    }

    let workspace_root = params
        .get("workspace_root")
        .and_then(Value::as_str)
        .unwrap_or(".");
    let source_paths = params
        .get("source_paths")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if source_paths.is_empty() {
        return error_response(id, -32602, "source_paths is empty".to_string());
    }

    let result = match provekit_lift_asm_x86_64::lift_paths(workspace_root, &source_paths) {
        Ok(result) => result,
        Err(err) => return error_response(id, 1007, err.to_string()),
    };

    let declarations = result
        .contracts
        .iter()
        .map(provekit_lift_asm_x86_64::contract_to_json)
        .collect::<Vec<_>>();

    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "kind": "ir-document",
            "declarations": declarations,
            "ir": declarations,
            "diagnostics": result.diagnostics,
            "refusals": result.refusals
        }
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
