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
use std::sync::Arc;

use chrono::{SecondsFormat, Utc};
use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value as CValue};
use provekit_ir_types::{
    MigrateReceiptEnvelope, PromotionDecisionEnvelope, PromotionDecisionHeader,
    PromotionDecisionMemento, PromotionDecisionMetadata, PromotionGate, PromotionResult,
    WitnessMemento,
};
use serde_json::json;
use serde_json::Value as Json;
use walkdir::WalkDir;

use crate::{EXIT_OK, EXIT_SOLVER_FAIL, EXIT_USER_ERROR, EXIT_VERIFY_FAIL};

pub fn run(args: crate::WitnessArgs) -> u8 {
    if args.command_or_contract == "consensus" {
        return run_consensus(args);
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
    let catalog_roots = if args.catalogs.is_empty() {
        let project_catalog = project_root.join(".provekit");
        if project_catalog.exists() {
            vec![project_catalog]
        } else {
            vec![project_root]
        }
    } else {
        args.catalogs.clone()
    };

    let witnesses = match collect_witnesses(&catalog_roots) {
        Ok(witnesses) => witnesses,
        Err(err) => {
            eprintln!("error: witness consensus catalog scan failed: {err}");
            return EXIT_USER_ERROR;
        }
    };
    let selected: Vec<WitnessMemento> = witnesses
        .into_iter()
        .filter(|witness| {
            witness.witness_for == concept
                && witness.fixture_state_cid == fixture
                && witness.outcome == "pass"
        })
        .collect();
    if selected.len() < args.min_witnesses {
        eprintln!(
            "witness consensus: rejected: {} matching passing witnesses, require {}",
            selected.len(),
            args.min_witnesses
        );
        return EXIT_VERIFY_FAIL;
    }

    let row_schema = match consensus_row_schema(&selected) {
        Ok(row_schema) => row_schema,
        Err(err) => {
            eprintln!("witness consensus: rejected: {err}");
            return EXIT_VERIFY_FAIL;
        }
    };

    let decision = match promotion_decision_for_consensus(
        &concept,
        &fixture,
        args.min_witnesses,
        &selected,
        row_schema,
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
            "witnesses_consulted": selected.len()
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&summary).unwrap_or_else(|_| summary.to_string())
        );
    } else if !args.out.quiet {
        println!("witness consensus: admitted");
        println!("  concept: {concept}");
        println!("  fixture: {fixture}");
        println!("  witnesses: {}", selected.len());
        println!("  agreement: byte-equal");
        println!("  receipt: {}", emit.display());
    }
    EXIT_OK
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

fn collect_witnesses(catalog_roots: &[PathBuf]) -> Result<Vec<WitnessMemento>, String> {
    let mut witnesses = Vec::new();
    for root in catalog_roots {
        if root.is_file() {
            collect_witnesses_from_file(root, &mut witnesses)?;
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
                collect_witnesses_from_file(entry.path(), &mut witnesses)?;
            }
        }
    }
    witnesses.sort_by(|left, right| left.cid.cmp(&right.cid));
    witnesses.dedup_by(|left, right| left.cid == right.cid);
    Ok(witnesses)
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

fn collect_witnesses_from_file(
    path: &Path,
    witnesses: &mut Vec<WitnessMemento>,
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
    if let Ok(witness) = serde_json::from_str::<WitnessMemento>(&text) {
        if witness.validate().is_ok() {
            witnesses.push(witness);
        }
        return Ok(());
    }
    if let Ok(receipt) = MigrateReceiptEnvelope::parse_json_str(&text) {
        witnesses.extend(receipt.witnesses);
    }
    Ok(())
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
) -> Result<PromotionDecisionMemento, String> {
    let mut evidence_cids: Vec<String> = witnesses.iter().map(|w| w.cid.clone()).collect();
    evidence_cids.sort();
    let subjects: Vec<String> = witnesses.iter().map(|w| w.subject.clone()).collect();
    let total_observations: u64 = witnesses.iter().map(|w| w.sample_count).sum();
    let policy_cid = cid_for_json(&json!({
        "kind": "consensus-policy",
        "min_witnesses": min_witnesses,
        "name": "provekit-witness-consensus-row-schema-v1"
    }));
    let decider_cid = cid_for_json(&json!({
        "kind": "decider",
        "name": "provekit-witness-consensus"
    }));
    let candidate_cid = cid_for_json(&json!({
        "concept": concept,
        "kind": "concept-candidate"
    }));
    let promoted_cid = cid_for_json(&json!({
        "concept": concept,
        "fixture_state_cid": fixture,
        "kind": "promoted-concept-tier",
        "tier": "empirically-witnessed"
    }));
    let reason = format!(
        "{} witnesses on 1 fixture, all measurements.row_schema byte-equal",
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
                "witnesses_consulted": evidence_cids
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

fn canonical_json_bytes(value: &Json) -> Vec<u8> {
    let canonical = json_to_value(value);
    encode_jcs(&canonical).into_bytes()
}

fn cid_for_json(value: &Json) -> String {
    blake3_512_of(&canonical_json_bytes(value))
}

fn json_to_value(j: &Json) -> Arc<CValue> {
    match j {
        Json::String(s) => CValue::string(s.clone()),
        Json::Number(n) => {
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
        Json::Bool(b) => CValue::boolean(*b),
        Json::Null => CValue::null(),
        Json::Array(items) => CValue::array(items.iter().map(json_to_value).collect()),
        Json::Object(map) => CValue::object(
            map.iter()
                .map(|(key, value)| (key.as_str(), json_to_value(value)))
                .collect::<Vec<_>>(),
        ),
    }
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
