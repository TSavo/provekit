// SPDX-License-Identifier: Apache-2.0
//
// Language-coverage tests: each Rust expression shape that lifts to an
// IrTerm or IrFormula gets one assertion here. Demonstrates that the
// substrate accepts realistic Rust source as input, not just toy fixtures.

use provekit_walk::lift::{lift_expr_to_term, lift_predicate};

fn parse_expr(src: &str) -> syn::Expr {
    syn::parse_str(src).expect("expr parses")
}

#[test]
fn references_pass_through() {
    // &x lifts as just x; &mut x same.
    let t1 = lift_expr_to_term(&parse_expr("&x")).unwrap();
    let t2 = lift_expr_to_term(&parse_expr("&mut x")).unwrap();
    let t3 = lift_expr_to_term(&parse_expr("x")).unwrap();
    assert_eq!(t1, t3);
    assert_eq!(t2, t3);
}

#[test]
fn casts_pass_through() {
    // `x as u32` lifts as just x.
    let t1 = lift_expr_to_term(&parse_expr("x as u32")).unwrap();
    let t2 = lift_expr_to_term(&parse_expr("x")).unwrap();
    assert_eq!(t1, t2);
}

#[test]
fn deref_passes_through() {
    // `*x` lifts as x for substitution purposes.
    let t1 = lift_expr_to_term(&parse_expr("*x")).unwrap();
    let t2 = lift_expr_to_term(&parse_expr("x")).unwrap();
    assert_eq!(t1, t2);
}

#[test]
fn field_access_lifts_as_ctor() {
    let t = lift_expr_to_term(&parse_expr("s.field")).unwrap();
    let json = serde_json::to_string(&t).unwrap();
    assert!(json.contains("\"field\""));
    assert!(json.contains("\".field\""), "field name encoded with leading dot: {}", json);
}

#[test]
fn index_lifts_as_ctor() {
    let t = lift_expr_to_term(&parse_expr("arr[0]")).unwrap();
    let json = serde_json::to_string(&t).unwrap();
    assert!(json.contains("\"index\""));
    assert!(json.contains("\"arr\""));
}

#[test]
fn method_call_lifts_with_method_name() {
    let t = lift_expr_to_term(&parse_expr("v.len()")).unwrap();
    let json = serde_json::to_string(&t).unwrap();
    assert!(json.contains("\"method:len\""));
    assert!(json.contains("\"v\""));
}

#[test]
fn method_call_with_args_includes_them() {
    let t = lift_expr_to_term(&parse_expr("buf.write(data, 5)")).unwrap();
    let json = serde_json::to_string(&t).unwrap();
    assert!(json.contains("\"method:write\""));
    assert!(json.contains("\"buf\""));
    assert!(json.contains("\"data\""));
    assert!(json.contains("5"));
}

#[test]
fn range_lifts_as_ctor() {
    let half_open = lift_expr_to_term(&parse_expr("0..10")).unwrap();
    let closed = lift_expr_to_term(&parse_expr("0..=10")).unwrap();
    let half_json = serde_json::to_string(&half_open).unwrap();
    let closed_json = serde_json::to_string(&closed).unwrap();
    assert!(half_json.contains("\"range\""));
    assert!(closed_json.contains("\"range_incl\""));
}

#[test]
fn tuple_lifts_as_ctor() {
    let t = lift_expr_to_term(&parse_expr("(a, b, 42)")).unwrap();
    let json = serde_json::to_string(&t).unwrap();
    assert!(json.contains("\"tuple\""));
    assert!(json.contains("\"a\""));
    assert!(json.contains("\"b\""));
    assert!(json.contains("42"));
}

#[test]
fn bool_literal_lifts() {
    let t_true = lift_expr_to_term(&parse_expr("true")).unwrap();
    let t_false = lift_expr_to_term(&parse_expr("false")).unwrap();
    let json_true = serde_json::to_string(&t_true).unwrap();
    let json_false = serde_json::to_string(&t_false).unwrap();
    assert!(json_true.contains("\"Bool\""));
    assert!(json_true.contains("true"));
    assert!(json_false.contains("false"));
}

#[test]
fn bitwise_ops_lift_as_ctor() {
    for (src, op) in [
        ("x & y", "&"),
        ("x | y", "|"),
        ("x ^ y", "^"),
        ("x << 2", "<<"),
        ("x >> 1", ">>"),
    ] {
        let t = lift_expr_to_term(&parse_expr(src)).unwrap();
        let json = serde_json::to_string(&t).unwrap();
        assert!(json.contains(&format!("\"{}\"", op)), "{} → {}: {}", src, op, json);
    }
}

#[test]
fn predicate_with_field_access() {
    // `s.len > 10` lifts as `(> (field s ".len") 10)`.
    let f = lift_predicate(&parse_expr("s.len > 10")).unwrap();
    let json = serde_json::to_string(&f).unwrap();
    assert!(json.contains("\">\""));
    assert!(json.contains("\"field\""));
    assert!(json.contains("\".len\""));
}

#[test]
fn predicate_with_method_call() {
    // `v.is_empty() == false` ⇒ atomic with method:is_empty term.
    let f = lift_predicate(&parse_expr("v.is_empty() == false")).unwrap();
    let json = serde_json::to_string(&f).unwrap();
    assert!(json.contains("\"=\""));
    assert!(json.contains("\"method:is_empty\""));
}
