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
        "blake3-512:d0405739130eeb2c5814c81584bb61f9bf079989e7c4d5e1ecd3c5066660ba53fc0e29ba0ecbdf2fb615da4359c647c04e44654ea7a924d50bb9a06a439fc5ba",
    );
}

#[test]
fn java_junit5_sugar_cid_self_consistent() {
    let path = repo_root().join("menagerie/java-language-signature/specs/sugar/java-junit5.json");
    assert_self_consistent(
        path,
        "blake3-512:95f3f61d9c904c43c0965e483c70db7d293fea01e2993664b1050260466d101bf1366b9bddf53ab4b287b90866295c3a5bee9d877f07c9b0f8355db70b3a6e16",
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
        "blake3-512:c1f42db4540bb0c6e4f132e81da5280b0f3a0fcad5b3c889ba8553295da7536123dcb6dea54861bf3dcb2f90ff92d001738cde4c51484f00e6a85fb504a63d0d",
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
        "blake3-512:1a50f6eb71a1586968edf7315eb504ddfe9dbb9464281b02f96f5eb4de27b17dcc88d06be1bb9c94f241482c29f268c6f79e7be43b0d1bc1adff2f7a0b0a01cd",
    );
}

#[test]
fn python_requests_canonical_bodies_cid_self_consistent() {
    let path = repo_root().join(
        "menagerie/python-language-signature/specs/body-templates/python-canonical-bodies-requests.json",
    );
    assert_self_consistent(
        path,
        "blake3-512:100f4c5b7ce1e21bbf63c942d696be3c19ce20c75f6c8be40fb8d7026428575d761e46f3cec9678396958dea2bffb41d1bd46dbc868595e31157bf289798ecaf",
    );
}

#[test]
fn typescript_canonical_bodies_cid_self_consistent() {
    let path = repo_root().join(
        "menagerie/typescript-language-signature/specs/body-templates/typescript-canonical-bodies.json",
    );
    assert_self_consistent(
        path,
        "blake3-512:b0beecd35a50a57fd8cd6b3455b15c6a2638cef7033f247b012f7056ca99d53c808ba190b4f9d33457bb0187d2d1ee8fe73c83de3cdfaee163d80a8d030736b3",
    );
}

// `typescript_better_sqlite3_canonical_bodies_cid_self_consistent` removed:
// the better-sqlite3 AND pg canonical-bodies JSONs are deleted. The realize kits
// resolve emission bodies from the shim `provekit.proof` via node_modules, and
// `body_template_cid` content-addresses the kit-returned entries with the
// universal sorted-JCS scheme. There is no on-disk JSON to self-check, so the
// former typescript_pg_canonical_bodies_cid_self_consistent test is removed
// (it referenced typescript-canonical-bodies-pg.json, deleted in #1468).

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
        "blake3-512:74f655e44075901e6941049332618f96807b18bedf0381ad7edfb593a0ada8084d07ed2f848817574874e6ec52fa26ec493656e36ec4c66229d668b5bd9db181",
    );
}

#[test]
fn rust_canonical_bodies_cid_self_consistent() {
    let path = repo_root()
        .join("menagerie/rust-language-signature/specs/body-templates/rust-canonical-bodies.json");
    assert_self_consistent(
        path,
        "blake3-512:04276b79510b3175da55d856aef8d2dcf3f73676b4a03ff5d45bbb86ab2083f8887431090b5285627fd902ed7fc9ce6bb026ac44cbbc6f41669241158325bef2",
    );
}
