// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use provekit_ir_types::ExamManifestMemento;

const EXPECTED_EXAM_MANIFEST_CID: &str = "blake3-512:0e0dc132f3e8bf58da065d7fc237e85c225c5c87fbc690a19a42d594e9b1e46ed78e8f0f5a855fa1b75581745f588a4737adb17bc59e9a72b3bb9f6bcb665dd0";

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

#[test]
fn exam_manifest_fixture_loads_and_pins_cid() {
    let fixture = std::fs::read_to_string(
        repo_root()
            .join("implementations/rust/libprovekit/tests/fixtures/exam_manifest/v1.example.json"),
    )
    .expect("read exam manifest fixture");
    let manifest: ExamManifestMemento =
        serde_json::from_str(&fixture).expect("parse exam manifest fixture");

    manifest.validate().expect("fixture validates");
    assert_eq!(manifest.header.cid, EXPECTED_EXAM_MANIFEST_CID);
    assert_eq!(
        manifest.recompute_header_cid().expect("recompute cid"),
        EXPECTED_EXAM_MANIFEST_CID
    );
}
