use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use provekit_lift_evm_bytecode::{
    lift_source_text, lift_success_response_json, parse_evm_text, Opcode,
};
use serde_json::Value as Json;

#[test]
fn parser_decodes_hex_push_add_stop() {
    let unit = parse_evm_text("add.hex", "0x600160020100").expect("hex EVM parses");

    assert_eq!(unit.instructions.len(), 4);
    assert_eq!(unit.instructions[0].pc, 0);
    assert_eq!(unit.instructions[1].pc, 2);
    assert_eq!(unit.instructions[2].pc, 4);
    assert_eq!(unit.instructions[3].pc, 5);
    assert_eq!(unit.instructions[0].opcode, Opcode::Push(1));
    assert_eq!(unit.instructions[0].immediate.as_deref(), Some("0x01"));
    assert_eq!(unit.instructions[2].opcode, Opcode::Add);
    assert_eq!(unit.instructions[3].opcode, Opcode::Stop);
}

#[test]
fn parser_uses_byte_offsets_for_assembly_pc() {
    let unit = parse_evm_text("pc.evmasm", "PUSH1 0x01\nPUSH2 0x0203\nADD\nSTOP\n")
        .expect("assembly EVM parses");

    let pcs: Vec<usize> = unit
        .instructions
        .iter()
        .map(|instruction| instruction.pc)
        .collect();
    assert_eq!(pcs, vec![0, 2, 5, 6]);
}

#[test]
fn assembly_extension_prevents_hex_misclassification() {
    let unit = parse_evm_text("all_hex_letters.evmasm", "DEAD\n")
        .expect("assembly-looking source parses as assembly");

    assert_eq!(unit.instructions.len(), 1);
    match &unit.instructions[0].opcode {
        Opcode::Unsupported { mnemonic, .. } => assert_eq!(mnemonic, "DEAD"),
        other => panic!("expected assembly mnemonic refusal, got {other:?}"),
    }
}

#[test]
fn extensionless_hex_heuristic_requires_0x_prefix() {
    let unit = parse_evm_text("all_hex_letters", "DEAD\n")
        .expect("extensionless source without 0x parses as assembly");

    assert_eq!(unit.instructions.len(), 1);
    match &unit.instructions[0].opcode {
        Opcode::Unsupported { mnemonic, .. } => assert_eq!(mnemonic, "DEAD"),
        other => panic!("expected assembly mnemonic refusal, got {other:?}"),
    }
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
fn lifts_equivalent_assembly_and_hex_to_identical_contracts() {
    let assembly =
        lift_source_text("trivial.evmasm", "PUSH1 0x2a\nSTOP\n").expect("assembly EVM lifts");
    let hex = lift_source_text("trivial.evmhex", "0x602a00\n").expect("hex EVM lifts");

    assert!(assembly.refusals.is_empty());
    assert!(hex.refusals.is_empty());
    assert_eq!(
        serde_json::to_vec(&assembly.declarations[0]).unwrap(),
        serde_json::to_vec(&hex.declarations[0]).unwrap()
    );
}

#[test]
fn refuses_empty_stack_stop_without_unterminated_lie() {
    let result = lift_source_text("empty_stop.evm", "STOP\n").expect("STOP parses");

    assert!(result.declarations.is_empty());
    assert_eq!(result.refusals.len(), 1);
    let refusal = &result.refusals[0];
    assert_eq!(refusal.kind, "stop-with-no-return-value");
    assert_eq!(refusal.instruction.as_deref(), Some("STOP"));
    assert_eq!(
        refusal.reason,
        "program terminates via STOP with an empty stack; no return value"
    );
    assert!(!refusal.reason.contains("stream ended without STOP"));
}

#[test]
fn refuses_return_until_memory_return_data_is_modeled() {
    let result = lift_source_text("return.evm", "PUSH1 0x00\nPUSH1 0x20\nRETURN\n")
        .expect("RETURN program parses");

    assert!(result.declarations.is_empty());
    assert_eq!(result.refusals.len(), 1);
    let refusal = &result.refusals[0];
    assert_eq!(refusal.kind, "unsupported-return-shape");
    assert_eq!(refusal.instruction.as_deref(), Some("RETURN"));
    assert_eq!(
        refusal.reason,
        "RETURN yields a memory slice; a bytes-return sort + memory-read effect are not yet modeled in this lifter slice"
    );
}

#[test]
fn signature_declares_all_emitted_stack_ops_with_arity_shapes() {
    let spec_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../../menagerie/evm-bytecode-language-signature/specs/evm_bytecode_signature.spec.json",
    );
    let spec: Json = serde_json::from_str(
        &std::fs::read_to_string(&spec_path)
            .unwrap_or_else(|err| panic!("read {}: {err}", spec_path.display())),
    )
    .expect("EVM signature spec parses");
    let operations = spec["post"]["primitiveOperations"]
        .as_array()
        .expect("primitiveOperations is an array");
    let names = operations
        .iter()
        .filter_map(|operation| operation["name"].as_str())
        .collect::<std::collections::BTreeSet<_>>();

    let emitted_ops = [
        "add", "mul", "sub", "div", "mod", "lt", "gt", "eq", "and", "or", "xor", "iszero", "not",
    ];
    for emitted in emitted_ops {
        assert!(
            names.contains(emitted),
            "signature must declare emitted op {emitted}"
        );
    }

    for operation in operations {
        assert!(
            operation.get("arity_shape").is_some(),
            "signature operation {} must declare arity_shape",
            operation["name"]
        );
    }

    let op_specs = spec["post"]["operations"]
        .as_array()
        .expect("operations is an array");
    let op_spec_names = op_specs
        .iter()
        .filter_map(Json::as_str)
        .collect::<std::collections::BTreeSet<_>>();
    for emitted in emitted_ops {
        let spec_file = format!("op_{emitted}.spec.json");
        assert!(
            op_spec_names.contains(spec_file.as_str()),
            "signature must register emitted op spec {spec_file}"
        );

        let op_spec_path = spec_path
            .parent()
            .expect("signature spec has parent directory")
            .join(&spec_file);
        let op_spec: Json = serde_json::from_str(
            &std::fs::read_to_string(&op_spec_path)
                .unwrap_or_else(|err| panic!("read {}: {err}", op_spec_path.display())),
        )
        .unwrap_or_else(|err| panic!("parse {}: {err}", op_spec_path.display()));
        assert_eq!(op_spec["fn_name"], format!("evm:{emitted}"));
        assert!(
            op_spec["post"].get("arity_shape").is_some(),
            "{spec_file} must declare post.arity_shape"
        );
    }
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
