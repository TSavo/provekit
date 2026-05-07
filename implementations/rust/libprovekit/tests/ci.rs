// SPDX-License-Identifier: Apache-2.0

use libprovekit::canonical::{json_cid, json_jcs};
use libprovekit::ci::{
    CIBlastRadiusInput, CIImpactInput, CIJobResult, CIJobResultInput, CINondeterminism,
    CINondeterminismMode, CIProducer, CIReuseInput, CIReuseReason,
};

fn cid(ch: char) -> String {
    format!("blake3-512:{}", ch.to_string().repeat(128))
}

fn blast_input(protocol_catalog_cid: String) -> CIBlastRadiusInput {
    CIBlastRadiusInput {
        job_key: "provekit/conformance/rust".into(),
        subject_kind: "kit".into(),
        subject: "rust".into(),
        protocol_catalog_cid,
        job_definition_cid: cid('1'),
        command_cid: cid('2'),
        runner_identity_cid: cid('3'),
        toolchain_cids: vec![cid('4')],
        source_closure_cid: cid('5'),
        lockfile_cids: vec![cid('6')],
        generated_input_cids: vec![cid('7')],
        fixture_cids: vec![cid('8')],
        relevant_spec_cids: vec![cid('9')],
        policy_cid: cid('a'),
        nondeterminism: CINondeterminism {
            network: CINondeterminismMode::Forbidden,
            clock: CINondeterminismMode::Declared,
            secrets: CINondeterminismMode::Forbidden,
            randomness: CINondeterminismMode::Forbidden,
        },
        additional_input_cids: vec![cid('b')],
    }
}

#[test]
fn blast_radius_manifest_is_closed_and_protocol_catalog_drives_identity() {
    let first = blast_input(cid('c'))
        .build()
        .expect("build first blast radius");
    first.validate().expect("first blast radius validates");

    assert_eq!(first.kind, "CIBlastRadius");
    assert_eq!(first.schema_version, "1");
    assert!(first.input_cids.contains(&first.protocol_catalog_cid));
    assert!(first.input_cids.contains(&first.job_definition_cid));
    assert!(first.input_cids.contains(&first.command_cid));
    assert!(first.input_cids.contains(&first.runner_identity_cid));
    assert!(first.input_cids.contains(&first.source_closure_cid));
    assert!(first.input_cids.contains(&first.policy_cid));
    assert!(first.input_cids.contains(&cid('b')));

    let second = blast_input(cid('d'))
        .build()
        .expect("build second blast radius");
    assert_ne!(
        first.cid().expect("first cid"),
        second.cid().expect("second cid"),
        "changing only the protocol catalog CID must invalidate the blast-radius CID"
    );
}

#[test]
fn blast_radius_validation_rejects_open_input_closure() {
    let mut manifest = blast_input(cid('c')).build().expect("build blast radius");
    manifest
        .input_cids
        .retain(|cid| cid != &manifest.policy_cid);

    let err = manifest
        .validate()
        .expect_err("manifest missing a required input CID should fail closed");
    assert!(
        err.to_string().contains("inputCids missing required CID"),
        "{err}"
    );
}

#[test]
fn job_result_body_claim_closes_over_blast_radius_outputs_runner_and_policy() {
    let blast = blast_input(cid('c')).build().expect("build blast radius");
    let claim = CIJobResultInput {
        job_key: blast.job_key.clone(),
        blast_radius_cid: blast.cid().expect("blast cid"),
        result: CIJobResult::Pass,
        output_cid: cid('d'),
        log_cid: cid('e'),
        started_at: "2026-05-07T00:00:00Z".into(),
        finished_at: "2026-05-07T00:01:00Z".into(),
        runner_identity_cid: blast.runner_identity_cid.clone(),
        policy_cid: blast.policy_cid.clone(),
        producer: CIProducer {
            kind: "ci-runner".into(),
            name: "github-actions".into(),
            version: "test".into(),
        },
        additional_input_cids: vec![cid('f')],
    }
    .build()
    .expect("build job result");

    claim.validate().expect("job result validates");
    assert_eq!(claim.kind, "CIJobResultBodyClaim");
    assert_eq!(claim.result, CIJobResult::Pass);
    assert!(claim.input_cids.contains(&claim.blast_radius_cid));
    assert!(claim.input_cids.contains(&claim.output_cid));
    assert!(claim.input_cids.contains(&claim.log_cid));
    assert!(claim.input_cids.contains(&claim.runner_identity_cid));
    assert!(claim.input_cids.contains(&claim.policy_cid));
    assert!(claim.input_cids.contains(&cid('f')));
}

#[test]
fn reuse_body_claim_distinguishes_identical_lookup_from_bridged_reuse() {
    let previous = cid('1');
    let current = previous.clone();
    let identical = CIReuseInput {
        job_key: "provekit/conformance/rust".into(),
        current_blast_radius_cid: current,
        previous_blast_radius_cid: previous,
        previous_result_witness_cid: cid('2'),
        reuse_reason: CIReuseReason::IdenticalInputClosure,
        bridge_witness_cids: vec![],
        policy_cid: cid('3'),
        additional_input_cids: vec![],
    }
    .build()
    .expect("build identical reuse");
    identical.validate().expect("identical reuse validates");

    let bridged = CIReuseInput {
        job_key: "provekit/conformance/java".into(),
        current_blast_radius_cid: cid('4'),
        previous_blast_radius_cid: cid('5'),
        previous_result_witness_cid: cid('6'),
        reuse_reason: CIReuseReason::BridgedByEvolution,
        bridge_witness_cids: vec![cid('7')],
        policy_cid: cid('8'),
        additional_input_cids: vec![],
    }
    .build()
    .expect("build bridged reuse");
    bridged.validate().expect("bridged reuse validates");

    let err = CIReuseInput {
        bridge_witness_cids: vec![],
        ..CIReuseInput {
            job_key: "provekit/conformance/java".into(),
            current_blast_radius_cid: cid('4'),
            previous_blast_radius_cid: cid('5'),
            previous_result_witness_cid: cid('6'),
            reuse_reason: CIReuseReason::BridgedByEvolution,
            bridge_witness_cids: vec![cid('7')],
            policy_cid: cid('8'),
            additional_input_cids: vec![],
        }
    }
    .build()
    .expect_err("bridged reuse without bridge witnesses must fail closed");
    assert!(
        err.to_string()
            .contains("bridged reuse requires at least one bridge witness"),
        "{err}"
    );
}

#[test]
fn canonical_helpers_emit_stable_cids_for_any_json_body() {
    let manifest = blast_input(cid('c')).build().expect("build blast radius");
    let value = serde_json::to_value(&manifest).expect("serialize manifest");
    let jcs = json_jcs(&value).expect("canonical JSON");

    assert!(jcs.starts_with("{\"commandCid\""));
    assert_eq!(
        json_cid(&value).expect("json cid"),
        manifest.cid().expect("manifest cid")
    );
}

#[test]
fn impact_body_claim_names_changed_reusable_and_required_jobs() {
    let impact = CIImpactInput {
        base_state_cid: cid('1'),
        candidate_state_cid: cid('2'),
        protocol_evolution_witness_cids: vec![cid('3')],
        changed_blast_radius_cids: vec![cid('4')],
        unchanged_blast_radius_cids: vec![cid('5')],
        required_job_keys: vec!["provekit/conformance/rust".into()],
        reusable_witness_cids: vec![cid('6')],
        refusal_cids: vec![],
        policy_cid: cid('7'),
        additional_input_cids: vec![cid('8')],
    }
    .build()
    .expect("build impact");

    impact.validate().expect("impact validates");
    assert_eq!(impact.kind, "CIImpactBodyClaim");
    assert!(impact.input_cids.contains(&impact.base_state_cid));
    assert!(impact.input_cids.contains(&impact.candidate_state_cid));
    assert!(impact.input_cids.contains(&impact.policy_cid));
    assert!(impact.input_cids.contains(&cid('3')));
    assert!(impact.input_cids.contains(&cid('8')));
}
