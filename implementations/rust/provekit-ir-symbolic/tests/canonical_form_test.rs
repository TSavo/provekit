//! Cross-language byte-equivalence test.
//!
//! Hand-derived from the TS reference implementation in
//! `src/ir/symbolic/`. Trace for `forAll(Int, x => gt(x, num(0)))` with
//! a fresh quantifier counter:
//!
//!   1. freshVar(Int) → { kind: "var", name: "_x0", sort: Int }
//!   2. body(x) = gt(x, num(0)) →
//!      { kind: "atomic", predicate: ">", args: [
//!          { kind: "var", name: "_x0", sort: Int },
//!          { kind: "const", value: 0, sort: Int },
//!        ]}
//!   3. forall →
//!      { kind: "forall", sort: Int,
//!        predicate: { kind: "lambda", varName: "_x0", sort: Int,
//!                     body: <atomic above> }}
//!
//! Field order in the TS object literals: { kind, sort, predicate } for
//! forall; { kind, varName, sort, body } for lambda; { kind, predicate,
//! args } for atomic; { kind, name, sort } for var; { kind, value, sort }
//! for const; { kind, name } for primitive sort. Rust struct field order
//! mirrors these one-for-one so `serde_json::to_string_pretty` emits the
//! same byte sequence as TS `JSON.stringify(value, null, 2)`.
//!
//! When the Rust kit gets an LSP/serializer port that's wire-shared with
//! the TS kit, this fixture is the contract.

use provekit_ir_symbolic::canonicalize::{to_canonical_json, to_json_value};
use provekit_ir_symbolic::property::_reset_collector;
use provekit_ir_symbolic::types::sorts;
use provekit_ir_symbolic::*;
use serde_json::{json, Value};

fn reset() { _reset_collector(); }

/// The expected JSON shape for `forAll(Int, x => gt(x, num(0)))` with the
/// quantifier counter freshly reset. The exact field order is the
/// load-bearing assertion.
const EXPECTED_PRETTY: &str = r#"{
  "kind": "forall",
  "sort": {
    "kind": "primitive",
    "name": "Int"
  },
  "predicate": {
    "kind": "lambda",
    "varName": "_x0",
    "sort": {
      "kind": "primitive",
      "name": "Int"
    },
    "body": {
      "kind": "atomic",
      "predicate": ">",
      "args": [
        {
          "kind": "var",
          "name": "_x0",
          "sort": {
            "kind": "primitive",
            "name": "Int"
          }
        },
        {
          "kind": "const",
          "value": 0,
          "sort": {
            "kind": "primitive",
            "name": "Int"
          }
        }
      ]
    }
  }
}"#;

#[test]
fn forall_int_gt_zero_matches_canonical_pretty_json() {
    reset();
    let f = forall!(x: sorts::int() => gt(x, num(0_i64)));
    let actual = to_canonical_json(&f).unwrap();
    assert_eq!(actual, EXPECTED_PRETTY,
        "Rust IR pretty-JSON diverged from the TS-equivalent fixture.\n\
         Field order in the IrFormula struct/enum may have drifted.");
}

#[test]
fn forall_int_gt_zero_matches_canonical_value() {
    reset();
    let f = forall!(x: sorts::int() => gt(x, num(0_i64)));
    let actual = to_json_value(&f).unwrap();
    let expected: Value = serde_json::from_str(EXPECTED_PRETTY).unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn quantifier_counter_increments_then_resets() {
    reset();
    let _ = forall!(x: sorts::int() => gt(x, num(0_i64)));
    let f2 = forall!(y: sorts::int() => gt(y, num(0_i64)));
    let v2 = to_json_value(&f2).unwrap();
    assert_eq!(v2["predicate"]["varName"], json!("_x1"),
        "second quantifier should bind to _x1 without a counter reset");

    reset();
    let f3 = forall!(z: sorts::int() => gt(z, num(0_i64)));
    let v3 = to_json_value(&f3).unwrap();
    assert_eq!(v3["predicate"]["varName"], json!("_x0"),
        "after reset, counter restarts at _x0");
}

#[test]
fn nested_quantifiers_get_distinct_var_names() {
    reset();
    let f = forall!(x: sorts::int() =>
        forall!(y: sorts::int() => gt(add(x.clone(), y), num(0_i64))));
    let v = to_json_value(&f).unwrap();
    assert_eq!(v["predicate"]["varName"], json!("_x0"));
    assert_eq!(v["predicate"]["body"]["predicate"]["varName"], json!("_x1"));
}

#[test]
fn empty_and_serializes_as_atomic_true() {
    let f = and(vec![]);
    let v = to_json_value(&f).unwrap();
    assert_eq!(v, json!({"kind": "atomic", "predicate": "true", "args": []}));
}

#[test]
fn empty_or_serializes_as_atomic_false() {
    let f = or(vec![]);
    let v = to_json_value(&f).unwrap();
    assert_eq!(v, json!({"kind": "atomic", "predicate": "false", "args": []}));
}

#[test]
fn iff_desugars_to_and_of_implies_pair() {
    let a = eq(num(0_i64), num(0_i64));
    let b = eq(num(1_i64), num(1_i64));
    let f = iff(a, b);
    let v = to_json_value(&f).unwrap();
    assert_eq!(v["kind"], json!("and"));
    let cs = v["conjuncts"].as_array().unwrap();
    assert_eq!(cs.len(), 2);
    assert_eq!(cs[0]["kind"], json!("implies"));
    assert_eq!(cs[1]["kind"], json!("implies"));
}

#[test]
fn implies_field_order_is_kind_antecedent_consequent() {
    let f = implies(eq(num(0_i64), num(0_i64)), eq(num(1_i64), num(1_i64)));
    let pretty = to_canonical_json(&f).unwrap();
    let kind_pos = pretty.find("\"kind\"").unwrap();
    let ante_pos = pretty.find("\"antecedent\"").unwrap();
    let cons_pos = pretty.find("\"consequent\"").unwrap();
    assert!(kind_pos < ante_pos && ante_pos < cons_pos,
        "implies field order must be kind → antecedent → consequent");
}
