// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::{Path, PathBuf};

use libprovekit::ci::check_ci_body;
use serde::Deserialize;
use serde_json::Value as Json;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VectorManifest {
    catalog_version: String,
    catalog_cid: String,
    protocol: String,
    vectors: Vec<Vector>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Vector {
    name: String,
    capability: String,
    body: String,
    expected_cid: Option<String>,
    should_pass: bool,
    error_contains: Option<String>,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("canonical repo root")
}

#[test]
fn cicp_conformance_vectors_match_rust_reference_checker() {
    let root = repo_root();
    let vector_dir = root.join("protocol/conformance/cicp");
    let manifest_path = vector_dir.join("vectors.json");
    let manifest: VectorManifest = serde_json::from_slice(
        &fs::read(&manifest_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", manifest_path.display())),
    )
    .unwrap_or_else(|e| panic!("parse {}: {e}", manifest_path.display()));

    assert_eq!(manifest.catalog_version, "v1.6.2-2026-05-07");
    assert_eq!(
        manifest.catalog_cid,
        "blake3-512:52bdb2be4b381cec2aff95db7755c84184878b45cd91882d262114a1abd2dd513f9ef3b250fb87093316fd0fcb48e4b97e109d463e57df5bda6aac0b1c719a0f"
    );
    assert_eq!(manifest.protocol, "content-addressed-ci-protocol");
    assert!(
        manifest.vectors.len() >= 5,
        "corpus should cover every CICP body family plus one refusal vector"
    );

    for vector in manifest.vectors {
        assert!(
            vector.capability.starts_with("cicp."),
            "{} capability should be namespaced to CICP",
            vector.name
        );
        let body_path = vector_dir.join(&vector.body);
        let body: Json = serde_json::from_slice(
            &fs::read(&body_path).unwrap_or_else(|e| panic!("read {}: {e}", body_path.display())),
        )
        .unwrap_or_else(|e| panic!("parse {}: {e}", body_path.display()));

        match check_ci_body(&body) {
            Ok(check) if vector.should_pass => {
                let expected = vector
                    .expected_cid
                    .as_ref()
                    .unwrap_or_else(|| panic!("{} missing expected_cid", vector.name));
                assert_eq!(
                    &check.cid, expected,
                    "{} CID drifted for {}",
                    vector.name, vector.body
                );
            }
            Ok(check) => panic!(
                "{} should have failed, but Rust accepted it as {} {}",
                vector.name, check.kind, check.cid
            ),
            Err(err) if !vector.should_pass => {
                let expected = vector.error_contains.as_ref().unwrap_or_else(|| {
                    panic!("{} failing vector missing error_contains", vector.name)
                });
                assert!(
                    err.to_string().contains(expected),
                    "{} error mismatch\n  got:  {}\n  want contains: {}",
                    vector.name,
                    err,
                    expected
                );
            }
            Err(err) => panic!("{} should have passed: {err}", vector.name),
        }
    }
}
