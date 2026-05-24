// SPDX-License-Identifier: Apache-2.0
//
// Stage 4 handshake demo driver.
//
// Orchestrates one verifier run with a layout like:
//
//   <project_root>/
//     node_modules/@example/go-validate-kit/<go-publisher>.proof   (Go-published)
//     node_modules/@example/rust-parse-kit/<parse-kit>.proof       (Rust-published)
//     <rust-consumer>.proof                                        (Rust-published)
//     .provekit/cache/                                             (implication mementos)
//
// The Go publisher is run separately (out of process) by the run.sh
// script before invoking this binary; the demo passes its path with
// --go-proof <path>.
//
// What this driver does:
//   1. Mints the parse-kit contract memento + bridge: parseInt with
//      `pre = forall n: Int. n > 0`. Source layer "ts", target layer
//      "rust-parse-kit".
//   2. Mints the consumer contract memento with one callsite shape:
//      `must("call-parseInt-of-validateInput", parseInt(validateInput(...)))`.
//      The IR ctor `validateInput` is what causes the handshake to
//      fire: arg_term IS a Ctor whose name maps to the Go-published
//      bridge.
//   3. Bundles each into v1.1.0 .proof files.
//   4. Runs the Rust verifier with cfg.cache_dir = .provekit/cache/.
//   5. Prints headline metrics: hash hits / cache hits / Z3+mints /
//      residue / violations / Z3 invocations.
//
// Flags:
//   --go-proof <path>      mandatory; path to the Go-published .proof
//   --project-dir <path>   mandatory; project_root for this run
//   --label <string>       optional; demo label (e.g. "Run B (warm)")
//   --print-cids           optional; print the CIDs of every artifact

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::rc::Rc;

use provekit_canonicalizer::blake3_512_of;
use provekit_claim_envelope::{
    mint_bridge, mint_contract, Authoring, MintBridgeArgs, MintContractArgs,
};
use provekit_ir_symbolic::serialize::formula_to_value;
use provekit_ir_symbolic::{
    atomic_, begin_collecting, finish, forall, gt, must, num, reset_collector, Int, Term,
};
use provekit_proof_envelope::{
    build_proof_envelope, ed25519_pubkey_string, Ed25519Seed, ProofEnvelopeInput,
};
use provekit_verifier::{Runner, RunnerConfig};

fn parse_args() -> Result<DemoArgs, String> {
    let mut go_proof: Option<PathBuf> = None;
    let mut project_dir: Option<PathBuf> = None;
    let mut label: String = "(unlabeled)".into();
    let mut print_cids = false;
    let mut argv = std::env::args().skip(1).collect::<Vec<_>>().into_iter();
    while let Some(a) = argv.next() {
        match a.as_str() {
            "--go-proof" => {
                go_proof = Some(PathBuf::from(
                    argv.next().ok_or("--go-proof needs a value")?,
                ));
            }
            "--project-dir" => {
                project_dir = Some(PathBuf::from(
                    argv.next().ok_or("--project-dir needs a value")?,
                ));
            }
            "--label" => {
                label = argv.next().ok_or("--label needs a value")?;
            }
            "--print-cids" => {
                print_cids = true;
            }
            other => return Err(format!("unknown arg: {other}")),
        }
    }
    Ok(DemoArgs {
        go_proof: go_proof.ok_or("--go-proof is required")?,
        project_dir: project_dir.ok_or("--project-dir is required")?,
        label,
        print_cids,
    })
}

struct DemoArgs {
    go_proof: PathBuf,
    project_dir: PathBuf,
    label: String,
    print_cids: bool,
}

fn copy_file(src: &Path, dst: &Path) -> std::io::Result<()> {
    if let Some(p) = dst.parent() {
        fs::create_dir_all(p)?;
    }
    fs::copy(src, dst)?;
    Ok(())
}

/// Build a ctor term by hand so we can use IR symbol names that the
/// kit's primitive helpers don't expose (`validateInput`).
fn ctor1(name: &str, arg: Rc<Term>) -> Rc<Term> {
    Rc::new(Term::Ctor {
        name: name.into(),
        args: vec![arg],
    })
}

fn run() -> Result<(), String> {
    let args = parse_args()?;

    if !args.go_proof.exists() {
        return Err(format!(
            "go .proof not found at {}",
            args.go_proof.display()
        ));
    }
    fs::create_dir_all(&args.project_dir)
        .map_err(|e| format!("create project_dir {}: {e}", args.project_dir.display()))?;
    let cache_dir = args.project_dir.join(".provekit").join("cache");
    fs::create_dir_all(&cache_dir)
        .map_err(|e| format!("create cache_dir {}: {e}", cache_dir.display()))?;

    println!("=== {label} ===", label = args.label);

    // ---- Install the Go publisher's .proof under node_modules/ ----
    let go_dst = args
        .project_dir
        .join("node_modules")
        .join("@example")
        .join("go-validate-kit")
        .join(
            args.go_proof
                .file_name()
                .ok_or("go .proof has no filename")?,
        );
    copy_file(&args.go_proof, &go_dst)
        .map_err(|e| format!("copy go .proof to {}: {e}", go_dst.display()))?;
    if args.print_cids {
        let go_cid = args
            .go_proof
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .trim_end_matches(".proof");
        println!("  go-validate-kit .proof CID: {go_cid}");
    }

    // ---- Author + publish the Rust parse-kit ---------------------
    reset_collector();
    begin_collecting();
    // parseInt's pre: forall n: Int. n > 0
    must("parseInt", forall(Int(), |n| gt(n, num(0))));
    let parse_kit_decls = finish();

    let parse_kit_seed: Ed25519Seed = [0x33; 32];
    let parse_kit_declared_at = "2026-04-30T12:00:00.000Z";
    let parse_kit_producer = "rust-parse-kit@1.0";

    let mut parse_kit_members: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut parse_kit_contract_cid = String::new();
    for d in &parse_kit_decls {
        let m = mint_contract(&MintContractArgs {
            contract_name: d.name.clone(),
            pre: d.pre.as_deref().map(formula_to_value),
            post: d.post.as_deref().map(formula_to_value),
            inv: d.inv.as_deref().map(formula_to_value),
            out_binding: d.out_binding.clone(),
            produced_by: parse_kit_producer.into(),
            produced_at: parse_kit_declared_at.into(),
            input_cids: vec![],
            authoring: Authoring::KitAuthor {
                author: parse_kit_producer.into(),
                note: None,
            },
            signer_seed: parse_kit_seed,
            formals: Vec::new(),
            formal_sorts: Vec::new(),
        })
        .map_err(|e| format!("mint parse-kit {}: {e}", d.name))?;
        parse_kit_contract_cid = m.cid.clone();
        parse_kit_members.insert(m.cid, m.canonical_bytes);
    }
    let parse_kit_bridge = mint_bridge(&MintBridgeArgs {
        produced_by: parse_kit_producer.into(),
        produced_at: parse_kit_declared_at.into(),
        source_symbol: "parseInt".into(),
        source_layer: "ts".into(),
        target_contract_cid: parse_kit_contract_cid.clone(),
        target_layer: "rust-parse-kit".into(),
        ir_arg_sorts: vec!["String".into()],
        ir_return_sort: "Int".into(),
        notes: String::new(),
        signer_seed: parse_kit_seed,
    });
    parse_kit_members.insert(
        parse_kit_bridge.cid.clone(),
        parse_kit_bridge.canonical_bytes,
    );
    let parse_pubkey = ed25519_pubkey_string(&parse_kit_seed);
    let parse_signer_cid = blake3_512_of(parse_pubkey.as_bytes());
    let parse_proof = build_proof_envelope(&ProofEnvelopeInput {
        name: "@example/rust-parse-kit".into(),
        version: "1.0.0".into(),
        binary_cid: None,
        metadata: None,
        members: parse_kit_members,
        signer_cid: parse_signer_cid,
        signer_seed: parse_kit_seed,
        declared_at: parse_kit_declared_at.into(),
    });
    let parse_path = args
        .project_dir
        .join("node_modules")
        .join("@example")
        .join("rust-parse-kit")
        .join(format!("{}.proof", parse_proof.cid));
    if let Some(p) = parse_path.parent() {
        fs::create_dir_all(p).map_err(|e| format!("mkdir parse-kit: {e}"))?;
    }
    fs::write(&parse_path, &parse_proof.bytes)
        .map_err(|e| format!("write parse-kit .proof: {e}"))?;
    if args.print_cids {
        println!("  rust-parse-kit  .proof CID: {}", parse_proof.cid);
    }

    // ---- Author + publish the Rust consumer ----------------------
    // The consumer's contract carries one bridged callsite:
    //   must("call-parseInt-of-validateInput",
    //        eq(parseInt(validateInput(str_const("user"))), num(1)))
    //
    // The verifier's enumerate_callsites stage finds the parseInt
    // ctor; resolve_target gives parseInt's pre. The handshake
    // discovers that the arg_term is itself the validateInput ctor: // looks up validateInput in pool.bridges_by_symbol → the Go
    // contract memento → its `post` formula. That post and the
    // parseInt pre are the (producer-post, consumer-pre) pair the
    // Tier 1 / Tier 2 / Tier 3 ladder exercises.
    reset_collector();
    begin_collecting();
    let validate_term = ctor1(
        "validateInput",
        Rc::new(Term::Const {
            value: provekit_ir_symbolic::ConstValue::String("user-input".into()),
            sort: provekit_ir_symbolic::Sort::string(),
        }),
    );
    let parse_call = ctor1("parseInt", validate_term);
    must(
        "call-parseInt-of-validateInput",
        atomic_("=", vec![parse_call, num(1)]),
    );
    let consumer_decls = finish();

    let consumer_seed: Ed25519Seed = [0x37; 32];
    let consumer_declared_at = "2026-04-30T15:30:00.000Z";
    let consumer_producer = "rust-consumer@1.0";

    let mut consumer_members: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut consumer_contract_cid = String::new();
    for d in &consumer_decls {
        let m = mint_contract(&MintContractArgs {
            contract_name: d.name.clone(),
            pre: d.pre.as_deref().map(formula_to_value),
            post: d.post.as_deref().map(formula_to_value),
            inv: d.inv.as_deref().map(formula_to_value),
            out_binding: d.out_binding.clone(),
            produced_by: consumer_producer.into(),
            produced_at: consumer_declared_at.into(),
            input_cids: vec![],
            authoring: Authoring::KitAuthor {
                author: consumer_producer.into(),
                note: None,
            },
            signer_seed: consumer_seed,
            formals: Vec::new(),
            formal_sorts: Vec::new(),
        })
        .map_err(|e| format!("mint consumer {}: {e}", d.name))?;
        consumer_contract_cid = m.cid.clone();
        consumer_members.insert(m.cid, m.canonical_bytes);
    }
    let consumer_pubkey = ed25519_pubkey_string(&consumer_seed);
    let consumer_signer_cid = blake3_512_of(consumer_pubkey.as_bytes());
    let consumer_proof = build_proof_envelope(&ProofEnvelopeInput {
        name: "@example/rust-consumer".into(),
        version: "1.0.0".into(),
        binary_cid: None,
        metadata: None,
        members: consumer_members,
        signer_cid: consumer_signer_cid,
        signer_seed: consumer_seed,
        declared_at: consumer_declared_at.into(),
    });
    let consumer_path = args
        .project_dir
        .join(format!("{}.proof", consumer_proof.cid));
    fs::write(&consumer_path, &consumer_proof.bytes)
        .map_err(|e| format!("write consumer .proof: {e}"))?;
    if args.print_cids {
        println!("  rust-consumer   .proof CID: {}", consumer_proof.cid);
        println!("  rust-consumer contract CID: {consumer_contract_cid}");
        println!("  rust-parse-kit contract CID: {parse_kit_contract_cid}");
    }

    // ---- Run the verifier with the handshake enabled -------------
    let z3_path = std::env::var("PROVEKIT_Z3").unwrap_or_else(|_| "z3".into());
    let cfg = RunnerConfig {
        project_root: args.project_dir.clone(),
        z3_path,
        cache_dir: Some(cache_dir.clone()),
        mint_seed: Some([0x44; 32]),
        mint_producer_id: Some("rust-verifier@1.0".into()),
        solvers_config: None,
        extra_projects: Vec::new(),
    };
    let runner = Runner::new(cfg);
    let (report, stats) = runner.run_with_tiers();

    println!(
        "  callsites:  total={} discharged={} violations={}",
        report.total_callsites, report.discharged, report.violations
    );
    println!("  tier-stats: hash={} cache={} vacuous={} z3+mint={} residue={} violations={} z3-invocations={}",
        stats.discharged_by_hash,
        stats.discharged_by_cache,
        stats.vacuous_discharge,
        stats.solved_and_minted,
        stats.residue,
        stats.violations,
        stats.z3_invocations(),
    );

    // List any cached implication mementos.
    if cache_dir.exists() {
        let mut found = 0usize;
        if let Ok(rd) = fs::read_dir(&cache_dir) {
            for e in rd.flatten() {
                if e.path().extension().and_then(|s| s.to_str()) == Some("proof") {
                    found += 1;
                    if args.print_cids {
                        println!("  cache memento: {}", e.file_name().to_string_lossy());
                    }
                }
            }
        }
        if found > 0 {
            println!(
                "  cache:      {found} signed implication memento(s) in {}",
                cache_dir.display()
            );
        }
    }

    // Per-row summary so the demo log carries the verdicts.
    for r in &report.rows {
        let tag = if r.status == "discharged" { "OK" } else { "X" };
        println!(
            "    [{tag}] {:<8} {} reason: {}",
            r.status, r.callsite.bridge_ir_name, r.reason
        );
    }

    // Demo emits structured machine-readable summary on the last
    // line so run.sh can parse the per-run numbers.
    println!(
        "STAGE4_SUMMARY label={:?} hash={} cache={} vacuous={} solved_minted={} residue={} violations={} z3_invocations={} total_callsites={}",
        args.label,
        stats.discharged_by_hash,
        stats.discharged_by_cache,
        stats.vacuous_discharge,
        stats.solved_and_minted,
        stats.residue,
        stats.violations,
        stats.z3_invocations(),
        report.total_callsites,
    );

    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ERROR: {e}");
            ExitCode::from(1)
        }
    }
}
