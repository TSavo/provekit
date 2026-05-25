// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use provekit_ir_types::ExamManifestMemento;

const EXPECTED_EXAM_MANIFEST_CID: &str = "blake3-512:0e0dc132f3e8bf58da065d7fc237e85c225c5c87fbc690a19a42d594e9b1e46ed78e8f0f5a855fa1b75581745f588a4737adb17bc59e9a72b3bb9f6bcb665dd0";
const EXPECTED_V1_1_EXAM_MANIFEST_CID: &str = "blake3-512:b38426ba10ee3a6c28e9e32cae9aa65cfb5b750950464d1e67e9d669956bd40288d25c247d0ec2d638fd63e2d235d944f419055c0374c78488b4be98da040451";

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

#[test]
fn v1_1_manifest_loads_and_finds_question_cid_by_kind_concept_and_language() {
    let manifest = libprovekit::exam_manifest::load_default_exam_manifest()
        .expect("default v1.1 exam manifest loads");
    assert_eq!(manifest.header.cid, EXPECTED_V1_1_EXAM_MANIFEST_CID);

    let add_rust = libprovekit::exam_manifest::exam_question_cid_for(
        &manifest,
        "morphism",
        "concept:add",
        "rust",
    )
    .expect("lookup add/rust")
    .expect("add/rust exists");
    let sub_rust = libprovekit::exam_manifest::exam_question_cid_for(
        &manifest,
        "morphism",
        "concept:sub",
        "rust",
    )
    .expect("lookup sub/rust")
    .expect("sub/rust exists");
    let add_python = libprovekit::exam_manifest::exam_question_cid_for(
        &manifest,
        "morphism",
        "concept:add",
        "python",
    )
    .expect("lookup add/python")
    .expect("add/python exists");

    let expected = manifest
        .header
        .content
        .questions
        .iter()
        .find(|question| {
            question.kind.as_str() == "morphism"
                && question.concept == "concept:add"
                && question
                    .parameters
                    .get("from_language")
                    .and_then(|v| v.as_str())
                    == Some("rust")
        })
        .expect("fixture has add/rust")
        .to_owned();
    assert_eq!(
        add_rust,
        libprovekit::exam_manifest::exam_question_cid(&expected).expect("question cid computes")
    );
    assert_ne!(add_rust, sub_rust);
    assert_ne!(add_rust, add_python);
}
