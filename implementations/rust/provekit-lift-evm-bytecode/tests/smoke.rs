use std::io::Write;
use std::process::{Command, Stdio};

use provekit_lift_evm_bytecode::{
    lift_source_text, lift_success_response_json, parse_evm_text, Opcode,
};
use serde_json::Value as Json;

#[test]
fn parser_decodes_hex_push_add_stop() {
    let unit = parse_evm_text("add.hex", "0x600160020100").expect("hex EVM parses");

    assert_eq!(unit.instructions.len(), 4);
    assert_eq!(unit.instructions[0].opcode, Opcode::Push(1));
    assert_eq!(unit.instructions[0].immediate.as_deref(), Some("0x01"));
    assert_eq!(unit.instructions[2].opcode, Opcode::Add);
    assert_eq!(unit.instructions[3].opcode, Opcode::Stop);
}

#[test]
fn lifts_stack_arithmetic_fixture_to_contract() {
    let source = include_str!("fixtures/add.evm");
    let result = lift_source_text("add.evm", source).expect("EVM fixture lifts");

    assert_eq!(result.declarations.len(), 1);
    assert!(
        result.refusals.is_empty(),
        "unexpected refusals: {:?}",
        result.refusals
    );
    let contract = &result.declarations[0];
    assert_eq!(contract["kind"], "function-contract");
    assert_eq!(contract["fnName"], "add");
    assert_eq!(contract["formals"], serde_json::json!([]));
    assert_eq!(contract["effects"], serde_json::json!([]));
    let post = serde_json::to_string(&contract["post"]).unwrap();
    assert!(post.contains("return_value"));
    assert!(post.contains("evm:add"));
    assert!(post.contains("0x01"));
    assert!(post.contains("0x02"));
}

#[test]
fn refuses_storage_writes_without_emitting_contract() {
    let result = lift_source_text("storage.evm", "PUSH1 0x00\nPUSH1 0x01\nSSTORE\nSTOP\n")
        .expect("unsupported EVM parses");

    assert!(result.declarations.is_empty());
    assert_eq!(result.refusals.len(), 1);
    let refusal = &result.refusals[0];
    assert_eq!(refusal.kind, "unsupported-opcode");
    assert_eq!(refusal.instruction.as_deref(), Some("SSTORE"));
    assert!(refusal.reason.contains("SSTORE"));
    assert!(refusal.reason.contains("storage"));
}

#[test]
fn rpc_response_wraps_lifted_contracts() {
    let result =
        lift_source_text("add.evm", include_str!("fixtures/add.evm")).expect("EVM fixture lifts");
    let response = lift_success_response_json(serde_json::json!(11), &result);

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 11);
    assert_eq!(response["result"]["kind"], "ir-document");
    assert_eq!(response["result"]["ir"][0]["fnName"], "add");
    assert!(response["result"].get("declarations").is_none());
}

#[test]
fn rpc_lift_rejects_absolute_source_paths() {
    let absolute_fixture_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/add.evm");
    assert!(
        absolute_fixture_path.is_absolute(),
        "test fixture path should be absolute"
    );

    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 13,
        "method": "lift",
        "params": {
            "surface": "evm-bytecode",
            "workspace_root": env!("CARGO_MANIFEST_DIR"),
            "source_paths": [absolute_fixture_path]
        }
    });
    let mut child = Command::new(env!("CARGO_BIN_EXE_provekit-lift-evm-bytecode"))
        .arg("--rpc")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn EVM bytecode lifter RPC server");
    {
        let stdin = child.stdin.as_mut().expect("RPC stdin is piped");
        writeln!(stdin, "{request}").expect("write RPC request");
    }

    let output = child.wait_with_output().expect("wait for RPC server");
    assert!(
        output.status.success(),
        "RPC process should exit cleanly on stdin EOF: {:?}",
        output.status
    );
    let stdout = String::from_utf8(output.stdout).expect("RPC stdout is UTF-8");
    let response: Json = serde_json::from_str(stdout.trim()).expect("RPC response parses as JSON");
    let message = response["error"]["message"]
        .as_str()
        .expect("absolute source path response has an error message");
    assert!(
        message.contains("evm-bytecode lift: path must be relative to workspace_root"),
        "unexpected RPC error message: {message}"
    );
}
