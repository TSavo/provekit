// SPDX-License-Identifier: Apache-2.0
//
// cmd witness: mint a witness memento extending the proof lattice.
//
// Anyone can witness: prove a new property from an existing contract.
// This is permissionless extension of the proof blockchain.
//
// Usage:
//   provekit witness <contract_cid> <property.ir.json>
//
// What happens:
//   1. Load the contract memento from the pool
//   2. Parse the property formula
//   3. Build implication: contract.post → property
//   4. Run solver
//   5. If unsat: mint witness memento, save to .provekit/witnesses/
//   6. The witness can be shipped with the package

use std::path::{Path, PathBuf};

use chrono::{SecondsFormat, Utc};
use libprovekit::promotion_decision_registry::{PromotionDecisionKey, PromotionStatus};
use provekit_ir_types::{
    CrossLanguageWitnessPair, MigrateReceiptEnvelope, PromotionDecisionEnvelope,
    PromotionDecisionHeader, PromotionDecisionMemento, PromotionDecisionMetadata, PromotionGate,
    PromotionResult, WitnessMemento,
};
use serde_json::json;
use serde_json::Value as Json;
use walkdir::WalkDir;

use crate::{EXIT_OK, EXIT_SOLVER_FAIL, EXIT_USER_ERROR, EXIT_VERIFY_FAIL};

pub fn run(args: crate::WitnessArgs) -> u8 {
    if args.command_or_contract == "consensus" {
        return run_consensus(args);
    }
    if args.command_or_contract == "status" {
        return run_status(args);
    }

    let project_root = args
        .project
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap());
    let contract_cid = args.command_or_contract.clone();
    let property = match args.property.as_ref() {
        Some(path) => path.clone(),
        None => {
            eprintln!("error: witness requires <CONTRACT_CID> <PROPERTY>");
            return EXIT_USER_ERROR;
        }
    };

    // 1. Load pool
    let pool = provekit_verifier::load_all_proofs::run(&project_root);

    // 2. Find contract
    let contract = match pool.mementos.get(&contract_cid) {
        Some(c) => c,
        None => {
            eprintln!("error: contract {contract_cid} not found in pool");
            return EXIT_USER_ERROR;
        }
    };

    // 3. Extract contract's post formula
    let post_formula = match extract_post(contract) {
        Some(f) => f,
        None => {
            eprintln!("error: contract has no post formula");
            return EXIT_USER_ERROR;
        }
    };

    // 4. Load property formula
    let property_formula = match load_formula(&property) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error: failed to load property: {e}");
            return EXIT_USER_ERROR;
        }
    };

    // 5. Build implication obligation
    let obligation = match build_witness_obligation(&post_formula, &property_formula) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("error: failed to build witness: {e}");
            return EXIT_USER_ERROR;
        }
    };

    // 6. Emit SMT-LIB
    let smt = match provekit_verifier::smt_emitter::emit(&obligation) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: SMT emission failed: {e}");
            return EXIT_SOLVER_FAIL;
        }
    };

    // 7. Run solver
    println!(
        "witness: proving {} implies property...",
        short(&contract_cid)
    );
    let result = run_solver(&args.z3, &smt);

    match result {
        SolverOutput::Unsat => {
            println!("witness: proven! Minting memento...");
            // TODO: mint witness memento
            // For now, print what would be minted
            println!("witness: memento would be:");
            println!("  antecedent: {contract_cid}");
            println!("  consequent: <property CID>");
            println!("  prover: z3");
            EXIT_OK
        }
        SolverOutput::Sat => {
            eprintln!("witness: FAILED: solver found counterexample");
            eprintln!("  The property does NOT follow from the contract.");
            EXIT_VERIFY_FAIL
        }
        SolverOutput::Unknown => {
            eprintln!("witness: UNDECIDABLE: solver could not determine");
            EXIT_SOLVER_FAIL
        }
        SolverOutput::Error(e) => {
            eprintln!("witness: SOLVER ERROR: {e}");
            EXIT_SOLVER_FAIL
        }
    }
}

fn run_consensus(args: crate::WitnessArgs) -> u8 {
    let project_root = args
        .project
        .unwrap_or_else(|| std::env::current_dir().unwrap());
    let concept = match args.concept.as_deref() {
        Some(concept) if !concept.trim().is_empty() => concept.to_string(),
        _ => {
            eprintln!("error: witness consensus requires --concept");
            return EXIT_USER_ERROR;
        }
    };
    let fixture = match args.require_fixture.as_deref() {
        Some(fixture) if !fixture.trim().is_empty() => fixture.to_string(),
        _ => {
            eprintln!("error: witness consensus requires --require-fixture");
            return EXIT_USER_ERROR;
        }
    };
    let emit = match args.emit.as_ref() {
        Some(path) => path.clone(),
        None => {
            eprintln!("error: witness consensus requires --emit <PATH>");
            return EXIT_USER_ERROR;
        }
    };
    let catalog_roots = crate::promotion_query::catalog_roots(&project_root, &args.catalogs);
    let concept_terms = crate::promotion_query::concept_query_terms(&project_root, &concept);

    let catalog = match collect_witness_catalog(&catalog_roots) {
        Ok(catalog) => catalog,
        Err(err) => {
            eprintln!("error: witness consensus catalog scan failed: {err}");
            return EXIT_USER_ERROR;
        }
    };
    let selected: Vec<WitnessMemento> = catalog
        .witnesses
        .into_iter()
        .filter(|witness| {
            concept_terms
                .iter()
                .any(|candidate| candidate == &witness.witness_for)
                && witness.fixture_state_cid == fixture
                && witness.outcome == "pass"
        })
        .collect();

    let consensus = match consensus_evidence(&selected, &catalog.cross_language_pairs) {
        Ok(consensus) => consensus,
        Err(err) => {
            eprintln!("witness consensus: rejected: {err}");
            return EXIT_VERIFY_FAIL;
        }
    };

    if consensus.witnesses.len() < args.min_witnesses {
        eprintln!(
            "witness consensus: rejected: {} matching passing witnesses, require {}",
            consensus.witnesses.len(),
            args.min_witnesses
        );
        return EXIT_VERIFY_FAIL;
    }

    let policy_path = match args.consensus_policy.as_ref() {
        Some(path) => path,
        None => {
            eprintln!("error: witness consensus requires --consensus-policy");
            return EXIT_USER_ERROR;
        }
    };
    let policy = match crate::promotion_query::load_consensus_policy(policy_path) {
        Ok(policy) => policy,
        Err(err) => {
            eprintln!("error: {err}");
            return EXIT_USER_ERROR;
        }
    };
    let policy_cid = policy
        .cid
        .clone()
        .expect("load_consensus_policy fills missing policy cid");
    let loss_dimensions = crate::promotion_query::concept_loss_dimensions(&project_root, &concept);
    let consensus_vector = consensus_vector(&consensus.witnesses, &loss_dimensions);
    let status_for_policy = PromotionStatus {
        key: PromotionDecisionKey::new(concept.clone(), fixture.clone()),
        decision_cids: Vec::new(),
        decision_policy_cids: vec![policy_cid.clone()],
        consensus_vector: consensus_vector.clone(),
        witnesses_consulted: consensus.witnesses.len() as u64,
    };
    if let Err(err) = policy.admits(&status_for_policy) {
        eprintln!("witness consensus: rejected by policy: {err}");
        return EXIT_VERIFY_FAIL;
    }

    let decision = match promotion_decision_for_consensus(
        &concept,
        &fixture,
        args.min_witnesses,
        &consensus.witnesses,
        consensus.row_schema,
        consensus_vector,
        policy_cid,
    ) {
        Ok(decision) => decision,
        Err(err) => {
            eprintln!("error: mint promotion decision: {err}");
            return EXIT_USER_ERROR;
        }
    };
    if let Some(parent) = emit.parent() {
        if let Err(err) = std::fs::create_dir_all(parent) {
            eprintln!("error: create {}: {err}", parent.display());
            return EXIT_USER_ERROR;
        }
    }
    let bytes = match serde_json::to_string_pretty(&decision) {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!("error: serialize promotion decision: {err}");
            return EXIT_USER_ERROR;
        }
    };
    if let Err(err) = std::fs::write(&emit, format!("{bytes}\n")) {
        eprintln!("error: write {}: {err}", emit.display());
        return EXIT_USER_ERROR;
    }

    if args.out.json {
        let summary = json!({
            "agreement": "byte-equal",
            "promotion_cid": decision.header.cid,
            "promotion_receipt": emit.display().to_string(),
            "result": "admitted",
            "witnesses_consulted": consensus.witnesses.len()
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&summary).unwrap_or_else(|_| summary.to_string())
        );
    } else if !args.out.quiet {
        println!("witness consensus: admitted");
        println!("  concept: {concept}");
        println!("  fixture: {fixture}");
        println!("  witnesses: {}", consensus.witnesses.len());
        println!("  agreement: byte-equal");
        println!("  receipt: {}", emit.display());
    }
    EXIT_OK
}

fn run_status(args: crate::WitnessArgs) -> u8 {
    let project_root = args
        .project
        .unwrap_or_else(|| std::env::current_dir().unwrap());
    let concept = match args.concept.as_deref() {
        Some(concept) if !concept.trim().is_empty() => concept.to_string(),
        _ => {
            eprintln!("error: witness status requires --concept");
            return EXIT_USER_ERROR;
        }
    };
    let fixture = match args.require_fixture.as_deref() {
        Some(fixture) if !fixture.trim().is_empty() => fixture.to_string(),
        _ => {
            eprintln!("error: witness status requires --require-fixture");
            return EXIT_USER_ERROR;
        }
    };

    match crate::promotion_query::query_consensus_vector(
        &project_root,
        &args.catalogs,
        &concept,
        &fixture,
    ) {
        Ok(Some(hit)) => {
            if args.out.json {
                let report = crate::promotion_query::status_json(&concept, &fixture, &hit, None);
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report).unwrap_or_else(|_| report.to_string())
                );
            } else if !args.out.quiet {
                println!("witness status: consensus-vector present");
                println!("  concept: {concept}");
                println!("  fixture: {fixture}");
                println!("  promoted_op: {}", hit.status.key.promoted_op);
                println!("  decisions: {}", hit.status.decision_cids.len());
                println!("  witnesses_consulted: {}", hit.status.witnesses_consulted);
            }
            EXIT_OK
        }
        Ok(None) => {
            if args.out.json {
                let report = crate::promotion_query::missing_json(&concept, &fixture);
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report).unwrap_or_else(|_| report.to_string())
                );
            } else {
                eprintln!("witness status: missing consensus vector");
            }
            EXIT_VERIFY_FAIL
        }
        Err(err) => {
            eprintln!("error: witness status catalog scan failed: {err}");
            EXIT_USER_ERROR
        }
    }
}

fn extract_post(contract: &Json) -> Option<Json> {
    contract.get("evidence")?.get("body")?.get("post").cloned()
}

fn load_formula(path: &PathBuf) -> Result<Json, Box<dyn std::error::Error>> {
    let bytes = if path.as_os_str() == "-" {
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut std::io::stdin(), &mut buf)?;
        buf
    } else {
        std::fs::read(path)?
    };
    Ok(serde_json::from_slice(&bytes)?)
}

fn build_witness_obligation(post: &Json, property: &Json) -> Result<Json, String> {
    // Build: post → property
    Ok(Json::Object({
        let mut m = serde_json::Map::new();
        m.insert("kind".to_string(), Json::String("implies".to_string()));
        m.insert(
            "operands".to_string(),
            Json::Array(vec![post.clone(), property.clone()]),
        );
        m
    }))
}

#[derive(Debug, Default)]
struct WitnessCatalog {
    witnesses: Vec<WitnessMemento>,
    cross_language_pairs: Vec<CrossLanguageWitnessPair>,
}

#[derive(Debug)]
struct ConsensusEvidence {
    witnesses: Vec<WitnessMemento>,
    row_schema: Json,
}

fn collect_witness_catalog(catalog_roots: &[PathBuf]) -> Result<WitnessCatalog, String> {
    let mut catalog = WitnessCatalog::default();
    for root in catalog_roots {
        if root.is_file() {
            collect_witnesses_from_file(root, &mut catalog)?;
            continue;
        }
        if !root.exists() {
            continue;
        }
        for entry in WalkDir::new(root)
            .into_iter()
            .filter_entry(|entry| should_descend(entry.path()))
        {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.file_type().is_file() {
                collect_witnesses_from_file(entry.path(), &mut catalog)?;
            }
        }
    }
    catalog
        .witnesses
        .sort_by(|left, right| left.cid.cmp(&right.cid));
    catalog
        .witnesses
        .dedup_by(|left, right| left.cid == right.cid);
    catalog.cross_language_pairs.sort_by(|left, right| {
        left.concept_site_cid
            .cmp(&right.concept_site_cid)
            .then(left.source_witness_cid.cmp(&right.source_witness_cid))
            .then(left.target_witness_cid.cmp(&right.target_witness_cid))
    });
    catalog.cross_language_pairs.dedup();
    Ok(catalog)
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

fn collect_witnesses_from_file(path: &Path, catalog: &mut WitnessCatalog) -> Result<(), String> {
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
    if let Ok(witness) = serde_json::from_str::<WitnessMemento>(&text) {
        if witness.validate().is_ok() {
            catalog.witnesses.push(witness);
        }
        return Ok(());
    }
    if let Ok(receipt) = MigrateReceiptEnvelope::parse_json_str(&text) {
        catalog.witnesses.extend(receipt.witnesses);
        catalog
            .cross_language_pairs
            .extend(receipt.cross_language_witness_pairs);
    }
    Ok(())
}

fn consensus_evidence(
    witnesses: &[WitnessMemento],
    pairs: &[CrossLanguageWitnessPair],
) -> Result<ConsensusEvidence, String> {
    match consensus_row_schema(witnesses) {
        Ok(row_schema) => {
            return Ok(ConsensusEvidence {
                witnesses: witnesses.to_vec(),
                row_schema,
            });
        }
        Err(err) if pairs.is_empty() => return Err(err),
        Err(_) => {}
    }
    consensus_pairwise_row_schema(witnesses, pairs)
}

fn consensus_pairwise_row_schema(
    witnesses: &[WitnessMemento],
    pairs: &[CrossLanguageWitnessPair],
) -> Result<ConsensusEvidence, String> {
    let by_cid = witnesses
        .iter()
        .map(|witness| (witness.cid.as_str(), witness))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut evidence_cids = std::collections::BTreeSet::new();
    let mut pair_count = 0usize;
    for pair in pairs {
        if pair.equivalence_outcome != "pass" {
            continue;
        }
        let Some(source) = by_cid.get(pair.source_witness_cid.as_str()) else {
            continue;
        };
        let Some(target) = by_cid.get(pair.target_witness_cid.as_str()) else {
            continue;
        };
        let source_schema = source
            .measurements
            .get("row_schema")
            .ok_or_else(|| format!("witness {} missing measurements.row_schema", source.cid))?;
        let target_schema = target
            .measurements
            .get("row_schema")
            .ok_or_else(|| format!("witness {} missing measurements.row_schema", target.cid))?;
        if canonical_json_bytes(source_schema) != canonical_json_bytes(target_schema) {
            return Err("cross_language_witness_pairs row_schema disagreement".to_string());
        }
        evidence_cids.insert(source.cid.clone());
        evidence_cids.insert(target.cid.clone());
        pair_count += 1;
    }
    if evidence_cids.is_empty() {
        return Err("measurements.row_schema disagreement".to_string());
    }
    let consensus_witnesses = witnesses
        .iter()
        .filter(|witness| evidence_cids.contains(&witness.cid))
        .cloned()
        .collect::<Vec<_>>();
    Ok(ConsensusEvidence {
        witnesses: consensus_witnesses,
        row_schema: json!({
            "agreement_axis": "cross_language_witness_pairs.measurements.row_schema",
            "pair_count": pair_count
        }),
    })
}

fn consensus_row_schema(witnesses: &[WitnessMemento]) -> Result<Json, String> {
    let mut schemas = witnesses.iter().map(|witness| {
        witness
            .measurements
            .get("row_schema")
            .cloned()
            .ok_or_else(|| format!("witness {} missing measurements.row_schema", witness.cid))
    });
    let first = schemas
        .next()
        .ok_or_else(|| "no witnesses selected".to_string())??;
    let first_bytes = canonical_json_bytes(&first);
    for schema in schemas {
        let schema = schema?;
        let bytes = canonical_json_bytes(&schema);
        if bytes != first_bytes {
            return Err("measurements.row_schema disagreement".to_string());
        }
    }
    Ok(first)
}

fn promotion_decision_for_consensus(
    concept: &str,
    fixture: &str,
    min_witnesses: usize,
    witnesses: &[WitnessMemento],
    row_schema: Json,
    consensus_vector: Json,
    policy_cid: String,
) -> Result<PromotionDecisionMemento, String> {
    let mut evidence_cids: Vec<String> = witnesses.iter().map(|w| w.cid.clone()).collect();
    evidence_cids.sort();
    let subjects: Vec<String> = witnesses.iter().map(|w| w.subject.clone()).collect();
    let total_observations: u64 = witnesses.iter().map(|w| w.sample_count).sum();
    let decider_cid = crate::promotion_query::content_cid_for_json(&json!({
        "kind": "decider",
        "name": "provekit-witness-consensus"
    }));
    let candidate_cid = crate::promotion_query::content_cid_for_json(&json!({
        "concept": concept,
        "kind": "concept-candidate"
    }));
    let promoted_cid = crate::promotion_query::content_cid_for_json(&json!({
        "concept": concept,
        "fixture_state_cid": fixture,
        "kind": "promoted-concept-tier",
        "tier": "empirically-witnessed"
    }));
    let reason = format!(
        "{} witnesses selected; consensus vector records the observed axes",
        witnesses.len()
    );
    let mut decision = PromotionDecisionMemento {
        envelope: PromotionDecisionEnvelope {
            declared_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            signature: String::new(),
            signer: decider_cid.clone(),
        },
        header: PromotionDecisionHeader {
            candidate_cid,
            cid: String::new(),
            decider_cid,
            decision_payload: json!({
                "agreement": "byte-equal",
                "fixtures_consulted": [fixture],
                "min_witnesses": min_witnesses,
                "promotion": "documentary -> empirically-witnessed",
                "promoted_op": concept,
                "reason": reason,
                "row_schema": row_schema,
                "subjects_consulted": subjects,
                "total_observations": total_observations,
                "witnesses_consulted": evidence_cids,
                "consensus_vector": consensus_vector
            }),
            evidence_cids,
            gate: PromotionGate::Threshold,
            kind: "promotion-decision".to_string(),
            policy_cid,
            promoted_cid,
            result: PromotionResult::Admitted,
            schema_version: "1".to_string(),
        },
        metadata: PromotionDecisionMetadata {
            counterexample_cids: None,
            note: Some("witness consensus promoted empirical contract tier".to_string()),
            source_url: None,
        },
    };
    decision.header.cid = decision.recompute_header_cid().map_err(|e| e.to_string())?;
    decision.validate().map_err(|e| e.to_string())?;
    Ok(decision)
}

fn consensus_vector(witnesses: &[WitnessMemento], loss_dimensions: &[String]) -> Json {
    let mut signer_keys = witnesses
        .iter()
        .map(|witness| {
            witness
                .signed_by
                .clone()
                .unwrap_or_else(|| "unsigned".to_string())
        })
        .collect::<std::collections::BTreeSet<_>>();
    if signer_keys.is_empty() {
        signer_keys.insert("unsigned".to_string());
    }
    let fixtures = witnesses
        .iter()
        .map(|witness| witness.fixture_state_cid.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let total_sample_count: u64 = witnesses.iter().map(|witness| witness.sample_count).sum();
    let mut observed_times = witnesses
        .iter()
        .filter_map(|witness| {
            chrono::DateTime::parse_from_rfc3339(&witness.observed_at)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        })
        .collect::<Vec<_>>();
    observed_times.sort();
    let first_observed_at = observed_times
        .first()
        .map(|dt| dt.to_rfc3339_opts(SecondsFormat::Millis, true));
    let last_observed_at = observed_times
        .last()
        .map(|dt| dt.to_rfc3339_opts(SecondsFormat::Millis, true));
    let span_seconds = match (observed_times.first(), observed_times.last()) {
        (Some(first), Some(last)) => (*last - *first).num_seconds().max(0),
        _ => 0,
    };
    let mut outcomes = std::collections::BTreeMap::<String, u64>::new();
    for witness in witnesses {
        *outcomes.entry(witness.outcome.clone()).or_default() += 1;
    }
    let witnessed_loss_dims = witnesses
        .iter()
        .flat_map(|witness| {
            witness
                .measurements
                .pointer("/observer/loss_dims_exercised")
                .and_then(Json::as_array)
                .into_iter()
                .flatten()
                .filter_map(Json::as_str)
                .map(str::to_string)
        })
        .filter(|dim| loss_dimensions.iter().any(|known| known == dim))
        .collect::<std::collections::BTreeSet<_>>();
    let named_loss_dims = loss_dimensions
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let unwitnessed_loss_dims = named_loss_dims
        .difference(&witnessed_loss_dims)
        .cloned()
        .collect::<Vec<_>>();

    json!({
        "unique_signers": signer_keys.len(),
        "unique_signer_keys": signer_keys.into_iter().collect::<Vec<_>>(),
        "unique_fixtures": fixtures.len(),
        "total_sample_count": total_sample_count,
        "loss_dim_coverage": {
            "named_in_concept_spec": named_loss_dims.into_iter().collect::<Vec<_>>(),
            "witnessed": witnessed_loss_dims.into_iter().collect::<Vec<_>>(),
            "unwitnessed": unwitnessed_loss_dims
        },
        "input_distribution_summary": {
            "shape": "unspanned"
        },
        "temporal_spread": {
            "first_observed_at": first_observed_at,
            "last_observed_at": last_observed_at,
            "span_seconds": span_seconds
        },
        "failure_mode_distribution": [
            {"outcome": "pass", "count": outcomes.get("pass").copied().unwrap_or(0)},
            {"outcome": "fail", "count": outcomes.get("fail").copied().unwrap_or(0)},
            {"outcome": "inconclusive", "count": outcomes.get("inconclusive").copied().unwrap_or(0)}
        ]
    })
}

fn canonical_json_bytes(value: &Json) -> Vec<u8> {
    crate::promotion_query::canonical_json_bytes(value)
}

enum SolverOutput {
    Unsat,
    Sat,
    Unknown,
    Error(String),
}

fn run_solver(z3_path: &str, smt: &str) -> SolverOutput {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = match Command::new(z3_path)
        .arg("-in")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return SolverOutput::Error(format!("failed to spawn z3: {e}")),
    };

    if let Some(mut stdin) = child.stdin.take() {
        if let Err(e) = stdin.write_all(smt.as_bytes()) {
            return SolverOutput::Error(format!("failed to write to z3 stdin: {e}"));
        }
    }

    let output = match child.wait_with_output() {
        Ok(o) => o,
        Err(e) => return SolverOutput::Error(format!("z3 wait failed: {e}")),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if stdout.trim() == "unsat" {
        SolverOutput::Unsat
    } else if stdout.trim() == "sat" {
        SolverOutput::Sat
    } else if stdout.trim() == "unknown" {
        SolverOutput::Unknown
    } else {
        SolverOutput::Error(format!(
            "unexpected output: {} (stderr: {})",
            stdout.trim(),
            stderr.trim()
        ))
    }
}

fn short(cid: &str) -> String {
    if cid.len() > 20 {
        format!("{}...", &cid[..20])
    } else {
        cid.to_string()
    }
}
