// SPDX-License-Identifier: Apache-2.0
//
// Witness ingestion: convert the raw contents that FORM a witness into a
// canonical WitnessMemento.
//
// A witness is raw external attestation -- the human-readable dump of a JUnit
// suite run, the JSON a vitest runner emits, a logger that printed `2+2=4`, a
// signature on a page. Heterogeneous evidence that something was observed to
// hold. This module is the single ingestion converter: contents-that-form-a-
// witness in, a canonical WitnessMemento out.
//
// The CID addresses WHAT is attested -- the observation (subject, fixture,
// measurements, outcome). The substrate does not dictate HOW it is attested:
// `signed_by` + `signature` are the attestation layer, excluded from the CID
// preimage. The CID is blake3-512 of the JCS of the observation; the signature
// (ed25519 here, but the mechanism is the attestor's, not the substrate's) is
// over the same bytes. `observed_at` is stamped at ingestion.

use provekit_canonicalizer::blake3_512_of;
use provekit_ir_types::WitnessMemento;
use provekit_proof_envelope::{ed25519_pubkey_string, ed25519_sign_string};
use serde_json::{json, Value as Json};

/// The raw facts that constitute a witness: WHAT was observed, with the
/// evidence verbatim. `measurements` carries the raw artifact unchanged (a
/// JUnit run's text, a vitest JSON object, a log line, a signature) -- no schema
/// is imposed on the observation.
pub struct WitnessContents {
    /// CID of the contract/property this witness attests to.
    pub witness_for: String,
    /// The concept / op / subject observed.
    pub subject: String,
    /// CID of the fixture / input state the observation was made under.
    pub fixture_state_cid: String,
    /// Observations folded into this witness (1 for a single run).
    pub sample_count: u64,
    /// The raw witness artifact, verbatim (any shape).
    pub measurements: Json,
    /// When the observation was made (RFC3339), as reported by the observer/
    /// runner. Part of the observation, not fabricated by the converter: the
    /// same observation observed at the same time is the same witness.
    pub observed_at: String,
    /// Observed result: "pass" | "fail" | "inconclusive".
    pub outcome: String,
}

/// Convert raw witness contents into a canonical, content-addressed, signed
/// WitnessMemento. This is THE witness converter -- every producer (a solver
/// discharge, an emitted-test run) reduces its observation to `WitnessContents`
/// and calls this. The minted CID is verified against `recompute_cid` so a
/// witness can never carry a CID divorced from its own content.
pub fn ingest_witness(
    contents: &WitnessContents,
    signer_seed: &[u8; 32],
) -> Result<WitnessMemento, String> {
    let signed_by = ed25519_pubkey_string(signer_seed);

    // Build the witness, then compute its CID exactly as validate() will: via
    // recompute_cid, which addresses the observation and excludes the
    // attestation layer (cid / signed_by / signature). Deriving the CID through
    // the same function that checks it makes cid == recompute_cid true by
    // construction -- no hand-rolled preimage that can drift from the canonical
    // serialization.
    let mut witness = WitnessMemento {
        kind: "witness".to_string(),
        schema_version: "1".to_string(),
        witness_for: contents.witness_for.clone(),
        subject: contents.subject.clone(),
        fixture_state_cid: contents.fixture_state_cid.clone(),
        observed_at: contents.observed_at.clone(),
        sample_count: contents.sample_count,
        measurements: contents.measurements.clone(),
        outcome: contents.outcome.clone(),
        signed_by: Some(signed_by),
        signature: None,
        cid: String::new(),
    };
    let cid = witness.recompute_cid().map_err(|e| e.to_string())?;

    // HOW it is attested: ed25519 over the address (the WHAT). The mechanism is
    // the attestor's, layered over the CID, not baked into it.
    let signature = ed25519_sign_string(signer_seed, cid.as_bytes());
    witness.cid = cid;
    witness.signature = Some(signature);

    // cid == recompute_cid by construction; validate() also checks field shape.
    witness.validate().map_err(|e| e.to_string())?;
    Ok(witness)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ingests_a_raw_test_output_into_a_valid_witness() {
        // A vitest-runner-style raw artifact: heterogeneous JSON, verbatim.
        let contents = WitnessContents {
            witness_for: "blake3-512:contractcid".to_string(),
            subject: "concept:adds".to_string(),
            fixture_state_cid: "blake3-512:fixturecid".to_string(),
            sample_count: 1,
            measurements: json!({
                "runner": "vitest",
                "stdout": "2+2=4",
                "assertions": [{"name": "adds", "passed": true}]
            }),
            observed_at: "2026-05-27T00:00:00.000Z".to_string(),
            outcome: "pass".to_string(),
        };
        let witness = ingest_witness(&contents, &[7u8; 32]).expect("ingest");
        // The minted CID addresses the WHAT and matches recompute (which
        // excludes the attestation layer).
        assert_eq!(witness.cid, witness.recompute_cid().expect("recompute"));
        assert!(witness.signed_by.is_some());
        assert!(witness.signature.is_some());
        assert_eq!(witness.outcome, "pass");
        // The raw artifact is carried verbatim.
        assert_eq!(witness.measurements["stdout"], json!("2+2=4"));

        // Same observation, different signer key -> SAME CID (same WHAT),
        // different attestation. The substrate addresses what, not how.
        let other = ingest_witness(&contents, &[9u8; 32]).expect("ingest 2");
        assert_eq!(
            witness.cid, other.cid,
            "CID is over the observation; the signer is not part of it"
        );
        assert_ne!(
            witness.signature, other.signature,
            "different key -> different attestation"
        );
    }
}
