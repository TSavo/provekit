// SPDX-License-Identifier: Apache-2.0
//
// provekit-showcase
//
// Two subcommands:
//
//   generate  --size N --output DIR [--seed S]
//      Synthesize N contract mementos and ~10*N implication mementos
//      using the foundation test key. Lay them out as one .proof file
//      per memento, sharded by CID prefix to avoid blowing up the
//      filesystem.
//
//   benchmark --lattice DIR --queries Q [--z3-path PATH]
//      Load the on-disk lattice into a hash-into-pool index. Pick Q
//      random (consumer-pre, publisher-post) pairs and time each of
//      the three discharge tiers:
//          Tier 1: BLAKE3-512 hash equality (memcmp-of-64).
//          Tier 2: cached implication lookup + Ed25519 verify.
//          Tier 3: Z3-from-scratch (SMT-LIB shelled to `z3 -in`).
//      Emit median + p10/p90 per tier and a compression-ratio line.
//
// Numbers from this binary are pasted verbatim into
// docs/launch/showcase-results.md.

mod bench;
mod fixture;

use std::path::PathBuf;
use std::process::ExitCode;

fn print_usage() {
    eprintln!("provekit-showcase {{generate,benchmark}} ...");
    eprintln!();
    eprintln!("  generate --size N --output DIR [--seed S]");
    eprintln!("    Build a deterministic lattice of N contracts +");
    eprintln!("    ~10*N implications under DIR (sharded ab/cd/<cid>.proof).");
    eprintln!();
    eprintln!("  benchmark --lattice DIR --queries Q [--z3-path PATH]");
    eprintln!("    Run Q random discharge queries through tiers 1/2/3");
    eprintln!("    and print the latency distribution.");
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().collect();
    if argv.len() < 2 {
        print_usage();
        return ExitCode::from(2);
    }
    match argv[1].as_str() {
        "generate" => match parse_generate(&argv[2..]) {
            Ok(g) => match fixture::generate(&g) {
                Ok(()) => ExitCode::from(0),
                Err(e) => {
                    eprintln!("generate failed: {e}");
                    ExitCode::from(1)
                }
            },
            Err(e) => {
                eprintln!("argument error: {e}");
                print_usage();
                ExitCode::from(2)
            }
        },
        "benchmark" => match parse_benchmark(&argv[2..]) {
            Ok(b) => match bench::run(&b) {
                Ok(()) => ExitCode::from(0),
                Err(e) => {
                    eprintln!("benchmark failed: {e}");
                    ExitCode::from(1)
                }
            },
            Err(e) => {
                eprintln!("argument error: {e}");
                print_usage();
                ExitCode::from(2)
            }
        },
        "hash-spec" => {
            // Helper: print the BLAKE3-512 self-identifying CID of a
            // file. Used by docs to compute spec-catalog CIDs at build
            // time so the whitepaper references real hashes.
            if argv.len() < 3 {
                eprintln!("hash-spec PATH");
                return ExitCode::from(2);
            }
            let p = PathBuf::from(&argv[2]);
            let bytes = match std::fs::read(&p) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("read {}: {e}", p.display());
                    return ExitCode::from(1);
                }
            };
            let cid = provekit_canonicalizer::blake3_512_of(&bytes);
            println!("{cid}");
            ExitCode::from(0)
        }
        other => {
            eprintln!("unknown subcommand: {other}");
            print_usage();
            ExitCode::from(2)
        }
    }
}

#[derive(Debug, Clone)]
pub struct GenerateArgs {
    pub size: usize,
    pub output: PathBuf,
    pub seed: u64,
}

fn parse_generate(rest: &[String]) -> Result<GenerateArgs, String> {
    let mut size: Option<usize> = None;
    let mut output: Option<PathBuf> = None;
    let mut seed: u64 = 0xC0FFEE_BEEF;
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--size" => {
                i += 1;
                let raw = rest.get(i).ok_or("--size needs a value")?;
                size = Some(parse_size(raw)?);
            }
            "--output" | "-o" => {
                i += 1;
                output = Some(PathBuf::from(rest.get(i).ok_or("--output needs a value")?));
            }
            "--seed" => {
                i += 1;
                seed = rest
                    .get(i)
                    .ok_or("--seed needs a value")?
                    .parse()
                    .map_err(|e: std::num::ParseIntError| e.to_string())?;
            }
            other => return Err(format!("unknown flag: {other}")),
        }
        i += 1;
    }
    Ok(GenerateArgs {
        size: size.ok_or("--size is required")?,
        output: output.ok_or("--output is required")?,
        seed,
    })
}

#[derive(Debug, Clone)]
pub struct BenchmarkArgs {
    pub lattice: PathBuf,
    pub queries: usize,
    pub z3_path: String,
    pub seed: u64,
}

fn parse_benchmark(rest: &[String]) -> Result<BenchmarkArgs, String> {
    let mut lattice: Option<PathBuf> = None;
    let mut queries: usize = 10_000;
    let mut z3_path: String = std::env::var("PROVEKIT_Z3").unwrap_or_else(|_| "z3".to_string());
    let mut seed: u64 = 0xBEEF_F00D;
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--lattice" => {
                i += 1;
                lattice = Some(PathBuf::from(rest.get(i).ok_or("--lattice needs a value")?));
            }
            "--queries" => {
                i += 1;
                queries = parse_size(rest.get(i).ok_or("--queries needs a value")?)?;
            }
            "--z3-path" => {
                i += 1;
                z3_path = rest.get(i).ok_or("--z3-path needs a value")?.clone();
            }
            "--seed" => {
                i += 1;
                seed = rest
                    .get(i)
                    .ok_or("--seed needs a value")?
                    .parse()
                    .map_err(|e: std::num::ParseIntError| e.to_string())?;
            }
            other => return Err(format!("unknown flag: {other}")),
        }
        i += 1;
    }
    Ok(BenchmarkArgs {
        lattice: lattice.ok_or("--lattice is required")?,
        queries,
        z3_path,
        seed,
    })
}

// Accept "100000" or "1e5" or "1_000_000".
fn parse_size(raw: &str) -> Result<usize, String> {
    let s: String = raw.chars().filter(|c| *c != '_').collect();
    if let Some(idx) = s.find(|c: char| c == 'e' || c == 'E') {
        let mantissa: f64 = s[..idx]
            .parse()
            .map_err(|e: std::num::ParseFloatError| e.to_string())?;
        let exp: i32 = s[idx + 1..]
            .parse()
            .map_err(|e: std::num::ParseIntError| e.to_string())?;
        let value = (mantissa * 10f64.powi(exp)).round();
        if value < 0.0 {
            return Err(format!("negative size: {raw}"));
        }
        return Ok(value as usize);
    }
    s.parse::<usize>().map_err(|e| e.to_string())
}
