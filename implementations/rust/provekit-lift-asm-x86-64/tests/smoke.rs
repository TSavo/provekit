use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use provekit_ir_types::IrFormula;
use serde_json::Value;

#[test]
fn mov_eax_edi_zero_extends_into_rax() {
    let insn = provekit_lift_asm_x86_64::Instruction::new_for_test("mov", &["eax", "edi"]);
    let initial = provekit_lift_asm_x86_64::MachineState::entry();

    let next = provekit_lift_asm_x86_64::apply_instruction(&initial, &insn)
        .expect("mov is in the core table");

    assert_eq!(next.register_expr("rax").to_string(), "zext32(low32(rdi))");
    assert!(next.effects().is_empty());
}

#[test]
fn add_eax_ebx_updates_result_and_core_flags() {
    let insn = provekit_lift_asm_x86_64::Instruction::new_for_test("add", &["eax", "ebx"]);
    let initial = provekit_lift_asm_x86_64::MachineState::entry();

    let next = provekit_lift_asm_x86_64::apply_instruction(&initial, &insn)
        .expect("add is in the core table");

    assert_eq!(
        next.register_expr("rax").to_string(),
        "zext32(add32(low32(rax), low32(rbx)))"
    );
    assert_eq!(
        next.flag_expr("ZF").to_string(),
        "eq32(add32(low32(rax), low32(rbx)), 0x0)"
    );
    assert_eq!(
        next.flag_expr("CF").to_string(),
        "carry_add32(low32(rax), low32(rbx))"
    );
    assert!(next.effects().is_empty());
}

#[test]
fn smoke_lifts_foo_s_to_function_contract_memento() {
    let contract = provekit_lift_asm_x86_64::lift_paths(
        env!("CARGO_MANIFEST_DIR"),
        &["tests/fixtures/foo.s".to_string()],
    )
    .expect("foo.s lifts")
    .contracts
    .into_iter()
    .find(|contract| contract.fn_name == "foo")
    .expect("foo contract is present");

    assert_eq!(contract.fn_name, "foo");
    assert!(contract.effects.effects.is_empty());
    assert_formula_mentions(&contract.post, "eax_post");
    assert_formula_mentions(&contract.post, "0xffffffea");
    assert_formula_mentions(&contract.post, "edi");
}

#[test]
fn rpc_lift_returns_ir_document_with_foo_contract() {
    let bin = env!("CARGO_BIN_EXE_provekit-lift-asm-x86-64");
    let mut child = Command::new(bin)
        .arg("--rpc")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn lifter");

    let mut stdin = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");
    let mut reader = BufReader::new(stdout);

    writeln!(
        stdin,
        "{}",
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "client": {"name": "test", "version": "0"},
                "protocol_version": "provekit-lift/1",
                "workspace_root": env!("CARGO_MANIFEST_DIR"),
                "config_path": ""
            }
        })
    )
    .expect("write initialize");

    let mut line = String::new();
    reader.read_line(&mut line).expect("read initialize");
    let init: Value = serde_json::from_str(&line).expect("initialize json");
    assert_eq!(init["result"]["protocol_version"], "provekit-lift/1");

    writeln!(
        stdin,
        "{}",
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "lift",
            "params": {
                "workspace_root": env!("CARGO_MANIFEST_DIR"),
                "surface": "x86-64:sysv",
                "source_paths": ["tests/fixtures/foo.s"],
                "options": {"layer": "all"}
            }
        })
    )
    .expect("write lift");

    line.clear();
    reader.read_line(&mut line).expect("read lift");
    let lifted: Value = serde_json::from_str(&line).expect("lift json");
    assert_eq!(lifted["result"]["kind"], "ir-document");

    let declarations = lifted["result"]["declarations"]
        .as_array()
        .expect("declarations array");
    assert_eq!(declarations.len(), 1);
    assert_eq!(declarations[0]["fnName"], "foo");
    assert_eq!(declarations[0]["kind"], "function-contract");

    writeln!(
        stdin,
        "{}",
        serde_json::json!({"jsonrpc": "2.0", "id": 3, "method": "shutdown"})
    )
    .expect("write shutdown");
    drop(stdin);

    let _ = child.wait().expect("wait");
}

fn assert_formula_mentions(formula: &IrFormula, needle: &str) {
    let json = serde_json::to_string(formula).expect("formula json");
    assert!(
        json.contains(needle),
        "formula did not mention {needle}: {json}"
    );
}
