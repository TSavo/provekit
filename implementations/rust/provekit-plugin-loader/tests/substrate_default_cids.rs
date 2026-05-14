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

const HTTP_REQUEST_CID: &str = "blake3-512:784dab96537ebae452cba5fdbcf88e07395d5e0634099055008d819f21d0fb51930fc29877afda069cdf0c1ec893fba5de47b025717fd024919c687381baee43";
const HTTP_RESPONSE_CID: &str = "blake3-512:38a31226e5e2f593fa12b1e7a2b18d9f7755301ce537115b34ac486aedcc479ca599327dbea7de0e0cee0d035b831ad4933436c2b7c8c84d4f4694dc42d161f5";

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

fn loss_dimensions_for(repo: &std::path::Path, spec: &str) -> Vec<String> {
    let path = repo.join(spec);
    let raw =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let root: serde_json::Value =
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
    let dims = root
        .get("loss_dimensions")
        .and_then(|v| v.as_array())
        .unwrap_or_else(|| panic!("{}: missing loss_dimensions array", path.display()));
    dims.iter()
        .map(|v| {
            v.as_str()
                .unwrap_or_else(|| panic!("{}: loss dimension is not a string", path.display()))
                .to_string()
        })
        .collect()
}

fn assert_loss_record_keys(
    path: &std::path::Path,
    binding: &serde_json::Value,
    expected: &[String],
) {
    let loss_record = binding
        .get("loss_record")
        .and_then(|v| v.as_object())
        .unwrap_or_else(|| panic!("{}: binding missing loss_record object", path.display()));
    let mut actual: Vec<String> = loss_record.keys().cloned().collect();
    actual.sort();
    let mut expected = expected.to_vec();
    expected.sort();
    assert_eq!(
        actual,
        expected,
        "{}: loss_record dimensions do not match concept loss_dimensions",
        path.display()
    );
    for (dimension, formula) in loss_record {
        assert!(
            formula.get("kind").is_some() && formula.get("name").is_some(),
            "{}: loss_record.{dimension} must be an IrFormula object with kind and name",
            path.display()
        );
    }
}

fn assert_http_sugar_cell(
    repo: &std::path::Path,
    rel: &str,
    target_language: &str,
    sugar_name: &str,
    pinned_cid: &str,
    request_dims: &[String],
    response_dims: &[String],
) {
    let path = repo.join(rel);
    let raw =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    assert!(
        !raw.contains('\u{2013}') && !raw.contains('\u{2014}'),
        "{}: HTTP sugar cells must not contain en or em dashes",
        path.display()
    );
    let root: serde_json::Value =
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
    let header = root
        .get("header")
        .and_then(|v| v.as_object())
        .unwrap_or_else(|| panic!("{}: missing header object", path.display()));
    assert_eq!(header.get("kind").and_then(|v| v.as_str()), Some("sugar"));
    assert_eq!(
        header.get("protocol_versions"),
        Some(&serde_json::json!(["pep/1.7.0"]))
    );
    let declared = declared_cid_for(&path);
    assert_eq!(declared, mint_cid_for(&path));
    assert_eq!(
        declared,
        pinned_cid,
        "{}: cid drifted from the pin in this test file",
        path.display()
    );

    let content = header
        .get("content")
        .and_then(|v| v.as_object())
        .unwrap_or_else(|| panic!("{}: missing header.content object", path.display()));
    assert_eq!(
        content.get("target_language").and_then(|v| v.as_str()),
        Some(target_language)
    );
    assert_eq!(
        content.get("sugar_name").and_then(|v| v.as_str()),
        Some(sugar_name)
    );

    let bindings = content
        .get("concept_bindings")
        .and_then(|v| v.as_array())
        .unwrap_or_else(|| panic!("{}: missing concept_bindings array", path.display()));
    assert_eq!(bindings.len(), 2, "{}: expected request and response bindings", path.display());

    let request = bindings
        .iter()
        .find(|binding| binding.get("concept").and_then(|v| v.as_str()) == Some("concept:http-request"))
        .unwrap_or_else(|| panic!("{}: missing concept:http-request binding", path.display()));
    assert_eq!(
        request.get("concept_cid").and_then(|v| v.as_str()),
        Some(HTTP_REQUEST_CID)
    );
    assert_loss_record_keys(&path, request, request_dims);

    let response = bindings
        .iter()
        .find(|binding| binding.get("concept").and_then(|v| v.as_str()) == Some("concept:http-response"))
        .unwrap_or_else(|| panic!("{}: missing concept:http-response binding", path.display()));
    assert_eq!(
        response.get("concept_cid").and_then(|v| v.as_str()),
        Some(HTTP_RESPONSE_CID)
    );
    assert_loss_record_keys(&path, response, response_dims);
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
        "blake3-512:e9c5dd414a182211d5b85e3f2052d491e85c90db5ed29fb1145c7d1ad7f4e34f71de80c12a210202c6bd7efe26639da0e4e1c446beca786245d4a3dc6708204b",
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
        "blake3-512:fb35f2196ebca55a964e1829a5f0f2842a0d3de53406fcca504e93b0aa1e55c09b7735740c382564817d279146479d8e4a3963377ee42878c6bb979da44a5cc7",
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
        "blake3-512:8ef55920ab677d4fb44260ed9993295585fd76cc7df67c28673424a2cf49c2f76164e4cc9f1f2739a063ed2f27cb11cd34024c54a6cdfa9af2828e9946d1ff0e",
    );
}

#[test]
fn http_sugar_cells_cover_all_concept_loss_dimensions() {
    let repo = repo_root();
    let request_dims = loss_dimensions_for(
        &repo,
        "menagerie/concept-shapes/specs/http-request_shape.spec.json",
    );
    let response_dims = loss_dimensions_for(
        &repo,
        "menagerie/concept-shapes/specs/http-response_shape.spec.json",
    );
    let cells = [
        (
            "menagerie/c-language-signature/specs/sugar/http-libcurl.json",
            "c",
            "http-libcurl",
            "blake3-512:61f7e58b2c310920b420c9f16519589b8ed39d652e67d025502ca291492a999687deaaad3e8f8555e7d50dd74e42a9f4d4f57469cefd798d983a9f3b1106dc96",
        ),
        (
            "menagerie/java-language-signature/specs/sugar/http-java-net-http.json",
            "java",
            "http-java-net-http",
            "blake3-512:2475eea5ffcc2aa65479f58f0a4fdffbca81eb831d5540ef6eee136f35b7c4709874486238e1407d5b3c95f6bbb3e4a006c0a861a108a77a03c9481974e1f0b0",
        ),
        (
            "menagerie/python-language-signature/specs/sugar/http-aiohttp.json",
            "python",
            "http-aiohttp",
            "blake3-512:6cad8767abf21ed558534b861a0ab6b2263076c90a934e58c652376b313999e9a3ba5d7cf231bcc5a7ed0a4395edbf168a7c1cbd64da6e1313a7712256362245",
        ),
        (
            "menagerie/python-language-signature/specs/sugar/http-httpx.json",
            "python",
            "http-httpx",
            "blake3-512:2f6d55f03eff91fbcde3261b0acd4d60bfebde167b9ebddeffd4641221a06cfd76860655ced7aa265607661383aebe80916d6df2c086c30d45e3b38815a71eb1",
        ),
        (
            "menagerie/python-language-signature/specs/sugar/http-requests.json",
            "python",
            "http-requests",
            "blake3-512:0857d3bee431c737e36a2e09c4513eb45e3a4b31b0845eb05d4e34c3882b9fc1ac59f64c406bb3fd7286f8acb4898c9e52f6b70f8a764dae978ccac2bf616d83",
        ),
        (
            "menagerie/python-language-signature/specs/sugar/http-urllib-request.json",
            "python",
            "http-urllib-request",
            "blake3-512:3899a8ce55434a90c34207adc0af4455ffeefc16115eb584f4c15eb61525097a8a8ce1bc343cab708f22d4227135c493241c3bb1aee729d04bc8e976e448da32",
        ),
    ];
    for (rel, target_language, sugar_name, pinned_cid) in cells {
        assert_http_sugar_cell(
            &repo,
            rel,
            target_language,
            sugar_name,
            pinned_cid,
            &request_dims,
            &response_dims,
        );
    }
}
