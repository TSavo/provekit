//! Mirror of `src/ir/symbolic/symbolic.test.ts` — primitive-shape assertions.

use provekit_ir_symbolic::types::sorts;
use provekit_ir_symbolic::*;
use serde_json::json;

#[test]
fn num_builds_int_constant() {
    let t = num(42_i64);
    assert_eq!(serde_json::to_value(&t).unwrap(),
        json!({"kind": "const", "value": 42, "sort": {"kind": "primitive", "name": "Int"}}));
}

#[test]
fn num_builds_real_for_non_integer_f64() {
    let t = num(1.5_f64);
    let v = serde_json::to_value(&t).unwrap();
    assert_eq!(v["sort"]["name"], json!("Real"));
    assert_eq!(v["value"], json!(1.5));
}

#[test]
fn str_builds_string_constant() {
    let t = str_("0");
    assert_eq!(serde_json::to_value(&t).unwrap(),
        json!({"kind": "const", "value": "0", "sort": {"kind": "primitive", "name": "String"}}));
}

#[test]
fn bool_builds_bool_constant() {
    assert_eq!(serde_json::to_value(&bool_(true)).unwrap(),
        json!({"kind": "const", "value": true, "sort": {"kind": "primitive", "name": "Bool"}}));
    assert_eq!(serde_json::to_value(&bool_(false)).unwrap()["value"], json!(false));
}

#[test]
fn real_always_builds_real() {
    let t = real(3.0);
    assert_eq!(serde_json::to_value(&t).unwrap()["sort"]["name"], json!("Real"));
}

#[test]
fn parse_int_builds_apply_ctor_returning_int() {
    let t = parse_int(str_("0"));
    let v = serde_json::to_value(&t).unwrap();
    assert_eq!(v["kind"], json!("ctor"));
    assert_eq!(v["name"], json!("parseInt"));
    assert_eq!(v["sort"]["name"], json!("Int"));
    assert_eq!(v["args"][0]["value"], json!("0"));
}

#[test]
fn parse_float_returns_real() {
    let t = parse_float(str_("0.5"));
    assert_eq!(serde_json::to_value(&t).unwrap()["sort"]["name"], json!("Real"));
}

#[test]
fn abs_preserves_input_sort() {
    let t = abs(num(-3_i64));
    assert_eq!(serde_json::to_value(&t).unwrap()["sort"]["name"], json!("Int"));
}

#[test]
fn is_nan_is_integer_return_bool() {
    assert_eq!(serde_json::to_value(&is_nan(num(0_i64))).unwrap()["sort"]["name"], json!("Bool"));
    assert_eq!(serde_json::to_value(&is_integer(num(0_i64))).unwrap()["sort"]["name"], json!("Bool"));
    assert_eq!(serde_json::to_value(&is_finite(num(0_i64))).unwrap()["sort"]["name"], json!("Bool"));
}

#[test]
fn max_preserves_first_arg_sort() {
    let t = max(num(1_i64), num(2_i64));
    assert_eq!(serde_json::to_value(&t).unwrap()["sort"]["name"], json!("Int"));
    let t2 = min(real(1.5), real(2.5));
    assert_eq!(serde_json::to_value(&t2).unwrap()["sort"]["name"], json!("Real"));
}

#[test]
fn floor_ceil_sign_return_int() {
    assert_eq!(serde_json::to_value(&floor(real(1.5))).unwrap()["sort"]["name"], json!("Int"));
    assert_eq!(serde_json::to_value(&ceil(real(1.5))).unwrap()["sort"]["name"], json!("Int"));
    assert_eq!(serde_json::to_value(&sign(num(-3_i64))).unwrap()["sort"]["name"], json!("Int"));
}

#[test]
fn sqrt_returns_real() {
    let t = sqrt(num(4_i64));
    let v = serde_json::to_value(&t).unwrap();
    assert_eq!(v["name"], json!("Math.sqrt"));
    assert_eq!(v["sort"]["name"], json!("Real"));
}

#[test]
fn string_array_helpers_have_correct_sorts() {
    assert_eq!(serde_json::to_value(&string_length(str_("hi"))).unwrap()["sort"]["name"], json!("Int"));
    assert_eq!(serde_json::to_value(&string_includes(str_("hi"), str_("h"))).unwrap()["sort"]["name"], json!("Bool"));
    assert_eq!(serde_json::to_value(&array_length(str_("[]"))).unwrap()["sort"]["name"], json!("Int"));
    assert_eq!(serde_json::to_value(&array_includes(str_("[]"), num(0_i64))).unwrap()["sort"]["name"], json!("Bool"));
}

#[test]
fn add_lifts_numbers_to_const_terms() {
    let t = add(2_i64, 3_i64);
    let v = serde_json::to_value(&t).unwrap();
    assert_eq!(v["kind"], json!("ctor"));
    assert_eq!(v["name"], json!("+"));
    assert_eq!(v["args"][0], json!({"kind": "const", "value": 2, "sort": {"kind": "primitive", "name": "Int"}}));
    assert_eq!(v["args"][1]["value"], json!(3));
}

#[test]
fn sub_mul_div_neg_build_expected_ctors() {
    assert_eq!(serde_json::to_value(&sub(5_i64, 3_i64)).unwrap()["name"], json!("-"));
    assert_eq!(serde_json::to_value(&mul(2_i64, 4_i64)).unwrap()["name"], json!("*"));
    let d = serde_json::to_value(&div(num(1_i64), num(2_i64))).unwrap();
    assert_eq!(d["name"], json!("/"));
    assert_eq!(d["sort"]["name"], json!("Real"));
    let n = serde_json::to_value(&neg(num(5_i64))).unwrap();
    assert_eq!(n["name"], json!("-"));
    assert_eq!(n["args"].as_array().unwrap().len(), 1);
}

#[test]
fn eq_builds_atomic_equality() {
    let f = eq(num(0_i64), num(0_i64));
    assert_eq!(serde_json::to_value(&f).unwrap(),
        json!({
            "kind": "atomic",
            "predicate": "=",
            "args": [
                {"kind": "const", "value": 0, "sort": {"kind": "primitive", "name": "Int"}},
                {"kind": "const", "value": 0, "sort": {"kind": "primitive", "name": "Int"}}
            ]
        }));
}

#[test]
fn neq_lt_lte_gt_gte_use_unicode_predicates() {
    assert_eq!(serde_json::to_value(&neq(0_i64, 1_i64)).unwrap()["predicate"], json!("\u{2260}"));
    assert_eq!(serde_json::to_value(&lt(0_i64, 1_i64)).unwrap()["predicate"], json!("<"));
    assert_eq!(serde_json::to_value(&lte(0_i64, 1_i64)).unwrap()["predicate"], json!("\u{2264}"));
    assert_eq!(serde_json::to_value(&gt(0_i64, 1_i64)).unwrap()["predicate"], json!(">"));
    assert_eq!(serde_json::to_value(&gte(0_i64, 1_i64)).unwrap()["predicate"], json!("\u{2265}"));
}

#[test]
fn is_true_is_false_build_truthiness_atomics() {
    assert_eq!(serde_json::to_value(&is_true(true)).unwrap()["predicate"], json!("true"));
    assert_eq!(serde_json::to_value(&is_false(false)).unwrap()["predicate"], json!("false"));
}

#[test]
fn connectives_compose() {
    let a = eq(num(0_i64), num(0_i64));
    let b = eq(num(1_i64), num(1_i64));
    assert_eq!(serde_json::to_value(&and(vec![a.clone(), b.clone()])).unwrap()["kind"], json!("and"));
    assert_eq!(serde_json::to_value(&or(vec![a.clone(), b.clone()])).unwrap()["kind"], json!("or"));
    assert_eq!(serde_json::to_value(&not(a.clone())).unwrap()["kind"], json!("not"));
    assert_eq!(serde_json::to_value(&implies(a.clone(), b.clone())).unwrap()["kind"], json!("implies"));
    // iff desugars to and(implies, implies)
    assert_eq!(serde_json::to_value(&iff(a, b)).unwrap()["kind"], json!("and"));
}

#[test]
fn empty_and_or_collapse_to_vacuous_atoms() {
    assert_eq!(serde_json::to_value(&and(vec![])).unwrap()["predicate"], json!("true"));
    assert_eq!(serde_json::to_value(&or(vec![])).unwrap()["predicate"], json!("false"));
}

#[test]
fn singleton_and_or_unwrap() {
    let a = eq(num(0_i64), num(0_i64));
    assert_eq!(serde_json::to_value(&and(vec![a.clone()])).unwrap()["kind"], json!("atomic"));
    assert_eq!(serde_json::to_value(&or(vec![a])).unwrap()["kind"], json!("atomic"));
}

#[test]
fn sorts_module_exposes_primitives() {
    assert_eq!(serde_json::to_value(sorts::int()).unwrap(),
        json!({"kind": "primitive", "name": "Int"}));
    assert_eq!(serde_json::to_value(sorts::bool_()).unwrap()["name"], json!("Bool"));
    assert_eq!(serde_json::to_value(sorts::real()).unwrap()["name"], json!("Real"));
    assert_eq!(serde_json::to_value(sorts::string()).unwrap()["name"], json!("String"));
}
