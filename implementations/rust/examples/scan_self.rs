// SPDX-License-Identifier: Apache-2.0
//
// scan_self: ProvekIt's first self-application.
//
// 1. Authors 5+ contracts ABOUT the Rust impl's IR-JSON parser via
//    `provekit_self_contracts::author_self_contracts`.
// 2. Mints them as a `.proof` file at /tmp/provekit-self-proofs/<hex>.proof.
// 3. Runs the verifier against that directory and prints the report.
//
// Expected outcome:
//   - 5+ mementos bundled, plus 1 bridge.
//   - At least one callsite enumerated (parse_formula bridges to
//     parse_formula_correct; contracts 1 and 3 contain that ctor in
//     their post).
//   - Most kit-defined-predicate callsites resolve to undecidable
//     (Z3 has no semantics for `roundTrips` / `isErr` /
//     `isMalformed`). That's the protocol's HONEST outcome.
//   - Standard-algebra contracts (5, 6, 7) don't enumerate as
//     callsites in their *own* memento: there's no bridge mapping
//     `not_arity_bounds` etc. to an IR symbol. They sit in the
//     pool as authored but verifiable-by-inspection mementos.
//
// Run:
//   cargo run --release --example scan_self
//
// To exercise the Z3 path set PROVEKIT_Z3 to a z3 binary; without it
// the resolve+instantiate stages still run, but solve_obligation
// fails to spawn and verdicts come back undecidable with the spawn
// error in `reason`.

use std::path::PathBuf;
use std::process::ExitCode;

use provekit_self_contracts::{author_all_invariants, mint_self_proof};
use provekit_verifier::{Runner, RunnerConfig};

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().collect();
    let out_dir = if argv.len() >= 2 {
        PathBuf::from(&argv[1])
    } else {
        PathBuf::from("/tmp/provekit-self-proofs")
    };

    println!("== ProvekIt self-scan ==");
    println!();
    println!("output dir: {}", out_dir.display());

    // Wipe and recreate so each run produces a fresh directory.
    let _ = std::fs::remove_dir_all(&out_dir);

    // ---- 1. Author -----------------------------------------------------------
    let (slabs, bridges) = author_all_invariants();
    let total: usize = slabs.iter().map(|s| s.contracts.len()).sum();
    println!();
    println!("authored:");
    println!(
        "  contracts: {} across {} .invariant.rs files",
        total,
        slabs.len()
    );
    for s in &slabs {
        println!("    [{}]", s.source.label);
        for d in &s.contracts {
            let pre = d.pre.as_ref().map(|_| "pre").unwrap_or("");
            let post = d.post.as_ref().map(|_| "post").unwrap_or("");
            let inv = d.inv.as_ref().map(|_| "inv").unwrap_or("");
            let slots: Vec<&str> = [pre, post, inv]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect();
            println!("      - {} ({})", d.name, slots.join(", "));
        }
    }
    println!("  bridges:   {}", bridges.len());
    for b in &bridges {
        println!(
            "    - {} -> {} ({:?} -> {})",
            b.source_symbol, b.target_contract_name, b.ir_arg_sorts, b.ir_return_sort
        );
    }

    // ---- 2. Mint -------------------------------------------------------------
    let mint = match mint_self_proof(&out_dir) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("ERROR: mint_self_proof: {e}");
            return ExitCode::from(1);
        }
    };
    println!();
    println!("minted:");
    println!("  .proof file:  {}", mint.path.display());
    println!("  bytes:        {}", mint.bytes_len);
    println!("  members:      {}", mint.member_count);
    println!("  catalog CID:  {}", mint.cid);

    // ---- 3. Verify -----------------------------------------------------------
    let cfg = RunnerConfig {
        project_root: out_dir.clone(),
        z3_path: std::env::var("PROVEKIT_Z3").unwrap_or_else(|_| "z3".into()),
        ..Default::default()
    };
    let runner = Runner::new(cfg);

    // First show the load+enumerate state directly.
    let (pool, callsites) = runner.run_load_and_enumerate();
    println!();
    println!("verifier (load + enumerate):");
    println!("  loaded mementos:              {}", pool.mementos.len());
    println!(
        "  bridges by sourceSymbol:      {}",
        pool.bridges_by_symbol.len()
    );
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

    // Now the full pipeline (will spawn Z3 per callsite).
    let report = Runner::new(RunnerConfig {
        project_root: out_dir.clone(),
        z3_path: std::env::var("PROVEKIT_Z3").unwrap_or_else(|_| "z3".into()),
        ..Default::default()
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
