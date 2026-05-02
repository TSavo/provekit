// SPDX-License-Identifier: Apache-2.0
//
// cmd witness — mint a witness memento extending the proof lattice.
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

use std::path::PathBuf;

use serde_json::Value as Json;

use crate::{EXIT_OK, EXIT_SOLVER_FAIL, EXIT_USER_ERROR, EXIT_VERIFY_FAIL};

pub fn run(args: crate::WitnessArgs) -> u8 {
    let project_root = args.project.unwrap_or_else(|| std::env::current_dir().unwrap());
    
    // 1. Load pool
    let pool = provekit_verifier::load_all_proofs::run(&project_root);
    
    // 2. Find contract
    let contract = match pool.mementos.get(&args.contract_cid) {
        Some(c) => c,
        None => {
            eprintln!("error: contract {} not found in pool", args.contract_cid);
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
    let property_formula = match load_formula(&args.property) {
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
    println!("witness: proving {} implies property...", short(&args.contract_cid));
    let result = run_solver(&args.z3, &smt);
    
    match result {
        SolverOutput::Unsat => {
            println!("witness: proven! Minting memento...");
            // TODO: mint witness memento
            // For now, print what would be minted
            println!("witness: memento would be:");
            println!("  antecedent: {}", args.contract_cid);
            println!("  consequent: <property CID>");
            println!("  prover: z3");
            EXIT_OK
        }
        SolverOutput::Sat => {
            eprintln!("witness: FAILED — solver found counterexample");
            eprintln!("  The property does NOT follow from the contract.");
            EXIT_VERIFY_FAIL
        }
        SolverOutput::Unknown => {
            eprintln!("witness: UNDECIDABLE — solver could not determine");
            EXIT_SOLVER_FAIL
        }
        SolverOutput::Error(e) => {
            eprintln!("witness: SOLVER ERROR — {e}");
            EXIT_SOLVER_FAIL
        }
    }
}

fn extract_post(contract: &Json) -> Option<Json> {
    contract.get("evidence")?
        .get("body")?
        .get("post")
        .cloned()
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
        m.insert("operands".to_string(), Json::Array(vec![post.clone(), property.clone()]));
        m
    }))
}

enum SolverOutput {
    Unsat,
    Sat,
    Unknown,
    Error(String),
}

fn run_solver(z3_path: &str, smt: &str) -> SolverOutput {
    use std::process::{Command, Stdio};
    use std::io::Write;
    
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
        SolverOutput::Error(format!("unexpected output: {} (stderr: {})", stdout.trim(), stderr.trim()))
    }
}

fn short(cid: &str) -> String {
    if cid.len() > 20 {
        format!("{}...", &cid[..20])
    } else {
        cid.to_string()
    }
}
