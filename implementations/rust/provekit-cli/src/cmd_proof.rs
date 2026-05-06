// SPDX-License-Identifier: Apache-2.0
//
// `provekit proof ...` — grouped helpers for .proof artifacts.

use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use owo_colors::OwoColorize;
use provekit_canonicalizer::blake3_512_of;
use provekit_verifier::cbor_decode::decode;
use serde_json::json;

use crate::{DumpArgs, HashArgs, OutputFlags, EXIT_OK, EXIT_VERIFY_FAIL};

#[derive(Parser, Debug, Clone)]
pub struct ProofArgs {
    #[command(subcommand)]
    pub cmd: ProofCmd,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ProofCmd {
    /// Compute the BLAKE3-512 CID of a .proof file.
    Hash(ProofFileArgs),
    /// Inspect a .proof envelope and print its members.
    Inspect(ProofFileArgs),
    /// Check that a .proof file decodes and its filename matches its content CID.
    Check(ProofFileArgs),
}

#[derive(Parser, Debug, Clone)]
pub struct ProofFileArgs {
    /// Path to a .proof file.
    pub proof_file: PathBuf,
    #[command(flatten)]
    pub out: OutputFlags,
}

pub fn run(args: ProofArgs) -> u8 {
    match args.cmd {
        ProofCmd::Hash(a) => crate::cmd_hash::run(HashArgs {
            file: Some(a.proof_file),
            out: a.out,
        }),
        ProofCmd::Inspect(a) => crate::cmd_dump::run(DumpArgs {
            proof_file: a.proof_file,
            out: a.out,
        }),
        ProofCmd::Check(a) => check(a),
    }
}

fn check(args: ProofFileArgs) -> u8 {
    match check_proof_file(&args.proof_file) {
        Ok(report) => {
            if args.out.json {
                let payload = json!({
                    "path": args.proof_file.display().to_string(),
                    "cid": report.cid,
                    "memberCount": report.member_count,
                    "ok": true,
                });
                println!("{}", serde_json::to_string_pretty(&payload).unwrap_or_default());
            } else if !args.out.quiet {
                println!("{}: {}", "proof".green().bold(), args.proof_file.display());
                println!("  cid: {}", report.cid);
                println!("  members: {}", report.member_count);
            }
            EXIT_OK
        }
        Err(e) => {
            eprintln!("{}: {e}", "error".red().bold());
            EXIT_VERIFY_FAIL
        }
    }
}

struct ProofCheckReport {
    cid: String,
    member_count: usize,
}

fn check_proof_file(path: &Path) -> Result<ProofCheckReport, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let cid = blake3_512_of(&bytes);
    check_filename_cid(path, &cid)?;

    let catalog = decode(&bytes).map_err(|e| format!("CBOR decode {}: {e}", path.display()))?;
    let map = catalog
        .as_map()
        .ok_or_else(|| "proof envelope root is not a CBOR map".to_string())?;
    let members = map
        .get("members")
        .and_then(|v| v.as_map())
        .ok_or_else(|| "proof envelope has no `members` map".to_string())?;

    Ok(ProofCheckReport {
        cid,
        member_count: members.len(),
    })
}

fn check_filename_cid(path: &Path, content_cid: &str) -> Result<(), String> {
    let filename = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| format!("proof path has no UTF-8 filename: {}", path.display()))?;
    if !filename.ends_with(".proof") {
        return Err(format!("proof filename must end in `.proof`: {filename}"));
    }

    let stem = filename.trim_end_matches(".proof");
    let expected_hex = content_cid.trim_start_matches("blake3-512:");
    let stem_hex = stem.trim_start_matches("blake3-512:");
    if stem_hex.len() != 128 || !stem_hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!(
            "proof filename must be `<cid>.proof` or `<hex>.proof`: {filename}"
        ));
    }
    if stem_hex != expected_hex {
        return Err(format!(
            "proof filename CID {stem_hex} does not match content CID {expected_hex}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ZERO_CID: &str = "blake3-512:d53d18c23212ea7b6300594bb89bce60218f6eff2b9d628b8cc42d3e79bbd5ab09994845815cc7185113418f9fc2edc7606b06f0d57a6d581e7cff5b290f3229";

    #[test]
    fn filename_cid_accepts_full_cid_stem() {
        let path = PathBuf::from(format!("{ZERO_CID}.proof"));
        check_filename_cid(&path, ZERO_CID).expect("matching filename CID");
    }

    #[test]
    fn filename_cid_rejects_mismatch() {
        let path = PathBuf::from(format!("{}.proof", "0".repeat(128)));
        let err = check_filename_cid(&path, ZERO_CID).expect_err("mismatch must fail");
        assert!(err.contains("does not match content CID"), "got: {err}");
    }
}
