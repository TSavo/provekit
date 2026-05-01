// SPDX-License-Identifier: Apache-2.0
//
// Smoke test: drive #[provekit::contract] on a couple of toy
// functions, then introspect the generated ContractDecl values
// through the inventory distributed slice. This test ALSO exercises
// `provekit_macros_rt::collect_all_contracts`, the entry point the
// self-contracts orchestrator will call.
//
// We assert:
//   1. Both contracts show up in `registered_contracts()`.
//   2. `name` defaults to the function ident.
//   3. The pre / post / inv slots populate exactly as written.
//   4. `out_binding` defaults to "out" but can be overridden.
//   5. Source-location metadata is populated (file/line non-zero).

#![allow(dead_code)]

use provekit_ir_symbolic::{eq, forall, gt, num, Int, String_};
use provekit_macros::contract;
use provekit_macros_rt::{collect_all_contracts, registered_contracts};

#[contract(pre = forall(Int(), |n| gt(n, num(0))))]
fn parse_positive_int(_s: &str) -> i64 {
    42
}

#[contract(
    post = forall(String_(), |s| eq(s.clone(), s)),
    out_binding = "result"
)]
fn echo_string(_s: &str) -> String {
    String::new()
}

#[contract(
    name = "explicit_override_name",
    inv = forall(Int(), |n| gt(n, num(-1))),
    out_binding = "y"
)]
fn loop_body() -> i64 {
    0
}

#[test]
fn registry_contains_all_three_decorated_functions() {
    let names: Vec<&'static str> = registered_contracts().map(|r| r.name).collect();
    assert!(
        names.contains(&"parse_positive_int"),
        "missing parse_positive_int in {names:?}"
    );
    assert!(names.contains(&"echo_string"), "missing echo_string in {names:?}");
    assert!(names.contains(&"loop_body"), "missing loop_body in {names:?}");
    assert!(names.len() >= 3, "expected at least 3 entries, got {}", names.len());
}

#[test]
fn parse_positive_int_pre_is_populated_and_post_inv_are_none() {
    let r = registered_contracts()
        .find(|r| r.name == "parse_positive_int")
        .expect("registration not found");
    let d = (r.builder)();
    assert_eq!(d.name, "parse_positive_int");
    assert!(d.pre.is_some(), "pre slot must be populated");
    assert!(d.post.is_none(), "post slot was not authored");
    assert!(d.inv.is_none(), "inv slot was not authored");
    assert_eq!(d.out_binding, "out", "out_binding default must be \"out\"");
}

#[test]
fn echo_string_post_populated_with_overridden_out_binding() {
    let r = registered_contracts()
        .find(|r| r.name == "echo_string")
        .expect("registration not found");
    let d = (r.builder)();
    assert!(d.pre.is_none());
    assert!(d.post.is_some());
    assert!(d.inv.is_none());
    assert_eq!(d.out_binding, "result");
}

#[test]
fn name_arg_overrides_fn_ident_in_contract_decl() {
    // The inventory's `name` field is the fn ident (so the orchestrator
    // can correlate macros and source locations); the *ContractDecl*'s
    // `name` is what `name = ...` overrides. Find by source name.
    let r = registered_contracts()
        .find(|r| r.name == "loop_body")
        .expect("registration not found");
    let d = (r.builder)();
    assert_eq!(
        d.name, "explicit_override_name",
        "the `name = \"...\"` arg must override the ContractDecl name"
    );
    assert!(d.inv.is_some());
    assert_eq!(d.out_binding, "y");
}

#[test]
fn source_location_metadata_is_populated() {
    let r = registered_contracts()
        .find(|r| r.name == "parse_positive_int")
        .expect("registration not found");
    assert!(
        r.source_path.contains("smoke.rs"),
        "source_path should reference this test file, got {}",
        r.source_path
    );
    assert!(r.source_line > 0, "source_line must be non-zero");
}

#[test]
fn collect_all_contracts_returns_sorted_decls() {
    let decls = collect_all_contracts();
    assert!(decls.len() >= 3, "expected at least 3 collected contracts");
    let mut sorted_names: Vec<String> = decls.iter().map(|d| d.name.clone()).collect();
    let prior = sorted_names.clone();
    sorted_names.sort();
    assert_eq!(prior, sorted_names, "collect_all_contracts must return name-sorted output");
    // Verify the toy decorations appear under expected names.
    let found_names: std::collections::HashSet<&str> =
        decls.iter().map(|d| d.name.as_str()).collect();
    assert!(found_names.contains("parse_positive_int"));
    assert!(found_names.contains("echo_string"));
    assert!(found_names.contains("explicit_override_name"));
}
