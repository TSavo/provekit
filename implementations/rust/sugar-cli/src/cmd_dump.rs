// SPDX-License-Identifier: Apache-2.0
//
// `provekit dump <PROOF-FILE>`: pretty-print catalog members + bodies.
//
// Uses `sugar_verifier::cbor_decode` to parse the CBOR catalog, then
// re-decodes each member's JCS-JSON body via serde_json. We surface the
// signer CID, declaredAt, member CIDs, evidence kind/body, and
// signatures.

use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use owo_colors::OwoColorize;
use sugar_canonicalizer::blake3_512_of;
use sugar_verifier::cbor_decode::{decode, CborValue};
use serde_json::{json, Map, Value as Json};

use crate::DumpArgs;

pub fn run(args: DumpArgs) -> u8 {
    match dump(&args.proof_file, args.out.json, args.out.quiet) {
        Ok(()) => crate::EXIT_OK,
        Err(e) => {
            eprintln!("{}: {e:#}", "error".red().bold());
            crate::EXIT_USER_ERROR
        }
    }
}

fn dump(path: &PathBuf, as_json: bool, quiet: bool) -> Result<()> {
    let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let derived_cid = blake3_512_of(&bytes);
    let catalog = decode(&bytes).with_context(|| format!("CBOR decode {}", path.display()))?;
    let map = catalog
        .as_map()
        .ok_or_else(|| anyhow!("catalog root is not a CBOR map"))?;

    let name = get_tstr(map, "name").unwrap_or("<unnamed>");
    let version = get_tstr(map, "version").unwrap_or("<unversioned>");
    let signer = get_tstr(map, "signer").unwrap_or("<unsigned>");
    let declared_at = get_tstr(map, "declaredAt").unwrap_or("<undated>");

    let members = map
        .get("members")
        .and_then(|v| v.as_map())
        .ok_or_else(|| anyhow!("catalog has no `members` map"))?;

    if as_json {
        let mut json_members = Map::new();
        for (cid, val) in members {
            json_members.insert(cid.clone(), member_to_json(val)?);
        }
        let payload = json!({
            "path": path.display().to_string(),
            "cid": derived_cid,
            "name": name,
            "version": version,
            "signer": signer,
            "declaredAt": declared_at,
            "memberCount": members.len(),
            "members": Json::Object(json_members),
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    if !quiet {
        println!("{}", "ProvekIt proof envelope".bold());
        println!("  file        : {}", path.display());
        println!("  cid         : {}", derived_cid.cyan());
        println!("  name        : {name}");
        println!("  version     : {version}");
        println!("  signer      : {signer}");
        println!("  declaredAt  : {declared_at}");
        println!("  members     : {}", members.len());
        println!();
        for (cid, val) in members {
            println!("  {} {}", "-".bold(), cid.cyan());
            print_member(val, "      ");
            println!();
        }
    }
    Ok(())
}

fn get_tstr<'a>(
    map: &'a std::collections::BTreeMap<String, CborValue>,
    key: &str,
) -> Option<&'a str> {
    map.get(key).and_then(|v| v.as_tstr())
}

fn member_to_json(val: &CborValue) -> Result<Json> {
    let bytes = val
        .as_bstr()
        .ok_or_else(|| anyhow!("member value is not a CBOR byte string"))?;
    let txt = std::str::from_utf8(bytes).context("member bytes not utf-8 (expected JCS-JSON)")?;
    let parsed: Json = serde_json::from_str(txt).context("parse member JSON")?;
    Ok(parsed)
}

fn print_member(val: &CborValue, indent: &str) {
    let bytes = match val.as_bstr() {
        Some(b) => b,
        None => {
            println!("{indent}<member is not bstr>");
            return;
        }
    };
    let txt = match std::str::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => {
            println!("{indent}<bytes not utf-8>");
            return;
        }
    };
    let parsed: Json = match serde_json::from_str(txt) {
        Ok(v) => v,
        Err(e) => {
            println!("{indent}<JSON parse failed: {e}>");
            return;
        }
    };
    let evidence = parsed.get("evidence");
    let kind = evidence
        .and_then(|e| e.get("kind"))
        .and_then(|k| k.as_str())
        .unwrap_or("<unknown>");
    let producer = parsed
        .get("producedBy")
        .and_then(|s| s.as_str())
        .unwrap_or("?");
    let produced_at = parsed
        .get("producedAt")
        .and_then(|s| s.as_str())
        .unwrap_or("?");
    let signature = parsed
        .get("producerSignature")
        .and_then(|s| s.as_str())
        .map(|s| short_sig(s))
        .unwrap_or_else(|| "<absent>".into());
    println!("{indent}kind        : {kind}");
    println!("{indent}producer    : {producer}");
    println!("{indent}producedAt  : {produced_at}");
    println!("{indent}signature   : {signature}");
    if let Some(body) = evidence.and_then(|e| e.get("body")) {
        let pretty = serde_json::to_string_pretty(body).unwrap_or_else(|_| body.to_string());
        let inner_indent = format!("{indent}  ");
        for line in pretty.lines() {
            println!("{inner_indent}{line}");
        }
    }
}

fn short_sig(s: &str) -> String {
    if s.len() <= 24 {
        s.to_string()
    } else {
        format!("{}...{}", &s[..16], &s[s.len() - 6..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_errors_cleanly() {
        let p = PathBuf::from("/nope/no-proof-here.proof");
        let r = dump(&p, false, true);
        assert!(r.is_err());
    }

    #[test]
    fn short_sig_collapses_long_strings() {
        let s = "ed25519:".to_string() + &"a".repeat(120);
        let out = short_sig(&s);
        assert!(out.contains("..."));
        assert!(out.len() < s.len());
    }
}
