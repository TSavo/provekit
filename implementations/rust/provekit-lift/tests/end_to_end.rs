// SPDX-License-Identifier: Apache-2.0
//
// End-to-end integration tests for provekit-lift.
//
// 1. Walk the planted `tests/fixtures/` directory.
// 2. Lift adapters mint per-shape ContractDecls.
// 3. Bundle into a `.proof` catalog.
// 4. Re-load through provekit-verifier::load_all_proofs and assert
//    every member envelope passes the trust-root + CID-redrive checks.

use std::path::{Path, PathBuf};

use provekit_lift::{lift_and_mint, lift_path, mint_proof, LiftOptions};

fn fixtures_dir() -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest).join("tests").join("fixtures")
}

#[test]
fn lifts_proptest_and_contracts_from_fixtures() {
    let report = lift_path(&fixtures_dir());
    let proptest = report
        .adapter_reports
        .iter()
        .find(|a| a.adapter == "proptest")
        .unwrap();
    let contracts = report
        .adapter_reports
        .iter()
        .find(|a| a.adapter == "contracts")
        .unwrap();
    assert!(
        proptest.lifted >= 5,
        "expected >=5 proptest lifts, got {} ({} seen, {} warnings)",
        proptest.lifted,
        proptest.seen,
        proptest.warnings.len()
    );
    assert!(
        contracts.lifted >= 3,
        "expected >=3 contracts lifts, got {}",
        contracts.lifted
    );
}

#[test]
fn lifts_prusti_creusot_flux_quickcheck_verus_from_fixtures() {
    let report = lift_path(&fixtures_dir());

    let find = |name: &str| {
        report
            .adapter_reports
            .iter()
            .find(|a| a.adapter == name)
            .unwrap_or_else(|| panic!("missing adapter report: {name}"))
    };

    let prusti = find("prusti");
    let creusot = find("creusot");
    let flux = find("flux");
    let quickcheck = find("quickcheck");
    let verus = find("verus");

    assert!(
        prusti.lifted >= 3,
        "expected >=3 prusti lifts, got {} ({} seen, {} warnings)",
        prusti.lifted,
        prusti.seen,
        prusti.warnings.len()
    );
    assert!(
        creusot.lifted >= 3,
        "expected >=3 creusot lifts, got {}",
        creusot.lifted
    );
    assert!(
        flux.lifted >= 3,
        "expected >=3 flux lifts, got {}",
        flux.lifted
    );
    assert!(
        quickcheck.lifted >= 3,
        "expected >=3 quickcheck lifts, got {}",
        quickcheck.lifted
    );

    // verus v0 is "skip everything with structured warning". Lift count
    // is 0 by design; we instead assert non-empty warnings.
    assert_eq!(verus.lifted, 0, "verus v0 must not lift anything");
    assert!(
        !verus.warnings.is_empty(),
        "verus v0 must emit at least one structured warning"
    );

    // Each non-verus adapter has exactly one deliberately-skipped
    // pattern in its fixture. That gives at least one warning each.
    assert!(
        !prusti.warnings.is_empty(),
        "prusti fixture has a #[prusti::predicate] item that should warn"
    );
    assert!(
        !creusot.warnings.is_empty(),
        "creusot fixture has a #[creusot::law] item that should warn"
    );
    assert!(
        !flux.warnings.is_empty(),
        "flux fixture has a #[flux::trusted] item that should warn"
    );
    assert!(
        !quickcheck.warnings.is_empty(),
        "quickcheck fixture has a TestResult-returning property that should warn"
    );
}

#[test]
fn proof_cid_is_deterministic_across_runs() {
    // Run lift_and_mint twice over the same fixture tree and assert the
    // resulting catalog CID is identical. This pins content-addressed
    // determinism: same inputs, same canonical IR, same blake3-512 CID.
    let nanos1 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir1 = std::env::temp_dir()
        .join(format!("provekit-lift-det1-{}-{nanos1}", std::process::id()));
    let dir2 = std::env::temp_dir()
        .join(format!("provekit-lift-det2-{}-{nanos1}", std::process::id()));
    let opts = LiftOptions::default();
    let (_r1, m1, _p1) = lift_and_mint(&fixtures_dir(), &dir1, &opts).expect("first run");
    let (_r2, m2, _p2) = lift_and_mint(&fixtures_dir(), &dir2, &opts).expect("second run");
    assert_eq!(
        m1.cid, m2.cid,
        "lift catalog CID must be deterministic across runs"
    );
    assert_eq!(
        m1.member_count, m2.member_count,
        "member count must be stable across runs"
    );
    let _ = std::fs::remove_dir_all(&dir1);
    let _ = std::fs::remove_dir_all(&dir2);
}

#[test]
fn lifted_proof_loads_through_verifier() {
    // Use a tempdir keyed off the test name + nanos so parallel tests
    // don't collide.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let out_dir = std::env::temp_dir().join(format!("provekit-lift-it-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&out_dir).unwrap();
    let opts = LiftOptions::default();
    let (_report, minted, path) =
        lift_and_mint(&fixtures_dir(), &out_dir, &opts).expect("lift_and_mint");
    assert!(path.exists(), "proof file written");
    assert!(minted.member_count >= 8);

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

#[test]
fn dedup_collapses_identical_ir_across_files() {
    // Build two declarations with identical IR via the proptest adapter.
    let src = r#"
        proptest! {
            #[test]
            fn p_equal_42(x: i64) {
                prop_assert_eq!(x, 42);
            }
        }
    "#;
    let f = syn::parse_file(src).unwrap();
    let a = provekit_lift::adapter_proptest::lift_file(&f, "a.rs");
    let b = provekit_lift::adapter_proptest::lift_file(&f, "b.rs");
    let mut decls = a.decls;
    // Same name same IR — should dedup at mint.
    decls.extend(b.decls);
    assert_eq!(decls.len(), 2);

    let opts = LiftOptions::default();
    let minted = mint_proof(&decls, &opts).expect("mint");
    assert_eq!(
        minted.member_count, 1,
        "two identical-IR contracts should collapse to one minted member"
    );
    assert_eq!(minted.deduplicated, 1);
}

#[test]
fn name_collision_on_different_ir_fails_loud() {
    let a_src = r#"
        proptest! {
            #[test]
            fn p_eq(x: i64) {
                prop_assert_eq!(x, 42);
            }
        }
    "#;
    let b_src = r#"
        proptest! {
            #[test]
            fn p_eq(x: i64) {
                prop_assert_eq!(x, 99);
            }
        }
    "#;
    let af = syn::parse_file(a_src).unwrap();
    let bf = syn::parse_file(b_src).unwrap();
    let mut decls = provekit_lift::adapter_proptest::lift_file(&af, "a.rs").decls;
    decls.extend(provekit_lift::adapter_proptest::lift_file(&bf, "b.rs").decls);
    let opts = LiftOptions::default();
    let r = mint_proof(&decls, &opts);
    match r {
        Err(provekit_lift::LiftMintError::NameCollisionDifferentIr(name)) => {
            assert_eq!(name, "p_eq");
        }
        other => panic!("expected NameCollisionDifferentIr, got {other:?}"),
    }
}

#[test]
fn cli_runs_against_fixtures() {
    // Smoke-test the run_cli entry by pointing it at the fixtures dir.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let out = std::env::temp_dir().join(format!("provekit-lift-cli-{}-{nanos}", std::process::id()));
    let flags = provekit_lift::CliFlags {
        workspace: Some(fixtures_dir()),
        target_dir: Some(out.clone()),
        quiet: true,
        rpc: false,
    };
    let code = provekit_lift::run_cli(flags);
    assert_eq!(code, 0);
    let entries: Vec<_> = std::fs::read_dir(&out)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "proof").unwrap_or(false))
        .collect();
    assert!(!entries.is_empty(), "expected at least one .proof file in {out:?}");
    let _ = std::fs::remove_dir_all(&out);
}
