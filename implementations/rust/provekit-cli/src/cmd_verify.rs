// SPDX-License-Identifier: Apache-2.0
//
// `provekit verify <kit>`: the keystone GATE verb (PR-9, issue #1405).
//
// This is the real `verify` verb. It is distinct from the old `verify`
// alias (which routed to `cmd_prove`, the six-stage prover / `--kit`
// lift-plugin conformance gate). `verify` runs the *kit-level
// verification flow* end-to-end and emits a verification receipt:
//
//   1. Lift the kit's contract claims. The lifter writes each
//      @sugar/@boundary annotation as a contract memento (referencing
//      its concept) into the kit's `.proof` catalog under `.provekit/`.
//      We load that catalog with `load_all_proofs` and enumerate the
//      contract claims via `enumerate_callsites` (the bridge/callsite
//      enumeration is exactly "function Y claims to satisfy concept X").
//
//   2. For each claim, resolve the target contract (its pre/post),
//      build the weakest-precondition / refinement obligation
//      ("does Y satisfy the contract"), using the same obligation
//      construction the prover uses (`resolve_target` + implication
//      post -> pre, falling back to `instantiate`).
//
//   3. Solver dispatch (resolved question Q-5, #1421). We inspect the
//      obligation's *theory class* with `provekit_verifier::classify`
//      and route to the best-fit solver via a SOLVER-DISPATCH TABLE
//      (`SolverPlan::Dispatch(DispatchConfig)`): obligation class ->
//      ordered solver choice. The backends already exist as crates;
//      this verb wires them, it does not build new solvers. The table
//      is the structure (see [`verify_dispatch_table`]); v1 routes LIA
//      / BV / strings to SMT-LIB (Z3 / cvc5 / bitwuzla), first-order
//      with quantifiers to Vampire, equational theory to Maude,
//      dependent / categorical to Lean, everything else to Z3.
//
//   4. Mint a witness memento citing the discharging solver, and sign
//      it (Ed25519, the existing proof-envelope signing helper). The
//      witness's signature records which solver produced the proof so
//      consumers can trust-discriminate.
//
//   5. Emit a verification receipt: per-claim pass/fail, the obligation
//      class, which solver each used, and the witness CIDs. JSON +
//      human-readable.
//
// SCOPE: This verb is a conductor. catalog lookup, wp/obligation
// construction, the solver-dispatch table, and signing all already
// exist in `provekit-verifier` / `provekit-proof-envelope`. PR-9 wires
// them into one verb; it reimplements none of them.

use std::path::PathBuf;

use clap::Parser;
use owo_colors::OwoColorize;
use provekit_canonicalizer::{blake3_512_of, encode_jcs};
use provekit_proof_envelope::{ed25519_pubkey_string, ed25519_sign_string};
use provekit_verifier::solvers::registry;
use provekit_verifier::{
    classify, enumerate_callsites, instantiate, load_all_proofs, resolve_target, run_plan,
    smt_emitter, DispatchConfig, FormulaTheory, MementoPool, ObligationVerdict, SolverHandle,
    SolverPlan, SolversConfig,
};
use serde_json::{json, Value as Json};

use crate::cmd_mint;
use crate::{EXIT_OK, EXIT_SOLVER_FAIL, EXIT_USER_ERROR, EXIT_VERIFY_FAIL};

// Ed25519 seed used to sign minted verification witnesses. Mirrors the
// runner's `RUN_SIGNER_SEED` convention (a deterministic developer seed;
// production minting overrides via the provenance key path). Distinct
// byte so verify-witness signers are distinguishable from run signers.
const VERIFY_SIGNER_SEED: [u8; 32] = [0x76; 32];

#[derive(Parser, Debug, Clone)]
pub struct VerifyArgs {
    /// Kit to verify (rust, go, cpp, ts, java, python, ...). Resolves to
    /// the kit's project root; its `.provekit/` catalog carries the
    /// lifted contract claims. Conflicts with `--project`.
    #[arg(conflicts_with = "project")]
    pub kit: Option<String>,

    /// Verify an explicit project root instead of a named kit. Defaults
    /// to the current directory when neither `--kit` positional nor this
    /// is given.
    #[arg(long)]
    pub project: Option<PathBuf>,

    /// Path to z3 binary (default: "z3" on PATH). Used for the SMT-LIB
    /// dispatch-table seat fallback.
    #[arg(long, default_value = "z3")]
    pub z3: String,

    /// Directory to write signed witness mementos into. Defaults to
    /// `<project>/.provekit/witnesses`.
    #[arg(long = "emit-witnesses")]
    pub emit_witnesses: Option<PathBuf>,

    #[command(flatten)]
    pub out: crate::OutputFlags,
}

/// The SOLVER-DISPATCH TABLE: obligation theory class -> solver seat.
///
/// This is the *structure* PR-9 introduces (resolved question Q-5,
/// #1421). It is a `DispatchConfig`, the same type `SolverPlan::Dispatch`
/// consumes, so adding a class -> solver mapping is a one-line edit and
/// the plan executor (`run_plan`) already knows how to route a formula
/// through it via `dispatch_for_formula`.
///
/// v1 routing (each seat is an existing backend crate, never a new
/// solver):
///   - linear-arithmetic / bitvectors / strings -> SMT-LIB (Z3 / cvc5 /
///     bitwuzla): the arithmetic + bitvector + array workhorse.
///   - equational-theory -> Maude (rewriting-logic / desugar-equivalence).
///   - dependent-type / categorical-structure -> Lean (theorem-level).
///   - default (and first-order with quantifiers that the SMT seats
///     punt on) -> Z3, with Vampire as the configured first-order seat
///     when the kit config registers it.
///
/// When the kit's `.provekit/config.toml` already declares a
/// `[solvers.dispatch]` table we honor it (the kit author's table wins);
/// otherwise this default table is used. Either way the routing is a
/// table, not a hardcoded single backend.
pub fn verify_dispatch_table() -> DispatchConfig {
    DispatchConfig {
        equational_theory: Some("maude".into()),
        strings: Some("cvc5".into()),
        bitvectors: Some("bitwuzla".into()),
        linear_arithmetic: Some("z3".into()),
        dependent_type: Some("lean".into()),
        categorical_structure: Some("lean".into()),
        default: Some("z3".into()),
    }
}

/// Per-claim verification outcome, the row of the receipt.
struct ClaimResult {
    property_name: String,
    property_cid: String,
    /// The obligation's theory class, as classified for dispatch.
    obligation_class: String,
    /// The solver seat the dispatch table routed this obligation to.
    routed_solver: String,
    /// The solver that actually returned the authoritative verdict.
    discharging_solver: String,
    verdict: ObligationVerdict,
    reason: String,
    /// CID of the signed witness minted for a discharged claim.
    witness_cid: Option<String>,
}

pub fn run(args: VerifyArgs) -> u8 {
    // Resolve the project root: a named kit, an explicit --project, or
    // the current directory.
    let project_root: PathBuf = if let Some(kit) = &args.kit {
        match cmd_mint::resolve_kit(kit) {
            Some((path, _surface, _lang)) => path,
            None => {
                let known: Vec<&str> = cmd_mint::KIT_TABLE.iter().map(|(a, _, _, _)| *a).collect();
                eprintln!(
                    "{}: unknown kit `{}`; known kits: {}",
                    "error".red().bold(),
                    kit,
                    known.join(", ")
                );
                return EXIT_USER_ERROR;
            }
        }
    } else {
        args.project.clone().unwrap_or_else(|| PathBuf::from("."))
    };

    if !project_root.exists() {
        eprintln!(
            "{}: project root not found: {} (run from repo root)",
            "error".red().bold(),
            project_root.display()
        );
        return EXIT_USER_ERROR;
    }

    let quiet = args.out.quiet;
    let json_out = args.out.json;

    if !quiet && !json_out {
        println!(
            "{}: {} contract claims from `{}`",
            "verify".cyan().bold(),
            "lifting".dimmed(),
            project_root.display()
        );
    }

    // Stage 1: lift the kit's contract claims = load the kit's `.proof`
    // catalog and enumerate its callsites/claims.
    let pool = load_all_proofs::run(&project_root);
    let callsites = enumerate_callsites::run(&pool);

    if callsites.is_empty() {
        // No contract claims to discharge. This is not a verification
        // failure: it is the verb correctly reporting an empty catalog.
        // (The lifter has not yet written claims, or the kit declares
        // none.) Surface it loudly so the operator knows to lift first.
        let kit_label = args.kit.clone().unwrap_or_else(|| project_root.display().to_string());
        if json_out {
            let out = json!({
                "kind": "verification-receipt",
                "schemaVersion": "1",
                "project": project_root.display().to_string(),
                "kit": args.kit,
                "claims": [],
                "totalClaims": 0,
                "discharged": 0,
                "failed": 0,
                "ok": true,
                "note": "no contract claims found in catalog; nothing to verify (lift the kit first)",
            });
            println!("{}", serde_json::to_string_pretty(&out).unwrap());
        } else if !quiet {
            println!(
                "{}: no contract claims found for `{}`; nothing to verify",
                "verify".yellow().bold(),
                kit_label
            );
            println!("  (lift the kit first: `provekit mint --kit={}` or run the kit's lifter)",
                args.kit.as_deref().unwrap_or("<kit>"));
        }
        return EXIT_OK;
    }

    // Build the solver-dispatch plan + registry. Honor the kit's own
    // config when present; otherwise use the default verify dispatch
    // table over a default registry.
    let (plan, solver_registry) = build_dispatch_plan_and_registry(&project_root, &args.z3);

    if !quiet && !json_out {
        println!(
            "  {} {} claims via solver-dispatch table",
            "discharging".dimmed(),
            callsites.len()
        );
    }

    // Witness output directory.
    let witness_dir = args
        .emit_witnesses
        .clone()
        .unwrap_or_else(|| project_root.join(".provekit").join("witnesses"));

    // Stage 2-4: per-claim discharge + witness mint.
    let mut results: Vec<ClaimResult> = Vec::with_capacity(callsites.len());
    for cs in &callsites {
        results.push(verify_one_claim(cs, &pool, &plan, &solver_registry, &witness_dir));
    }

    // Stage 5: emit receipt.
    let discharged = results
        .iter()
        .filter(|r| r.verdict == ObligationVerdict::Discharged)
        .count();
    let failed = results.len() - discharged;
    let ok = failed == 0;

    if json_out {
        emit_json_receipt(&project_root, &args.kit, &results, discharged, failed, ok);
    } else {
        emit_human_receipt(&results, discharged, failed, ok, quiet);
    }

    // Exit code: any non-discharged claim is a verification failure;
    // an Undecidable verdict is a solver failure (exit 3) only when
    // there were no hard violations.
    if ok {
        EXIT_OK
    } else if results
        .iter()
        .any(|r| r.verdict == ObligationVerdict::Unsatisfied)
    {
        EXIT_VERIFY_FAIL
    } else {
        // All failures were Undecidable / Disagreement: solver failure.
        EXIT_SOLVER_FAIL
    }
}

/// Build the dispatch plan + solver registry for verification.
///
/// Precedence:
///   1. If the kit's `.provekit/config.toml` declares a solver config,
///      build the registry from it. If that config declares a
///      `[solvers.dispatch]` table, use it (kit author's table wins);
///      otherwise overlay the default verify dispatch table over the
///      kit's registered solver seats.
///   2. Otherwise: default registry (Z3 at `--z3`) + the default verify
///      dispatch table.
fn build_dispatch_plan_and_registry(
    project_root: &std::path::Path,
    z3_path: &str,
) -> (SolverPlan, std::collections::HashMap<String, SolverHandle>) {
    if let Ok(Some(sc)) = SolversConfig::load(project_root) {
        let reg = registry::build(&sc);
        // Use the kit's own dispatch table if it declared one; else the
        // default verify table routed over the kit's registered seats.
        let plan = match SolverPlan::from_config(&sc) {
            SolverPlan::Dispatch(d) => SolverPlan::Dispatch(d),
            _ => SolverPlan::Dispatch(verify_dispatch_table()),
        };
        return (plan, reg);
    }
    // Fallback: a single Z3 seat is enough to drive the LIA / default
    // rows of the dispatch table. Other seats simply miss the registry
    // and `run_plan` reports Undecidable for those rows (loudly), which
    // is the correct posture for a host lacking those backends.
    let reg = registry::build_default_z3(z3_path);
    (SolverPlan::Dispatch(verify_dispatch_table()), reg)
}

/// Discharge a single contract claim and mint a witness if it holds.
fn verify_one_claim(
    cs: &provekit_verifier::CallSite,
    pool: &MementoPool,
    plan: &SolverPlan,
    solver_registry: &std::collections::HashMap<String, SolverHandle>,
    witness_dir: &std::path::Path,
) -> ClaimResult {
    let mut result = ClaimResult {
        property_name: cs.property_name.clone(),
        property_cid: cs.property_cid.clone(),
        obligation_class: FormulaTheory::Default.as_str().to_string(),
        routed_solver: "<none>".to_string(),
        discharging_solver: "<none>".to_string(),
        verdict: ObligationVerdict::Undecidable,
        reason: String::new(),
        witness_cid: None,
    };

    // Resolve the claim's target contract (its pre formula = the
    // refinement target).
    let resolved = match resolve_target::run(cs, pool) {
        Ok(r) => r,
        Err(e) => {
            result.reason = format!("resolve-target: {e}");
            return result;
        }
    };

    // Build the obligation. When the target has no precondition the
    // claim is vacuously discharged (nothing to prove). Otherwise build
    // the wp / refinement obligation: instantiate the resolved pre with
    // the call's arg term (the same obligation the prover builds).
    let Some(_pre) = resolved.ir_formula.as_ref() else {
        result.verdict = ObligationVerdict::Discharged;
        result.reason = "vacuous: target carries no precondition".to_string();
        result.obligation_class = "vacuous".to_string();
        result.discharging_solver = "none (vacuous)".to_string();
        return result;
    };

    let obligation = match instantiate::run(&resolved, &cs.arg_term) {
        Ok(ob) => ob.ir_formula,
        Err(e) => {
            result.reason = format!("instantiate: {e}");
            return result;
        }
    };

    // Classify the obligation for the dispatch table + emit SMT-LIB.
    let theory = classify(&obligation);
    result.obligation_class = theory.as_str().to_string();
    result.routed_solver = routed_seat(plan, theory);

    let smt = match smt_emitter::emit(&obligation) {
        Ok(s) => s,
        Err(e) => {
            result.reason = format!("smt-emit: {e}");
            return result;
        }
    };

    // Dispatch through the solver-dispatch table.
    let (verdict, reason, invs) = run_plan(plan, solver_registry, &smt, Some(&obligation));
    result.verdict = verdict;
    result.reason = reason;
    result.discharging_solver = invs
        .iter()
        .find(|i| i.authoritative)
        .map(|i| format!("{}@{}", i.result.solver_name, i.result.solver_version))
        .unwrap_or_else(|| "<none>".to_string());

    // Mint + sign a witness for a discharged claim.
    if verdict == ObligationVerdict::Discharged {
        match mint_verification_witness(cs, &obligation, &result.discharging_solver, witness_dir) {
            Ok(cid) => result.witness_cid = Some(cid),
            Err(e) => {
                // Minting failure does not flip the verdict (the proof
                // held); surface it on the row.
                result.reason = format!("{} [witness-mint warning: {e}]", result.reason);
            }
        }
    }

    result
}

/// The solver seat the dispatch table routes `theory` to (for the
/// receipt's per-claim "routed solver" field). Returns the table's
/// seat for the class, falling back to its `default`.
fn routed_seat(plan: &SolverPlan, theory: FormulaTheory) -> String {
    let SolverPlan::Dispatch(d) = plan else {
        return "<plan-not-dispatch>".to_string();
    };
    let by_theory = match theory {
        FormulaTheory::EquationalTheory => d.equational_theory.as_deref(),
        FormulaTheory::Strings => d.strings.as_deref(),
        FormulaTheory::Bitvectors => d.bitvectors.as_deref(),
        FormulaTheory::LinearArithmetic => d.linear_arithmetic.as_deref(),
        FormulaTheory::DependentType => d.dependent_type.as_deref(),
        FormulaTheory::CategoricalStructure => d.categorical_structure.as_deref(),
        FormulaTheory::Default => None,
    };
    by_theory
        .or(d.default.as_deref())
        .unwrap_or("<unrouted>")
        .to_string()
}

/// Mint a signed `WitnessMemento` citing the discharging solver and
/// write it to `witness_dir`. Returns the witness CID.
///
/// The signature is over JCS({header-content}) with the verify signer
/// seed; `signed_by` carries the signer's self-identifying public key,
/// and the discharging solver is recorded in `measurements.solver` so
/// consumers can trust-discriminate by prover.
fn mint_verification_witness(
    cs: &provekit_verifier::CallSite,
    obligation: &Json,
    discharging_solver: &str,
    witness_dir: &std::path::Path,
) -> Result<String, String> {
    std::fs::create_dir_all(witness_dir).map_err(|e| format!("create {}: {e}", witness_dir.display()))?;

    let pubkey = ed25519_pubkey_string(&VERIFY_SIGNER_SEED);
    let observed_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    // The obligation CID is the fixture state this witness pins to.
    let obligation_jcs = jcs_of_json(obligation)?;
    let obligation_cid = blake3_512_of(obligation_jcs.as_bytes());

    let measurements = json!({
        "solver": discharging_solver,
        "obligation_cid": obligation_cid,
        "bridge_ir_name": cs.bridge_ir_name,
        "source_layer": cs.bridge_source_layer,
        "target_layer": cs.bridge_target_layer,
    });

    // Build the content over which the CID + signature are computed.
    // (Mirrors the WitnessMemento envelope/header convention: the CID is
    // blake3-512 of the JCS of the signable content; the signature is
    // over the same bytes.)
    let signable = json!({
        "kind": "witness",
        "schemaVersion": "1",
        "witness_for": cs.property_cid,
        "subject": cs.bridge_target_cid,
        "fixture_state_cid": obligation_cid,
        "observed_at": observed_at,
        "sample_count": 1,
        "measurements": measurements,
        "outcome": "pass",
        "signed_by": pubkey,
    });
    let signable_jcs = jcs_of_json(&signable)?;
    let cid = blake3_512_of(signable_jcs.as_bytes());
    let signature = ed25519_sign_string(&VERIFY_SIGNER_SEED, signable_jcs.as_bytes());

    let witness = provekit_ir_types::WitnessMemento {
        kind: "witness".to_string(),
        schema_version: "1".to_string(),
        witness_for: cs.property_cid.clone(),
        subject: cs.bridge_target_cid.clone(),
        fixture_state_cid: obligation_cid,
        observed_at,
        sample_count: 1,
        measurements,
        outcome: "pass".to_string(),
        signed_by: Some(pubkey),
        signature: Some(signature),
        cid: cid.clone(),
    };

    let bytes = serde_json::to_string_pretty(&witness)
        .map_err(|e| format!("serialize witness: {e}"))?;
    let hex = cid.trim_start_matches("blake3-512:");
    let path = witness_dir.join(format!("witness-{hex}.json"));
    std::fs::write(&path, format!("{bytes}\n")).map_err(|e| format!("write {}: {e}", path.display()))?;

    Ok(cid)
}

fn jcs_of_json(v: &Json) -> Result<String, String> {
    let canonical = json_to_canonical(v)?;
    Ok(encode_jcs(&canonical))
}

/// Convert serde_json into the canonicalizer's Value for JCS encoding.
/// (Mirrors the runner's `json_to_canonical`; integers only — witness
/// payloads carry no floats.)
fn json_to_canonical(value: &Json) -> Result<std::sync::Arc<provekit_canonicalizer::Value>, String> {
    use provekit_canonicalizer::Value as CV;
    match value {
        Json::Null => Ok(CV::null()),
        Json::Bool(b) => Ok(CV::boolean(*b)),
        Json::Number(n) => n
            .as_i64()
            .map(CV::integer)
            .ok_or_else(|| format!("unsupported non-integer number in witness payload: {n}")),
        Json::String(s) => Ok(CV::string(s.clone())),
        Json::Array(items) => Ok(CV::array(
            items
                .iter()
                .map(json_to_canonical)
                .collect::<Result<Vec<_>, _>>()?,
        )),
        Json::Object(obj) => Ok(CV::object(
            obj.iter()
                .map(|(k, v)| Ok((k.clone(), json_to_canonical(v)?)))
                .collect::<Result<Vec<_>, String>>()?,
        )),
    }
}

fn emit_json_receipt(
    project_root: &std::path::Path,
    kit: &Option<String>,
    results: &[ClaimResult],
    discharged: usize,
    failed: usize,
    ok: bool,
) {
    let claims: Vec<Json> = results
        .iter()
        .map(|r| {
            json!({
                "property": r.property_name,
                "propertyCid": r.property_cid,
                "obligationClass": r.obligation_class,
                "routedSolver": r.routed_solver,
                "dischargingSolver": r.discharging_solver,
                "status": r.verdict.as_str(),
                "pass": r.verdict == ObligationVerdict::Discharged,
                "reason": r.reason,
                "witnessCid": r.witness_cid,
            })
        })
        .collect();
    let out = json!({
        "kind": "verification-receipt",
        "schemaVersion": "1",
        "project": project_root.display().to_string(),
        "kit": kit,
        "claims": claims,
        "totalClaims": results.len(),
        "discharged": discharged,
        "failed": failed,
        "ok": ok,
    });
    match serde_json::to_string_pretty(&out) {
        Ok(s) => println!("{s}"),
        Err(e) => eprintln!("{}: serialize receipt JSON: {e}", "error".red().bold()),
    }
}

fn emit_human_receipt(
    results: &[ClaimResult],
    discharged: usize,
    failed: usize,
    ok: bool,
    quiet: bool,
) {
    if quiet {
        return;
    }
    println!();
    println!("{}", "ProvekIt verification receipt".bold());
    for r in results {
        let status = match r.verdict {
            ObligationVerdict::Discharged => "pass".green().to_string(),
            ObligationVerdict::Unsatisfied => "FAIL".red().to_string(),
            ObligationVerdict::Undecidable => "undecidable".yellow().to_string(),
            ObligationVerdict::Disagreement => "disagreement".yellow().to_string(),
        };
        println!(
            "  [{}] {}  (class={}, solver={})",
            status, r.property_name, r.obligation_class, r.discharging_solver
        );
        if let Some(cid) = &r.witness_cid {
            println!("        witness: {}", short_cid(cid).dimmed());
        }
        if !r.reason.is_empty() {
            println!("        {}", r.reason.dimmed());
        }
    }
    println!();
    let summary = format!(
        "{} claims: {} discharged, {} failed",
        results.len(),
        discharged,
        failed
    );
    if ok {
        println!("{}: {}", "pass".green().bold(), summary);
    } else {
        println!("{}: {}", "FAIL".red().bold(), summary);
    }
}

fn short_cid(cid: &str) -> String {
    let hex = cid.trim_start_matches("blake3-512:");
    let take: String = hex.chars().take(16).collect();
    format!("blake3-512:{take}...")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_table_routes_each_theory_class() {
        // The solver-dispatch table is the structure: every theory class
        // maps to a seat, with a default fallback. This is the test that
        // PR-9's "the table is exercised, and supports adding more"
        // requirement leans on.
        let plan = SolverPlan::Dispatch(verify_dispatch_table());
        assert_eq!(routed_seat(&plan, FormulaTheory::LinearArithmetic), "z3");
        assert_eq!(routed_seat(&plan, FormulaTheory::Bitvectors), "bitwuzla");
        assert_eq!(routed_seat(&plan, FormulaTheory::Strings), "cvc5");
        assert_eq!(routed_seat(&plan, FormulaTheory::EquationalTheory), "maude");
        assert_eq!(routed_seat(&plan, FormulaTheory::DependentType), "lean");
        assert_eq!(
            routed_seat(&plan, FormulaTheory::CategoricalStructure),
            "lean"
        );
        // Default class with no class-specific seat falls back to default.
        assert_eq!(routed_seat(&plan, FormulaTheory::Default), "z3");
    }

    #[test]
    fn dispatch_routes_lia_obligation_to_smt_seat() {
        // At least one obligation routed to SMT-LIB/Z3 (PR-9 verification
        // requirement). A linear-arithmetic obligation classifies as LIA,
        // which the table routes to the z3 SMT seat.
        let lia = json!({
            "kind": "atomic",
            "name": ">",
            "args": [{"kind":"var","name":"n"}, {"kind":"const","value":0}]
        });
        assert_eq!(classify(&lia), FormulaTheory::LinearArithmetic);
        let plan = SolverPlan::Dispatch(verify_dispatch_table());
        assert_eq!(routed_seat(&plan, classify(&lia)), "z3");
    }

    #[test]
    fn witness_payload_canonicalizes_and_signs() {
        // A minted witness must canonicalize (JCS) and produce a verifiable
        // CID + signature. This exercises the signing path without needing
        // a real solver or catalog.
        let tmp = std::env::temp_dir().join(format!(
            "provekit-verify-test-{}",
            std::process::id()
        ));
        let cs = provekit_verifier::CallSite {
            bridge_ir_name: "demo_bridge".into(),
            bridge_target_cid: "blake3-512:target".into(),
            bridge_source_layer: "rust".into(),
            bridge_target_layer: "concept".into(),
            bridge_target_proof_cid: None,
            property_name: "demo_property".into(),
            property_cid: "blake3-512:prop".into(),
            arg_term: None,
        };
        let obligation = json!({"kind":"atomic","name":"true","args":[]});
        let cid = mint_verification_witness(&cs, &obligation, "z3@4.x", &tmp)
            .expect("witness mint must succeed");
        assert!(cid.starts_with("blake3-512:"));
        // The witness file exists and re-parses as a WitnessMemento.
        let hex = cid.trim_start_matches("blake3-512:");
        let path = tmp.join(format!("witness-{hex}.json"));
        let bytes = std::fs::read_to_string(&path).expect("witness file written");
        let parsed: provekit_ir_types::WitnessMemento =
            serde_json::from_str(&bytes).expect("witness re-parses");
        assert_eq!(parsed.outcome, "pass");
        assert_eq!(parsed.measurements["solver"], "z3@4.x");
        assert!(parsed.signature.is_some());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
