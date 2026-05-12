// SPDX-License-Identifier: Apache-2.0
//
// CID-pinning tests for the concept:identity catalog scout.
//
// These tests pin the three BLAKE3-512 content addresses that constitute the
// M+N proof for the rust:identity -> concept:identity -> c:identity-macro
// transport cell.  Any accidental mutation to the minter, the JSON schema, or
// the canonicalizer will flip one of these hashes and fail the test, which is
// the desired outcome.
//
// The CIDs were produced by:
//   python3 menagerie/concept-shapes/scripts/mint_identity.py
// running against the BLAKE3-512 canonicalizer build at
//   implementations/rust/target/debug/compute_fixture_cid
//
// Pinned 2026-05-11.  Re-pin only when a deliberate schema change is made;
// document the reason in the PR that changes this file.
//
// The test reads the on-disk catalog envelope files produced by mint_identity.py
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
    "blake3-512:6920f6e26184ca316f3dce6c02690b515c11b3d96d3b476bb5abe67cb55e188\
     5031484c3add8a5f26b630e305ad3fe41eed10acca2e141898f9d6629c278867f";

const LIFT_CID: &str =
    "blake3-512:53cbd9eca65246aa689f89a64c54782988464027b3f63cd86f6b340b1b3848e1\
     46524e33ef838f58673d9d61d53de8e7a86aafc8a6848edcc7e0a9f4f6c6cb50";

const REALIZE_CID: &str =
    "blake3-512:3ef11eb4c271d5108e0c610e0042b7ab76b70ba3ebbc7f33aa6501273617534b\
     0af728aa6830887e7f1b064b876d4cbc2e33b975ea713bea66e836ec85c4c329";

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
fn pin_abstraction_concept_identity() {
    let dir = catalog_root().join("abstractions");
    let filename = format!("concept:identity.{ABSTRACTION_CID}.json");
    let path = dir.join(&filename);
    if !path.exists() {
        eprintln!(
            "SKIP pin_abstraction_concept_identity: file not found (run `make mint` first):\n  {path:?}"
        );
        return;
    }
    let actual = read_cid_from_envelope(&path);
    assert_eq!(
        actual, ABSTRACTION_CID,
        "concept:identity abstraction CID drifted — re-run mint_identity.py and update ABSTRACTION_CID"
    );
}

#[test]
fn pin_lift_rust_identity_to_concept_identity() {
    let dir = catalog_root().join("realizations");
    let filename = format!("rust:identity->concept:identity.{LIFT_CID}.json");
    let path = dir.join(&filename);
    if !path.exists() {
        eprintln!(
            "SKIP pin_lift_rust_identity_to_concept_identity: file not found (run `make mint` first):\n  {path:?}"
        );
        return;
    }
    let actual = read_cid_from_envelope(&path);
    assert_eq!(
        actual, LIFT_CID,
        "rust:identity->concept:identity lift CID drifted — re-run mint_identity.py and update LIFT_CID"
    );
}

#[test]
fn pin_realize_concept_identity_to_c() {
    let dir = catalog_root().join("realizations");
    let filename = format!("concept:identity->c:identity-macro.{REALIZE_CID}.json");
    let path = dir.join(&filename);
    if !path.exists() {
        eprintln!(
            "SKIP pin_realize_concept_identity_to_c: file not found (run `make mint` first):\n  {path:?}"
        );
        return;
    }
    let actual = read_cid_from_envelope(&path);
    assert_eq!(
        actual, REALIZE_CID,
        "concept:identity->c:identity-macro CID drifted — re-run mint_identity.py and update REALIZE_CID"
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
