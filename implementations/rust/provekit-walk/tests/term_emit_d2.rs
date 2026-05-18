// SPDX-License-Identifier: Apache-2.0

use provekit_walk::emit::rust_function_term_json;

fn parse_named(src: &str, name: &str) -> syn::ItemFn {
    let file: syn::File = syn::parse_str(src).unwrap();
    file.items
        .into_iter()
        .find_map(|item| match item {
            syn::Item::Fn(f) if f.sig.ident == name => Some(f),
            _ => None,
        })
        .unwrap()
}

fn term_json(src: &str, name: &str) -> serde_json::Value {
    let item_fn = parse_named(src, name);
    let bytes = rust_function_term_json(&item_fn, "d2.rs").unwrap();
    serde_json::from_slice(&bytes).expect("term JSON")
}

#[test]
fn lowers_let_binding_to_rust_let_op() {
    let parsed = term_json(
        r#"
            fn with_let(x: i32) -> i32 {
                let y = x + 1;
                y
            }
        "#,
        "with_let",
    );
    assert_eq!(
        parsed["term_surface"].as_str(),
        Some("let(pattern_bind(y), add(x, 1), return(y))")
    );
    assert_eq!(parsed["term"]["name"].as_str(), Some("let"));
}

#[test]
fn lowers_struct_return_type_with_partial_loss_record() {
    let parsed = term_json(
        r#"
            struct Point { x: i32, y: i32 }
            fn make_point(x: i32) -> Point {
                Point { x, y: x + 1 }
            }
        "#,
        "make_point",
    );
    assert_eq!(
        parsed["handling"].as_str(),
        Some("handles-partially-with-loss-record")
    );
    assert_eq!(
        parsed["loss_record"][0]["loss"].as_str(),
        Some("return-type-user-defined")
    );
    assert_eq!(
        parsed["term_surface"].as_str(),
        Some("return(Point{x: x, y: add(x, 1)})")
    );
}

#[test]
fn lowers_result_return_type_with_partial_loss_record() {
    let parsed = term_json(
        r#"
            fn ok_value(x: i32) -> Result<i32, i32> {
                Ok(x)
            }
        "#,
        "ok_value",
    );
    assert_eq!(
        parsed["handling"].as_str(),
        Some("handles-partially-with-loss-record")
    );
    assert!(parsed["term_surface"].as_str().unwrap().contains("call:Ok"));
    assert!(parsed["loss_record"]
        .as_array()
        .unwrap()
        .iter()
        .any(|loss| loss["loss"] == "return-type-result"));
}

#[test]
fn lowers_option_return_type_with_partial_loss_record() {
    let parsed = term_json(
        r#"
            fn maybe_value(x: i32) -> Option<i32> {
                Some(x)
            }
        "#,
        "maybe_value",
    );
    assert_eq!(
        parsed["handling"].as_str(),
        Some("handles-partially-with-loss-record")
    );
    assert!(parsed["term_surface"]
        .as_str()
        .unwrap()
        .contains("call:Some"));
    assert!(parsed["loss_record"]
        .as_array()
        .unwrap()
        .iter()
        .any(|loss| loss["loss"] == "return-type-option"));
}

#[test]
fn lowers_byte_vec_return_type_with_partial_loss_record() {
    let parsed = term_json(
        r#"
            fn bytes() -> Vec<u8> {
                vec![1u8, 2u8, 3u8]
            }
        "#,
        "bytes",
    );
    assert_eq!(
        parsed["handling"].as_str(),
        Some("handles-partially-with-loss-record")
    );
    assert_eq!(
        parsed["term_surface"].as_str(),
        Some("return(array([1, 2, 3]))")
    );
    assert!(parsed["loss_record"]
        .as_array()
        .unwrap()
        .iter()
        .any(|loss| loss["loss"] == "return-type-byte-vec"));
}

#[test]
fn lowers_simple_call_expression_without_ffi_effect_loss() {
    let parsed = term_json(
        r#"
            fn helper(x: i32) -> i32 { x + 1 }
            fn caller(x: i32) -> i32 { helper(x) }
        "#,
        "caller",
    );
    assert_eq!(
        parsed["term_surface"].as_str(),
        Some("return(call:helper(helper, [x]))")
    );
    assert!(parsed["loss_record"].as_array().unwrap().is_empty());
    assert!(parsed["effect_occurrences"].as_array().unwrap().is_empty());
}

#[test]
fn lowers_qualified_constructor_call_with_receiver_prefix() {
    let parsed = term_json(
        r#"
            struct Arc<T>(T);
            enum Value { Null }
            fn null() -> Arc<Value> { Arc::new(Value::Null) }
        "#,
        "null",
    );
    assert_eq!(
        parsed["term_surface"].as_str(),
        Some("return(call:new(Arc::new, [Null]))")
    );
    assert_eq!(parsed["term"]["args"][0]["args"][0]["name"], "Arc::new");
    assert!(!parsed["loss_record"]
        .as_array()
        .unwrap()
        .iter()
        .any(|loss| { loss["loss"] == "trait-path-truncated" && loss["detail"] == "Arc :: new" }));
}

#[test]
fn lowers_simple_method_call_expression_without_ffi_effect_loss() {
    let parsed = term_json(
        r#"
            struct Counter(i32);
            impl Counter {
                fn bump(&self, by: i32) -> i32 { self.0 + by }
            }
            fn caller(c: Counter) -> i32 { c.bump(1) }
        "#,
        "caller",
    );
    assert_eq!(
        parsed["term_surface"].as_str(),
        Some("return(method:bump(c, [1]))")
    );
    assert!(parsed["loss_record"].as_array().unwrap().is_empty());
    assert!(parsed["effect_occurrences"].as_array().unwrap().is_empty());
}

#[test]
fn lowers_statement_method_chain_receiver_as_nested_method_term() {
    let parsed = term_json(
        r#"
            struct Receiver;
            fn caller(expr: Receiver) {
                expr.method1().method2();
            }
        "#,
        "caller",
    );
    assert_eq!(
        parsed["term_surface"].as_str(),
        Some("method:method2(method:method1(expr, []), [])")
    );
    assert_eq!(parsed["term"]["name"].as_str(), Some("method:method2"));
    assert_eq!(
        parsed["term"]["args"][0]["name"].as_str(),
        Some("method:method1")
    );
    assert_eq!(parsed["term"]["args"][0]["args"][0]["name"], "expr");
}
