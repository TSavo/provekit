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

/// `--rpc` mode: speak the lift-plugin protocol over NDJSON-on-stdio.
/// Returns the proof-envelope shape (the plugin owns the full pipeline).
/// Spec: protocol/specs/2026-04-30-lift-plugin-protocol.md
fn run_rpc_mode() -> ExitCode {
    use std::io::{BufRead, BufReader, Write};
    let stdin = std::io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let stdout = std::io::stdout();

    loop {
        let mut line = String::new();
        let n = match reader.read_line(&mut line) {
            Ok(n) => n,
            Err(e) => {
                eprintln!("rpc: read stdin: {e}");
                return ExitCode::from(1);
            }
        };
        if n == 0 {
            return ExitCode::from(0); // EOF
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let req: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("rpc: parse: {e}");
                continue;
            }
        };
        let id = req.get("id").cloned().unwrap_or(serde_json::Value::Null);
        let method = req.get("method").and_then(|v| v.as_str()).unwrap_or("");

        let resp = match method {
            "initialize" => serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "name": "rust-self-contracts",
                    "version": env!("CARGO_PKG_VERSION"),
                    "protocol_version": "provekit-lift/1",
                    "capabilities": {
                        "authoring_surfaces": ["rust-self-contracts"],
                        "ir_version": "v1.1.0",
                        "emits_signed_mementos": true
                    }
                }
            }),
            "lift" => {
                let tmp =
                    std::env::temp_dir().join(format!("provekit-rpc-mint-{}", std::process::id()));
                let _ = std::fs::remove_dir_all(&tmp);
                let _ = std::fs::create_dir_all(&tmp);
                match mint_self_proof(&tmp) {
                    Ok(m) => {
                        let bytes = match std::fs::read(&m.path) {
                            Ok(b) => b,
                            Err(e) => {
                                eprintln!("rpc: read proof bytes: {e}");
                                let err = serde_json::json!({"jsonrpc":"2.0","id":id,
                                    "error":{"code":-32603,"message":format!("read proof bytes: {e}")}});
                                let _ = writeln!(stdout.lock(), "{err}");
                                continue;
                            }
                        };
                        let _ = std::fs::remove_dir_all(&tmp);
                        use base64::Engine;
                        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                        serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {
                                "kind": "proof-envelope",
                                "filename_cid": m.cid,
                                "contract_set_cid": m.contract_set_cid,
                                "bytes_base64": b64,
                                "diagnostics": []
                            }
                        })
                    }
                    Err(e) => serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {"code": 1005, "message": format!("LIFT_FAILED: {e}")}
                    }),
                }
            }
            "shutdown" => {
                let resp = serde_json::json!({"jsonrpc": "2.0", "id": id, "result": null});
                let _ = writeln!(stdout.lock(), "{resp}");
                return ExitCode::from(0);
            }
            other => serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {"code": -32601, "message": format!("METHOD_NOT_FOUND: {other}")}
            }),
        };
        if writeln!(stdout.lock(), "{resp}").is_err() {
            return ExitCode::from(1);
        }
    }
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().collect();

    // --rpc takes over stdin/stdout for the lift-plugin protocol; do not
    // print human-readable banners in that mode.
    if argv.iter().any(|a| a == "--rpc") {
        return run_rpc_mode();
    }

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
    let det_dir =
        std::env::temp_dir().join(format!("provekit-self-determinism-{}", std::process::id()));
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
    println!("  contractSetCid:     {}", mint.contract_set_cid);

    if mint_a.cid != mint.cid {
        eprintln!();
        eprintln!("ERROR: byte-determinism check FAILED:");
        eprintln!("  run A CID: {}", mint_a.cid);
        eprintln!("  run B CID: {}", mint.cid);
        let _ = std::fs::remove_dir_all(&det_dir);
        return ExitCode::from(2);
    }
    if mint_a.contract_set_cid != mint.contract_set_cid {
        eprintln!();
        eprintln!("ERROR: contractSetCid determinism check FAILED:");
        eprintln!("  run A contractSetCid: {}", mint_a.contract_set_cid);
        eprintln!("  run B contractSetCid: {}", mint.contract_set_cid);
        let _ = std::fs::remove_dir_all(&det_dir);
        return ExitCode::from(2);
    }
    let _ = std::fs::remove_dir_all(&det_dir);
    println!("  determinism check:  OK (two runs produced identical CIDs and contractSetCid)");

    // ---- 3. Verify -----------------------------------------------------------
    let cfg = RunnerConfig {
        project_root: out_dir.clone(),
        z3_path: std::env::var("PROVEKIT_Z3").unwrap_or_else(|_| "z3".into()),
        ..Default::default()
    };
    let runner = Runner::new(cfg);

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

    // Full pipeline.
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
