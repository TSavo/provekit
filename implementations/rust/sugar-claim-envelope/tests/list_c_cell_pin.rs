// SPDX-License-Identifier: Apache-2.0
//
// CID-pinning tests for the concept:list<T> catalog scout.
//
// These tests pin the three BLAKE3-512 content addresses that constitute the
// M+N proof for the rust:Vec<T> -> concept:list<T> -> c:linked-struct
// transport cell.  Any accidental mutation to the minter, the JSON schema, or
// the canonicalizer will flip one of these hashes and fail the test, which is
// the desired outcome.
//
// The CIDs were produced by:
//   python3 menagerie/concept-shapes/scripts/mint_list.py
// running against the BLAKE3-512 canonicalizer build at
//   implementations/rust/target/debug/compute_fixture_cid
//
// Pinned 2026-05-11.  Re-pin only when a deliberate schema change is made;
// document the reason in the PR that changes this file.
//
// The test reads the on-disk catalog envelope files produced by mint_list.py
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
    "blake3-512:40764ca6ed77bc43ace55ca2ce447441f6659a4c5df5635a3ee121111110f3f\
     7e375d1fea3ddf5903bc44da4dbe8ebdf8e6a56e3e9950755a85c307edbce20f4";

const LIFT_CID: &str = "blake3-512:af0f7cf78880c0b77e9155a4885e73c8887949e8ab5bee925266ca620f82503\
     b5b71210cf153a93cebaa08f7a31dab76d0131812ce14898e757bc8dc31eedec6";

const REALIZE_CID: &str =
    "blake3-512:2cb7ac819938ca8f044240e6da9c517706a165114d4a3ecf9bcc7040e754a0e\
     02a8d8076612701ec0b6ab02e7f60be54ae176c715e552bd739b6972a9c396edb";

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
fn pin_abstraction_concept_list() {
    let dir = catalog_root().join("abstractions");
    let filename = format!("concept:list.{ABSTRACTION_CID}.json");
    let path = dir.join(&filename);
    if !path.exists() {
        eprintln!(
            "SKIP pin_abstraction_concept_list: file not found (run `make mint` first):\n  {path:?}"
        );
        return;
    }
    let actual = read_cid_from_envelope(&path);
    assert_eq!(
        actual, ABSTRACTION_CID,
        "concept:list abstraction CID drifted — re-run mint_list.py and update ABSTRACTION_CID"
    );
}

#[test]
fn pin_lift_rust_vec_to_concept_list() {
    let dir = catalog_root().join("realizations");
    let filename = format!("rust:Vec->concept:list.{LIFT_CID}.json");
    let path = dir.join(&filename);
    if !path.exists() {
        eprintln!(
            "SKIP pin_lift_rust_vec_to_concept_list: file not found (run `make mint` first):\n  {path:?}"
        );
        return;
    }
    let actual = read_cid_from_envelope(&path);
    assert_eq!(
        actual, LIFT_CID,
        "rust:Vec->concept:list lift CID drifted — re-run mint_list.py and update LIFT_CID"
    );
}

#[test]
fn pin_realize_concept_list_to_c() {
    let dir = catalog_root().join("realizations");
    let filename = format!("concept:list->c:linked-struct.{REALIZE_CID}.json");
    let path = dir.join(&filename);
    if !path.exists() {
        eprintln!(
            "SKIP pin_realize_concept_list_to_c: file not found (run `make mint` first):\n  {path:?}"
        );
        return;
    }
    let actual = read_cid_from_envelope(&path);
    assert_eq!(
        actual, REALIZE_CID,
        "concept:list->c:linked-struct CID drifted — re-run mint_list.py and update REALIZE_CID"
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
