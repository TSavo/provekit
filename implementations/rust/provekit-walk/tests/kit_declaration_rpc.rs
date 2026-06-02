use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use provekit_claim_envelope::{KitDeclaration, KIT_DECLARATION_RPC_METHOD};
use serde_json::{json, Value};

#[test]
fn walk_rpc_serves_kit_declaration_over_stdio() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_provekit-walk-rpc"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn provekit-walk-rpc");
    let mut stdin = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");
    let mut reader = BufReader::new(stdout);

    writeln!(
        stdin,
        "{}",
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}})
    )
    .expect("write initialize");
    let init_response = read_response(&mut reader);
    assert!(
        init_response.get("error").is_none(),
        "initialize failed: {init_response}"
    );

    writeln!(
        stdin,
        "{}",
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": KIT_DECLARATION_RPC_METHOD,
            "params": {}
        })
    )
    .expect("write kit declaration request");
    let declaration_response = read_response(&mut reader);
    assert!(
        declaration_response.get("error").is_none(),
        "kit declaration RPC failed: {declaration_response}"
    );

    let declaration: KitDeclaration = serde_json::from_value(
        declaration_response
            .get("result")
            .cloned()
            .expect("kit declaration result"),
    )
    .expect("kit declaration schema");
    declaration.validate().expect("valid declaration");

    assert_eq!(declaration.kit.id, "provekit-walk-rpc");
    assert_eq!(declaration.kit.language, "rust");
    assert!(
        declaration
            .rpc
            .methods
            .iter()
            .any(|method| method.name == KIT_DECLARATION_RPC_METHOD),
        "declaration must advertise its declaration RPC method"
    );
    assert_eq!(declaration.effect_kinds, ["concept:panic-freedom"]);
    assert!(declaration.effect_leaves.is_empty());
    assert!(declaration.guard_predicates.is_empty());
    assert!(declaration.control_carriers.is_empty());

    writeln!(
        stdin,
        "{}",
        json!({"jsonrpc":"2.0","id":3,"method":"shutdown","params":{}})
    )
    .expect("write shutdown");
    let shutdown_response = read_response(&mut reader);
    assert!(
        shutdown_response.get("error").is_none(),
        "shutdown failed: {shutdown_response}"
    );
    drop(stdin);
    let status = child.wait().expect("wait for provekit-walk-rpc");
    assert!(status.success(), "provekit-walk-rpc exited with {status}");
}

fn read_response(reader: &mut impl BufRead) -> Value {
    let mut line = String::new();
    reader.read_line(&mut line).expect("read response");
    assert!(!line.trim().is_empty(), "empty JSON-RPC response");
    serde_json::from_str(line.trim()).expect("JSON-RPC response JSON")
}
