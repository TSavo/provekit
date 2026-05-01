// SPDX-License-Identifier: Apache-2.0
//
// mint-self-contracts — the orchestrator binary.
//
// 1. Walks every `.invariant.rs` file in the workspace via the
//    orchestrator at `provekit-self-contracts/src/lib.rs`.
// 2. Authors all contracts; mints them as signed mementos plus the
//    closed-loop bridge memento; bundles into a `.proof` file at
//    `target/release/provekit-self-contracts.proof` (default) or the
//    user-supplied path.
// 3. Asserts the output CID is byte-deterministic by minting twice
//    into separate temp dirs and comparing the resulting CIDs. Fails
//    loud if they differ.
// 4. Loads the produced .proof through the verifier; runs the full
//    pipeline; prints the per-callsite verdict report.
//
// Honest expectation:
//   Most contracts use kit-defined atomic predicates that Z3 has no
//   semantics for (`roundTrips`, `isErr`, `isMalformed`,
//   `cidMatchesFilename`, `bridgeKnownInPool`, etc.). Those callsite
//   verdicts resolve to "undecidable", which is the protocol's HONEST
//   outcome — the value of these contracts is the LIVING DOCS shape,
//   not the discharge. Standard-algebra contracts (=, <, >, integer
//   constants) reach Z3 cleanly and discharge or unsatisfy.
//
// Run:
//   cargo run --release --bin mint-self-contracts
//   cargo run --release --bin mint-self-contracts -- /tmp/provekit-self
//
// To exercise Z3 set PROVEKIT_Z3 to a z3 binary; without it the
// resolve+instantiate stages still run, but solve_obligation fails to
// spawn and verdicts come back undecidable with the spawn error in
// `reason`.

use std::path::PathBuf;
use std::process::ExitCode;

use provekit_self_contracts::{author_all_invariants, mint_self_proof};
use provekit_verifier::{Runner, RunnerConfig};

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().collect();
    let out_dir: PathBuf = if argv.len() >= 2 {
        PathBuf::from(&argv[1])
    } else {
        // Default: the workspace's release target dir.
        // The build orchestrator's contract is "target/release/<cid>.proof".
        let mut p = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        // Walk up until we see a `Cargo.toml` containing `[workspace]`.
        loop {
            let manifest = p.join("Cargo.toml");
            if manifest.exists() {
                if let Ok(s) = std::fs::read_to_string(&manifest) {
                    if s.contains("[workspace]") {
                        break;
                    }
                }
            }
            match p.parent() {
                Some(parent) => p = parent.to_path_buf(),
                None => {
                    // Fall back to /tmp.
                    p = PathBuf::from("/tmp/provekit-self-proofs");
                    break;
                }
            }
        }
        p.push("target");
        p.push("release");
        p
    };

    println!("== ProvekIt self-contracts orchestrator ==");
    println!();
    println!("output dir: {}", out_dir.display());

    // ---- 1. Author -----------------------------------------------------------
    let (slabs, bridges) = author_all_invariants();
    println!();
    println!("authored:");
    let mut total: usize = 0;
    for s in &slabs {
        total += s.contracts.len();
        println!(
            "  {:>22}  {:>2} contracts  ({})",
            s.source.label,
            s.contracts.len(),
            s.source.path
        );
    }
    println!("  {:>22}  {:>2} contracts (TOTAL)", "[ALL]", total);
    println!();
    println!("  bridges:   {}", bridges.len());
    for b in &bridges {
        println!(
            "    - {} -> {} ({:?} -> {})",
            b.source_symbol, b.target_contract_name, b.ir_arg_sorts, b.ir_return_sort
        );
    }

    // ---- 2. Mint -------------------------------------------------------------
    //
    // Wipe a determinism-check temp dir and mint there first; then mint
    // again into the real out_dir. The two CIDs MUST match.
    let det_dir = std::env::temp_dir().join(format!(
        "provekit-self-determinism-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&det_dir);

    let mint_a = match mint_self_proof(&det_dir) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("ERROR: mint_self_proof (determinism A): {e}");
            return ExitCode::from(1);
        }
    };

    let mint = match mint_self_proof(&out_dir) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("ERROR: mint_self_proof: {e}");
            return ExitCode::from(1);
        }
    };

    println!();
    println!("minted:");
    println!("  .proof file:        {}", mint.path.display());
    println!("  bytes:              {}", mint.bytes_len);
    println!("  members:            {}", mint.member_count);
    println!("  total contracts:    {}", mint.total_contracts);
    println!("  catalog CID:        {}", mint.cid);

    if mint_a.cid != mint.cid {
        eprintln!();
        eprintln!("ERROR: byte-determinism check FAILED:");
        eprintln!("  run A CID: {}", mint_a.cid);
        eprintln!("  run B CID: {}", mint.cid);
        let _ = std::fs::remove_dir_all(&det_dir);
        return ExitCode::from(2);
    }
    let _ = std::fs::remove_dir_all(&det_dir);
    println!("  determinism check:  OK (two runs produced identical CIDs)");

    // ---- 3. Verify -----------------------------------------------------------
    let cfg = RunnerConfig {
        project_root: out_dir.clone(),
        z3_path: std::env::var("PROVEKIT_Z3").unwrap_or_else(|_| "z3".into()),
    };
    let runner = Runner::new(cfg);

    let (pool, callsites) = runner.run_load_and_enumerate();
    println!();
    println!("verifier (load + enumerate):");
    println!("  loaded mementos:              {}", pool.mementos.len());
    println!("  bridges by sourceSymbol:      {}", pool.bridges_by_symbol.len());
    println!("  enumerated callsites:         {}", callsites.len());
    if !pool.load_errors.is_empty() {
        println!();
        println!("  LOAD ERRORS:");
        for e in &pool.load_errors {
            println!("    - {}: {}", e.proof_path, e.reason);
        }
    }

    if !callsites.is_empty() {
        println!();
        println!("  callsite detail:");
        for cs in &callsites {
            println!(
                "    - {} (in {}@{}...) -> bridge target {}",
                cs.bridge_ir_name,
                cs.property_name,
                cs.property_cid.chars().take(20).collect::<String>(),
                cs.bridge_target_cid.chars().take(20).collect::<String>(),
            );
        }
    }

    // Full pipeline.
    let report = Runner::new(RunnerConfig {
        project_root: out_dir.clone(),
        z3_path: std::env::var("PROVEKIT_Z3").unwrap_or_else(|_| "z3".into()),
    })
    .run();

    println!();
    println!("verifier report:");
    println!("  total callsites:  {}", report.total_callsites);
    println!("  discharged:       {}", report.discharged);
    println!("  flagged:          {}", report.violations);
    println!("  load errors:      {}", report.load_errors.len());

    if !report.rows.is_empty() {
        println!();
        println!("  per-callsite rows:");
        for row in &report.rows {
            println!(
                "    [{}] {} (in {}): {}",
                row.status, row.callsite.bridge_ir_name, row.callsite.property_name, row.reason
            );
        }
    }

    println!();
    println!("== done. self-application: live. ==");
    ExitCode::SUCCESS
}
