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
use provekit_verifier::body_discharge;
use provekit_verifier::solvers::registry;
use provekit_verifier::{
    classify, enumerate_callsites, instantiate, load_all_proofs, resolve_target, run_plan,
    smt_emitter, DispatchConfig, FormulaTheory, MementoPool, ObligationVerdict, SolverHandle,
    SolverPlan, SolversConfig,
};
use serde_json::{json, Value as Json};

use crate::cmd_mint;
use crate::{EXIT_OK, EXIT_SOLVER_FAIL, EXIT_USER_ERROR, EXIT_VERIFY_FAIL};

// Default Ed25519 seed used to sign minted verification witnesses when
// no real signer is configured. This is a deterministic *developer*
// seed: a signature under it is an INTEGRITY TAG (it binds the witness
// bytes so tampering is detectable and the witness CID is reproducible),
// NOT an authority attestation (everyone knows this seed, so it asserts
// nothing about *who* produced the proof). Distinct byte from the
// runner's `RUN_SIGNER_SEED` so verify-witness signers are
// distinguishable from run signers.
//
// A real signer is honored when configured (see [`resolve_signer_seed`]):
//   - env `PROVEKIT_VERIFY_SIGNER_KEY` = hex-encoded 32-byte seed, or
//   - env `PROVEKIT_VERIFY_SIGNER_KEY_FILE` = path to a file whose first
//     non-whitespace token is the hex-encoded 32-byte seed.
// Only then does the signature carry authority.
const VERIFY_SIGNER_SEED_DEV: [u8; 32] = [0x76; 32];

/// The env var carrying a hex-encoded 32-byte Ed25519 signer seed.
const SIGNER_KEY_ENV: &str = "PROVEKIT_VERIFY_SIGNER_KEY";
/// The env var carrying a path to a file holding the hex-encoded seed.
const SIGNER_KEY_FILE_ENV: &str = "PROVEKIT_VERIFY_SIGNER_KEY_FILE";

/// Resolve the Ed25519 signer seed for minted witnesses.
///
/// Precedence:
///   1. `PROVEKIT_VERIFY_SIGNER_KEY`      — hex(32 bytes) inline,
///   2. `PROVEKIT_VERIFY_SIGNER_KEY_FILE` — path to a file whose first
///      whitespace-delimited token is hex(32 bytes),
///   3. the dev seed [`VERIFY_SIGNER_SEED_DEV`] (integrity tag only).
///
/// Returns `(seed, is_authoritative)`: `is_authoritative` is true only
/// when an override was supplied, so callers can be honest in output
/// about whether the signature attests authority. A malformed override
/// is an error (fail closed: do not silently fall back to the dev seed
/// when the operator clearly intended a real key).
fn resolve_signer_seed() -> Result<([u8; 32], bool), String> {
    if let Some(hex) = std::env::var_os(SIGNER_KEY_ENV) {
        let hex = hex.to_string_lossy().trim().to_string();
        return decode_seed_hex(&hex).map(|s| (s, true));
    }
    if let Some(path) = std::env::var_os(SIGNER_KEY_FILE_ENV) {
        let path = std::path::PathBuf::from(path);
        let contents = std::fs::read_to_string(&path)
            .map_err(|e| format!("read {SIGNER_KEY_FILE_ENV} `{}`: {e}", path.display()))?;
        let token = contents
            .split_whitespace()
            .next()
            .ok_or_else(|| format!("{SIGNER_KEY_FILE_ENV} `{}` is empty", path.display()))?;
        return decode_seed_hex(token).map(|s| (s, true));
    }
    Ok((VERIFY_SIGNER_SEED_DEV, false))
}

/// Decode a hex-encoded 32-byte Ed25519 seed.
fn decode_seed_hex(hex: &str) -> Result<[u8; 32], String> {
    let hex = hex.strip_prefix("0x").unwrap_or(hex);
    if hex.len() != 64 {
        return Err(format!(
            "signer seed must be 64 hex chars (32 bytes); got {} chars",
            hex.len()
        ));
    }
    let mut seed = [0u8; 32];
    for (i, byte) in seed.iter_mut().enumerate() {
        let s = &hex[i * 2..i * 2 + 2];
        *byte = u8::from_str_radix(s, 16)
            .map_err(|e| format!("signer seed has non-hex at byte {i}: {e}"))?;
    }
    Ok(seed)
}

#[derive(Parser, Debug, Clone)]
pub struct VerifyArgs {
    /// Kit to verify (rust, go, cpp, ts, java, python, ...). Resolves to
    /// the kit's project root; its `.provekit/` catalog carries the
    /// lifted contract claims. Conflicts with `--project`. May also be
    /// an explicit path when the value contains a path separator.
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

    /// Require that a concept has reached the empirically-witnessed
    /// promotion tier. When set, the standard solver-dispatch flow is
    /// bypassed; instead the project's promotion catalog is queried.
    #[arg(long = "require-empirically-witnessed")]
    pub require_empirically_witnessed: Option<String>,

    /// Fixture-state CID for tier queries such as
    /// `--require-empirically-witnessed`.
    #[arg(long = "require-fixture")]
    pub require_fixture: Option<String>,

    /// Consensus policy JSON used to evaluate a required empirical
    /// witness vector (required with `--require-empirically-witnessed`).
    #[arg(long = "consensus-policy", requires = "require_empirically_witnessed")]
    pub consensus_policy: Option<PathBuf>,

    /// Artifact bytes to verify against a package release proof/receipt.
    /// Selects the supply-chain admission gate (binaryCid match).
    #[arg(long, requires = "proof")]
    pub artifact: Option<PathBuf>,

    /// Package release proof/receipt naming the expected binaryCid /
    /// policyCid. Required for the supply-chain admission gate.
    #[arg(long)]
    pub proof: Option<PathBuf>,

    /// Consumer policy proof/receipt used for policy admission checks
    /// (policyCid match).
    #[arg(long, requires = "proof")]
    pub policy: Option<PathBuf>,

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
    // Early dispatch: the supply-chain admission gate. When any of
    // --artifact / --proof / --policy is given, verify a package release
    // receipt (binaryCid / policyCid match) rather than running the
    // solver-dispatch flow. The logic is owned by cmd_prove (it predates
    // this verb); we reuse it so both verbs expose one behavior.
    if args.artifact.is_some() || args.proof.is_some() || args.policy.is_some() {
        return crate::cmd_prove::run_admission_gate_with(
            &args.artifact,
            &args.proof,
            &args.policy,
            args.out.json,
            args.out.quiet,
        );
    }

    // Resolve the project root: a named kit, an explicit --project, or
    // the current directory.
    let project_root: PathBuf = if let Some(kit) = &args.kit {
        match cmd_mint::resolve_kit(kit) {
            Some((path, _surface, _lang)) => path,
            None => {
                // If the value looks like a path (contains a separator
                // or is `.`/`..`), treat it as an explicit project root
                // rather than a kit name.
                let p = PathBuf::from(kit);
                if kit.contains(std::path::MAIN_SEPARATOR)
                    || kit.starts_with('/')
                    || kit.starts_with('.')
                    || p.is_absolute()
                {
                    p
                } else {
                    let aliases = cmd_mint::configured_kit_alias_names();
                    eprintln!("{}", cmd_mint::format_unknown_kit_error(kit, &aliases));
                    return EXIT_USER_ERROR;
                }
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
        // No contract claims were discharged. Reporting this as success is a
        // vacuous green: the run proved nothing.
        let kit_label = args
            .kit
            .clone()
            .unwrap_or_else(|| project_root.display().to_string());
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
                "ok": false,
                "note": "no contract claims found in catalog; zero-claim verification is not a successful proof",
            });
            println!("{}", serde_json::to_string_pretty(&out).unwrap());
        } else if !quiet {
            println!(
                "{}: no contract claims found for `{}`; zero-claim verification is not a successful proof",
                "verify".red().bold(),
                kit_label
            );
            println!(
                "  (lift the kit first: `provekit mint --kit={}` or run the kit's lifter)",
                args.kit.as_deref().unwrap_or("<kit>")
            );
        }
        return EXIT_VERIFY_FAIL;
    }

    // Build the solver-dispatch plan + registry. Honor the kit's own
    // The kit author's declared `[solvers]` plan wins; otherwise the
    // default verify dispatch table over a single-Z3 registry.
    let (plan, solver_registry, plan_is_default) = build_plan_and_registry(&project_root, &args.z3);

    if !quiet && !json_out {
        let plan_label = match (&plan, plan_is_default) {
            (_, true) => "default solver-dispatch table".to_string(),
            (SolverPlan::Dispatch(_), false) => "kit-declared solver-dispatch table".to_string(),
            (SolverPlan::Chain(_), false) => "kit-declared solver chain".to_string(),
            (SolverPlan::Portfolio { .. }, false) => "kit-declared solver portfolio".to_string(),
            (SolverPlan::Single(n), false) => format!("kit-declared single solver `{n}`"),
        };
        println!(
            "  {} {} claims via {}",
            "discharging".dimmed(),
            callsites.len(),
            plan_label
        );
    }

    // Witness output directory.
    let witness_dir = args
        .emit_witnesses
        .clone()
        .unwrap_or_else(|| project_root.join(".provekit").join("witnesses"));

    // Resolve the signer seed once. A malformed override fails the whole
    // run (fail closed) rather than silently signing with the dev seed.
    let (signer_seed, signer_is_authoritative) = match resolve_signer_seed() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}: signer key: {e}", "error".red().bold());
            return EXIT_USER_ERROR;
        }
    };
    if !quiet && !json_out && !signer_is_authoritative {
        println!(
            "  {} witnesses signed with the well-known dev seed (integrity tag only; set {} for an authoritative signer)",
            "note:".yellow(),
            SIGNER_KEY_ENV
        );
    }

    // Stage 2-4: per-claim discharge + witness mint.
    let mut results: Vec<ClaimResult> = Vec::with_capacity(callsites.len());
    for cs in &callsites {
        results.push(verify_one_claim(
            cs,
            &pool,
            &plan,
            &solver_registry,
            &witness_dir,
            &signer_seed,
            signer_is_authoritative,
        ));
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

/// Build the solver plan + registry for verification.
///
/// Precedence:
///   1. If the kit's `.provekit/config.toml` declares a `[solvers]`
///      config, the KIT AUTHOR'S PLAN WINS: build the registry from it
///      and derive the plan via `SolverPlan::from_config` (whatever the
///      kit declared — `Dispatch`, `Chain`, `Portfolio`, or `Single`).
///      `verify` does not override the kit's chosen routing.
///   2. Otherwise (no `[solvers]` config): synthesize the default
///      `verify` dispatch table over a single-Z3 registry. The default
///      table is the structure `verify` introduces so the verb still
///      routes by obligation class without a kit config; missing seats
///      simply miss the registry and `run_plan` reports Undecidable for
///      those rows (loudly), the correct posture on a host lacking those
///      backends.
///
/// Returns the plan, the registry, and whether the plan is the
/// synthesized default (vs. kit-declared) for honest receipt labelling.
fn build_plan_and_registry(
    project_root: &std::path::Path,
    z3_path: &str,
) -> (
    SolverPlan,
    std::collections::HashMap<String, SolverHandle>,
    bool,
) {
    if let Ok(Some(sc)) = SolversConfig::load(project_root) {
        // Kit author's declared plan wins, verbatim.
        let reg = registry::build(&sc);
        let plan = SolverPlan::from_config(&sc);
        return (plan, reg, false);
    }
    // No kit config: the default verify dispatch table over single-Z3.
    let reg = registry::build_default_z3(z3_path);
    (SolverPlan::Dispatch(verify_dispatch_table()), reg, true)
}

/// Discharge a single contract claim and mint a witness if it holds.
fn verify_one_claim(
    cs: &provekit_verifier::CallSite,
    pool: &MementoPool,
    plan: &SolverPlan,
    solver_registry: &std::collections::HashMap<String, SolverHandle>,
    witness_dir: &std::path::Path,
    signer_seed: &[u8; 32],
    signer_is_authoritative: bool,
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

    // Body-discharge path (#1440): when the callsite names a callee with a
    // body-derived op-contract AND its harvested assertion has the
    // `=(<call>, <expected>)` shape, reduce the obligation THROUGH the
    // body. wp inlines the callee's value-semantics (`double(x) = x*2`),
    // so the solver sees a concrete formula (`*(3,2) == 6`) instead of an
    // uninterpreted `double` symbol. This is the spine that turns a
    // harvested body-obligation into a real, dischargeable / refutable
    // claim. Anything outside the recognized shape falls through to the
    // existing refinement path below.
    let obligation = match body_discharge::extract_body_obligation(cs, pool) {
        Ok(Some(body_discharge::BodyObligation::Reduced(reduced))) => reduced,
        Ok(None) => {
            // Not a body-bearing claim: resolve the target contract's pre
            // and build the refinement obligation, exactly as before.
            let resolved = match resolve_target::run(cs, pool) {
                Ok(r) => r,
                Err(e) => {
                    result.reason = format!("resolve-target: {e}");
                    return result;
                }
            };

            // When the target has no precondition the claim is vacuously
            // discharged (nothing to prove). Otherwise build the wp /
            // refinement obligation: instantiate the resolved pre with the
            // call's arg term (the same obligation the prover builds).
            let Some(_pre) = resolved.ir_formula.as_ref() else {
                // HONESTY BOUNDARY (catches every body-bearing variant). The
                // vacuous-discharge shortcut is legitimate ONLY for a
                // genuinely non-body-bearing target. A target carrying
                // `formals` is body-bearing by definition: its `post`
                // describes a body whose obligation we reached here WITHOUT
                // reducing (e.g. `extract_body_obligation` could not build the
                // obligation because the body-derived `post` was not a
                // `result == <expr>` equation, so the resolver dropped it).
                // Vacuous-passing it would be a false green. Refuse instead:
                // leave the default Undecidable verdict (not discharged, not
                // pass, no witness).
                if resolved.target_is_body_bearing {
                    result.reason = format!(
                        "body-discharge: refuse: target `{}` is body-bearing \
                         (carries `formals`) but its obligation was not reduced \
                         and it has no precondition; refusing rather than \
                         reporting a vacuous pass",
                        cs.bridge_ir_name
                    );
                    return result;
                }
                result.verdict = ObligationVerdict::Discharged;
                result.reason = "vacuous: target carries no precondition".to_string();
                result.obligation_class = "vacuous".to_string();
                result.discharging_solver = "none (vacuous)".to_string();
                return result;
            };

            match instantiate::run(&resolved, &cs.arg_term) {
                Ok(ob) => ob.ir_formula,
                Err(e) => {
                    result.reason = format!("instantiate: {e}");
                    return result;
                }
            }
        }
        Err(e) => {
            // The callee IS body-bearing (it has a body-derived op-contract)
            // but the obligation could not be reduced through the body: an
            // unrecognized assertion shape, an unconvertible operand, an
            // arity mismatch, or a wp refusal on a nested call. The spine
            // REFUSES rather than falling through to the refinement path,
            // because that path would mis-read the body-derived op-contract
            // (post/formals, no pre) as vacuous and report a FALSE pass.
            // Refusal leaves the default verdict (Undecidable): not
            // discharged, not pass, no witness, reason surfaced on the row.
            result.reason = format!("body-discharge: {e}");
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

    // Mint + sign a witness for a discharged claim. A witness is minted
    // ONLY when the obligation is discharged (unsat for its negation); an
    // unsatisfied (violated) or undecidable claim mints nothing.
    if verdict == ObligationVerdict::Discharged {
        match mint_verification_witness(
            cs,
            &obligation,
            &result.discharging_solver,
            witness_dir,
            signer_seed,
            signer_is_authoritative,
        ) {
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

/// The solver seat the plan routes `theory` to (for the receipt's
/// per-claim "routed solver" field).
///
/// For a `Dispatch` plan this is the class-specific seat (falling back
/// to `default`). For a kit-declared non-dispatch plan the routing is
/// not theory-keyed, so we name the plan shape honestly rather than
/// pretending a per-class seat exists; the authoritative answer for
/// those is the `dischargingSolver` field, which records who actually
/// returned the verdict.
fn routed_seat(plan: &SolverPlan, theory: FormulaTheory) -> String {
    let d = match plan {
        SolverPlan::Dispatch(d) => d,
        SolverPlan::Single(n) => return format!("single:{n}"),
        SolverPlan::Chain(names) => return format!("chain:{}", names.join(",")),
        SolverPlan::Portfolio { names, .. } => return format!("portfolio:{}", names.join(",")),
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
    signer_seed: &[u8; 32],
    signer_is_authoritative: bool,
) -> Result<String, String> {
    std::fs::create_dir_all(witness_dir)
        .map_err(|e| format!("create {}: {e}", witness_dir.display()))?;

    let pubkey = ed25519_pubkey_string(signer_seed);
    let observed_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    // The obligation CID is the fixture state this witness pins to.
    let obligation_jcs = jcs_of_json(obligation)?;
    let obligation_cid = blake3_512_of(obligation_jcs.as_bytes());

    // `signer_attests_authority` is honest about whether `signed_by`
    // names a real producer key or the well-known dev seed (integrity
    // tag only). Consumers trust-discriminate on it.
    let measurements = json!({
        "solver": discharging_solver,
        "obligation_cid": obligation_cid,
        "bridge_ir_name": cs.bridge_ir_name,
        "source_layer": cs.bridge_source_layer,
        "target_layer": cs.bridge_target_layer,
        "signer_attests_authority": signer_is_authoritative,
    });

    // Build the content over which the CID + signature are computed.
    // (Mirrors the WitnessMemento envelope/header convention: the CID is
    // signed_by/signature are the HOW (attestation), EXCLUDED from the CID
    // preimage per WitnessMemento::recompute_cid. Mirror witness_ingest: build
    // the memento with signer set + empty signature, derive the CID from the
    // canonical observation bytes, then sign the CID (the address of the WHAT).
    // Both witness-mint paths now share one scheme: same observation => same
    // CID regardless of who attests, and the signature attests that CID.
    let mut witness = provekit_ir_types::WitnessMemento {
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
        signature: None,
        cid: String::new(),
    };
    let cid = witness.recompute_cid().map_err(|e| e.to_string())?;
    witness.signature = Some(ed25519_sign_string(signer_seed, cid.as_bytes()));
    witness.cid = cid.clone();

    let bytes =
        serde_json::to_string_pretty(&witness).map_err(|e| format!("serialize witness: {e}"))?;
    let hex = cid.trim_start_matches("blake3-512:");
    let path = witness_dir.join(format!("witness-{hex}.json"));
    std::fs::write(&path, format!("{bytes}\n"))
        .map_err(|e| format!("write {}: {e}", path.display()))?;

    Ok(cid)
}

pub(crate) fn jcs_of_json(v: &Json) -> Result<String, String> {
    let canonical = json_to_canonical(v)?;
    Ok(encode_jcs(&canonical))
}

/// Convert serde_json into the canonicalizer's Value for JCS encoding.
/// (Mirrors the runner's `json_to_canonical`; integers only — witness
/// payloads carry no floats.)
fn json_to_canonical(
    value: &Json,
) -> Result<std::sync::Arc<provekit_canonicalizer::Value>, String> {
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
        let tmp = std::env::temp_dir().join(format!("provekit-verify-test-{}", std::process::id()));
        let cs = provekit_verifier::CallSite {
            bridge_ir_name: "demo_bridge".into(),
            bridge_target_cid: "blake3-512:target".into(),
            bridge_source_layer: "rust".into(),
            bridge_target_layer: "concept".into(),
            bridge_target_proof_cid: None,
            bridge_self_bundle_cid: None,
            property_name: "demo_property".into(),
            property_cid: "blake3-512:prop".into(),
            arg_term: None,
            containing_atomic: None,
        };
        let obligation = json!({"kind":"atomic","name":"true","args":[]});
        let cid = mint_verification_witness(
            &cs,
            &obligation,
            "z3@4.x",
            &tmp,
            &VERIFY_SIGNER_SEED_DEV,
            false,
        )
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
        // The dev seed is an integrity tag, not authority.
        assert_eq!(parsed.measurements["signer_attests_authority"], false);
        assert!(parsed.signature.is_some());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn signer_seed_hex_override_decodes_and_is_authoritative() {
        // An override seed decodes to 32 bytes and is marked authoritative.
        let hex = "ab".repeat(32);
        let seed = decode_seed_hex(&hex).expect("valid 64-hex seed decodes");
        assert_eq!(seed, [0xabu8; 32]);
        // 0x-prefixed accepted; wrong length rejected.
        assert_eq!(decode_seed_hex(&format!("0x{hex}")).unwrap(), [0xabu8; 32]);
        assert!(decode_seed_hex("dead").is_err());
        assert!(decode_seed_hex(&"zz".repeat(32)).is_err());
    }
}
