// SPDX-License-Identifier: Apache-2.0

use libsugar::substrate_default_cids::{CONTRACT_OBSERVATION_CONCEPT_CID, LOG_EMIT_CONCEPT_CID};

#[test]
fn contract_observation_concept_cid_is_self_identifying_blake3_512() {
    assert!(CONTRACT_OBSERVATION_CONCEPT_CID.starts_with("blake3-512:"));
    assert_eq!(
        CONTRACT_OBSERVATION_CONCEPT_CID.len(),
        "blake3-512:".len() + 128
    );
}

#[test]
fn log_emit_concept_cid_is_self_identifying_blake3_512() {
    assert!(LOG_EMIT_CONCEPT_CID.starts_with("blake3-512:"));
    assert_eq!(LOG_EMIT_CONCEPT_CID.len(), "blake3-512:".len() + 128);
}

#[test]
fn substrate_default_concept_cids_are_distinct() {
    assert_ne!(CONTRACT_OBSERVATION_CONCEPT_CID, LOG_EMIT_CONCEPT_CID);
}
