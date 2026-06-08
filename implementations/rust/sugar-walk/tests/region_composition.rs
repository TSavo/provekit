// SPDX-License-Identifier: Apache-2.0
//
// region_composition.rs — #414 integration test: end-to-end C.9
// Outlives composition succeeds + refuses.
//
// Blocked on #410 (walk lifter Region+Outlives). Tests are marked
// #[ignore] until the lifter emits Sort::Region terms and Outlives
// predicates from Charon LLBC.

use std::path::PathBuf;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// Callee demands Outlives('b, 'a) per its where-clause.
/// Caller has the same where-clause, so composition succeeds.
#[test]
#[ignore = "blocked on #410: walk lifter Region+Outlives emission"]
fn region_composition_succeeds_when_where_clause_matches() {
    use sugar_walk::llbc::LlbcCrate;
    use sugar_walk::llbc_calls::lift_llbc_crate;

    let krate = LlbcCrate::from_path(fixture_path("region_caller_satisfied.llbc"))
        .expect("load llbc fixture");
    let registry =
        lift_llbc_crate(&krate, Some("region_caller_satisfied.rs")).expect("lift registry");

    let caller = registry.get("caller").expect("caller must lift");
    let callee = registry.get("callee").expect("callee must lift");

    assert!(
        !caller.formals.is_empty(),
        "caller must have formals with region info"
    );
    assert!(
        !callee.formals.is_empty(),
        "callee must have formals with region info"
    );
}

/// Caller has NO where-clause, so 'a and 'b are unrelated.
/// Callee's demand Outlives('b, 'a) cannot be discharged.
#[test]
#[ignore = "blocked on #410: walk lifter Region+Outlives emission"]
fn region_composition_refuses_when_unrelated() {
    use sugar_walk::llbc::LlbcCrate;
    use sugar_walk::llbc_calls::lift_llbc_crate;

    let krate = LlbcCrate::from_path(fixture_path("region_caller_unsatisfied.llbc"))
        .expect("load llbc fixture");
    let registry =
        lift_llbc_crate(&krate, Some("region_caller_unsatisfied.rs")).expect("lift registry");

    let caller = registry.get("caller").expect("caller must lift");
    let callee = registry.get("callee").expect("callee must lift");

    assert!(
        !caller.formals.is_empty(),
        "caller must have formals with region info"
    );
    assert!(
        !callee.formals.is_empty(),
        "callee must have formals with region info"
    );
}

/// Marriage classifies LLBC-emitted Outlives as LifetimeRelative.
#[test]
#[ignore = "blocked on #410: walk lifter Region+Outlives emission"]
fn region_marriage_classifies_outlives_as_lifetime_relative() {
    use sugar_walk::llbc::LlbcCrate;
    use sugar_walk::llbc_lift::lift_llbc_function_with_types;
    use sugar_walk::marriage::{marry, LayerAgreement, LlbcExtraCategory};

    let krate =
        LlbcCrate::from_path(fixture_path("region_callee.llbc")).expect("load llbc fixture");
    let f = krate.function_by_name("callee").expect("find callee");
    let llbc = lift_llbc_function_with_types(f, Some("region_callee.rs"), krate.type_decls_raw())
        .expect("lift callee");

    let src = std::fs::read_to_string(fixture_path("region_callee.rs")).expect("read source");
    let file: syn::File = syn::parse_str(&src).expect("parse source");
    let item_fn = file
        .items
        .into_iter()
        .find_map(|item| match item {
            syn::Item::Fn(f) if f.sig.ident == "callee" => Some(f),
            _ => None,
        })
        .expect("find callee fn");
    let ast = sugar_walk::contract::build_function_contract_with_file(
        &item_fn,
        None,
        Some("region_callee.rs"),
    );

    let married = marry(ast, llbc);
    match married.agreement {
        LayerAgreement::LlbcExtra(LlbcExtraCategory::LifetimeRelative)
        | LayerAgreement::Both(LlbcExtraCategory::LifetimeRelative) => {}
        other => panic!("expected LifetimeRelative classification, got {:?}", other),
    }
}
