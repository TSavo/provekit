// SPDX-License-Identifier: Apache-2.0

use provekit_walk::emit::rust_function_term_json_for_file;

fn term_json(src: &str, name: &str) -> serde_json::Value {
    let file: syn::File = syn::parse_str(src).unwrap();
    let bytes = rust_function_term_json_for_file(&file, name, "d3.rs").unwrap();
    serde_json::from_slice(&bytes).expect("term JSON")
}

fn assert_partial_loss(parsed: &serde_json::Value, dimension: &str) {
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
fn accepts_procedural_macro_as_named_loss() {
    let parsed = term_json(
        r#"
            #[instrument]
            fn traced(x: i32) -> i32 {
                x
            }
        "#,
        "traced",
    );
    assert_partial_loss(&parsed, "procedural-macro");
}

#[test]
fn accepts_trait_path_truncation_as_named_loss() {
    let parsed = term_json(
        r#"
            mod math {
                pub const VALUE: i32 = 1;
            }

            fn caller() -> i32 {
                math::VALUE
            }
        "#,
        "caller",
    );
    assert_partial_loss(&parsed, "trait-path-truncated");
}

#[test]
fn accepts_impl_associated_type_as_named_loss() {
    let parsed = term_json(
        r#"
            trait Shape {
                type Output;
                fn value(&self) -> i32;
            }

            struct UnitShape;

            impl Shape for UnitShape {
                type Output = i32;

                fn value(&self) -> i32 {
                    1
                }
            }
        "#,
        "value",
    );
    assert_partial_loss(&parsed, "impl-associated-type-not-lowered");
}

#[test]
fn accepts_abi_attribute_as_named_loss() {
    let parsed = term_json(
        r#"
            extern "C" fn exposed(x: i32) -> i32 {
                x
            }
        "#,
        "exposed",
    );
    assert_partial_loss(&parsed, "abi-attribute-not-carried");
}

#[test]
fn accepts_statement_macro_as_named_loss() {
    let parsed = term_json(
        r#"
            fn checked(x: i32) -> i32 {
                debug_assert!(x >= 0);
                x
            }
        "#,
        "checked",
    );
    assert_partial_loss(&parsed, "macro-not-expanded");
}
