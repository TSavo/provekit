// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::canonical::json_cid;
use crate::canonical::{is_blake3_512_cid, serializable_cid};
use crate::{ProvekitError, Result};
use serde_json::Value as Json;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CINondeterminismMode {
    Forbidden,
    Declared,
    Unrestricted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CINondeterminism {
    pub network: CINondeterminismMode,
    pub clock: CINondeterminismMode,
    pub secrets: CINondeterminismMode,
    pub randomness: CINondeterminismMode,
}

impl Default for CINondeterminism {
    fn default() -> Self {
        Self {
            network: CINondeterminismMode::Forbidden,
            clock: CINondeterminismMode::Forbidden,
            secrets: CINondeterminismMode::Forbidden,
            randomness: CINondeterminismMode::Forbidden,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CIBlastRadiusInput {
    pub job_key: String,
    pub subject_kind: String,
    pub subject: String,
    pub protocol_catalog_cid: String,
    pub job_definition_cid: String,
    pub command_cid: String,
    pub runner_identity_cid: String,
    pub toolchain_cids: Vec<String>,
    pub source_closure_cid: String,
    pub lockfile_cids: Vec<String>,
    pub generated_input_cids: Vec<String>,
    pub fixture_cids: Vec<String>,
    pub relevant_spec_cids: Vec<String>,
    pub policy_cid: String,
    pub nondeterminism: CINondeterminism,
    pub additional_input_cids: Vec<String>,
}

impl CIBlastRadiusInput {
    pub fn build(self) -> Result<CIBlastRadius> {
        let mut input_cids = cid_set();
        insert_required(&mut input_cids, &self.protocol_catalog_cid);
        insert_required(&mut input_cids, &self.job_definition_cid);
        insert_required(&mut input_cids, &self.command_cid);
        insert_required(&mut input_cids, &self.runner_identity_cid);
        insert_many(&mut input_cids, &self.toolchain_cids);
        insert_required(&mut input_cids, &self.source_closure_cid);
        insert_many(&mut input_cids, &self.lockfile_cids);
        insert_many(&mut input_cids, &self.generated_input_cids);
        insert_many(&mut input_cids, &self.fixture_cids);
        insert_many(&mut input_cids, &self.relevant_spec_cids);
        insert_required(&mut input_cids, &self.policy_cid);
        insert_many(&mut input_cids, &self.additional_input_cids);

        let manifest = CIBlastRadius {
            kind: "CIBlastRadius".into(),
            schema_version: "1".into(),
            job_key: self.job_key,
            subject_kind: self.subject_kind,
            subject: self.subject,
            protocol_catalog_cid: self.protocol_catalog_cid,
            job_definition_cid: self.job_definition_cid,
            command_cid: self.command_cid,
            runner_identity_cid: self.runner_identity_cid,
            toolchain_cids: sorted_unique(self.toolchain_cids),
            source_closure_cid: self.source_closure_cid,
            lockfile_cids: sorted_unique(self.lockfile_cids),
            generated_input_cids: sorted_unique(self.generated_input_cids),
            fixture_cids: sorted_unique(self.fixture_cids),
            relevant_spec_cids: sorted_unique(self.relevant_spec_cids),
            policy_cid: self.policy_cid,
            nondeterminism: self.nondeterminism,
            input_cids: input_cids.into_iter().collect(),
        };
        manifest.validate()?;
        Ok(manifest)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct CIBlastRadius {
    pub kind: String,
    pub schema_version: String,
    pub job_key: String,
    pub subject_kind: String,
    pub subject: String,
    pub protocol_catalog_cid: String,
    pub job_definition_cid: String,
    pub command_cid: String,
    pub runner_identity_cid: String,
    pub toolchain_cids: Vec<String>,
    pub source_closure_cid: String,
    pub lockfile_cids: Vec<String>,
    pub generated_input_cids: Vec<String>,
    pub fixture_cids: Vec<String>,
    pub relevant_spec_cids: Vec<String>,
    pub policy_cid: String,
    pub nondeterminism: CINondeterminism,
    pub input_cids: Vec<String>,
}

impl CIBlastRadius {
    pub fn cid(&self) -> Result<String> {
        serializable_cid(self)
    }

    pub fn validate(&self) -> Result<()> {
        require_equal("kind", &self.kind, "CIBlastRadius")?;
        require_equal("schemaVersion", &self.schema_version, "1")?;
        require_nonempty("jobKey", &self.job_key)?;
        require_one_of(
            "subjectKind",
            &self.subject_kind,
            &["kit", "workflow", "protocol", "proof-bundle", "artifact"],
        )?;
        require_nonempty("subject", &self.subject)?;

        let mut required = cid_set();
        require_cid_field(
            "protocolCatalogCid",
            &self.protocol_catalog_cid,
            &mut required,
        )?;
        require_cid_field("jobDefinitionCid", &self.job_definition_cid, &mut required)?;
        require_cid_field("commandCid", &self.command_cid, &mut required)?;
        require_cid_field(
            "runnerIdentityCid",
            &self.runner_identity_cid,
            &mut required,
        )?;
        require_cid_vec("toolchainCids", &self.toolchain_cids, &mut required)?;
        require_cid_field("sourceClosureCid", &self.source_closure_cid, &mut required)?;
        require_cid_vec("lockfileCids", &self.lockfile_cids, &mut required)?;
        require_cid_vec(
            "generatedInputCids",
            &self.generated_input_cids,
            &mut required,
        )?;
        require_cid_vec("fixtureCids", &self.fixture_cids, &mut required)?;
        require_cid_vec("relevantSpecCids", &self.relevant_spec_cids, &mut required)?;
        require_cid_field("policyCid", &self.policy_cid, &mut required)?;
        require_input_cids_close(&self.input_cids, &required)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CIJobResult {
    Pass,
    Fail,
    Flaky,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CIProducer {
    pub kind: String,
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone)]
pub struct CIJobResultInput {
    pub job_key: String,
    pub blast_radius_cid: String,
    pub result: CIJobResult,
    pub output_cid: String,
    pub log_cid: String,
    pub started_at: String,
    pub finished_at: String,
    pub runner_identity_cid: String,
    pub policy_cid: String,
    pub producer: CIProducer,
    pub additional_input_cids: Vec<String>,
}

impl CIJobResultInput {
    pub fn build(self) -> Result<CIJobResultBodyClaim> {
        let mut input_cids = cid_set();
        insert_required(&mut input_cids, &self.blast_radius_cid);
        insert_required(&mut input_cids, &self.output_cid);
        insert_required(&mut input_cids, &self.log_cid);
        insert_required(&mut input_cids, &self.runner_identity_cid);
        insert_required(&mut input_cids, &self.policy_cid);
        insert_many(&mut input_cids, &self.additional_input_cids);

        let claim = CIJobResultBodyClaim {
            kind: "CIJobResultBodyClaim".into(),
            schema_version: "1".into(),
            job_key: self.job_key,
            blast_radius_cid: self.blast_radius_cid,
            result: self.result,
            output_cid: self.output_cid,
            log_cid: self.log_cid,
            started_at: self.started_at,
            finished_at: self.finished_at,
            runner_identity_cid: self.runner_identity_cid,
            policy_cid: self.policy_cid,
            input_cids: input_cids.into_iter().collect(),
            producer: self.producer,
        };
        claim.validate()?;
        Ok(claim)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct CIJobResultBodyClaim {
    pub kind: String,
    pub schema_version: String,
    pub job_key: String,
    pub blast_radius_cid: String,
    pub result: CIJobResult,
    pub output_cid: String,
    pub log_cid: String,
    pub started_at: String,
    pub finished_at: String,
    pub runner_identity_cid: String,
    pub policy_cid: String,
    pub input_cids: Vec<String>,
    pub producer: CIProducer,
}

impl CIJobResultBodyClaim {
    pub fn cid(&self) -> Result<String> {
        serializable_cid(self)
    }

    pub fn validate(&self) -> Result<()> {
        require_equal("kind", &self.kind, "CIJobResultBodyClaim")?;
        require_equal("schemaVersion", &self.schema_version, "1")?;
        require_nonempty("jobKey", &self.job_key)?;
        require_nonempty("startedAt", &self.started_at)?;
        require_nonempty("finishedAt", &self.finished_at)?;
        require_producer(&self.producer)?;

        let mut required = cid_set();
        require_cid_field("blastRadiusCid", &self.blast_radius_cid, &mut required)?;
        require_cid_field("outputCid", &self.output_cid, &mut required)?;
        require_cid_field("logCid", &self.log_cid, &mut required)?;
        require_cid_field(
            "runnerIdentityCid",
            &self.runner_identity_cid,
            &mut required,
        )?;
        require_cid_field("policyCid", &self.policy_cid, &mut required)?;
        require_input_cids_close(&self.input_cids, &required)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CIReuseReason {
    IdenticalInputClosure,
    BridgedByEvolution,
}

#[derive(Debug, Clone)]
pub struct CIReuseInput {
    pub job_key: String,
    pub current_blast_radius_cid: String,
    pub previous_blast_radius_cid: String,
    pub previous_result_witness_cid: String,
    pub reuse_reason: CIReuseReason,
    pub bridge_witness_cids: Vec<String>,
    pub policy_cid: String,
    pub additional_input_cids: Vec<String>,
}

impl CIReuseInput {
    pub fn build(self) -> Result<CIReuseBodyClaim> {
        let mut input_cids = cid_set();
        insert_required(&mut input_cids, &self.current_blast_radius_cid);
        insert_required(&mut input_cids, &self.previous_blast_radius_cid);
        insert_required(&mut input_cids, &self.previous_result_witness_cid);
        insert_many(&mut input_cids, &self.bridge_witness_cids);
        insert_required(&mut input_cids, &self.policy_cid);
        insert_many(&mut input_cids, &self.additional_input_cids);

        let claim = CIReuseBodyClaim {
            kind: "CIReuseBodyClaim".into(),
            schema_version: "1".into(),
            job_key: self.job_key,
            current_blast_radius_cid: self.current_blast_radius_cid,
            previous_blast_radius_cid: self.previous_blast_radius_cid,
            previous_result_witness_cid: self.previous_result_witness_cid,
            reuse_reason: self.reuse_reason,
            bridge_witness_cids: sorted_unique(self.bridge_witness_cids),
            policy_cid: self.policy_cid,
            input_cids: input_cids.into_iter().collect(),
        };
        claim.validate()?;
        Ok(claim)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct CIReuseBodyClaim {
    pub kind: String,
    pub schema_version: String,
    pub job_key: String,
    pub current_blast_radius_cid: String,
    pub previous_blast_radius_cid: String,
    pub previous_result_witness_cid: String,
    pub reuse_reason: CIReuseReason,
    pub bridge_witness_cids: Vec<String>,
    pub policy_cid: String,
    pub input_cids: Vec<String>,
}

impl CIReuseBodyClaim {
    pub fn cid(&self) -> Result<String> {
        serializable_cid(self)
    }

    pub fn validate(&self) -> Result<()> {
        require_equal("kind", &self.kind, "CIReuseBodyClaim")?;
        require_equal("schemaVersion", &self.schema_version, "1")?;
        require_nonempty("jobKey", &self.job_key)?;

        match self.reuse_reason {
            CIReuseReason::IdenticalInputClosure => {
                if self.current_blast_radius_cid != self.previous_blast_radius_cid {
                    return Err(message(
                        "identical-input-closure reuse requires currentBlastRadiusCid to equal previousBlastRadiusCid",
                    ));
                }
            }
            CIReuseReason::BridgedByEvolution => {
                if self.bridge_witness_cids.is_empty() {
                    return Err(message(
                        "bridged reuse requires at least one bridge witness",
                    ));
                }
            }
        }

        let mut required = cid_set();
        require_cid_field(
            "currentBlastRadiusCid",
            &self.current_blast_radius_cid,
            &mut required,
        )?;
        require_cid_field(
            "previousBlastRadiusCid",
            &self.previous_blast_radius_cid,
            &mut required,
        )?;
        require_cid_field(
            "previousResultWitnessCid",
            &self.previous_result_witness_cid,
            &mut required,
        )?;
        require_cid_vec(
            "bridgeWitnessCids",
            &self.bridge_witness_cids,
            &mut required,
        )?;
        require_cid_field("policyCid", &self.policy_cid, &mut required)?;
        require_input_cids_close(&self.input_cids, &required)
    }
}

#[derive(Debug, Clone)]
pub struct CIImpactInput {
    pub base_state_cid: String,
    pub candidate_state_cid: String,
    pub protocol_evolution_witness_cids: Vec<String>,
    pub changed_blast_radius_cids: Vec<String>,
    pub unchanged_blast_radius_cids: Vec<String>,
    pub required_job_keys: Vec<String>,
    pub reusable_witness_cids: Vec<String>,
    pub refusal_cids: Vec<String>,
    pub policy_cid: String,
    pub additional_input_cids: Vec<String>,
}

impl CIImpactInput {
    pub fn build(self) -> Result<CIImpactBodyClaim> {
        let mut input_cids = cid_set();
        insert_required(&mut input_cids, &self.base_state_cid);
        insert_required(&mut input_cids, &self.candidate_state_cid);
        insert_many(&mut input_cids, &self.protocol_evolution_witness_cids);
        insert_many(&mut input_cids, &self.changed_blast_radius_cids);
        insert_many(&mut input_cids, &self.unchanged_blast_radius_cids);
        insert_many(&mut input_cids, &self.reusable_witness_cids);
        insert_many(&mut input_cids, &self.refusal_cids);
        insert_required(&mut input_cids, &self.policy_cid);
        insert_many(&mut input_cids, &self.additional_input_cids);

        let claim = CIImpactBodyClaim {
            kind: "CIImpactBodyClaim".into(),
            schema_version: "1".into(),
            base_state_cid: self.base_state_cid,
            candidate_state_cid: self.candidate_state_cid,
            protocol_evolution_witness_cids: sorted_unique(self.protocol_evolution_witness_cids),
            changed_blast_radius_cids: sorted_unique(self.changed_blast_radius_cids),
            unchanged_blast_radius_cids: sorted_unique(self.unchanged_blast_radius_cids),
            required_job_keys: sorted_unique(self.required_job_keys),
            reusable_witness_cids: sorted_unique(self.reusable_witness_cids),
            refusal_cids: sorted_unique(self.refusal_cids),
            policy_cid: self.policy_cid,
            input_cids: input_cids.into_iter().collect(),
        };
        claim.validate()?;
        Ok(claim)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct CIImpactBodyClaim {
    pub kind: String,
    pub schema_version: String,
    pub base_state_cid: String,
    pub candidate_state_cid: String,
    pub protocol_evolution_witness_cids: Vec<String>,
    pub changed_blast_radius_cids: Vec<String>,
    pub unchanged_blast_radius_cids: Vec<String>,
    pub required_job_keys: Vec<String>,
    pub reusable_witness_cids: Vec<String>,
    pub refusal_cids: Vec<String>,
    pub policy_cid: String,
    pub input_cids: Vec<String>,
}

impl CIImpactBodyClaim {
    pub fn cid(&self) -> Result<String> {
        serializable_cid(self)
    }

    pub fn validate(&self) -> Result<()> {
        require_equal("kind", &self.kind, "CIImpactBodyClaim")?;
        require_equal("schemaVersion", &self.schema_version, "1")?;
        require_nonempty_strings("requiredJobKeys", &self.required_job_keys)?;

        let mut required = cid_set();
        require_cid_field("baseStateCid", &self.base_state_cid, &mut required)?;
        require_cid_field(
            "candidateStateCid",
            &self.candidate_state_cid,
            &mut required,
        )?;
        require_cid_vec(
            "protocolEvolutionWitnessCids",
            &self.protocol_evolution_witness_cids,
            &mut required,
        )?;
        require_cid_vec(
            "changedBlastRadiusCids",
            &self.changed_blast_radius_cids,
            &mut required,
        )?;
        require_cid_vec(
            "unchangedBlastRadiusCids",
            &self.unchanged_blast_radius_cids,
            &mut required,
        )?;
        require_cid_vec(
            "reusableWitnessCids",
            &self.reusable_witness_cids,
            &mut required,
        )?;
        require_cid_vec("refusalCids", &self.refusal_cids, &mut required)?;
        require_cid_field("policyCid", &self.policy_cid, &mut required)?;
        require_input_cids_close(&self.input_cids, &required)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CIBodyCheck {
    pub kind: String,
    pub cid: String,
}

pub fn check_ci_body(value: &Json) -> Result<CIBodyCheck> {
    let kind = value
        .get("kind")
        .and_then(|v| v.as_str())
        .ok_or_else(|| message("CI body missing string field `kind`"))?;

    match kind {
        "CIBlastRadius" => {
            let body: CIBlastRadius = serde_json::from_value(value.clone())
                .map_err(|e| message(format!("parse CIBlastRadius: {e}")))?;
            body.validate()?;
        }
        "CIJobResultBodyClaim" => {
            let body: CIJobResultBodyClaim = serde_json::from_value(value.clone())
                .map_err(|e| message(format!("parse CIJobResultBodyClaim: {e}")))?;
            body.validate()?;
        }
        "CIReuseBodyClaim" => {
            let body: CIReuseBodyClaim = serde_json::from_value(value.clone())
                .map_err(|e| message(format!("parse CIReuseBodyClaim: {e}")))?;
            body.validate()?;
        }
        "CIImpactBodyClaim" => {
            let body: CIImpactBodyClaim = serde_json::from_value(value.clone())
                .map_err(|e| message(format!("parse CIImpactBodyClaim: {e}")))?;
            body.validate()?;
        }
        other => return Err(message(format!("unsupported CI body kind `{other}`"))),
    }

    Ok(CIBodyCheck {
        kind: kind.to_string(),
        cid: json_cid(value)?,
    })
}

fn cid_set() -> BTreeSet<String> {
    BTreeSet::new()
}

fn insert_required(out: &mut BTreeSet<String>, cid: &str) {
    out.insert(cid.to_string());
}

fn insert_many(out: &mut BTreeSet<String>, cids: &[String]) {
    out.extend(cids.iter().cloned());
}

fn sorted_unique(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn require_equal(field: &str, actual: &str, expected: &str) -> Result<()> {
    if actual == expected {
        Ok(())
    } else {
        Err(message(format!(
            "{field} must be `{expected}`, got `{actual}`"
        )))
    }
}

fn require_nonempty(field: &str, value: &str) -> Result<()> {
    if value.is_empty() {
        Err(message(format!("{field} must be non-empty")))
    } else {
        Ok(())
    }
}

fn require_nonempty_strings(field: &str, values: &[String]) -> Result<()> {
    for value in values {
        require_nonempty(field, value)?;
    }
    Ok(())
}

fn require_one_of(field: &str, actual: &str, allowed: &[&str]) -> Result<()> {
    if allowed.contains(&actual) {
        Ok(())
    } else {
        Err(message(format!(
            "{field} must be one of {}, got `{actual}`",
            allowed.join("|")
        )))
    }
}

fn require_cid_field(field: &str, cid: &str, required: &mut BTreeSet<String>) -> Result<()> {
    if !is_blake3_512_cid(cid) {
        return Err(message(format!("{field} is not a blake3-512 CID")));
    }
    required.insert(cid.to_string());
    Ok(())
}

fn require_cid_vec(field: &str, cids: &[String], required: &mut BTreeSet<String>) -> Result<()> {
    for cid in cids {
        if !is_blake3_512_cid(cid) {
            return Err(message(format!(
                "{field} contains non-blake3-512 CID `{cid}`"
            )));
        }
        required.insert(cid.clone());
    }
    Ok(())
}

fn require_input_cids_close(input_cids: &[String], required: &BTreeSet<String>) -> Result<()> {
    let mut input_set = BTreeSet::new();
    for cid in input_cids {
        if !is_blake3_512_cid(cid) {
            return Err(message(format!(
                "inputCids contains non-blake3-512 CID `{cid}`"
            )));
        }
        input_set.insert(cid.as_str());
    }
    for cid in required {
        if !input_set.contains(cid.as_str()) {
            return Err(message(format!("inputCids missing required CID `{cid}`")));
        }
    }
    Ok(())
}

fn require_producer(producer: &CIProducer) -> Result<()> {
    require_nonempty("producer.kind", &producer.kind)?;
    require_nonempty("producer.name", &producer.name)?;
    require_nonempty("producer.version", &producer.version)
}

fn message(message: impl Into<String>) -> ProvekitError {
    ProvekitError::Message(message.into())
}
