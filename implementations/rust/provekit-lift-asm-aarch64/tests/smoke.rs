use provekit_lift_asm_aarch64::{lift_source_text, parse_assembly_text, semantics_for_instruction};

fn fixture(name: &str) -> String {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name);
    std::fs::read_to_string(path).expect("fixture should be readable")
}

#[test]
fn smoke_lifts_foo_to_function_contract() {
    let source = fixture("foo.s");
    let result = lift_source_text("foo.s", &source).expect("lift should succeed");

    assert_eq!(result.declarations.len(), 1);
    assert!(
        result.refusals.is_empty(),
        "unexpected refusals: {:?}",
        result.refusals
    );

    let contract = &result.declarations[0];
    assert_eq!(contract["kind"], "function-contract");
    assert_eq!(contract["fnName"], "foo");
    assert_eq!(contract["formals"], serde_json::json!(["w0"]));
    assert_eq!(contract["effects"], serde_json::json!([]));

    let post = serde_json::to_string(&contract["post"]).unwrap();
    assert!(post.contains("ite"));
    assert!(post.contains("w0_out"));
    assert!(post.contains("w0"));
    assert!(post.contains("-22"));
}

#[test]
fn parser_recognizes_labels_and_instructions() {
    let source = fixture("foo.s");
    let unit = parse_assembly_text("foo.s", &source).expect("parse should succeed");
    assert_eq!(unit.functions.len(), 1);
    assert_eq!(unit.functions[0].name, "foo");
    assert_eq!(unit.functions[0].instructions.len(), 4);
}

#[test]
fn adds_semantics_updates_result_and_flags() {
    let source = "f:\n    adds x2, x0, x1\n    ret\n";
    let unit = parse_assembly_text("inline.s", source).expect("parse should succeed");
    let sem = semantics_for_instruction(&unit.functions[0].instructions[0])
        .expect("adds semantics should be present");

    let text = serde_json::to_string(&sem.postconditions).unwrap();
    assert!(text.contains("x2_out"));
    assert!(text.contains("bvadd64"));
    assert!(text.contains("N_out"));
    assert!(text.contains("Z_out"));
    assert!(text.contains("C_out"));
    assert!(text.contains("V_out"));
}

#[test]
fn memory_semantics_reports_read_effect_and_preconditions() {
    let source = "f:\n    ldr x0, [x1]\n    ret\n";
    let unit = parse_assembly_text("inline.s", source).expect("parse should succeed");
    let sem = semantics_for_instruction(&unit.functions[0].instructions[0])
        .expect("ldr semantics should be present");

    assert_eq!(sem.effects, vec!["MemRead:x1"]);
    let pre = serde_json::to_string(&sem.preconditions).unwrap();
    assert!(pre.contains("aligned64"));
    assert!(pre.contains("valid_read64"));
}
