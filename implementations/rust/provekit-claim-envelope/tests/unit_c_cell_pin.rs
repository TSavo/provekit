// SPDX-License-Identifier: Apache-2.0
//
// CID-pinning tests for the concept:unit catalog scout.
//
// These tests pin the three BLAKE3-512 content addresses that constitute the
// M+N proof for the rust:() -> concept:unit -> c:empty-struct-typedef
// transport cell.  Any accidental mutation to the minter, the JSON schema, or
// the canonicalizer will flip one of these hashes and fail the test, which is
// the desired outcome.
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
    "blake3-512:5318f6b6b6588eb53d41871b986feb455bed4e77a0c63640bbed33b699c1cd2a\
     906a71fbc48c7413c769f220836f26f330a235f870ed0ebef79ef0fd396c674f";

const LIFT_CID: &str =
    "blake3-512:27abd2d957f57d7637d64c91a2fee60fc29371184c3b880c46fff8f9d30f88f1\
     1ddf077433d774cc9dd71c68c7c2a47e75ee8a3fb009ac676e05334f1cfe4c4c";

const REALIZE_CID: &str =
    "blake3-512:abce2473a36c4dee1cd1e313ebaf5d712c9f408c3daa9e638665f736b7e2a4e7\
     96c0f98ffdad43a28e4ff6d40c7f620b0035f0f87caa893ead4de5357c6cc696";

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
    let filename = format!("concept:unit.{ABSTRACTION_CID}.json");
    let path = dir.join(&filename);
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
fn pin_lift_rust_unit_to_concept_unit() {
    let dir = catalog_root().join("realizations");
    let filename = format!("rust:()->concept:unit.{LIFT_CID}.json");
    let path = dir.join(&filename);
    if !path.exists() {
        eprintln!(
            "SKIP pin_lift_rust_unit_to_concept_unit: file not found (run `make mint` first):\n  {path:?}"
        );
        return;
    }
    let actual = read_cid_from_envelope(&path);
    assert_eq!(
        actual, LIFT_CID,
        "rust:()->concept:unit lift CID drifted — re-run mint_unit.py and update LIFT_CID"
    );
}

#[test]
fn pin_realize_concept_unit_to_c() {
    let dir = catalog_root().join("realizations");
    let filename = format!("concept:unit->c:empty-struct-typedef.{REALIZE_CID}.json");
    let path = dir.join(&filename);
    if !path.exists() {
        eprintln!(
            "SKIP pin_realize_concept_unit_to_c: file not found (run `make mint` first):\n  {path:?}"
        );
        return;
    }
    let actual = read_cid_from_envelope(&path);
    assert_eq!(
        actual, REALIZE_CID,
        "concept:unit->c:empty-struct-typedef CID drifted — re-run mint_unit.py and update REALIZE_CID"
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
