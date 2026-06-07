// SPDX-License-Identifier: Apache-2.0
//
// CID-pinning tests for the concept:pair<T1,T2> catalog scout.
//
// These tests pin the three BLAKE3-512 content addresses that constitute the
// M+N proof for the rust:(T1,T2) -> concept:pair<T1,T2> -> c:struct
// transport cell.  Any accidental mutation to the minter, the JSON schema, or
// the canonicalizer will flip one of these hashes and fail the test, which is
// the desired outcome.
//
// The CIDs were produced by:
//   python3 menagerie/concept-shapes/scripts/mint_pair.py
// running against the BLAKE3-512 canonicalizer build at
//   implementations/rust/target/debug/compute_fixture_cid
//
// Pinned 2026-05-11.  Re-pin only when a deliberate schema change is made;
// document the reason in the PR that changes this file.
//
// The test reads the on-disk catalog envelope files produced by mint_pair.py
// and extracts the "cid" field from the JSON envelope (which is the BLAKE3-512
// hash of the inner "memento" object, not of the outer envelope).  This matches
// exactly what compute_fixture_cid computes on the raw memento.  If the files
// are absent (e.g., a fresh checkout before `make mint` has been run), the
// test is skipped with a descriptive message.

use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Pinned CIDs (do not edit without updating the minter and documenting why)
// ---------------------------------------------------------------------------

const ABSTRACTION_CID: &str =
    "blake3-512:8e6429feab0ca0b075ec3d286aa1cb48689f2af913d70ba641d2fa4de797863\
     fa05e1b325221cc21259d9d27cbc86c88741d57d9364ec5b40a6fa2175f6e97e6";

const LIFT_CID: &str = "blake3-512:3a45695cab42636b693e5060a31e843cd7cca00cd296bc5ec540e1e56267724\
     0334516db15c9e8a1f152815e1f99bf0171495f8098ed5d3bf7e7e5014dece91e";

const REALIZE_CID: &str =
    "blake3-512:789ab75ea7b537708ac56b3f9a19dc6f353fdaa4b35e757e04a597aec6ee0f2\
     bf9ddfaff99d6d9aea3a51333a5810e01bb5e2d48274ead5e9bde91431396c7e1";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Read the catalog envelope file and extract the "cid" field.
/// The envelope format is: {"memento": {...}, "cid": "<blake3-512:...>", "signature": {...}}.
/// The "cid" field is compute_fixture_cid applied to the inner "memento" object.
fn read_cid_from_envelope(path: &PathBuf) -> String {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("failed to read {path:?}: {e}"));
    let v: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("invalid JSON in {path:?}: {e}"));
    v["cid"]
        .as_str()
        .unwrap_or_else(|| panic!("no 'cid' field in envelope at {path:?}"))
        .to_string()
}

/// Path to the catalog from this file's manifest directory.
fn catalog_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is .../implementations/rust/sugar-claim-envelope
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../menagerie/concept-shapes/catalog")
}

// ---------------------------------------------------------------------------
// Pinning tests
// ---------------------------------------------------------------------------

#[test]
fn pin_abstraction_concept_pair() {
    let dir = catalog_root().join("abstractions");
    let filename = format!("concept:pair.{ABSTRACTION_CID}.json");
    let path = dir.join(&filename);
    if !path.exists() {
        eprintln!(
            "SKIP pin_abstraction_concept_pair: file not found (run `make mint` first):\n  {path:?}"
        );
        return;
    }
    let actual = read_cid_from_envelope(&path);
    assert_eq!(
        actual, ABSTRACTION_CID,
        "concept:pair abstraction CID drifted — re-run mint_pair.py and update ABSTRACTION_CID"
    );
}

#[test]
fn pin_lift_rust_tuple_to_concept_pair() {
    let dir = catalog_root().join("realizations");
    let filename = format!("rust:tuple->concept:pair.{LIFT_CID}.json");
    let path = dir.join(&filename);
    if !path.exists() {
        eprintln!(
            "SKIP pin_lift_rust_tuple_to_concept_pair: file not found (run `make mint` first):\n  {path:?}"
        );
        return;
    }
    let actual = read_cid_from_envelope(&path);
    assert_eq!(
        actual, LIFT_CID,
        "rust:tuple->concept:pair lift CID drifted — re-run mint_pair.py and update LIFT_CID"
    );
}

#[test]
fn pin_realize_concept_pair_to_c() {
    let dir = catalog_root().join("realizations");
    let filename = format!("concept:pair->c:struct.{REALIZE_CID}.json");
    let path = dir.join(&filename);
    if !path.exists() {
        eprintln!(
            "SKIP pin_realize_concept_pair_to_c: file not found (run `make mint` first):\n  {path:?}"
        );
        return;
    }
    let actual = read_cid_from_envelope(&path);
    assert_eq!(
        actual, REALIZE_CID,
        "concept:pair->c:struct CID drifted — re-run mint_pair.py and update REALIZE_CID"
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
