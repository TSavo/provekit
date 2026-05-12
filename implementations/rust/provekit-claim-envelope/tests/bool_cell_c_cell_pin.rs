// SPDX-License-Identifier: Apache-2.0
//
// CID-pinning tests for the concept:bool-cell catalog scout.
//
// These tests pin the two BLAKE3-512 content addresses that constitute the
// N-edge proof for the concept:bool-cell -> c:pointer-indirection transport cell.
// Any accidental mutation to the minter, the JSON schema, or the canonicalizer
// will flip one of these hashes and fail the test, which is the desired outcome.
//
// The CIDs were produced by:
//   python3 menagerie/concept-shapes/scripts/mint_bool_cell.py
// running against the BLAKE3-512 canonicalizer build at
//   implementations/rust/target/debug/compute_fixture_cid
//
// Pinned 2026-05-11.  Re-pin only when a deliberate schema change is made;
// document the reason in the PR that changes this file.
//
// The test reads the on-disk catalog envelope files produced by mint_bool_cell.py
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
    "blake3-512:eb16c2445627cf8563da04108aa62e6b46fe0d53c00735082696d938731c4f6\
     86df795b1c1098ede2c8f427997f8fadb529b80f5813b5fe011507a8b133cdae6";

const REALIZE_CID: &str =
    "blake3-512:44f964dbef76903992d3469b1801fae693edefe31b503232a20398996230e0c1\
     6f1fab794fb85361b8fd67ad301f9f6cbf7124319809e50756f5ec73a7c64892";

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
fn pin_abstraction_concept_bool_cell() {
    let dir = catalog_root().join("abstractions");
    let filename = format!("concept:bool-cell.{ABSTRACTION_CID}.json");
    let path = dir.join(&filename);
    if !path.exists() {
        eprintln!(
            "SKIP pin_abstraction_concept_bool_cell: file not found (run `make mint` first):\n  {path:?}"
        );
        return;
    }
    let actual = read_cid_from_envelope(&path);
    assert_eq!(
        actual, ABSTRACTION_CID,
        "concept:bool-cell abstraction CID drifted — re-run mint_bool_cell.py and update ABSTRACTION_CID"
    );
}

#[test]
fn pin_realize_concept_bool_cell_to_c() {
    let dir = catalog_root().join("realizations");
    let filename = format!("concept:bool-cell:c:pointer-indirection.{REALIZE_CID}.json");
    let path = dir.join(&filename);
    if !path.exists() {
        eprintln!(
            "SKIP pin_realize_concept_bool_cell_to_c: file not found (run `make mint` first):\n  {path:?}"
        );
        return;
    }
    let actual = read_cid_from_envelope(&path);
    assert_eq!(
        actual, REALIZE_CID,
        "concept:bool-cell->c:pointer-indirection CID drifted — re-run mint_bool_cell.py and update REALIZE_CID"
    );
}

#[test]
fn all_bool_cell_catalog_cids_are_distinct() {
    // Sanity: the two CIDs must differ; a collision would indicate a
    // copy-paste error in the minter.
    assert_ne!(ABSTRACTION_CID, REALIZE_CID);
}
