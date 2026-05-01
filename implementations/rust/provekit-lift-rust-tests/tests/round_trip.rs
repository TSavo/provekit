// SPDX-License-Identifier: Apache-2.0
//
// Integration test: end-to-end fixture lift -> mint -> .proof file ->
// verifier load. Asserts:
//   - 4 simple `assert_eq!` lifted
//   - 1 `assert!` with binop lifted
//   - 1 deliberately-skipped method-call assertion produces a warning
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

// 1 assert! with a binop body.
#[test]
fn count_positive() {{
    assert!(count > 0);
}}

// 1 deliberately-skipped: method call, not in v0 whitelist.
#[test]
fn skipped_method_call() {{
    assert_eq!("hello".len(), 5);
}}
"#
    )
    .unwrap();
}

#[test]
fn fixture_lifts_5_skips_1_and_round_trips_through_verifier() {
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
        rt.lifted, 5,
        "expected 5 lifted (4 assert_eq! + 1 assert!), warnings: {:?}",
        rt.warnings
    );
    assert_eq!(
        rt.warnings.len(),
        1,
        "expected exactly 1 honest skip (the method-call assertion)"
    );
    assert!(rt.warnings[0].item_name.starts_with("skipped_method_call"));

    // .proof filename = `<cid>.proof`.
    assert!(proof_path.exists(), "proof file should exist on disk");
    let fname = proof_path.file_name().unwrap().to_string_lossy().to_string();
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
    // Each lifted assert is a unique-named contract -> one member each.
    assert_eq!(
        minted.member_count, 5,
        "expected 5 distinct mementos (one per lifted assertion)"
    );

    println!("FIXTURE_PROOF_CID={}", minted.cid);
    let _ = std::fs::remove_dir_all(&td);
}
