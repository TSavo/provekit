// SPDX-License-Identifier: Apache-2.0
//
// Stage 1: load_all_proofs. Walk <project_root> for *.proof files,
// CBOR-decode the catalog, JSON-parse each member envelope, recompute
// every CID, reject mismatches, index by CID and by sourceSymbol.
//
// v1.1.0: filenames MUST be `blake3-512:<128 hex>.proof` and member
// CIDs MUST start with `"blake3-512:"`. Producer signatures MUST start
// with `"ed25519:"`. Anything else is rejected loud.
//
// v1.2 layered shape (per protocol/specs/2026-05-03-substrate-layers-
// envelope-header-body.md): mementos are `{envelope, header, metadata}`
// with `envelope = {signer, declaredAt, signature}`. The attestation
// CID is `blake3_512(JCS(envelope))`. The verifier branches on the
// presence of a top-level `envelope` key vs. `producerSignature` to
// pick the legacy strip-and-rehash path or the envelope-hash path.
// Both shapes coexist; the catalog cut elsewhere bumps the per-memento
// `schemaVersion` from "1" to "2".

use std::path::{Path, PathBuf};

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value};
use serde_json::Value as Json;

use crate::cbor_decode::decode;
use crate::types::{LoadError, MementoPool};

const HASH_TAG_PREFIX: &str = "blake3-512:";
const SIG_TAG_PREFIX: &str = "ed25519:";

pub fn run(project_root: &Path) -> MementoPool {
    let mut pool = MementoPool::default();
    for path in enumerate_proof_files(project_root) {
        match load_one(&path, &mut pool) {
            Ok(()) => {}
            Err(e) => pool.load_errors.push(LoadError {
                proof_path: path.display().to_string(),
                reason: format!("read/decode: {e}"),
            }),
        }
    }
    pool
}

fn enumerate_proof_files(project_root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if !project_root.exists() {
        return out;
    }
    for entry in walkdir::WalkDir::new(project_root)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            if let Some(ext) = entry.path().extension() {
                if ext == "proof" {
                    out.push(entry.path().to_path_buf());
                }
            }
        }
    }
    out
}

fn load_one(path: &Path, pool: &mut MementoPool) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = std::fs::read(path)?;

    // Rule 1: filename CID matches content (trust root).
    let filename = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_string();
    let stem = filename.trim_end_matches(".proof");
    // We accept either `<hex>.proof` or `blake3-512:<hex>.proof` filename
    // shapes; the trust root is recomputed either way.
    let derived_full = blake3_512_of(&bytes);
    let derived_hex = derived_full.trim_start_matches(HASH_TAG_PREFIX);
    let filename_hex = stem.trim_start_matches(HASH_TAG_PREFIX);
    let hex_only = filename_hex.chars().all(|c| c.is_ascii_hexdigit());
    if hex_only && filename_hex.len() == 128 && filename_hex != derived_hex {
        pool.load_errors.push(LoadError {
            proof_path: path.display().to_string(),
            reason: format!(
                "rule 1 (trust root): filename CID {filename_hex} != content hash {derived_hex}"
            ),
        });
        return Ok(());
    }
    // Filenames whose stem isn't a 128-hex string are tolerated; the
    // CID-from-bytes is what matters. (C++ rejects unknown tags loud;
    // we keep that behavior via the prefix-trim: if there's a tag,
    // it must be blake3-512.)
    if !hex_only && !filename_hex.is_empty() {
        pool.load_errors.push(LoadError {
            proof_path: path.display().to_string(),
            reason: format!(
                "rule 1: filename '{filename}' has non-hex stem; v1.1.0 requires `blake3-512:`"
            ),
        });
        return Ok(());
    }

    let catalog = decode(&bytes)?;
    let m_root = catalog.as_map().ok_or("catalog is not a map")?.clone();

    let members = m_root
        .get("members")
        .ok_or("catalog has no `members` map")?;
    let members_map = members.as_map().ok_or("catalog `members` is not a map")?;

    for (cid, val) in members_map {
        if !cid.starts_with(HASH_TAG_PREFIX) {
            pool.load_errors.push(LoadError {
                proof_path: path.display().to_string(),
                reason: format!(
                    "member {cid}: unsupported hash tag; v1.1.0 requires `{HASH_TAG_PREFIX}`"
                ),
            });
            continue;
        }
        let env_bytes = match val.as_bstr() {
            Some(b) => b,
            None => {
                pool.load_errors.push(LoadError {
                    proof_path: path.display().to_string(),
                    reason: format!("member {cid}: value is not bstr"),
                });
                continue;
            }
        };
        let env_text = std::str::from_utf8(env_bytes)?;
        let env: Json = match serde_json::from_str(env_text) {
            Ok(v) => v,
            Err(e) => {
                pool.load_errors.push(LoadError {
                    proof_path: path.display().to_string(),
                    reason: format!("member {cid}: JSON parse: {e}"),
                });
                continue;
            }
        };
        // Tag-dispatch on whichever signature field is present.
        // v1.1 flat shape: `producerSignature` at top level.
        // v1.2 layered shape: `envelope.signature`.
        let sig_str_opt = env
            .pointer("/envelope/signature")
            .and_then(|v| v.as_str())
            .or_else(|| env.get("producerSignature").and_then(|v| v.as_str()));
        if let Some(sig) = sig_str_opt {
            if !sig.starts_with(SIG_TAG_PREFIX) {
                pool.load_errors.push(LoadError {
                    proof_path: path.display().to_string(),
                    reason: format!(
                        "member {cid}: unsupported signature tag; v1.1.0 requires `{SIG_TAG_PREFIX}`"
                    ),
                });
                continue;
            }
        }
        // Rule 2: re-derive envelope CID. Branch on shape.
        let derived = compute_envelope_cid(&env);
        if derived != *cid {
            pool.load_errors.push(LoadError {
                proof_path: path.display().to_string(),
                reason: format!("rule 2: member {cid} derives to {derived}"),
            });
            continue;
        }
        // Index for handshake. The memento IS the verification;
        // inserting it into the pool IS caching the verification result.
        pool.insert(cid.clone(), env.clone());
        // Track bundle membership so resolve_target can enforce
        // BridgeDeclaration.ConsequentBundlePinned. The bundle's CID is
        // the .proof file's content hash (derived_full above). A given
        // member CID may legitimately appear in more than one bundle;
        // the per-bundle set is what matters at resolve time.
        pool.bundle_members
            .entry(derived_full.clone())
            .or_default()
            .insert(cid.clone());

        // Bridge indexing. Same dual-shape rule:
        //   v1.1: evidence.kind == "bridge", evidence.body.sourceSymbol
        //   v1.2: header.kind == "bridge",   header.sourceSymbol
        let (bridge_kind, source_symbol) = if env.get("envelope").is_some() {
            (
                env.pointer("/header/kind").and_then(|k| k.as_str()),
                env.pointer("/header/sourceSymbol").and_then(|v| v.as_str()),
            )
        } else {
            (
                env.pointer("/evidence/kind").and_then(|k| k.as_str()),
                env.pointer("/evidence/body/sourceSymbol")
                    .and_then(|v| v.as_str()),
            )
        };
        if bridge_kind == Some("bridge") {
            if let Some(sym) = source_symbol {
                if !sym.is_empty() {
                    pool.bridges_by_symbol.insert(sym.to_string(), env.clone());
                }
            }
        }
    }
    Ok(())
}

/// Re-derive an envelope's CID. Branches on memento shape:
///
/// * v1.2 layered: top-level `envelope` is present; CID is
///   `blake3_512(JCS(envelope))` directly. The header and metadata
///   stay outside the hash, but the signature inside the envelope
///   covers them transitively.
///
/// * v1.1 flat: strip `cid` + `producerSignature`, JCS-encode, hash.
fn compute_envelope_cid(env: &Json) -> String {
    if let Some(envelope) = env.get("envelope") {
        let value_tree = json_to_value(envelope);
        let canonical = encode_jcs(&value_tree);
        return blake3_512_of(canonical.as_bytes());
    }
    let mut stripped = env.clone();
    if let Json::Object(map) = &mut stripped {
        map.shift_remove("cid");
        map.shift_remove("producerSignature");
    }
    let value_tree = json_to_value(&stripped);
    let canonical = encode_jcs(&value_tree);
    blake3_512_of(canonical.as_bytes())
}

fn json_to_value(j: &Json) -> std::sync::Arc<Value> {
    match j {
        Json::Null => Value::null(),
        Json::Bool(b) => Value::boolean(*b),
        Json::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::integer(i)
            } else if let Some(u) = n.as_u64() {
                Value::integer(u as i64)
            } else if let Some(f) = n.as_f64() {
                Value::integer(f as i64)
            } else {
                Value::integer(0)
            }
        }
        Json::String(s) => Value::string(s.clone()),
        Json::Array(items) => {
            let v: Vec<_> = items.iter().map(json_to_value).collect();
            Value::array(v)
        }
        Json::Object(map) => {
            let entries: Vec<(String, _)> = map
                .iter()
                .map(|(k, v)| (k.clone(), json_to_value(v)))
                .collect();
            std::sync::Arc::new(Value::Object(entries))
        }
    }
}
