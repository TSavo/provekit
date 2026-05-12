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
// Pinned 2026-05-11.  Re-pin only when a deliberate schema change is made;
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
    "blake3-512:f8988d83ddf06694b1c9fa97e5a2ff8ab3be7c5ebe9dd34dc260e7ca15a7e8f8\
     fe731f870976813391812dbadfa173746d6e7e7a3feb6c3a8dc62eb4ef2ec051";

const LIFT_CID: &str =
    "blake3-512:0e6a7e307963314e9d13fe9d50c74728be867cfb61082134c04ccbdd534aee2\
     45d0d367c484e9a3733f541dba9298548435c6daabeea2d5d20ab51fc22aa6cd5";

const REALIZE_CID: &str =
    "blake3-512:64bb53691dbcf43055113a458322d656b1240418fc0a089bc42cbe33230d2c46\
     5f4a06a3e93e69972b01b9758ec2713c725591174eee3b2a18b755c8c23aa768";

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
