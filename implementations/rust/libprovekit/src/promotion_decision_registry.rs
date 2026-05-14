// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};

use provekit_ir_types::{MigrateReceiptEnvelope, PromotionDecisionMemento, PromotionResult};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PromotionDecisionKey {
    pub promoted_op: String,
    pub fixture_cid: String,
}

impl PromotionDecisionKey {
    pub fn new(promoted_op: impl Into<String>, fixture_cid: impl Into<String>) -> Self {
        Self {
            promoted_op: promoted_op.into(),
            fixture_cid: fixture_cid.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PromotionStatus {
    pub key: PromotionDecisionKey,
    pub decision_cids: Vec<String>,
    pub decision_policy_cids: Vec<String>,
    pub consensus_vector: Value,
    pub witnesses_consulted: u64,
}

#[derive(Debug, Clone, Default)]
pub struct PromotionDecisionRegistry {
    decisions: BTreeMap<String, PromotionDecisionMemento>,
    by_key: BTreeMap<PromotionDecisionKey, BTreeSet<String>>,
    summaries: BTreeMap<String, PromotionSummary>,
}

#[derive(Debug, Clone)]
struct PromotionSummary {
    decision_policy_cid: String,
    consensus_vector: Value,
    witnesses_consulted: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConsensusPolicy {
    #[serde(default)]
    pub cid: Option<String>,
    #[serde(default)]
    pub thresholds: Vec<ConsensusThreshold>,
    #[serde(default)]
    pub allow_failures: bool,
    #[serde(default)]
    pub require_loss_dim_coverage: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConsensusThreshold {
    pub axis: String,
    pub predicate: String,
}

#[derive(Debug, thiserror::Error)]
pub enum PromotionDecisionRegistryError {
    #[error("promotion-decision-registry: parse failed as PromotionDecisionMemento ({decision}) and MigrateReceiptEnvelope ({receipt})")]
    Parse { decision: String, receipt: String },
    #[error("promotion-decision-registry: invalid decision: {0}")]
    Invalid(String),
    #[error(
        "promotion-decision-registry: header.cid mismatch: claimed {claimed}, recomputed {actual}"
    )]
    HeaderCidMismatch { claimed: String, actual: String },
    #[error("promotion-decision-registry: parse consensus policy: {0}")]
    PolicyParse(String),
    #[error("promotion-decision-registry: invalid consensus policy: {0}")]
    InvalidPolicy(String),
}

impl ConsensusPolicy {
    pub fn from_json_str(text: &str) -> Result<Self, PromotionDecisionRegistryError> {
        let policy: Self = serde_json::from_str(text)
            .map_err(|err| PromotionDecisionRegistryError::PolicyParse(err.to_string()))?;
        if policy.thresholds.is_empty() {
            return Err(PromotionDecisionRegistryError::InvalidPolicy(
                "thresholds must be non-empty".to_string(),
            ));
        }
        for threshold in &policy.thresholds {
            parse_predicate(&threshold.predicate).map_err(|err| {
                PromotionDecisionRegistryError::InvalidPolicy(format!(
                    "threshold axis `{}` predicate `{}`: {err}",
                    threshold.axis, threshold.predicate
                ))
            })?;
        }
        if let Some(mode) = policy.require_loss_dim_coverage.as_deref() {
            if mode != "all-named" {
                return Err(PromotionDecisionRegistryError::InvalidPolicy(format!(
                    "unsupported require_loss_dim_coverage `{mode}`"
                )));
            }
        }
        Ok(policy)
    }

    pub fn admits(&self, status: &PromotionStatus) -> Result<(), String> {
        if !self.allow_failures && outcome_count(&status.consensus_vector, "fail") > 0 {
            return Err("policy rejected: failure_mode_distribution contains fail".to_string());
        }
        if self.require_loss_dim_coverage.as_deref() == Some("all-named") {
            let unwitnessed = status
                .consensus_vector
                .pointer("/loss_dim_coverage/unwitnessed")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0);
            if unwitnessed > 0 {
                return Err("policy rejected: loss_dim_coverage is not all-named".to_string());
            }
        }
        for threshold in &self.thresholds {
            eval_predicate(&threshold.predicate, status).map_err(|err| {
                format!(
                    "policy rejected on axis `{}` predicate `{}`: {err}",
                    threshold.axis, threshold.predicate
                )
            })?;
        }
        Ok(())
    }
}

impl PromotionDecisionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn admit_json_str(&mut self, text: &str) -> Result<usize, PromotionDecisionRegistryError> {
        match serde_json::from_str::<PromotionDecisionMemento>(text) {
            Ok(decision) => self.admit(decision),
            Err(decision_err) => match MigrateReceiptEnvelope::parse_json_str(text) {
                Ok(receipt) => self.admit_many(receipt.promotion_decisions),
                Err(receipt_err) => Err(PromotionDecisionRegistryError::Parse {
                    decision: decision_err.to_string(),
                    receipt: receipt_err.to_string(),
                }),
            },
        }
    }

    pub fn admit_many<I>(&mut self, decisions: I) -> Result<usize, PromotionDecisionRegistryError>
    where
        I: IntoIterator<Item = PromotionDecisionMemento>,
    {
        let mut indexed = 0usize;
        for decision in decisions {
            indexed += self.admit(decision)?;
        }
        Ok(indexed)
    }

    pub fn admit(
        &mut self,
        decision: PromotionDecisionMemento,
    ) -> Result<usize, PromotionDecisionRegistryError> {
        decision
            .validate()
            .map_err(|err| PromotionDecisionRegistryError::Invalid(err.to_string()))?;
        let actual = decision
            .recompute_header_cid()
            .map_err(|err| PromotionDecisionRegistryError::Invalid(err.to_string()))?;
        if actual != decision.header.cid {
            return Err(PromotionDecisionRegistryError::HeaderCidMismatch {
                claimed: decision.header.cid.clone(),
                actual,
            });
        }

        let decision_cid = decision.header.cid.clone();
        if self.decisions.contains_key(&decision_cid) {
            return Ok(0);
        }

        let entries = index_entries(&decision);
        self.decisions.insert(decision_cid.clone(), decision);
        for (key, summary) in entries {
            self.by_key
                .entry(key)
                .or_default()
                .insert(decision_cid.clone());
            self.summaries.insert(decision_cid.clone(), summary);
        }
        Ok(self
            .by_key
            .values()
            .filter(|cids| cids.contains(&decision_cid))
            .count())
    }

    pub fn get(&self, key: &PromotionDecisionKey) -> Option<PromotionStatus> {
        self.statuses(key).into_iter().max_by(|left, right| {
            left.witnesses_consulted
                .cmp(&right.witnesses_consulted)
                .then(left.decision_cids.cmp(&right.decision_cids))
        })
    }

    pub fn statuses(&self, key: &PromotionDecisionKey) -> Vec<PromotionStatus> {
        let Some(cids) = self.by_key.get(key) else {
            return Vec::new();
        };
        cids.iter()
            .filter_map(|cid| {
                let summary = self.summaries.get(cid)?;
                Some(PromotionStatus {
                    key: key.clone(),
                    decision_cids: vec![cid.clone()],
                    decision_policy_cids: vec![summary.decision_policy_cid.clone()],
                    consensus_vector: summary.consensus_vector.clone(),
                    witnesses_consulted: summary.witnesses_consulted,
                })
            })
            .collect()
    }
}

fn index_entries(
    decision: &PromotionDecisionMemento,
) -> Vec<(PromotionDecisionKey, PromotionSummary)> {
    if decision.header.result != PromotionResult::Admitted {
        return Vec::new();
    }
    let Some(promoted_op) = payload_string(&decision.header.decision_payload, "promoted_op") else {
        return Vec::new();
    };
    let fixtures = payload_string_array(&decision.header.decision_payload, "fixtures_consulted");
    if fixtures.is_empty() {
        return Vec::new();
    }
    let summary = PromotionSummary {
        decision_policy_cid: decision.header.policy_cid.clone(),
        consensus_vector: consensus_vector(&decision.header.decision_payload, decision),
        witnesses_consulted: witnesses_consulted(decision),
    };
    fixtures
        .into_iter()
        .map(|fixture| {
            (
                PromotionDecisionKey::new(promoted_op.clone(), fixture),
                summary.clone(),
            )
        })
        .collect()
}

fn consensus_vector(payload: &Value, decision: &PromotionDecisionMemento) -> Value {
    payload
        .get("consensus_vector")
        .cloned()
        .unwrap_or_else(|| {
            json!({
                "failure_mode_distribution": [
                    {"outcome": "pass", "count": witnesses_consulted(decision)},
                    {"outcome": "fail", "count": 0},
                    {"outcome": "inconclusive", "count": 0}
                ],
                "input_distribution_summary": {"shape": "unspanned"},
                "loss_dim_coverage": {
                    "named_in_concept_spec": [],
                    "unwitnessed": [],
                    "witnessed": []
                },
                "temporal_spread": {
                    "first_observed_at": null,
                    "last_observed_at": null,
                    "span_seconds": 0
                },
                "total_sample_count": payload.get("total_observations").and_then(Value::as_u64).unwrap_or(0),
                "unique_fixtures": payload.get("fixtures_consulted").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
                "unique_signer_keys": ["unsigned"],
                "unique_signers": 1
            })
        })
}

fn eval_predicate(predicate: &str, status: &PromotionStatus) -> Result<(), String> {
    let (left, op, right) = parse_predicate(predicate)?;
    let actual = metric_value(left, status).ok_or_else(|| format!("unknown metric `{left}`"))?;
    let ok = match op {
        ">=" => actual >= right,
        "<=" => actual <= right,
        "==" => actual == right,
        ">" => actual > right,
        "<" => actual < right,
        _ => return Err(format!("unsupported operator `{op}`")),
    };
    if ok {
        Ok(())
    } else {
        Err(format!("{left} was {actual}, required {op}{right}"))
    }
}

fn parse_predicate(predicate: &str) -> Result<(&str, &str, u64), String> {
    for op in [">=", "<=", "==", ">", "<"] {
        if let Some((left, right)) = predicate.split_once(op) {
            let right = right
                .trim()
                .parse::<u64>()
                .map_err(|err| format!("threshold is not an integer: {err}"))?;
            return Ok((left.trim(), op, right));
        }
    }
    Err("unsupported predicate grammar".to_string())
}

fn metric_value(metric: &str, status: &PromotionStatus) -> Option<u64> {
    match metric {
        "n" | "witnesses_consulted" => Some(status.witnesses_consulted),
        other => status.consensus_vector.get(other).and_then(Value::as_u64),
    }
}

fn outcome_count(vector: &Value, outcome: &str) -> u64 {
    vector
        .get("failure_mode_distribution")
        .and_then(Value::as_array)
        .and_then(|items| {
            items.iter().find_map(|item| {
                (item.get("outcome").and_then(Value::as_str) == Some(outcome))
                    .then(|| item.get("count").and_then(Value::as_u64).unwrap_or(0))
            })
        })
        .unwrap_or(0)
}

fn payload_string(payload: &Value, field: &str) -> Option<String> {
    payload
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn payload_string_array(payload: &Value, field: &str) -> Vec<String> {
    payload
        .get(field)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn witnesses_consulted(decision: &PromotionDecisionMemento) -> u64 {
    if let Some(items) = decision
        .header
        .decision_payload
        .get("witnesses_consulted")
        .and_then(Value::as_array)
    {
        return items.len() as u64;
    }
    if let Some(count) = decision
        .header
        .decision_payload
        .get("witnesses_consulted")
        .and_then(Value::as_u64)
    {
        return count;
    }
    decision.header.evidence_cids.len() as u64
}
