// SPDX-License-Identifier: Apache-2.0
//
// Integration tests for provekit-lift-kani.
//
// Plan:
// 1. Walk the planted `tests/fixtures/` directory.
// 2. Confirm the kani adapter lifts exactly N contracts and emits the
//    expected number of skip warnings.
// 3. Mint the lifted decls into a `.proof` catalog.
// 4. Re-load through provekit-verifier::load_all_proofs and assert
//    every member envelope passes the trust-root + CID-redrive checks.

use std::path::{Path, PathBuf};

use provekit_lift::{lift_and_mint, lift_path, LiftOptions};

fn fixtures_dir() -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest).join("tests").join("fixtures")
}

#[test]
fn kani_adapter_lifts_expected_counts_from_fixture() {
    let report = lift_path(&fixtures_dir());
    let kani = report
        .adapter_reports
        .iter()
        .find(|a| a.adapter == "kani")
        .expect("kani adapter registered in lift_path");

    // Fixture has 4 fully liftable functions (each with requires +
    // ensures), 1 should_panic-only function (skipped), and 1
    // method-call predicate (parsed function but unliftable).
    assert_eq!(
        kani.lifted, 4,
        "expected 4 lifted kani contracts from fixture, got {} (warnings: {:?})",
        kani.lifted, kani.warnings
    );

    // `seen` counts every function the adapter touched (all 6).
    assert_eq!(
        kani.seen, 6,
        "expected adapter to see 6 kani-annotated functions, got {}",
        kani.seen
    );

    // Skip warnings: should_panic + proof markers + method-call shape.
    assert!(
        kani.warnings
            .iter()
            .any(|w| w.reason.contains("should_panic")),
        "expected a should_panic skip warning"
    );
    assert!(
        kani.warnings
            .iter()
            .any(|w| w.reason.contains("kani::proof")),
        "expected a kani::proof skip warning"
    );
    assert!(
        kani.warnings
            .iter()
            .any(|w| w.reason.contains("not in v0 lift whitelist")
                || w.reason.contains("liftable")),
        "expected a fancy-expression skip warning, got {:?}",
        kani.warnings
    );
}

#[test]
fn kani_lifted_proof_round_trips_through_verifier() {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let out_dir = std::env::temp_dir().join(format!(
        "provekit-lift-kani-it-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&out_dir).unwrap();

    let opts = LiftOptions::default();
    let (_report, minted, path) =
        lift_and_mint(&fixtures_dir(), &out_dir, &opts).expect("lift_and_mint");
    assert!(path.exists(), "proof file written");
    assert!(
        minted.member_count >= 4,
        "expected >=4 kani members, got {}",
        minted.member_count
    );
    assert!(minted.cid.starts_with("blake3-512:"));

    let pool = provekit_verifier::load_all_proofs::run(&out_dir);
    assert!(
        pool.load_errors.is_empty(),
        "verifier load errors: {:?}",
        pool.load_errors
    );
    assert!(
        pool.mementos.len() >= minted.member_count,
        "verifier indexed fewer mementos ({}) than minted ({})",
        pool.mementos.len(),
        minted.member_count
    );
    let _ = std::fs::remove_dir_all(&out_dir);
}
