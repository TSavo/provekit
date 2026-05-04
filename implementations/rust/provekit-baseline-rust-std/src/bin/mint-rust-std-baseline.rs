// SPDX-License-Identifier: Apache-2.0
//
// mint-rust-std-baseline — orchestrator binary for the rust-std
// foundation-baseline catalog.
//
// 1. Walks the seven slab files (string/vec/option/result/slice/
//    hashmap/iter), authoring all baseline ContractDecls.
// 2. Mints each ContractDecl as a signed memento under the foundation
//    v0 ed25519 seed; wraps the disclaimer text as a `kind=disclaimer`
//    layered memento.
// 3. Bundles into a `.proof` envelope at `<out_dir>/<cid>.proof` with
//    advisory metadata per `docs/contributing/baseline-catalog-rubric.md`
//    §3 (`signer_role: foundation-baseline`, `baseline.{version,
//    language, language_version, kit_version, disclaimer_cid}`).
// 4. Asserts byte-determinism by minting twice into separate temp dirs.
//
// Run:
//   cargo run --release --bin mint-rust-std-baseline
//   cargo run --release --bin mint-rust-std-baseline -- /tmp/rust-std-baseline
//
// Final landing path (move after mint):
//   .provekit/baselines/rust-std-baseline-v1.proof

use std::path::PathBuf;
use std::process::ExitCode;

use provekit_baseline_rust_std::{
    author_all_invariants, mint_baseline, BASELINE_KIT_VERSION,
    BASELINE_LANGUAGE, BASELINE_LANGUAGE_VERSION, BASELINE_VERSION,
    SIGNER_ROLE,
};

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().collect();

    let out_dir: PathBuf = if argv.len() >= 2 {
        PathBuf::from(&argv[1])
    } else {
        let mut p = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
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
                    p = PathBuf::from("/tmp/provekit-rust-std-baseline");
                    break;
                }
            }
        }
        p.push("target");
        p.push("release");
        p
    };

    println!("== ProvekIt rust-std baseline orchestrator ==");
    println!();
    println!("output dir:        {}", out_dir.display());
    println!("baseline:          v{}", BASELINE_VERSION);
    println!("language:          {} ({})", BASELINE_LANGUAGE, BASELINE_LANGUAGE_VERSION);
    println!("kit version:       {}", BASELINE_KIT_VERSION);
    println!("signer role:       {}", SIGNER_ROLE);

    // Pre-walk for visibility.
    let slabs = author_all_invariants();
    println!();
    println!("authored:");
    let mut total: usize = 0;
    for s in &slabs {
        total += s.contracts.len();
        println!(
            "  {:>14}  {:>3} contracts  ({})",
            s.source.label,
            s.contracts.len(),
            s.source.path
        );
    }
    println!("  {:>14}  {:>3} contracts (TOTAL)", "[ALL]", total);

    // Determinism check + final mint.
    let det_dir = std::env::temp_dir().join(format!(
        "provekit-rust-std-baseline-determinism-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&det_dir);

    let mint_a = match mint_baseline(&det_dir) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("ERROR: mint_baseline (determinism A): {e}");
            return ExitCode::from(1);
        }
    };

    let mint = match mint_baseline(&out_dir) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("ERROR: mint_baseline: {e}");
            return ExitCode::from(1);
        }
    };

    println!();
    println!("minted:");
    println!("  .proof file:        {}", mint.path.display());
    println!("  bytes:              {}", mint.bytes.len());
    println!("  members:            {}", mint.member_count);
    println!("  contracts:          {}", mint.contract_count);
    println!("  distinct builtins:  {}", mint.distinct_builtin_count);
    println!("  catalog CID:        {}", mint.cid);
    println!("  contractSetCid:     {}", mint.contract_set_cid);
    println!("  disclaimer_cid:     {}", mint.disclaimer_cid);

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
        eprintln!("  run A: {}", mint_a.contract_set_cid);
        eprintln!("  run B: {}", mint.contract_set_cid);
        let _ = std::fs::remove_dir_all(&det_dir);
        return ExitCode::from(2);
    }
    let _ = std::fs::remove_dir_all(&det_dir);
    println!("  determinism check:  OK");

    // Compliance floor checks (rubric §"Compliance checklist").
    println!();
    println!("compliance:");
    if mint.distinct_builtin_count < 50 {
        eprintln!(
            "  ERROR: builtin floor violated: {} < 50",
            mint.distinct_builtin_count
        );
        return ExitCode::from(3);
    }
    println!(
        "  builtin count:      {} (>= 50 floor)",
        mint.distinct_builtin_count
    );

    let mut by_builtin: std::collections::BTreeMap<String, usize> = Default::default();
    for name in mint.contract_cids.keys() {
        let prefix = match name.rsplit_once("__") {
            Some((p, _)) => p.to_string(),
            None => name.clone(),
        };
        *by_builtin.entry(prefix).or_default() += 1;
    }
    let underdense: Vec<(String, usize)> = by_builtin
        .iter()
        .filter(|(_, n)| **n < 2)
        .map(|(k, v)| (k.clone(), *v))
        .collect();
    if !underdense.is_empty() {
        eprintln!();
        eprintln!(
            "  ERROR: predicate density floor violated for {} builtin(s):",
            underdense.len()
        );
        for (b, n) in &underdense {
            eprintln!("    - {b}: {n} contracts (need >= 2)");
        }
        return ExitCode::from(3);
    }
    println!("  predicate density:  >= 2 per builtin (OK)");

    println!();
    println!("== done. rust-std baseline minted. ==");
    println!();
    println!("To land at the canonical path, copy:");
    println!(
        "  cp {} .provekit/baselines/rust-std-baseline-v1.proof",
        mint.path.display()
    );

    ExitCode::SUCCESS
}
