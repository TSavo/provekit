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
    let raw =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let root: serde_json::Value =
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
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
        declared,
        minted,
        "{}: declared cid does not match recomputed cid.\n  \
         declared = {declared}\n  \
         minted   = {minted}\n  \
         Re-run mint-plugin-cid and update the file's header.cid.",
        path.display()
    );
    assert_eq!(
        declared,
        pinned,
        "{}: cid drifted from the pin in this test file.\n  \
         pinned in test = {pinned}\n  \
         declared in file = {declared}\n  \
         If this drift is intentional, update the pinned value in this test.",
        path.display()
    );
}

#[test]
fn java_canonical_sugar_cid_self_consistent() {
    let path =
        repo_root().join("menagerie/java-language-signature/specs/sugar/java-canonical.json");
    assert_self_consistent(
        path,
        "blake3-512:b7ad1160f00d892d310fb33ac3372a4ebb2f89fec563cab1719e7006ab3d7593aae2162b882aedbec1b97e44957240b3c7e8ab1675456f0539c4ad3f45d22a7b",
    );
}

#[test]
fn java_bean_validation_sugar_cid_self_consistent() {
    let path =
        repo_root().join("menagerie/java-language-signature/specs/sugar/java-bean-validation.json");
    assert_self_consistent(
        path,
        "blake3-512:dbfbb31b64445c500281daf4577285a2f5ac1073336a1e8f9fcc7d745508d2eeba0e5974e5d258ce88998d19630119c9a06fa665764369e910e7d466fc742ca1",
    );
}

#[test]
fn java_junit5_sugar_cid_self_consistent() {
    let path = repo_root().join("menagerie/java-language-signature/specs/sugar/java-junit5.json");
    assert_self_consistent(
        path,
        "blake3-512:eb878a4dd45a863bacdd6b86c9a0c32fac3d91be2a33a4ce3ffbbc0d372f2e5dbc074305bed7a51248b47ab0305ff63411428655cc3f015497b19ee6538bdf49",
    );
}

#[test]
fn java_function_comment_sugar_cid_self_consistent() {
    let path = repo_root()
        .join("menagerie/java-language-signature/specs/sugar/java-function-comment.json");
    assert_self_consistent(
        path,
        "blake3-512:574800417e6f4f57e561dbe9c437adc691b2cd2369d964cbc329348cb715b161f3b38f6f7ccfd41537d033741488c081ec01b6f7cb3f04ba724b7003fa05a7b6",
    );
}

#[test]
fn java_canonical_bodies_cid_self_consistent() {
    let path = repo_root()
        .join("menagerie/java-language-signature/specs/body-templates/java-canonical-bodies.json");
    assert_self_consistent(
        path,
        "blake3-512:8070d5de8bedf4a21c765b6b7f48518c43a1a8547b5e893b7af045467541d7d085d7b1b64857f64f05927a6fabee55002310db293adec3e59b17b56730bd22b9",
    );
}

#[test]
fn python_canonical_sugar_cid_self_consistent() {
    let path =
        repo_root().join("menagerie/python-language-signature/specs/sugar/python-canonical.json");
    assert_self_consistent(
        path,
        "blake3-512:611d311ef2679410a8daac0fcde13ca637d800930fa49abd21539619c7e9b1c4410b047750fcd2a5cec08e6a25fa070879611642e1807eb5e03bac70664383a8",
    );
}

#[test]
fn python_canonical_bodies_cid_self_consistent() {
    let path = repo_root().join(
        "menagerie/python-language-signature/specs/body-templates/python-canonical-bodies.json",
    );
    assert_self_consistent(
        path,
        "blake3-512:f211f2509d6a7deacc2d23d33f87d1259d2835ba7292231f377d7f234f3d22f56dbd2f348314f313accc62ec50ffcec40df6266ca9eb78ca2160a5b80468725e",
    );
}

#[test]
fn python_requests_canonical_bodies_cid_self_consistent() {
    let path = repo_root().join(
        "menagerie/python-language-signature/specs/body-templates/python-canonical-bodies-requests.json",
    );
    assert_self_consistent(
        path,
        "blake3-512:6612c844a35e288836a838e2a7615be2791eee36e159b3df333db0f63266cbe56db8754e33a8b33cce4fb89927f6e9236c2c5b0cc093eb6d80b1f1f124a2c52b",
    );
}

#[test]
fn c_canonical_sugar_cid_self_consistent() {
    let path = repo_root().join("menagerie/c-language-signature/specs/sugar/c-canonical.json");
    assert_self_consistent(
        path,
        "blake3-512:d1c75065f168ddd510c5c66ce3c8ad4278a803758e002e2f97d8b265172a006705ec96b9d0f4bf16edac22565709a4b741b01e9e56bccaab6c372bb177acd538",
    );
}

#[test]
fn c_canonical_bodies_cid_self_consistent() {
    let path = repo_root()
        .join("menagerie/c-language-signature/specs/body-templates/c-canonical-bodies.json");
    assert_self_consistent(
        path,
        "blake3-512:f0948c8f4f00359bfdafb3c1bc19227ffc70f95807dcc53450f4c84be2d44bc77770aaa54f3058014febbad276a759ebcf8cc2acabf5e6e6bddada8167c684a2",
    );
}

#[test]
fn rust_canonical_bodies_cid_self_consistent() {
    let path = repo_root()
        .join("menagerie/rust-language-signature/specs/body-templates/rust-canonical-bodies.json");
    assert_self_consistent(
        path,
        "blake3-512:aa5946b88c2798cd7399ec22db6f170970a8d0f9dac88b1e81fc0a31ab40eddada236730450594894f745096cf033607dbbb35d0cd2a59300fa9ac5e2df340cd",
    );
}
