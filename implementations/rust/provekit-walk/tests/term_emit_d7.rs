// SPDX-License-Identifier: Apache-2.0

use provekit_walk::emit::rust_function_term_json_for_file;

fn term_json(src: &str, name: &str) -> serde_json::Value {
    let file: syn::File = syn::parse_str(src).unwrap();
    let bytes = rust_function_term_json_for_file(&file, name, "d7.rs").unwrap();
    serde_json::from_slice(&bytes).expect("term JSON")
}

fn assert_loss(parsed: &serde_json::Value, dimension: &str) {
    assert_eq!(
        parsed["handling"].as_str(),
        Some("handles-partially-with-loss-record")
    );
    assert!(parsed["loss_record"]
        .as_array()
        .unwrap()
        .iter()
        .any(|loss| loss["loss"] == dimension));
}

#[test]
fn d7_lowers_mut_let_binding_with_path_call_rhs() {
    let parsed = term_json(
        r#"
            fn init_hasher() {
                let mut hasher = blake3::Hasher::new();
            }
        "#,
        "init_hasher",
    );

    assert_eq!(
        parsed["term_surface"].as_str(),
        Some("let(pattern_bind(hasher), call:new(blake3::Hasher::new, []), skip)")
    );
    assert_loss(&parsed, "let-binding-mutability");
}

#[test]
fn d7_lowers_let_binding_with_method_call_rhs() {
    let parsed = term_json(
        r#"
            struct Expr;
            impl Expr {
                fn some_method(self, arg: i32) -> i32 { arg }
            }
            fn use_method(expr: Expr, arg: i32) {
                let result = expr.some_method(arg);
            }
        "#,
        "use_method",
    );

    assert_eq!(
        parsed["term_surface"].as_str(),
        Some("let(pattern_bind(result), method:some_method(expr, [arg]), skip)")
    );
    assert_loss(&parsed, "ffi-call-unresolved-effect");
}

#[test]
fn d7_lowers_wildcard_let_binding_with_path_call_rhs() {
    let parsed = term_json(
        r#"
            fn discard(out: Vec<u8>) {
                let _ = hex::encode(out);
            }
        "#,
        "discard",
    );

    assert_eq!(
        parsed["term_surface"].as_str(),
        Some("let(pattern_wild(), call:encode(hex::encode, [out]), skip)")
    );
    assert_loss(&parsed, "ffi-call-unresolved-effect");
}

#[test]
fn d7_lowers_closure_in_value_position_with_loss_record() {
    let parsed = term_json(
        r#"
            fn make_incrementer() {
                let inc = |x| x + 1;
            }
        "#,
        "make_incrementer",
    );

    assert_eq!(
        parsed["term_surface"].as_str(),
        Some("let(pattern_bind(inc), closure([x], add(x, 1)), skip)")
    );
    assert_loss(&parsed, "closure-captures-environment");
}
