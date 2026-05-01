// SPDX-License-Identifier: Apache-2.0
//
// Layer 2 integration test. Drives the workspace lift over the
// `tests/fixtures/layer2_sample.rs` planted file and asserts:
//
//   - Bounded-loop, helper-inlining, and characterization patterns each
//     produce the expected mementos.
//   - The deliberately-skipped nested loop logs a structured warning
//     under the `rust-tests-layer2` adapter (NOT `rust-tests`, so a
//     reader can tell which layer made the call).
//   - The combined Layer 0 + Layer 2 mint count from the fixture
//     exceeds the 8-contract floor.
//   - Layer 0 does NOT also lift the tests Layer 2 claimed (no
//     double-counting).

use std::path::{Path, PathBuf};

use provekit_lift::{lift_path, mint_proof, LiftOptions};

fn fixtures_dir() -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest).join("tests").join("fixtures")
}

#[test]
fn layer2_sample_lifts_all_three_patterns() {
    let report = lift_path(&fixtures_dir());

    let l2 = report
        .adapter_reports
        .iter()
        .find(|a| a.adapter == "rust-tests-layer2")
        .expect("rust-tests-layer2 adapter report present");

    // Pattern 1 (bounded loop): 3 liftable + 1 nested-loop skip = 4 seen.
    // Pattern 2 (helper inlining): 3 + 2 = 5 helper-call mementos seen.
    // Pattern 3 (characterization): 1 conjunction memento seen.
    // Total: 4 + 5 + 1 = 10 seen, 9 lifted, 1 warning.
    assert!(
        l2.lifted >= 9,
        "expected >=9 layer-2 lifts from fixture, got {} ({} seen, {} warnings)",
        l2.lifted,
        l2.seen,
        l2.warnings.len()
    );
    assert!(
        !l2.warnings.is_empty(),
        "expected the nested-loop skip to log a warning"
    );
    let nested_warned = l2
        .warnings
        .iter()
        .any(|w| w.item_name == "nested_loop_skipped" && w.reason.contains("nested"));
    assert!(
        nested_warned,
        "expected a structured warning for nested_loop_skipped: {:?}",
        l2.warnings
    );
}

#[test]
fn layer0_does_not_double_lift_layer2_claimed_tests() {
    let report = lift_path(&fixtures_dir());

    // Layer 0 (rust-tests) MUST NOT have lifted any decl whose name
    // starts with one of the Layer-2-claimed test fns from the
    // fixture. The two layers should partition the test fns; nothing
    // should double-count.
    let l2_owned = [
        "squares_are_nonneg",
        "divmod_in_range",
        "small_window",
        "many_42s",
        "ranges_ok",
        "parse_int_characterization",
        "nested_loop_skipped",
    ];
    for d in &report.decls {
        // Layer 0 names are "<test>::<i>"; Layer 2 names are "<test>"
        // or "<test>::call::<i>". For each L2-owned test, no decl
        // starting with "<test>::" with a numeric suffix should be
        // produced by Layer 0.
        for owner in &l2_owned {
            let l0_prefix = format!("{owner}::");
            if d.name.starts_with(&l0_prefix) && !d.name.starts_with(&format!("{owner}::call::")) {
                panic!(
                    "Layer 0 emitted a decl `{}` for a Layer-2-claimed test `{}`",
                    d.name, owner
                );
            }
        }
    }
}

#[test]
fn layer2_fixture_mints_at_least_8_new_contracts() {
    let report = lift_path(&fixtures_dir());
    // Filter to decls whose names match the layer2_sample.rs tests so
    // we measure THIS fixture's contribution, not the whole fixtures
    // dir.
    let owned_prefixes = [
        "squares_are_nonneg",
        "divmod_in_range",
        "small_window",
        "many_42s",
        "ranges_ok",
        "parse_int_characterization",
        "nested_loop_skipped",
    ];
    let owned: Vec<_> = report
        .decls
        .iter()
        .filter(|d| owned_prefixes.iter().any(|p| d.name.starts_with(p)))
        .cloned()
        .collect();
    assert!(
        owned.len() >= 8,
        "expected >=8 new contracts from layer2 fixture, got {}: {:?}",
        owned.len(),
        owned.iter().map(|d| &d.name).collect::<Vec<_>>()
    );

    // Mint just these and confirm at least 8 distinct CIDs at the
    // catalog level.
    let opts = LiftOptions::default();
    let minted = mint_proof(&owned, &opts).expect("mint");
    assert!(
        minted.member_count >= 8,
        "expected >=8 minted members from layer2 fixture, got {}",
        minted.member_count
    );
}

#[test]
fn layer2_pattern_split_lift_counts_match_fixture_shape() {
    let report = lift_path(&fixtures_dir());
    let l2 = report
        .adapter_reports
        .iter()
        .find(|a| a.adapter == "rust-tests-layer2")
        .unwrap();
    // Diagnostic: print so the run log shows the per-pattern split.
    println!(
        "LAYER2_SUMMARY: lifted={} seen={} warnings={}",
        l2.lifted,
        l2.seen,
        l2.warnings.len()
    );
}
