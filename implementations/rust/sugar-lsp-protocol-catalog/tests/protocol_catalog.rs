// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::PathBuf;

use sugar_lsp_protocol_catalog::{
    embedded_protocol_catalog_cid, protocol_catalog_cid_from_repo,
    EXPECTED_LSP_PROTOCOL_CATALOG_CID, LSP_PROTOCOL_CATALOG_REPO_PATH,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("repo root")
}

#[test]
fn lsp_protocol_catalog_cid_is_pinned_and_deterministic() {
    let repo = repo_root();
    let first = protocol_catalog_cid_from_repo(&repo).expect("first CID");
    let second = protocol_catalog_cid_from_repo(&repo).expect("second CID");
    let embedded = embedded_protocol_catalog_cid().expect("embedded CID");

    println!("protocol_catalog_cid={first}");

    assert_eq!(first, second, "catalog CID must be deterministic");
    assert_eq!(
        first, embedded,
        "embedded catalog bytes and repo catalog file must agree"
    );
    assert_eq!(
        first, EXPECTED_LSP_PROTOCOL_CATALOG_CID,
        "pinned CID must match the canonical LSP protocol catalog"
    );
}

#[test]
fn lsp_protocol_catalog_declares_shared_surface() {
    let catalog_path = repo_root().join(LSP_PROTOCOL_CATALOG_REPO_PATH);
    let bytes = fs::read(&catalog_path)
        .unwrap_or_else(|err| panic!("read {}: {err}", catalog_path.display()));
    let json: serde_json::Value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|err| panic!("parse {}: {err}", catalog_path.display()));

    assert_eq!(json["kind"].as_str(), Some("provekit-lsp-protocol-catalog"));
    assert_eq!(json["schemaVersion"].as_str(), Some("1"));
    assert_eq!(
        json["protocol"]["id"].as_str(),
        Some("provekit-lsp-shared/1")
    );
    assert_eq!(json["protocol"]["version"].as_str(), Some("1"));

    let methods = json["methods"]
        .as_array()
        .expect("methods must be an array")
        .iter()
        .filter_map(|method| method["name"].as_str())
        .collect::<Vec<_>>();
    assert_eq!(methods, ["initialize", "analyzeDocument", "shutdown"]);

    let diagnostic_codes = json["diagnosticCodes"]
        .as_array()
        .expect("diagnosticCodes must be an array")
        .iter()
        .filter_map(|code| code["code"].as_str())
        .collect::<Vec<_>>();
    assert!(diagnostic_codes.contains(&"provekit.lsp.implication_failed"));
    assert!(diagnostic_codes.contains(&"provekit.lsp.lift_gap"));
    assert!(diagnostic_codes
        .iter()
        .all(|code| code.starts_with("provekit.lsp.")));

    let status_axes = json["statusAxes"]
        .as_array()
        .expect("statusAxes must be an array")
        .iter()
        .filter_map(|axis| axis.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        status_axes,
        ["lift", "materialize", "emit", "check", "prove"]
    );

    let range = &json["analysisResult"]["rangeContract"];
    assert_eq!(range["lineBase"].as_i64(), Some(1));
    assert_eq!(range["columnBase"].as_i64(), Some(0));
    assert_eq!(
        range["fields"]
            .as_array()
            .expect("range fields")
            .iter()
            .filter_map(|field| field.as_str())
            .collect::<Vec<_>>(),
        ["start_line", "start_col", "end_line", "end_col"]
    );
}

#[test]
fn lsp_protocol_catalog_cid_is_not_the_version_string_hash() {
    let version_string_cid = sugar_canonicalizer::blake3_512_of(b"provekit-lsp-shared/1");
    assert_ne!(
        EXPECTED_LSP_PROTOCOL_CATALOG_CID, version_string_cid,
        "protocol_catalog_cid must hash the catalog document, not the protocol version string"
    );
}
