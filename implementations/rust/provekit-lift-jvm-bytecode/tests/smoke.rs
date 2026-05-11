use provekit_ir_compiler_jvm_bytecode::compile_jasmin;
use provekit_lift_jvm_bytecode::{lift_source_text, lift_success_response_json, parse_jasmin_text};
use serde_json::Value as Json;

#[test]
fn parser_accepts_checked_jasmin_fixture() {
    let source = include_str!("fixtures/foo.j");
    let unit = parse_jasmin_text("Foo.j", source).expect("Jasmin fixture parses");

    assert_eq!(unit.class_name.as_deref(), Some("Foo"));
    assert_eq!(unit.methods.len(), 1);
    assert_eq!(unit.methods[0].name, "foo");
    assert_eq!(unit.methods[0].descriptor, "(I)I");
    assert_eq!(unit.methods[0].instructions.len(), 12);
}

#[test]
fn lifts_checked_jasmin_fixture_to_contract() {
    let source = include_str!("fixtures/foo.j");
    let result = lift_source_text("Foo.j", source).expect("JVM bytecode lifts");

    assert_eq!(result.declarations.len(), 1);
    assert!(
        result.refusals.is_empty(),
        "unexpected refusals: {:?}",
        result.refusals
    );
    assert_eq!(result.declarations[0]["fnName"], "foo");
    let post = serde_json::to_string(&result.declarations[0]["post"]).unwrap();
    assert!(post.contains("jvm:icmp_eq"));
    assert!(post.contains("jvm:ite"));
}

#[test]
fn c11_to_jvm_realizer_output_relifts_as_jvm_contract() {
    let input: Json = serde_json::from_str(include_str!(
        "../../provekit-ir-compiler-jvm-bytecode/tests/fixtures/foo.term.json"
    ))
    .expect("foo term JSON parses");
    let jasmin = compile_jasmin(&input).expect("C11 term compiles to JVM Jasmin");

    let result = lift_source_text("Foo.j", &jasmin).expect("JVM realizer output relifts");

    assert_eq!(result.declarations.len(), 1);
    assert!(
        result.refusals.is_empty(),
        "unexpected refusals: {:?}",
        result.refusals
    );
    let contract = &result.declarations[0];
    assert_eq!(contract["kind"], "function-contract");
    assert_eq!(contract["fnName"], "foo");
    assert_eq!(contract["formals"], serde_json::json!(["local0"]));
    let post = serde_json::to_string(&contract["post"]).unwrap();
    assert!(post.contains("return_value"));
    assert!(post.contains("jvm:ineg"));
    assert!(post.contains("local0"));
}

#[test]
fn rpc_response_wraps_relifted_contracts() {
    let source = include_str!("fixtures/foo.j");
    let result = lift_source_text("Foo.j", source).expect("JVM bytecode lifts");
    let response = lift_success_response_json(serde_json::json!(7), &result);

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 7);
    assert_eq!(response["result"]["kind"], "ir-document");
    assert_eq!(response["result"]["ir"][0]["fnName"], "foo");
    assert!(response["result"].get("declarations").is_none());
}
