use provekit_ir_types::IrFormula;

const FOO_SOURCE_PATH: &str = "tests/fixtures/foo.s";
const FOO_DISASSEMBLY: &str = include_str!(concat!("fixtures/foo.gnu-", "obj", "dump.txt"));

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
fn smoke_lifts_vendored_foo_disassembly_to_function_contract_memento() {
    let contract = lift_foo_fixture()
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
fn rpc_lift_response_returns_ir_document_with_foo_contract() {
    let lifted = provekit_lift_asm_x86_64::lift_success_response_json(
        serde_json::json!(2),
        &lift_foo_fixture(),
    );
    assert_eq!(lifted["result"]["kind"], "ir-document");

    let declarations = lifted["result"]["declarations"]
        .as_array()
        .expect("declarations array");
    assert_eq!(declarations.len(), 1);
    assert_eq!(declarations[0]["fnName"], "foo");
    assert_eq!(declarations[0]["kind"], "function-contract");
}

fn lift_foo_fixture() -> provekit_lift_asm_x86_64::LiftResult {
    provekit_lift_asm_x86_64::lift_disassembly_text(FOO_SOURCE_PATH, FOO_DISASSEMBLY)
        .expect("foo disassembly lifts")
}

fn assert_formula_mentions(formula: &IrFormula, needle: &str) {
    let json = serde_json::to_string(formula).expect("formula json");
    assert!(
        json.contains(needle),
        "formula did not mention {needle}: {json}"
    );
}
