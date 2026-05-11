use std::io::Write;
use std::process::{Command, Stdio};

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
    assert!(unit.methods[0].is_static);
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
fn lifts_instance_method_with_this_in_local_zero() {
    let source = include_str!("fixtures/instance_method.j");
    let unit =
        parse_jasmin_text("InstanceExample.j", source).expect("instance Jasmin fixture parses");
    assert!(!unit.methods[0].is_static);
    assert_eq!(unit.methods[0].arg_count, 1);
    assert_eq!(unit.methods[0].local_slot_count(), 2);

    let result = lift_source_text("InstanceExample.j", source).expect("JVM bytecode lifts");

    assert_eq!(result.declarations.len(), 1);
    assert!(
        result.refusals.is_empty(),
        "unexpected refusals: {:?}",
        result.refusals
    );
    let contract = &result.declarations[0];
    assert_eq!(contract["fnName"], "add");
    assert_eq!(contract["formals"], serde_json::json!(["local0", "local1"]));

    let post = serde_json::to_string(&contract["post"]).unwrap();
    assert!(post.contains("local1"));
    assert!(post_equates_return_to_local(contract, "local1"));
    assert!(!post_equates_return_to_local(contract, "local0"));
    assert!(!has_undeclared_local_reference(contract));
}

#[test]
fn lifts_read_only_static_field_method() {
    let source = include_str!("fixtures/static_read_only.j");
    let result = lift_source_text("StaticReadOnly.j", source).expect("JVM bytecode lifts");

    assert_eq!(result.declarations.len(), 1);
    assert!(
        result.refusals.is_empty(),
        "unexpected refusals: {:?}",
        result.refusals
    );

    let contract = &result.declarations[0];
    assert_eq!(contract["fnName"], "read");
    assert_eq!(
        contract["effects"],
        serde_json::json!([{"kind":"reads","target":"StaticReadOnly/value I"}])
    );
    let post = serde_json::to_string(&contract["post"]).unwrap();
    assert!(post.contains("static:StaticReadOnly/value I"));
}

#[test]
fn refuses_static_field_read_after_write() {
    let source = include_str!("fixtures/static_read_after_write.j");
    let result = lift_source_text("StaticReadAfterWrite.j", source).expect("JVM bytecode lifts");

    assert!(
        result.declarations.is_empty(),
        "stale static read method must not emit declarations: {:?}",
        result.declarations
    );
    assert_eq!(result.refusals.len(), 1);
    let refusal = &result.refusals[0];
    assert_eq!(refusal.function.as_deref(), Some("stale"));
    assert_eq!(
        refusal.reason,
        "jvm-bytecode lift refuses methods that read a static field after writing it: stale-read modeling not yet supported (static:StaticReadAfterWrite/value I)"
    );
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

#[test]
fn rpc_lift_rejects_absolute_source_paths() {
    let absolute_fixture_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/foo.j");
    assert!(
        absolute_fixture_path.is_absolute(),
        "test fixture path should be absolute"
    );

    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "lift",
        "params": {
            "surface": "jvm-bytecode",
            "workspace_root": env!("CARGO_MANIFEST_DIR"),
            "source_paths": [absolute_fixture_path]
        }
    });
    let mut child = Command::new(env!("CARGO_BIN_EXE_provekit-lift-jvm-bytecode"))
        .arg("--rpc")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn JVM bytecode lifter RPC server");
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
        message.contains("jvm-bytecode lift: path must be relative to workspace_root"),
        "unexpected RPC error message: {message}"
    );
}

fn has_undeclared_local_reference(contract: &Json) -> bool {
    let formals = contract["formals"]
        .as_array()
        .expect("contract formals are an array")
        .iter()
        .map(|formal| {
            formal
                .as_str()
                .expect("contract formal is a string")
                .to_string()
        })
        .collect::<std::collections::BTreeSet<_>>();
    let mut references = Vec::new();
    collect_local_references(&contract["post"], &mut references);
    references
        .into_iter()
        .any(|reference| !formals.contains(&reference))
}

fn post_equates_return_to_local(contract: &Json, local: &str) -> bool {
    contains_return_equation(&contract["post"], local)
}

fn contains_return_equation(value: &Json, local: &str) -> bool {
    match value {
        Json::Object(map) => {
            let is_return_equation = map.get("kind").and_then(Json::as_str) == Some("atomic")
                && map.get("name").and_then(Json::as_str) == Some("=")
                && map
                    .get("args")
                    .and_then(Json::as_array)
                    .is_some_and(|args| {
                        args.len() == 2
                            && is_var_named(&args[0], "return_value")
                            && is_var_named(&args[1], local)
                    });
            is_return_equation
                || map
                    .values()
                    .any(|child| contains_return_equation(child, local))
        }
        Json::Array(items) => items
            .iter()
            .any(|item| contains_return_equation(item, local)),
        _ => false,
    }
}

fn is_var_named(value: &Json, name: &str) -> bool {
    value.get("kind").and_then(Json::as_str) == Some("var")
        && value.get("name").and_then(Json::as_str) == Some(name)
}

fn collect_local_references(value: &Json, references: &mut Vec<String>) {
    match value {
        Json::Object(map) => {
            if let Some(name) = map.get("name").and_then(Json::as_str) {
                if is_input_local_name(name) {
                    references.push(name.to_string());
                }
            }
            for child in map.values() {
                collect_local_references(child, references);
            }
        }
        Json::Array(items) => {
            for item in items {
                collect_local_references(item, references);
            }
        }
        _ => {}
    }
}

fn is_input_local_name(name: &str) -> bool {
    name.strip_prefix("local")
        .is_some_and(|suffix| !suffix.is_empty() && suffix.chars().all(|ch| ch.is_ascii_digit()))
}
