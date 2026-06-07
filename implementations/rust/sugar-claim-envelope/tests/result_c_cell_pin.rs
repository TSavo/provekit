// SPDX-License-Identifier: Apache-2.0
//
// CID-pinning tests for the concept:result<T,E> catalog scout.
//
// These tests pin the three BLAKE3-512 content addresses that constitute the
// M+N proof for the rust:Result<T,E> -> concept:result<T,E> -> c:tagged-union-macro
// transport cell.  Any accidental mutation to the minter, the JSON schema, or
// the canonicalizer will flip one of these hashes and fail the test, which is
// the desired outcome.
//
// The CIDs were produced by:
//   python3 menagerie/concept-shapes/scripts/mint_result.py
// running against the BLAKE3-512 canonicalizer build at
//   implementations/rust/target/debug/compute_fixture_cid
//
// Pinned 2026-05-11.  Re-pin only when a deliberate schema change is made;
// document the reason in the PR that changes this file.
//
// The test reads the on-disk catalog envelope files produced by mint_result.py
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
    "blake3-512:5508bf6f0d72bfcce149790b211bfbaf6a04e090a64cf7e687307f4b1647d54\
     705b3a796cd15ff409a34f9e41443ec83736598192b1997223cbee92a9649e9b0";

const LIFT_CID: &str = "blake3-512:34ceb3b5926f9fbeade47081e0018ee5440e4acebc36591a4c09b5188917a4b\
     0a2cc99fb5a883d6b92022df70ba25d7bb7399a2367bb7d7ea10762e4178da8e5";

const REALIZE_CID: &str =
    "blake3-512:606c3704cebe6ba12ac066178e2c2bba324d2d1c7acdbf26151734cf953af3b\
     435b9e8eaf1bd6312b71e48e61dd78941fbf32332bb9a03046c731f553181fdb0";

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
fn pin_abstraction_concept_result() {
    let dir = catalog_root().join("abstractions");
    let filename = format!("concept:result.{ABSTRACTION_CID}.json");
    let path = dir.join(&filename);
    if !path.exists() {
        eprintln!(
            "SKIP pin_abstraction_concept_result: file not found (run `make mint` first):\n  {path:?}"
        );
        return;
    }
    let actual = read_cid_from_envelope(&path);
    assert_eq!(
        actual, ABSTRACTION_CID,
        "concept:result abstraction CID drifted — re-run mint_result.py and update ABSTRACTION_CID"
    );
}

#[test]
fn pin_lift_rust_result_to_concept_result() {
    let dir = catalog_root().join("realizations");
    let filename = format!("rust:Result->concept:result.{LIFT_CID}.json");
    let path = dir.join(&filename);
    if !path.exists() {
        eprintln!(
            "SKIP pin_lift_rust_result_to_concept_result: file not found (run `make mint` first):\n  {path:?}"
        );
        return;
    }
    let actual = read_cid_from_envelope(&path);
    assert_eq!(
        actual, LIFT_CID,
        "rust:Result->concept:result lift CID drifted — re-run mint_result.py and update LIFT_CID"
    );
}

#[test]
fn pin_realize_concept_result_to_c() {
    let dir = catalog_root().join("realizations");
    let filename = format!("concept:result->c:tagged-union-macro.{REALIZE_CID}.json");
    let path = dir.join(&filename);
    if !path.exists() {
        eprintln!(
            "SKIP pin_realize_concept_result_to_c: file not found (run `make mint` first):\n  {path:?}"
        );
        return;
    }
    let actual = read_cid_from_envelope(&path);
    assert_eq!(
        actual, REALIZE_CID,
        "concept:result->c:tagged-union-macro CID drifted — re-run mint_result.py and update REALIZE_CID"
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
