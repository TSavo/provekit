//! Mirror of the collector / describe / must / bridge tests in
//! `src/ir/symbolic/symbolic.test.ts`.

use provekit_ir_symbolic::property::{
    _reset_collector, begin_collecting, BridgeSpec, Declaration,
};
use provekit_ir_symbolic::types::sorts;
use provekit_ir_symbolic::*;
use provekit_ir_symbolic::{describe as describe_macro, exists, forall, must as must_macro, bridge as bridge_macro};

fn reset() { _reset_collector(); }

#[test]
fn property_collects_declaration() {
    reset();
    let handle = begin_collecting();
    property::property("zeroIsZero", eq(parse_int(str_("0")), num(0_i64)));
    let decls = handle.finish();
    assert_eq!(decls.len(), 1);
    match &decls[0] {
        Declaration::Property { name, formula } => {
            assert_eq!(name, "zeroIsZero");
            assert_eq!(serde_json::to_value(formula).unwrap()["kind"], serde_json::json!("atomic"));
        }
        _ => panic!("expected property declaration"),
    }
}

#[test]
fn bridge_collects_declaration() {
    reset();
    let handle = begin_collecting();
    bridge_macro!("parseIntBridgesV8", BridgeSpec {
        source_symbol: "global.parseInt".into(),
        source_layer: "ts-kit@1.0".into(),
        target_contract_cid: "abc1234567890def".into(),
        target_layer: "V8@12.4".into(),
        notes: Some("the canonical bridge".into()),
    });
    let decls = handle.finish();
    assert_eq!(decls.len(), 1);
    match &decls[0] {
        Declaration::Bridge { name, source_symbol, target_contract_cid, notes, .. } => {
            assert_eq!(name, "parseIntBridgesV8");
            assert_eq!(source_symbol, "global.parseInt");
            assert_eq!(target_contract_cid, "abc1234567890def");
            assert_eq!(notes.as_deref(), Some("the canonical bridge"));
        }
        _ => panic!("expected bridge"),
    }
}

#[test]
fn multiple_property_and_bridge_calls_collect_in_order() {
    reset();
    let handle = begin_collecting();
    property::property("p1", eq(num(0_i64), num(0_i64)));
    bridge_macro!("b1", BridgeSpec {
        source_symbol: "x".into(),
        source_layer: "L1".into(),
        target_contract_cid: "0".repeat(32),
        target_layer: "L2".into(),
        notes: None,
    });
    property::property("p2", eq(num(1_i64), num(1_i64)));
    let decls = handle.finish();
    let kinds: Vec<&str> = decls.iter().map(|d| match d {
        Declaration::Property { .. } => "property",
        Declaration::Bridge { .. } => "bridge",
    }).collect();
    assert_eq!(kinds, vec!["property", "bridge", "property"]);
    let names: Vec<&str> = decls.iter().map(|d| d.name()).collect();
    assert_eq!(names, vec!["p1", "b1", "p2"]);
}

#[test]
#[should_panic(expected = "outside an active collector")]
fn calling_property_without_active_collector_panics() {
    reset();
    property::property("x", eq(num(0_i64), num(0_i64)));
}

#[test]
#[should_panic(expected = "already active")]
fn nested_begin_collecting_panics() {
    reset();
    let _h = begin_collecting();
    let _h2 = begin_collecting();
}

#[test]
fn describe_must_registers_full_path_name() {
    reset();
    let handle = begin_collecting();
    describe_macro!("parseInt", {
        must_macro!("canReturnZero",
            exists!(s: sorts::string() => eq(parse_int(s), num(0_i64))));
    });
    let decls = handle.finish();
    assert_eq!(decls.len(), 1);
    assert_eq!(decls[0].name(), "parseInt > canReturnZero");
}

#[test]
fn nested_describes_build_path() {
    reset();
    let handle = begin_collecting();
    describe_macro!("Math", {
        describe_macro!("abs", {
            must_macro!("non-negative",
                forall!(x: sorts::int() => gt(abs(x), num(-1_i64))));
        });
    });
    let decls = handle.finish();
    assert_eq!(decls.len(), 1);
    assert_eq!(decls[0].name(), "Math > abs > non-negative");
}

#[test]
fn describe_pops_segment_after_body() {
    reset();
    let handle = begin_collecting();
    describe_macro!("a", {
        must_macro!("inner", eq(num(0_i64), num(0_i64)));
    });
    must_macro!("outer", eq(num(1_i64), num(1_i64)));
    let decls = handle.finish();
    let names: Vec<&str> = decls.iter().map(|d| d.name()).collect();
    assert_eq!(names, vec!["a > inner", "outer"]);
}

#[test]
fn must_skip_is_noop() {
    reset();
    let handle = begin_collecting();
    describe_macro!("parseInt", {
        property::must_skip("legacy invariant", eq(num(0_i64), num(0_i64)));
        must_macro!("real invariant", eq(num(1_i64), num(1_i64)));
    });
    let decls = handle.finish();
    assert_eq!(decls.len(), 1);
    assert_eq!(decls[0].name(), "parseInt > real invariant");
}

#[test]
fn describe_skip_does_not_run_body() {
    reset();
    let handle = begin_collecting();
    let mut body_ran = 0_i32;
    let body_ran_ref = &mut body_ran;
    property::describe_skip("never", || {
        *body_ran_ref += 1;
    });
    let decls = handle.finish();
    assert_eq!(decls.len(), 0);
    assert_eq!(body_ran, 0);
}

#[test]
fn forall_wraps_body_builder() {
    reset();
    let f = forall!(x: sorts::int() => gt(x, num(0_i64)));
    let v = serde_json::to_value(&f).unwrap();
    assert_eq!(v["kind"], serde_json::json!("forall"));
    assert_eq!(v["sort"]["name"], serde_json::json!("Int"));
    assert_eq!(v["predicate"]["body"]["kind"], serde_json::json!("atomic"));
}

#[test]
fn exists_wraps_body_builder() {
    reset();
    let f = exists!(s: sorts::string() => eq(parse_int(s), num(0_i64)));
    assert_eq!(serde_json::to_value(&f).unwrap()["kind"], serde_json::json!("exists"));
}

#[test]
fn nested_quantifiers_compose() {
    reset();
    let f = forall!(x: sorts::int() =>
        forall!(y: sorts::int() => gt(add(x.clone(), y), num(0_i64))));
    assert_eq!(serde_json::to_value(&f).unwrap()["kind"], serde_json::json!("forall"));
}

#[test]
fn reset_collector_lets_new_begin_collecting_succeed_after_leak() {
    reset();
    let _leaked = begin_collecting();
    std::mem::forget(_leaked); // simulate the panic-leak path: handle dropped without finishing
    reset();
    let handle = begin_collecting();
    property::property("ok", eq(num(0_i64), num(0_i64)));
    let decls = handle.finish();
    assert_eq!(decls.len(), 1);
}
