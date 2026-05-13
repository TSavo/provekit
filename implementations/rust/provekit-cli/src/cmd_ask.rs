// SPDX-License-Identifier: Apache-2.0
//
// `provekit ask <FORMULA-FILE>`: Librarian query.
//
// Parse an IR-JSON formula via `provekit_ir_symbolic::parse_formula`,
// re-serialize to canonical JCS bytes via `formula_to_value`, hash via
// BLAKE3-512. Then walk `.proof` files in the project root looking for
// the resulting CID inside any member envelope (substring match on the
// JCS-JSON bytes). Prints either a witness pointer or "miss".

use std::io::Read;
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use owo_colors::OwoColorize;
use provekit_canonicalizer::{blake3_512_of, encode_jcs};
use provekit_ir_symbolic::parse::parse_formula;
use provekit_ir_symbolic::serialize::formula_to_value;
use provekit_verifier::cbor_decode::decode;
use serde_json::{json, Value as Json};

use crate::AskArgs;

pub fn run(args: AskArgs) -> u8 {
    match ask(
        &args.formula,
        args.project.as_deref(),
        args.out.json,
        args.out.quiet,
    ) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("{}: {e:#}", "error".red().bold());
            crate::EXIT_USER_ERROR
        }
    }
}

fn ask(
    formula_file: &PathBuf,
    project: Option<&std::path::Path>,
    as_json: bool,
    quiet: bool,
) -> Result<u8> {
    let raw = if formula_file.as_os_str() == "-" {
        let mut s = String::new();
        std::io::stdin()
            .read_to_string(&mut s)
            .context("read stdin")?;
        s
    } else {
        std::fs::read_to_string(formula_file)
            .with_context(|| format!("read {}", formula_file.display()))?
    };

    let json: Json =
        serde_json::from_str(&raw).context("parse formula file as JSON (expected IR-JSON)")?;
    let formula = parse_formula(&json).map_err(|e| anyhow!("parse IR formula: {e}"))?;
    let canonical = encode_jcs(&formula_to_value(&formula));
    let cid = blake3_512_of(canonical.as_bytes());

    let project_root = project
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let hits = find_cid_in_proofs(&cid, &project_root);

    if as_json {
        let payload = json!({
            "queryCid": cid,
            "hits": hits,
            "project": project_root.display().to_string(),
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else if !quiet {
        println!("{}", "ProvekIt librarian".bold());
        println!("  query CID : {}", cid.cyan());
        println!("  project   : {}", project_root.display());
        if hits.is_empty() {
            println!("  result    : {}", "miss".yellow());
        } else {
            println!("  result    : {}", format!("{} hit(s)", hits.len()).green());
            for h in &hits {
                println!("    - {} :: {}", h.proof_path, h.member_cid);
            }
        }
    }
    Ok(if hits.is_empty() {
        crate::EXIT_OK
    } else {
        crate::EXIT_OK
    })
}

#[derive(Debug, Clone, serde::Serialize)]
struct Hit {
    #[serde(rename = "proofPath")]
    proof_path: String,
    #[serde(rename = "memberCid")]
    member_cid: String,
}

fn find_cid_in_proofs(needle_cid: &str, project_root: &std::path::Path) -> Vec<Hit> {
    let mut out = Vec::new();
    if !project_root.exists() {
        return out;
    }
    for entry in walkdir::WalkDir::new(project_root)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let p = entry.path();
        if p.extension().map(|x| x == "proof").unwrap_or(false) {
            let bytes = match std::fs::read(p) {
                Ok(b) => b,
                Err(_) => continue,
            };
            let cat = match decode(&bytes) {
                Ok(c) => c,
                Err(_) => continue,
            };
            if let Some(map) = cat.as_map() {
                if let Some(members) = map.get("members").and_then(|v| v.as_map()) {
                    for (cid, val) in members {
                        if let Some(body) = val.as_bstr() {
                            // Substring match on member CID or body bytes.
                            if cid.contains(needle_cid)
                                || std::str::from_utf8(body)
                                    .map(|t| t.contains(needle_cid))
                                    .unwrap_or(false)
                            {
                                out.push(Hit {
                                    proof_path: p.display().to_string(),
                                    member_cid: cid.clone(),
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_project_root_returns_empty() {
        let hits = find_cid_in_proofs(
            "blake3-512:0",
            std::path::Path::new("/no/such/dir/exists/please"),
        );
        assert!(hits.is_empty());
    }

    #[test]
    fn hash_matches_pipeline() {
        // Round-trip: parse a formula, JCS-encode, hash via lib, confirm prefix.
        let raw = r#"{"kind":"atomic","name":">","args":[{"kind":"var","name":"x"},{"kind":"const","value":0,"sort":{"kind":"primitive","name":"Int"}}]}"#;
        let j: Json = serde_json::from_str(raw).unwrap();
        let f = parse_formula(&j).unwrap();
        let canonical = encode_jcs(&formula_to_value(&f));
        let cid = blake3_512_of(canonical.as_bytes());
        assert!(cid.starts_with("blake3-512:"));
        assert_eq!(cid.len(), "blake3-512:".len() + 128);
    }
}
