// SPDX-License-Identifier: Apache-2.0
//
// Integration test: end-to-end fixture lift -> mint -> .proof file ->
// verifier load. Asserts:
//   - 4 simple `assert_eq!` callsites lifted
//   - 1 no-call `assert!` with binop skips honestly
//   - 1 deliberately-skipped `format!()` macro operand produces a warning
//     (v0.5 widened the operand whitelist to include method calls; the
//      negative shape moved to format!-style operand-position macros)
//   - The resulting `<cid>.proof` file round-trips through the
//     provekit-verifier's load_all_proofs stage with no load errors and
//     the expected member count.

use std::io::Write;
use std::path::Path;

use provekit_lift::{lift_and_mint, LiftOptions};

fn write_fixture(dir: &Path) {
    let p = dir.join("fixture_tests.rs");
    let mut f = std::fs::File::create(&p).unwrap();
    writeln!(
        f,
        r#"
// 4 simple liftable assert_eq! tests.
#[test]
fn parse_int_42() {{
    assert_eq!(parse_int("42"), 42);
}}

#[test]
fn parse_int_zero() {{
    assert_eq!(parse_int("0"), 0);
}}

#[test]
fn add_one_one() {{
    assert_eq!(add_one(1), 2);
}}

#[test]
fn neg_one_negates() {{
    assert_eq!(neg(1), -1);
}}

// 1 assert! with no callsite.
#[test]
fn count_positive() {{
    assert!(count > 0);
}}

// 1 deliberately-skipped: format! macro in operand position is not in
// the v0.5 whitelist (only `vec![...]` is).
#[test]
fn skipped_format_macro() {{
    assert_eq!(s, format!("{{}}", x));
}}
"#
    )
    .unwrap();
}

#[test]
fn fixture_lifts_4_skips_2_and_round_trips_through_verifier() {
    // Per-test temp dir.
    let base = std::env::temp_dir();
    use std::sync::atomic::{AtomicU64, Ordering};
    static CTR: AtomicU64 = AtomicU64::new(0);
    let n = CTR.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let td = base.join(format!(
        "provekit-lift-rust-tests-it-{}-{}-{}",
        std::process::id(),
        nanos,
        n
    ));
    std::fs::create_dir_all(&td).unwrap();

    write_fixture(&td);

    let opts = LiftOptions::default();
    let (report, minted, proof_path) =
        lift_and_mint(&td, &td, &opts).expect("lift_and_mint should succeed");

    // Find the rust-tests adapter report.
    let rt = report
        .adapter_reports
        .iter()
        .find(|a| a.adapter == "rust-tests")
        .expect("adapter `rust-tests` should appear in the report");

    assert_eq!(
        rt.seen, 6,
        "expected 6 assertion candidates seen across the fixture"
    );
    assert_eq!(
        rt.lifted, 4,
        "expected 4 lifted callsite assertions, warnings: {:?}",
        rt.warnings
    );
    assert_eq!(
        rt.warnings.len(),
        2,
        "expected exactly 2 honest skips (no callsite + format! macro)"
    );
    assert!(rt
        .warnings
        .iter()
        .any(|w| w.item_name == "count_positive" && w.reason.contains("callsite")));
    assert!(rt
        .warnings
        .iter()
        .any(|w| w.item_name == "skipped_format_macro" && w.reason.contains("format")));

    // .proof filename = `<cid>.proof`.
    assert!(proof_path.exists(), "proof file should exist on disk");
    let fname = proof_path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let expected = format!("{}.proof", minted.cid);
    assert_eq!(fname, expected, "filename should be <cid>.proof");
    assert!(
        minted.cid.starts_with("blake3-512:"),
        "CID must use blake3-512 prefix, got {}",
        minted.cid
    );

    // Round-trip: load the .proof through the verifier's stage 1.
    let pool = provekit_verifier::load_all_proofs::run(&td);
    assert!(
        pool.load_errors.is_empty(),
        "verifier should have zero load errors, got {:?}",
        pool.load_errors
    );
    // Each lifted callsite assertion is a unique-named contract -> one member each.
    assert_eq!(
        minted.member_count, 4,
        "expected 4 distinct mementos (one per lifted callsite assertion)"
    );

    // REGRESSION (PR-22, #1440): the harvested proof alone yields ZERO
    // callsites, because `enumerate_callsites` only emits a callsite when a
    // harvested call ctor matches a BRIDGE `sourceSymbol`. Before this PR
    // the round-trip test never asserted enumeration, so the
    // bridge-is-required invariant was untested. Assert it both ways: empty
    // before bind, non-empty after.
    assert!(
        provekit_verifier::enumerate_callsites::run(&pool).is_empty(),
        "harvested proof with no bridge must enumerate zero callsites"
    );

    // After bind: a `bind_function_bridge` for `parse_int` makes the
    // harvested `parse_int(\"42\")` call enumerate as a discharge callsite.
    let mut pool = pool;
    let item_fn: syn::ItemFn =
        syn::parse_str("fn parse_int(s: &str) -> i64 { 42 }").expect("parse parse_int fixture");
    let contract = provekit_walk::contract::build_function_contract(&item_fn, None);
    let members =
        libprovekit::core::bind::bind_function_bridge(&contract, "rust", "rust-kit", None)
            .expect("bind_function_bridge");
    pool.mementos.insert(
        members.op_contract.cid.as_str().to_string(),
        members.op_contract.envelope.clone(),
    );
    pool.mementos.insert(
        members.bridge.cid.as_str().to_string(),
        members.bridge.envelope.clone(),
    );
    pool.bridges_by_symbol
        .insert("parse_int".to_string(), members.bridge.envelope.clone());

    let callsites = provekit_verifier::enumerate_callsites::run(&pool);
    assert!(
        !callsites.is_empty(),
        "after harvest+bind, enumerate_callsites must be non-empty (the bridge wires the harvested call to a dischargeable target)"
    );
    assert!(
        callsites.iter().any(|cs| cs.bridge_ir_name == "parse_int"),
        "the parse_int callsite must enumerate; got {:?}",
        callsites.iter().map(|c| &c.bridge_ir_name).collect::<Vec<_>>()
    );

    println!("FIXTURE_PROOF_CID={}", minted.cid);
    let _ = std::fs::remove_dir_all(&td);
}
