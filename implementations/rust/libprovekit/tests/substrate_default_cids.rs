// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use libprovekit::substrate_default_cids::{CONTRACT_OBSERVATION_CONCEPT_CID, LOG_EMIT_CONCEPT_CID};

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
fn contract_observation_concept_cid_is_pinned_to_catalog_row() {
    let cids = std::fs::read_to_string(repo_root().join("menagerie/concept-shapes/cids.tsv"))
        .expect("read cids.tsv");
    let expected =
        format!("shape\tconcept:contract-observation\t{CONTRACT_OBSERVATION_CONCEPT_CID}\t");
    assert!(
        cids.lines().any(|line| line.starts_with(&expected)),
        "cids.tsv must carry the pinned concept:contract-observation CID"
    );
}

#[test]
fn log_emit_concept_cid_is_pinned_to_catalog_row() {
    let cids = std::fs::read_to_string(repo_root().join("menagerie/concept-shapes/cids.tsv"))
        .expect("read cids.tsv");
    let expected = format!("shape\tconcept:log-emit\t{LOG_EMIT_CONCEPT_CID}\t");
    assert!(
        cids.lines().any(|line| line.starts_with(&expected)),
        "cids.tsv must carry the pinned concept:log-emit CID"
    );
}
