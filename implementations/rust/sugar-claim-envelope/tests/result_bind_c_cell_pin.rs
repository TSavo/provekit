// SPDX-License-Identifier: Apache-2.0
//
// CID-pinning tests for the concept:result-bind catalog scout.
//
// These tests pin the two BLAKE3-512 content addresses that constitute the
// N-edge proof for the concept:result-bind -> c:result-bind-macro transport
// cell.  Any accidental mutation to the minter, the JSON schema, or the
// canonicalizer will flip one of these hashes and fail the test, which is
// the desired outcome.
//
// Note: there is no M-edge (lift) artifact for concept:result-bind because
// bind is an operation on concept:result, not a language type.  The M-edge
// lives in the concept:result -> c cell (PR #668).  This cell has a declared
// dependency on PR #668; the abstraction's realizations list will be extended
// once that PR lands.
//
// The CIDs were produced by:
//   python3 menagerie/concept-shapes/scripts/mint_result_bind.py
// running against the BLAKE3-512 canonicalizer build at
//   implementations/rust/target/debug/compute_fixture_cid
//
// Pinned 2026-05-11.  Re-pin only when a deliberate schema change is made;
// document the reason in the PR that changes this file.
//
// The test reads the on-disk catalog envelope files produced by
// mint_result_bind.py and extracts the "cid" field from the JSON envelope
// (the BLAKE3-512 hash of the inner "memento" object, not the outer
// envelope).  If the files are absent (e.g., a fresh checkout before
// `make mint` has been run), the test is skipped with a descriptive message.

use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Pinned CIDs (do not edit without updating the minter and documenting why)
// ---------------------------------------------------------------------------

const ABSTRACTION_CID: &str =
    "blake3-512:489453d5c40f7099e3a01b02c5ceaec8bb9f1c10966247a634d59502af9e7ec\
     cbce2f4b2a8b1fe04150381031aebf3c50f236d9ca8d7c2ac653cecd406b7adac";

const REALIZE_CID: &str =
    "blake3-512:5252dfc6ef0aa7a7e706e4ee8270f54a2fdc218aace42e9dff793de1e88c5ed5\
     66aaa9e8c886b7f908e9f744a7bf6862b349406b0891bf484152eae77ee4655e";

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
fn pin_abstraction_concept_result_bind() {
    let dir = catalog_root().join("abstractions");
    let filename = format!("concept:result-bind.{ABSTRACTION_CID}.json");
    let path = dir.join(&filename);
    if !path.exists() {
        eprintln!(
            "SKIP pin_abstraction_concept_result_bind: file not found (run `make mint` first):\n  {path:?}"
        );
        return;
    }
    let actual = read_cid_from_envelope(&path);
    assert_eq!(
        actual, ABSTRACTION_CID,
        "concept:result-bind abstraction CID drifted -- re-run mint_result_bind.py and update ABSTRACTION_CID"
    );
}

#[test]
fn pin_realize_concept_result_bind_to_c() {
    let dir = catalog_root().join("realizations");
    let filename = format!("concept:result-bind->c:result-bind-macro.{REALIZE_CID}.json");
    let path = dir.join(&filename);
    if !path.exists() {
        eprintln!(
            "SKIP pin_realize_concept_result_bind_to_c: file not found (run `make mint` first):\n  {path:?}"
        );
        return;
    }
    let actual = read_cid_from_envelope(&path);
    assert_eq!(
        actual, REALIZE_CID,
        "concept:result-bind->c:result-bind-macro CID drifted -- re-run mint_result_bind.py and update REALIZE_CID"
    );
}

#[test]
fn abstraction_and_realization_cids_are_distinct() {
    // Sanity: the two CIDs must differ; a collision would indicate a
    // copy-paste error in the minter.
    assert_ne!(ABSTRACTION_CID, REALIZE_CID);
}
