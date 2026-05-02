// SPDX-License-Identifier: Apache-2.0
//
// recompute-spec-cids
//
// Catalog freeze tool (current target: v1.3.1). Computes BLAKE3-512 CIDs for every protocol
// spec file listed in `protocol/specs/2026-04-30-protocol-catalog.json`,
// substitutes them into the catalog (replacing `RECOMPUTE-AFTER-*`
// placeholders), then computes the catalog's own CID as
// BLAKE3-512(JCS(catalog-json)).
//
// Modes:
//
//   recompute-spec-cids                 read catalog, recompute, fail if
//                                       any CID drifts; do NOT write
//                                       (safe default; equivalent to the
//                                       legacy `--verify`)
//   recompute-spec-cids --verify        no-op alias for the default;
//                                       retained because the on-disk
//                                       protocol-catalog-format spec
//                                       (§5) names `--verify` literally
//                                       and removing the flag would edit
//                                       a normative spec
//   recompute-spec-cids --write         compute CIDs and write the
//                                       substituted catalog back to disk;
//                                       the only mode that mutates the
//                                       working tree
//
// History note: prior to the audit-#180 fix, the default mode was the
// mutating one and `--verify` was the read-only mode. Running the tool
// without args silently changed the catalog. The current default is
// read-only; `--write` is required to mutate. No back-compat shim for
// the old mutating-default behavior; if a script relied on it, it now
// fails fast with a refusal-to-write message and the script must add
// `--write` explicitly.
//
// Spec bytes: hashed verbatim (raw file bytes; no canonicalization).
// Catalog bytes: hashed in JCS-canonical form (RFC 8785) using the same
// `provekit-canonicalizer::encode_jcs` the Rust peer implementation uses,
// guaranteeing cross-language hash agreement.
//
// The catalog file as committed is human-readable JSON (insertion order,
// trailing newline). The CID is computed over the JCS form of the same
// data, NOT the file bytes.
//
// Catalog property key -> spec file basename (without extension):
const SPEC_MAP: &[(&str, &str)] = &[
    ("ir-formal-grammar", "2026-04-30-ir-formal-grammar.md"),
    ("canonicalization-grammar", "2026-04-30-canonicalization-grammar.md"),
    ("memento-envelope-grammar", "2026-04-30-memento-envelope-grammar.md"),
    (
        "signatures-and-non-repudiation",
        "2026-04-30-signatures-and-non-repudiation.md",
    ),
    (
        "chain-validity-and-fail-closed",
        "2026-04-30-chain-validity-and-fail-closed.md",
    ),
    ("ir-extension-protocol", "2026-04-30-ir-extension-protocol.md"),
    ("proof-file-format", "2026-04-30-proof-file-format.md"),
    ("semantic-envelope", "2026-04-29-the-semantic-envelope.md"),
    (
        "supply-chain-via-semantic-envelope",
        "2026-04-29-supply-chain-via-semantic-envelope.md",
    ),
    ("handshake-algorithm", "2026-04-30-handshake-algorithm.md"),
    (
        "per-language-kit-standard",
        "2026-04-29-per-language-kit-standard.md",
    ),
    (
        "lattice-tractability-theorem",
        "2026-04-30-lattice-tractability-theorem.md",
    ),
    (
        "contract-merge-semantics",
        "2026-04-30-contract-merge-semantics.md",
    ),
    (
        "protocol-catalog-format",
        "2026-04-30-protocol-catalog-format.md",
    ),
    (
        "agent-plugin-protocol",
        "2026-04-30-agent-plugin-protocol.md",
    ),
    (
        "ir-compiler-protocol",
        "2026-04-30-ir-compiler-protocol.md",
    ),
    (
        "multi-solver-protocol",
        "2026-04-30-multi-solver-protocol.md",
    ),
    (
        "lift-plugin-protocol",
        "2026-04-30-lift-plugin-protocol.md",
    ),
    (
        "correctness-is-a-hash",
        "2026-04-29-correctness-is-a-hash.md",
    ),
    (
        "lsp-protocol",
        "2026-04-30-lsp-protocol.md",
    ),
];

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value};
use serde_json::Value as JsonValue;

fn specs_dir() -> PathBuf {
    // Resolve relative to this source file's location: <repo>/tools/recompute-spec-cids/src/main.rs
    // Walk up two parents -> tools/recompute-spec-cids -> tools -> <repo>/protocol/specs
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = Path::new(manifest)
        .parent()
        .and_then(|p| p.parent())
        .expect("CARGO_MANIFEST_DIR has two ancestors");
    repo_root.join("protocol").join("specs")
}

/// Convert a serde_json::Value into a canonicalizer Value tree. Numbers
/// are restricted to i64 (the catalog has no floats); anything outside
/// i64 panics deliberately (loud failure beats silent miscanonicalization).
fn to_canonical(j: &JsonValue) -> Arc<Value> {
    match j {
        JsonValue::Null => Value::null(),
        JsonValue::Bool(b) => Value::boolean(*b),
        JsonValue::Number(n) => {
            let i = n
                .as_i64()
                .unwrap_or_else(|| panic!("non-i64 number in catalog: {}", n));
            Value::integer(i)
        }
        JsonValue::String(s) => Value::string(s.clone()),
        JsonValue::Array(items) => Value::array(items.iter().map(to_canonical).collect()),
        JsonValue::Object(map) => {
            let entries: Vec<(String, Arc<Value>)> = map
                .iter()
                .map(|(k, v)| (k.clone(), to_canonical(v)))
                .collect();
            Value::object(entries)
        }
    }
}

fn hash_spec_bytes(path: &Path) -> std::io::Result<String> {
    let bytes = fs::read(path)?;
    Ok(blake3_512_of(&bytes))
}

fn run(write: bool) -> Result<(), String> {
    let specs = specs_dir();
    let catalog_path = specs.join("2026-04-30-protocol-catalog.json");

    // 1. Hash every spec file.
    let mut cids: BTreeMap<String, String> = BTreeMap::new();
    let mut missing: Vec<String> = Vec::new();
    for (key, file) in SPEC_MAP {
        let path = specs.join(file);
        match hash_spec_bytes(&path) {
            Ok(cid) => {
                cids.insert((*key).to_string(), cid);
            }
            Err(_) => {
                missing.push(format!("{}  ({})", key, file));
            }
        }
    }
    if !missing.is_empty() {
        return Err(format!(
            "spec files missing on disk:\n  {}",
            missing.join("\n  ")
        ));
    }

    // 2. Load the catalog as JSON (preserving insertion order via
    //    serde_json's preserve_order feature).
    let catalog_text = fs::read_to_string(&catalog_path)
        .map_err(|e| format!("read {}: {}", catalog_path.display(), e))?;
    let mut catalog: JsonValue = serde_json::from_str(&catalog_text)
        .map_err(|e| format!("parse catalog json: {}", e))?;

    // 3. Cross-check: every catalog property key must be in our SPEC_MAP,
    //    and every SPEC_MAP key must be in the catalog. Surface any drift.
    let catalog_keys: Vec<String> = catalog
        .get("properties")
        .and_then(|p| p.as_object())
        .ok_or_else(|| "catalog missing `properties` object".to_string())?
        .keys()
        .cloned()
        .collect();
    let map_keys: Vec<String> = SPEC_MAP.iter().map(|(k, _)| (*k).to_string()).collect();

    for k in &catalog_keys {
        if !map_keys.contains(k) {
            return Err(format!(
                "catalog has property `{}` but SPEC_MAP does not list it",
                k
            ));
        }
    }
    for k in &map_keys {
        if !catalog_keys.contains(k) {
            return Err(format!(
                "SPEC_MAP lists `{}` but catalog has no such property",
                k
            ));
        }
    }

    // 4. Substitute CIDs into the catalog properties.
    {
        let props = catalog
            .get_mut("properties")
            .and_then(|p| p.as_object_mut())
            .ok_or_else(|| "catalog properties not an object".to_string())?;
        for (key, _) in SPEC_MAP {
            let cid = cids
                .get(*key)
                .ok_or_else(|| format!("missing CID for {}", key))?;
            props.insert((*key).to_string(), JsonValue::String(cid.clone()));
        }
    }

    // 5. Compute catalog CID over JCS-canonical bytes of the substituted
    //    catalog. This is the single value that names the current protocol version.
    let canonical_value = to_canonical(&catalog);
    let jcs_bytes = encode_jcs(&canonical_value);
    let catalog_cid = blake3_512_of(jcs_bytes.as_bytes());

    // 6. Default mode (read-only): confirm the on-disk catalog already
    //    has these CIDs in place (i.e., re-running this tool produces no
    //    diff). Only mutate when --write was passed explicitly.
    if write {
        // Write back as human-readable JSON, two-space indent + trailing
        // newline to match the existing on-disk style.
        let mut out = serde_json::to_string_pretty(&catalog)
            .map_err(|e| format!("serialize catalog: {}", e))?;
        out.push('\n');
        fs::write(&catalog_path, out)
            .map_err(|e| format!("write {}: {}", catalog_path.display(), e))?;
    } else {
        let on_disk_props = catalog_text
            .parse::<JsonValue>()
            .map_err(|e| format!("re-parse catalog: {}", e))?;
        let on_disk_props = on_disk_props
            .get("properties")
            .and_then(|p| p.as_object())
            .ok_or_else(|| "catalog properties missing on re-parse".to_string())?;
        for (key, _) in SPEC_MAP {
            let want = cids
                .get(*key)
                .ok_or_else(|| format!("internal: no CID for {}", key))?;
            let got = on_disk_props
                .get(*key)
                .and_then(|v| v.as_str())
                .ok_or_else(|| format!("on-disk catalog missing property {}", key))?;
            if got != want {
                return Err(format!(
                    "catalog CID drift for {}:\n  on-disk: {}\n  recomputed: {}\n\n\
                     refusing to write without explicit --write flag.",
                    key, got, want
                ));
            }
        }
    }

    // 7. Report.
    println!("# Protocol catalog freeze (v1.3.1)");
    println!();
    println!("Catalog file:    {}", catalog_path.display());
    println!("Catalog CID:     {}", catalog_cid);
    println!("JCS byte count:  {}", jcs_bytes.len());
    println!();
    println!("| Spec key | CID |");
    println!("|---|---|");
    for (key, _) in SPEC_MAP {
        let cid = cids.get(*key).expect("present");
        println!("| `{}` | `{}` |", key, cid);
    }

    Ok(())
}

fn main() -> ExitCode {
    // Default is read-only (the safe behavior). --verify is a no-op
    // alias kept for the legacy invocation that the on-disk
    // protocol-catalog-format spec §5 names literally; --write is the
    // only flag that mutates the working tree.
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut write = false;
    for arg in &args {
        match arg.as_str() {
            "--write" => write = true,
            "--verify" => { /* no-op: default is already read-only */ }
            "-h" | "--help" => {
                println!(
                    "recompute-spec-cids\n\
                     \n\
                     Default (no args)  read-only verify; fails on CID drift.\n\
                     --verify           alias for the default (kept for spec-text compatibility).\n\
                     --write            recompute and write the catalog back to disk."
                );
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("recompute-spec-cids: unknown argument `{}`", other);
                eprintln!("usage: recompute-spec-cids [--write | --verify]");
                return ExitCode::FAILURE;
            }
        }
    }
    match run(write) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("recompute-spec-cids: {}", e);
            ExitCode::FAILURE
        }
    }
}
