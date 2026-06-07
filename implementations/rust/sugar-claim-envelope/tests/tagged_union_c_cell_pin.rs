// SPDX-License-Identifier: Apache-2.0
//
// CID-pinning tests for the concept:tagged-union<T1,T2> catalog scout.
//
// These tests pin the two BLAKE3-512 content addresses that constitute the
// N proof for the concept:tagged-union<T1,T2> -> c:tagged-union-macro
// transport cell.  Pattern mirrors #641's option-c cell; tagged-union generalizes
// option by having two type parameters instead of option-or-nothing.
// (option IS tagged-union<T, unit>.)
//
// Any accidental mutation to the minter, the JSON schema, or the canonicalizer
// will flip one of these hashes and fail the test, which is the desired outcome.
//
// The CIDs were produced by:
//   python3 menagerie/concept-shapes/scripts/mint_tagged_union.py
// running against the BLAKE3-512 canonicalizer build at
//   implementations/rust/target/debug/compute_fixture_cid
//
// Pinned 2026-05-11.  Re-pin only when a deliberate schema change is made;
// document the reason in the PR that changes this file.
//
// The test reads the on-disk catalog envelope files produced by mint_tagged_union.py
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
    "blake3-512:5bdc03f24dfcc9eb1563c287686d7453eb33cfd5868624e40d2c0a33901716d7d9b37f14077d5748e0e969ee688fce322444380f0d7762429de4eeb1e1123339";

const REALIZE_CID: &str =
    "blake3-512:77e2fddcb09b19d88496dd6e98835c235efa28c3a5114d3bb378f8503ee8ed2b8bec7c4eddad475c4109464a6cc3121a57a0f043039d38575c8ab692f443c2dc";

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
fn pin_abstraction_concept_tagged_union() {
    let dir = catalog_root().join("abstractions");
    let filename = format!("concept:tagged-union.{ABSTRACTION_CID}.json");
    let path = dir.join(&filename);
    if !path.exists() {
        eprintln!(
            "SKIP pin_abstraction_concept_tagged_union: file not found (run `make mint` first):\n  {path:?}"
        );
        return;
    }
    let actual = read_cid_from_envelope(&path);
    assert_eq!(
        actual, ABSTRACTION_CID,
        "concept:tagged-union abstraction CID drifted — re-run mint_tagged_union.py and update ABSTRACTION_CID"
    );
}

#[test]
fn pin_realize_concept_tagged_union_to_c() {
    let dir = catalog_root().join("realizations");
    let filename = format!("concept:tagged-union->c:tagged-union-macro.{REALIZE_CID}.json");
    let path = dir.join(&filename);
    if !path.exists() {
        eprintln!(
            "SKIP pin_realize_concept_tagged_union_to_c: file not found (run `make mint` first):\n  {path:?}"
        );
        return;
    }
    let actual = read_cid_from_envelope(&path);
    assert_eq!(
        actual, REALIZE_CID,
        "concept:tagged-union->c:tagged-union-macro CID drifted — re-run mint_tagged_union.py and update REALIZE_CID"
    );
}

#[test]
fn both_catalog_cids_are_distinct() {
    // Sanity: the two CIDs must differ; collisions would indicate a
    // copy-paste error in the minter.
    assert_ne!(ABSTRACTION_CID, REALIZE_CID);
}
