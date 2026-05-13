// SPDX-License-Identifier: Apache-2.0
//
// CI drift-prevention tests for substrate default plugin CIDs.
//
// Every default plugin file shipped in `menagerie/<lang>-language-signature/specs/.../`
// declares a `cid` in its header. That CID MUST equal what `compute_plugin_cid`
// produces from the header (§6.1 of plugin-protocol). If anyone edits a plugin
// file's content without re-running `mint-plugin-cid` and updating the declared
// `cid`, this test fails and tells them exactly what the new CID should be.
//
// These tests serve two purposes:
//  1. Document the canonical default plugin set per `2026-05-13-body-template-memento.md` §4
//     ("the substrate registers ONE default body-template per target language").
//  2. Catch silent drift between declared CID and re-computed CID (the kind of
//     drift that nearly shipped a literal `PLACEHOLDER_TO_BE_MINTED` in PR #765).
//
// Pinned values are deliberately duplicated from the JSON files so a change to
// either side surfaces immediately.

use std::path::PathBuf;

use provekit_plugin_loader::cid::compute_plugin_cid;
use provekit_plugin_loader::types::PluginHeader;

/// Repo root: this crate is at implementations/rust/provekit-plugin-loader/.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Mint the CID for a plugin file given its absolute path.
///
/// Reads `header` out of the file's root object, parses it as `PluginHeader`
/// (the existing `cid` field is ignored by `compute_plugin_cid` per §6.1), and
/// returns the recomputed CID.
fn mint_cid_for(path: &PathBuf) -> String {
    let raw = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let root: serde_json::Value = serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
    let header_val = root
        .get("header")
        .unwrap_or_else(|| panic!("{}: missing top-level `header`", path.display()))
        .clone();
    let header: PluginHeader = serde_json::from_value(header_val)
        .unwrap_or_else(|e| panic!("{}: header shape: {e}", path.display()));
    compute_plugin_cid(&header)
}

/// Read the declared `cid` from a plugin file's header.
fn declared_cid_for(path: &PathBuf) -> String {
    let raw = std::fs::read_to_string(path).unwrap();
    let root: serde_json::Value = serde_json::from_str(&raw).unwrap();
    root.get("header")
        .and_then(|h| h.get("cid"))
        .and_then(|c| c.as_str())
        .unwrap_or_else(|| panic!("{}: header.cid missing or not a string", path.display()))
        .to_string()
}

/// Assert a plugin file's declared CID equals what the minting recipe produces.
fn assert_self_consistent(path: PathBuf, pinned: &str) {
    let declared = declared_cid_for(&path);
    let minted = mint_cid_for(&path);
    assert_eq!(
        declared, minted,
        "{}: declared cid does not match recomputed cid.\n  \
         declared = {declared}\n  \
         minted   = {minted}\n  \
         Re-run mint-plugin-cid and update the file's header.cid.",
        path.display()
    );
    assert_eq!(
        declared, pinned,
        "{}: cid drifted from the pin in this test file.\n  \
         pinned in test = {pinned}\n  \
         declared in file = {declared}\n  \
         If this drift is intentional, update the pinned value in this test.",
        path.display()
    );
}

#[test]
fn java_canonical_sugar_cid_self_consistent() {
    let path = repo_root()
        .join("menagerie/java-language-signature/specs/sugar/java-canonical.json");
    assert_self_consistent(
        path,
        "blake3-512:b7ad1160f00d892d310fb33ac3372a4ebb2f89fec563cab1719e7006ab3d7593aae2162b882aedbec1b97e44957240b3c7e8ab1675456f0539c4ad3f45d22a7b",
    );
}

#[test]
fn java_canonical_bodies_cid_self_consistent() {
    let path = repo_root()
        .join("menagerie/java-language-signature/specs/body-templates/java-canonical-bodies.json");
    assert_self_consistent(
        path,
        "blake3-512:e9c5dd414a182211d5b85e3f2052d491e85c90db5ed29fb1145c7d1ad7f4e34f71de80c12a210202c6bd7efe26639da0e4e1c446beca786245d4a3dc6708204b",
    );
}

#[test]
fn python_canonical_sugar_cid_self_consistent() {
    let path = repo_root()
        .join("menagerie/python-language-signature/specs/sugar/python-canonical.json");
    assert_self_consistent(
        path,
        "blake3-512:611d311ef2679410a8daac0fcde13ca637d800930fa49abd21539619c7e9b1c4410b047750fcd2a5cec08e6a25fa070879611642e1807eb5e03bac70664383a8",
    );
}

#[test]
fn python_canonical_bodies_cid_self_consistent() {
    let path = repo_root()
        .join("menagerie/python-language-signature/specs/body-templates/python-canonical-bodies.json");
    assert_self_consistent(
        path,
        "blake3-512:fb35f2196ebca55a964e1829a5f0f2842a0d3de53406fcca504e93b0aa1e55c09b7735740c382564817d279146479d8e4a3963377ee42878c6bb979da44a5cc7",
    );
}
