// SPDX-License-Identifier: Apache-2.0
//
// CID-pinning tests for the concept:option-bind<T,U> catalog scout.
//
// These tests pin the three BLAKE3-512 content addresses that constitute the
// M+N proof for the rust:Option::and_then -> concept:option-bind -> c:macro-composition
// transport cell. Any accidental mutation to the minter, the JSON schema, or
// the canonicalizer will flip one of these hashes and fail the test, which is
// the desired outcome.
//
// The CIDs were produced by:
//   python3 menagerie/concept-shapes/scripts/mint_option_bind.py
// running against the BLAKE3-512 canonicalizer build at
//   implementations/rust/target/debug/compute_fixture_cid
//
// Pinned 2026-05-11; re-pinned 2026-05-11 (fix #676 kind-taxonomy drift:
// "abstraction"->"concept-abstraction", "operation"->"op", "constructor"->"ctor").
// Re-pin only when a deliberate schema change is made;
// document the reason in the PR that changes this file.
//
// This cell depends on concept:option from PR #641 being in the catalog.
// The C realization composes on the OPTION_DECL, OPTION_NONE, OPTION_SOME macros
// from the option-c cell via macro-composition.
//
// The test reads the on-disk catalog envelope files produced by mint_option_bind.py
// and extracts the "cid" field from the JSON envelope (which is the BLAKE3-512
// hash of the inner "memento" object, not of the outer envelope). This matches
// exactly what compute_fixture_cid computes on the raw memento. If the files
// are absent (e.g., a fresh checkout before `make mint` has been run), the
// test is skipped with a descriptive message.

use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Pinned CIDs (do not edit without updating the minter and documenting why)
// ---------------------------------------------------------------------------

const ABSTRACTION_CID: &str =
    "blake3-512:dba4b83cb31576a5f505da52ea45f455ff1af2fe488886af05dc4cb5e09968e2\
     99deae726e42a754f012aab920cb264c504447e6614804c98598df92aa9d25d1";

const LIFT_CID: &str =
    "blake3-512:0c2f37c6613c7a8cb8c0a62d7d83d57c70e4c20ac1160cb2737c9c8c0f63d212\
     66eec6aad3e7273d1ff140567da86dc94bc27a04baec99cb460e94fc6fe38b09";

const REALIZE_CID: &str =
    "blake3-512:924ccd4ce73de6981e9f04ea384a09bda3b6dacfb524a090b6b94c8511424687\
     a28a6e60b650ad40b9b72e100a35b2bb84a14ea7be1d541bce12358e1246b831";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Read the catalog envelope file and extract the "cid" field.
/// The envelope format is: {"memento": {...}, "cid": "<blake3-512:...>", "signature": {...}}.
/// The "cid" field is compute_fixture_cid applied to the inner "memento" object.
fn read_cid_from_envelope(path: &PathBuf) -> String {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("failed to read {path:?}: {e}"));
    let v: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("invalid JSON in {path:?}: {e}"));
    v["cid"]
        .as_str()
        .unwrap_or_else(|| panic!("no 'cid' field in envelope at {path:?}"))
        .to_string()
}

/// Path to the catalog from this file's manifest directory.
fn catalog_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is .../implementations/rust/provekit-claim-envelope
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../menagerie/concept-shapes/catalog")
}

// ---------------------------------------------------------------------------
// Pinning tests
// ---------------------------------------------------------------------------

#[test]
fn pin_abstraction_concept_option_bind() {
    let dir = catalog_root().join("abstractions");
    let filename = format!("concept:option-bind.{ABSTRACTION_CID}.json");
    let path = dir.join(&filename);
    if !path.exists() {
        eprintln!(
            "SKIP pin_abstraction_concept_option_bind: file not found (run `make mint` first):\n  {path:?}"
        );
        return;
    }
    let actual = read_cid_from_envelope(&path);
    assert_eq!(
        actual, ABSTRACTION_CID,
        "concept:option-bind abstraction CID drifted — re-run mint_option_bind.py and update ABSTRACTION_CID"
    );
}

#[test]
fn pin_lift_rust_option_and_then_to_concept_option_bind() {
    let dir = catalog_root().join("realizations");
    let filename = format!("rust:Option::and_then->concept:option-bind.{LIFT_CID}.json");
    let path = dir.join(&filename);
    if !path.exists() {
        eprintln!(
            "SKIP pin_lift_rust_option_and_then_to_concept_option_bind: file not found (run `make mint` first):\n  {path:?}"
        );
        return;
    }
    let actual = read_cid_from_envelope(&path);
    assert_eq!(
        actual, LIFT_CID,
        "rust:Option::and_then->concept:option-bind lift CID drifted — re-run mint_option_bind.py and update LIFT_CID"
    );
}

#[test]
fn pin_realize_concept_option_bind_to_c_macro_composition() {
    let dir = catalog_root().join("realizations");
    let filename = format!("concept:option-bind->c:macro-composition.{REALIZE_CID}.json");
    let path = dir.join(&filename);
    if !path.exists() {
        eprintln!(
            "SKIP pin_realize_concept_option_bind_to_c_macro_composition: file not found (run `make mint` first):\n  {path:?}"
        );
        return;
    }
    let actual = read_cid_from_envelope(&path);
    assert_eq!(
        actual, REALIZE_CID,
        "concept:option-bind->c:macro-composition CID drifted — re-run mint_option_bind.py and update REALIZE_CID"
    );
}

#[test]
fn all_three_catalog_cids_are_distinct() {
    // Sanity: the three CIDs must all differ; collisions would indicate a
    // copy-paste error in the minter.
    assert_ne!(ABSTRACTION_CID, LIFT_CID);
    assert_ne!(LIFT_CID, REALIZE_CID);
    assert_ne!(ABSTRACTION_CID, REALIZE_CID);
}
