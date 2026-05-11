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

#[test]
fn x86_lifter_accepts_c11_inline_asm_link_edge_source() {
    let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../menagerie/c11-language-signature/example/asm_link.term.json");
    let fixture = std::fs::read_to_string(&fixture_path)
        .unwrap_or_else(|err| panic!("read {}: {err}", fixture_path.display()));
    let value: serde_json::Value = serde_json::from_str(&fixture).expect("C11 asm term JSON");
    let asm_source = find_asm_link_edge_assembly_source(&value).expect("assembly_source slot");

    let temp_path = std::env::temp_dir().join(format!(
        "provekit-inline-asm-link-{}-{}.s",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    std::fs::write(&temp_path, asm_source).expect("write emitted asm source");
    let lifted =
        provekit_lift_asm_x86_64::lift_paths(".", &[temp_path.to_string_lossy().to_string()])
            .expect("x86 lifter accepts emitted asm source");
    let _ = std::fs::remove_file(&temp_path);

    assert!(
        lifted
            .contracts
            .iter()
            .any(|contract| contract.fn_name.starts_with("provekit_inline_asm_")),
        "expected x86 lifter contract for C-emitted inline asm source"
    );
}

fn lift_foo_fixture() -> provekit_lift_asm_x86_64::LiftResult {
    provekit_lift_asm_x86_64::lift_disassembly_text(FOO_SOURCE_PATH, FOO_DISASSEMBLY)
        .expect("foo disassembly lifts")
}

fn find_asm_link_edge_assembly_source(value: &serde_json::Value) -> Option<&str> {
    if value.get("kind").and_then(serde_json::Value::as_str) == Some("op")
        && value.get("name").and_then(serde_json::Value::as_str) == Some("asm-link-edge")
    {
        return value
            .get("args")
            .and_then(serde_json::Value::as_array)
            .and_then(|args| args.get(7))
            .and_then(|slot| slot.get("value"))
            .and_then(serde_json::Value::as_str);
    }
    match value {
        serde_json::Value::Array(items) => {
            items.iter().find_map(find_asm_link_edge_assembly_source)
        }
        serde_json::Value::Object(map) => map.values().find_map(find_asm_link_edge_assembly_source),
        _ => None,
    }
}

fn assert_formula_mentions(formula: &IrFormula, needle: &str) {
    let json = serde_json::to_string(formula).expect("formula json");
    assert!(
        json.contains(needle),
        "formula did not mention {needle}: {json}"
    );
}
