// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use libprovekit::promotion_decision_registry::{
    ConsensusPolicy, PromotionDecisionKey, PromotionDecisionRegistry, PromotionStatus,
};
use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value as CValue};
use serde_json::{json, Value};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub(crate) struct ConsensusQueryHit {
    pub status: PromotionStatus,
}

pub(crate) fn query_consensus_vector(
    project_root: &Path,
    catalogs: &[PathBuf],
    concept: &str,
    fixture: &str,
) -> Result<Option<ConsensusQueryHit>, String> {
    let roots = catalog_roots(project_root, catalogs);
    let registry = load_promotion_registry(&roots)?;
    let terms = concept_query_terms(project_root, concept);
    for term in terms {
        let key = PromotionDecisionKey::new(term, fixture);
        if let Some(status) = registry.get(&key) {
            return Ok(Some(ConsensusQueryHit { status }));
        }
    }
    Ok(None)
}

pub(crate) fn query_consensus_vector_for_policy(
    project_root: &Path,
    catalogs: &[PathBuf],
    concept: &str,
    fixture: &str,
    policy: &ConsensusPolicy,
) -> Result<Option<ConsensusQueryHit>, String> {
    let roots = catalog_roots(project_root, catalogs);
    let registry = load_promotion_registry(&roots)?;
    let terms = concept_query_terms(project_root, concept);
    let mut rejected = None;
    for term in terms {
        let key = PromotionDecisionKey::new(term, fixture);
        for status in registry.statuses(&key) {
            if policy.admits(&status).is_ok() {
                return Ok(Some(ConsensusQueryHit { status }));
            }
            rejected.get_or_insert(ConsensusQueryHit { status });
        }
    }
    Ok(rejected)
}

pub(crate) fn concept_query_terms(project_root: &Path, concept: &str) -> Vec<String> {
    let mut terms = BTreeSet::new();
    terms.insert(concept.to_string());
    if let Some(resolved) = resolve_concept_counterpart(project_root, concept) {
        terms.insert(resolved);
    }
    // Family expansion: a grouping concept (e.g. concept:family:sql-query, which
    // rolls up the cardinality concepts sql-query-row/-all/-iterate after the
    // #1469 split) declares a `members` array in its catalog index entry. When
    // present, the query terms include each member name AND its CID, so witness
    // consensus / verify aggregate witnesses across the whole family rather than
    // a single concept. Non-family concepts have no `members` and are unaffected.
    if let Some((_, entry)) = resolve_concept_index_entry(project_root, concept) {
        if let Some(members) = entry.get("members").and_then(Value::as_array) {
            for member in members {
                if let Some(name) = member.as_str() {
                    terms.insert(name.to_string());
                    if let Some(cid) = resolve_concept_counterpart(project_root, name) {
                        terms.insert(cid);
                    }
                }
            }
        }
    }
    terms.into_iter().collect()
}

pub(crate) fn concept_loss_dimensions(project_root: &Path, concept: &str) -> Vec<String> {
    let Some((repo_root, entry)) = resolve_concept_index_entry(project_root, concept) else {
        return Vec::new();
    };
    let Some(path) = entry.get("path").and_then(Value::as_str) else {
        return Vec::new();
    };
    let spec_path = repo_root
        .join("menagerie")
        .join("concept-shapes")
        .join("catalog")
        .join(path);
    let Ok(raw) = std::fs::read_to_string(spec_path) else {
        return Vec::new();
    };
    let Ok(spec) = serde_json::from_str::<Value>(&raw) else {
        return Vec::new();
    };
    spec.get("loss_dimensions")
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

pub(crate) fn catalog_roots(project_root: &Path, catalogs: &[PathBuf]) -> Vec<PathBuf> {
    if !catalogs.is_empty() {
        return catalogs.to_vec();
    }
    let project_catalog = project_root.join(".provekit");
    if project_catalog.exists() {
        vec![project_catalog]
    } else {
        vec![project_root.to_path_buf()]
    }
}

pub(crate) fn load_promotion_registry(
    catalog_roots: &[PathBuf],
) -> Result<PromotionDecisionRegistry, String> {
    let mut registry = PromotionDecisionRegistry::new();
    for root in catalog_roots {
        if root.is_file() {
            admit_promotion_file(root, &mut registry)?;
            continue;
        }
        if !root.exists() {
            continue;
        }
        for entry in WalkDir::new(root)
            .into_iter()
            .filter_entry(|entry| should_descend(entry.path()))
        {
            let entry = entry.map_err(|err| err.to_string())?;
            if entry.file_type().is_file() {
                admit_promotion_file(entry.path(), &mut registry)?;
            }
        }
    }
    Ok(registry)
}

pub(crate) fn load_consensus_policy(path: &Path) -> Result<ConsensusPolicy, String> {
    let raw =
        std::fs::read_to_string(path).map_err(|err| format!("read {}: {err}", path.display()))?;
    let value: Value = serde_json::from_str(&raw)
        .map_err(|err| format!("parse consensus policy {}: {err}", path.display()))?;
    let mut policy = ConsensusPolicy::from_json_str(&raw).map_err(|err| format!("{err}"))?;
    if policy.cid.as_deref().map(str::is_empty).unwrap_or(true) {
        policy.cid = Some(content_cid_for_json(&value));
    }
    Ok(policy)
}

pub(crate) fn content_cid_for_json(value: &Value) -> String {
    blake3_512_of(&canonical_json_bytes(value))
}

pub(crate) fn canonical_json_bytes(value: &Value) -> Vec<u8> {
    encode_jcs(&json_to_value(value)).into_bytes()
}

pub(crate) fn status_json(
    requested_concept: &str,
    fixture: &str,
    hit: &ConsensusQueryHit,
    policy: Option<&ConsensusPolicy>,
) -> Value {
    let policy_verdict = policy.map(|policy| policy.admits(&hit.status));
    let ok = policy_verdict.as_ref().map(|r| r.is_ok()).unwrap_or(true);
    let reason = policy_verdict
        .as_ref()
        .and_then(|result| result.as_ref().err())
        .cloned();
    json!({
        "ok": ok,
        "verdict": if ok { "accepted" } else { "rejected" },
        "reason": reason,
        "requirement": {
            "concept": requested_concept,
            "fixture_state_cid": fixture,
            "policy_cid": policy.and_then(|policy| policy.cid.clone())
        },
        "promotion": {
            "consensus_vector": hit.status.consensus_vector.clone(),
            "decision_cids": hit.status.decision_cids.clone(),
            "decision_policy_cids": hit.status.decision_policy_cids.clone(),
            "promoted_op": hit.status.key.promoted_op.clone(),
            "witnesses_consulted": hit.status.witnesses_consulted
        }
    })
}

pub(crate) fn missing_json(requested_concept: &str, fixture: &str) -> Value {
    json!({
        "ok": false,
        "verdict": "rejected",
        "reason": "required empirically-witnessed promotion was not found",
        "requirement": {
            "concept": requested_concept,
            "fixture_state_cid": fixture,
            "policy_cid": null
        }
    })
}

fn admit_promotion_file(
    path: &Path,
    registry: &mut PromotionDecisionRegistry,
) -> Result<(), String> {
    let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
        return Ok(());
    };
    if !matches!(ext, "json" | "proof") {
        return Ok(());
    }
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(_) => return Ok(()),
    };
    let _ = registry.admit_json_str(&text);
    Ok(())
}

fn should_descend(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return true;
    };
    !matches!(
        name,
        ".git" | ".worktrees" | "target" | "node_modules" | "vendor" | ".venv"
    )
}

fn resolve_concept_counterpart(project_root: &Path, concept: &str) -> Option<String> {
    let (_, entry) = resolve_concept_index_entry(project_root, concept)?;
    let entry_cid = entry.get("cid").and_then(Value::as_str)?;
    let name = entry.get("name").and_then(Value::as_str)?;
    if concept == name {
        Some(entry_cid.to_string())
    } else if concept == entry_cid {
        Some(name.to_string())
    } else {
        None
    }
}

fn resolve_concept_index_entry(project_root: &Path, concept: &str) -> Option<(PathBuf, Value)> {
    let repo_root = repo_root_for_concepts(project_root)?;
    let index_path = repo_root
        .join("menagerie")
        .join("concept-shapes")
        .join("catalog")
        .join("index.json");
    let raw = std::fs::read_to_string(index_path).ok()?;
    let index: Value = serde_json::from_str(&raw).ok()?;
    let entries = index
        .get("entries")
        .and_then(Value::as_object)
        .or_else(|| index.as_object())?;
    for (cid, entry) in entries {
        let entry_cid = entry.get("cid").and_then(Value::as_str).unwrap_or(cid);
        let name = entry.get("name").and_then(Value::as_str);
        if name == Some(concept) || concept == entry_cid {
            return Some((repo_root, entry.clone()));
        }
    }
    None
}

fn repo_root_for_concepts(project_root: &Path) -> Option<PathBuf> {
    for base in [project_root.to_path_buf(), std::env::current_dir().ok()?] {
        for candidate in base.ancestors() {
            if candidate
                .join("menagerie")
                .join("concept-shapes")
                .join("catalog")
                .join("index.json")
                .exists()
            {
                return Some(candidate.to_path_buf());
            }
        }
    }
    None
}

fn json_to_value(j: &Value) -> Arc<CValue> {
    match j {
        Value::String(s) => CValue::string(s.clone()),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                CValue::integer(i)
            } else if let Some(u) = n.as_u64() {
                if let Ok(i) = i64::try_from(u) {
                    CValue::integer(i)
                } else {
                    CValue::string(n.to_string())
                }
            } else {
                CValue::string(n.to_string())
            }
        }
        Value::Bool(b) => CValue::boolean(*b),
        Value::Null => CValue::null(),
        Value::Array(items) => CValue::array(items.iter().map(json_to_value).collect()),
        Value::Object(map) => CValue::object(
            map.iter()
                .map(|(key, value)| (key.as_str(), json_to_value(value)))
                .collect::<Vec<_>>(),
        ),
    }
}
