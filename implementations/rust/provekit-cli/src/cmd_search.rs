// SPDX-License-Identifier: Apache-2.0
//
// `provekit search --consequent <FORMULA>` / `--antecedent <FORMULA>`.
//
// Hash the input formula via the canonicalizer, then walk *.proof
// files and return any IMPLICATION memento whose `consequentHash` (or
// `antecedentHash`) matches the query CID. v0 substring match on JCS
// bytes; v1 will use the CID->memento store.

use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use owo_colors::OwoColorize;
use provekit_canonicalizer::{blake3_512_of, encode_jcs};
use provekit_ir_symbolic::parse::parse_formula;
use provekit_ir_symbolic::serialize::formula_to_value;
use provekit_verifier::cbor_decode::decode;
use serde_json::{json, Value as Json};

use crate::SearchArgs;

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub enum Slot {
    Antecedent,
    Consequent,
}

pub fn run(args: SearchArgs) -> u8 {
    let (slot, formula_path) = match (args.consequent.clone(), args.antecedent.clone()) {
        (Some(p), None) => (Slot::Consequent, p),
        (None, Some(p)) => (Slot::Antecedent, p),
        (None, None) => {
            eprintln!(
                "{}: must pass --consequent <FILE> or --antecedent <FILE>",
                "error".red().bold()
            );
            return crate::EXIT_USER_ERROR;
        }
        (Some(_), Some(_)) => {
            eprintln!(
                "{}: --consequent and --antecedent are mutually exclusive",
                "error".red().bold()
            );
            return crate::EXIT_USER_ERROR;
        }
    };

    match search(slot, &formula_path, args.project.as_deref(), args.out.json, args.out.quiet) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}: {e:#}", "error".red().bold());
            crate::EXIT_USER_ERROR
        }
    }
}

fn search(
    slot: Slot,
    formula_file: &PathBuf,
    project: Option<&std::path::Path>,
    as_json: bool,
    quiet: bool,
) -> Result<u8> {
    let raw = std::fs::read_to_string(formula_file)
        .with_context(|| format!("read {}", formula_file.display()))?;
    let j: Json = serde_json::from_str(&raw).context("parse JSON formula")?;
    let f = parse_formula(&j).map_err(|e| anyhow!("parse IR formula: {e}"))?;
    let canonical = encode_jcs(&formula_to_value(&f));
    let cid = blake3_512_of(canonical.as_bytes());

    let project_root = project.map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    let hits = find_implication_hits(&slot, &cid, &project_root);

    if as_json {
        let payload = json!({
            "slot": match slot { Slot::Antecedent => "antecedent", Slot::Consequent => "consequent" },
            "queryCid": cid,
            "hits": hits,
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else if !quiet {
        println!("{}", "ProvekIt search".bold());
        println!("  slot      : {:?}", slot);
        println!("  query CID : {}", cid.cyan());
        if hits.is_empty() {
            println!("  result    : {}", "miss".yellow());
        } else {
            println!("  result    : {}", format!("{} hit(s)", hits.len()).green());
            for h in &hits {
                println!("    - {}  ({})", h.implication_cid, h.proof_path);
            }
        }
    }
    Ok(crate::EXIT_OK)
}

#[derive(Debug, Clone, serde::Serialize)]
struct Hit {
    #[serde(rename = "implicationCid")]
    implication_cid: String,
    #[serde(rename = "proofPath")]
    proof_path: String,
}

fn find_implication_hits(slot: &Slot, needle: &str, project_root: &std::path::Path) -> Vec<Hit> {
    let mut out = Vec::new();
    if !project_root.exists() {
        return out;
    }
    let needed_field = match slot {
        Slot::Antecedent => "antecedentHash",
        Slot::Consequent => "consequentHash",
    };
    for entry in walkdir::WalkDir::new(project_root)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let p = entry.path();
        if !p.extension().map(|x| x == "proof").unwrap_or(false) {
            continue;
        }
        let bytes = match std::fs::read(p) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let cat = match decode(&bytes) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let map = match cat.as_map() {
            Some(m) => m,
            None => continue,
        };
        let members = match map.get("members").and_then(|v| v.as_map()) {
            Some(m) => m,
            None => continue,
        };
        for (cid, val) in members {
            let body = match val.as_bstr() {
                Some(b) => b,
                None => continue,
            };
            let txt = match std::str::from_utf8(body) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let parsed: Json = match serde_json::from_str(txt) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let evidence = match parsed.get("evidence") {
                Some(e) => e,
                None => continue,
            };
            let kind = evidence
                .get("kind")
                .and_then(|k| k.as_str())
                .unwrap_or("");
            if kind != "implication" {
                continue;
            }
            let body_obj = match evidence.get("body") {
                Some(b) => b,
                None => continue,
            };
            let field = body_obj
                .get(needed_field)
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if field == needle {
                out.push(Hit {
                    implication_cid: cid.clone(),
                    proof_path: p.display().to_string(),
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_with_no_proofs_is_empty() {
        let dir = std::env::temp_dir().join(format!(
            "provekit-cli-search-empty-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let hits = find_implication_hits(&Slot::Antecedent, "blake3-512:0", &dir);
        assert!(hits.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }
}
