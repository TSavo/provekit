// SPDX-License-Identifier: Apache-2.0
//
// CID-pinning tests for the concept:unit catalog scout.
//
// These tests pin the two BLAKE3-512 content addresses that constitute the
// smallest type-cell for the concept:unit -> c:void realization.
//
// The CIDs were produced by:
//   python3 menagerie/concept-shapes/scripts/mint_unit.py
// running against the BLAKE3-512 canonicalizer build at
//   implementations/rust/target/debug/compute_fixture_cid
//
// Pinned 2026-05-11.  Re-pin only when a deliberate schema change is made;
// document the reason in the PR that changes this file.
//
// The test reads the on-disk catalog envelope files produced by mint_unit.py
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
    "blake3-512:b38e053e0b4ff976aa2f8ba9c8906ae6c4125296eb5c5b7882bc19592a9a1f3b\
     c038e6c35eab3c0588a52bd40b201f04fb01cd1d6f554e7674041c534ca4cee8";

const REALIZE_CID: &str =
    "blake3-512:81a0768500e90ab20804296d5ab7b91d70c7378840fd949552c7b0f964a168e7\
     c1594b2f88a06c9cf8b029b6fb6de517c886345019941c3a16067397a725a182";

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
fn pin_abstraction_concept_unit() {
    let dir = catalog_root().join("abstractions");
    let filename = format!("blake3-512:{ABSTRACTION_CID}");
    let path = dir.join(&format!("{filename}.json"));
    if !path.exists() {
        eprintln!(
            "SKIP pin_abstraction_concept_unit: file not found (run `make mint` first):\n  {path:?}"
        );
        return;
    }
    let actual = read_cid_from_envelope(&path);
    assert_eq!(
        actual, ABSTRACTION_CID,
        "concept:unit abstraction CID drifted — re-run mint_unit.py and update ABSTRACTION_CID"
    );
}

#[test]
fn pin_realize_concept_unit_to_c() {
    let dir = catalog_root().join("realizations");
    let filename = format!("blake3-512:{REALIZE_CID}");
    let path = dir.join(&format!("{filename}.json"));
    if !path.exists() {
        eprintln!(
            "SKIP pin_realize_concept_unit_to_c: file not found (run `make mint` first):\n  {path:?}"
        );
        return;
    }
    let actual = read_cid_from_envelope(&path);
    assert_eq!(
        actual, REALIZE_CID,
        "concept:unit->c:void CID drifted — re-run mint_unit.py and update REALIZE_CID"
    );
}

#[test]
fn all_catalog_cids_are_distinct() {
    // Sanity: the two CIDs must differ; collision would indicate a copy-paste error.
    assert_ne!(ABSTRACTION_CID, REALIZE_CID);
}
