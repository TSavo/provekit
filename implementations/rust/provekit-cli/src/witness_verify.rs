// SPDX-License-Identifier: Apache-2.0
//
// The WITNESS VERIFY DIMENSION.
//
// Verification lives HERE, in the rust CLI. The kit oracle (python / java)
// RESOLVES a witness body over RPC; it is UNTRUSTED and must be verified. For
// each `witness-memento` in the `.proof`:
//
//   1. SIGNATURE -- rust verifies the ed25519 mark over the witness CID with the
//      substrate's own primitive (`ed25519_verify_string`), not the oracle's word.
//   2. RESOLVE + RECOMPUTE -- rust calls `provekit.plugin.resolve_witness` on the
//      kit oracle to fetch the body bytes (from the witness package, or by
//      re-running), then blake3's those bytes ITSELF and compares to the pinned
//      `witness_cid`.
//
// A body the oracle hands back that does NOT recompute to the pinned CID is a
// BROKEN ORACLE -- caught here because rust does the math anyway. Any mismatch
// refuses, loudly. Trust the recomputation, never the resolver.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use serde_json::{json, Value};

use provekit_canonicalizer::blake3_512_of;
use provekit_proof_envelope::ed25519_verify_string;
use provekit_verifier::MementoPool;

/// One witness-memento's verdict from the rust verifier.
#[derive(Debug, Clone)]
pub struct WitnessVerifyResult {
    pub witness_cid: String,
    /// "verified" | "refused" | "broken-oracle"
    pub verdict: String,
    /// The checks rust performed and passed, e.g. ["signature", "content-address:recompute"].
    pub checks: Vec<String>,
    pub reason: String,
}

impl WitnessVerifyResult {
    pub fn is_ok(&self) -> bool {
        self.verdict == "verified"
    }
}

/// Verify every `witness-memento` in the pool. Returns one result per witness
/// (empty when the `.proof` carries no witnesses).
pub fn verify_witnesses(project_root: &Path, pool: &MementoPool) -> Vec<WitnessVerifyResult> {
    let mut out = Vec::new();
    let resolvers = find_resolvers(project_root);
    for env in pool.mementos.values() {
        if env.pointer("/header/kind").and_then(|v| v.as_str()) != Some("witness-memento") {
            continue;
        }
        // The envelope separates METADATA from CONTENT. Route + index by the
        // HEADER (the metadata: kind, witnessCid, signer); verify the BODY (the
        // signed content). The witness's actual run-body is resolved separately
        // from the package -- the deepest content/metadata split.
        let witness_cid = env
            .pointer("/header/witnessCid")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let signer = env
            .pointer("/header/signer")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let body = env.get("body").cloned().unwrap_or(Value::Null);
        let signature = body.get("signature").and_then(|v| v.as_str()).unwrap_or("");
        let mut checks: Vec<String> = Vec::new();

        // ENVELOPE INTEGRITY: the content must match its metadata. The body's
        // own witness_cid must equal the header's witnessCid, or the envelope
        // was tampered (content swapped under unchanged metadata).
        let body_cid = body.get("witness_cid").and_then(|v| v.as_str()).unwrap_or("");
        if body_cid != witness_cid {
            out.push(WitnessVerifyResult {
                witness_cid,
                verdict: "refused".to_string(),
                checks,
                reason: "envelope integrity: body witness_cid disagrees with header metadata"
                    .to_string(),
            });
            continue;
        }

        // 1. SIGNATURE -- rust's own primitive, over the witness CID bytes.
        if witness_cid.is_empty()
            || !ed25519_verify_string(signer, signature, witness_cid.as_bytes())
        {
            out.push(WitnessVerifyResult {
                witness_cid,
                verdict: "refused".to_string(),
                checks,
                reason: "signature invalid -- cannot trust the mark".to_string(),
            });
            continue;
        }
        checks.push("signature".to_string());

        // OUTCOME: a witness recording a FAILED run is not a discharge, even with
        // a valid CID + signature. Refuse it. Witnesses with no `outcome` (a poem,
        // a CI log, a compiler report) skip this check and proceed to recompute.
        if body.get("outcome").and_then(|v| v.as_str()) == Some("failed") {
            out.push(WitnessVerifyResult {
                witness_cid,
                verdict: "refused".to_string(),
                checks,
                reason: "witness records a FAILED run -- not a discharge".to_string(),
            });
            continue;
        }

        // 2. RESOLVE via the kit oracle, then RECOMPUTE the CID here. Try each
        // declared resolver; ACCEPT the first whose returned body BLAKE3's to the
        // pinned witness_cid. The content-address check is the arbiter, so this is
        // sound (no kit can forge a hashing body) and order-independent: a
        // first-found scan could otherwise refuse a valid witness just because an
        // unrelated kit's manifest sorted first. If none hash, report the most
        // informative refusal seen (broken package > honest drift > resolve error).
        if resolvers.is_empty() {
            out.push(WitnessVerifyResult {
                witness_cid,
                verdict: "refused".to_string(),
                checks,
                reason: "no witness resolver declared (manifest `resolve_witness_command`); \
                         cannot resolve the body to recompute"
                    .to_string(),
            });
            continue;
        }
        let mut verified: Option<(String, Vec<String>)> = None;
        let mut broken_oracle: Option<String> = None;
        let mut drift: Option<String> = None;
        let mut errors: Vec<String> = Vec::new();
        for (argv, working_dir, method) in &resolvers {
            match resolve_body(argv, working_dir.as_deref(), method, project_root, &body) {
                Ok((resolved_by, bytes)) => {
                    let computed = blake3_512_of(&bytes);
                    if computed == witness_cid {
                        let mut c = checks.clone();
                        c.push(format!("content-address:{resolved_by}"));
                        verified = Some((resolved_by, c));
                        break;
                    } else if resolved_by == "package" {
                        // The package paired this CID with bytes that do NOT hash
                        // to it. The resolver delivered wrong content -- a BROKEN
                        // ORACLE / tampered package -- caught because rust did the
                        // math anyway.
                        broken_oracle.get_or_insert(format!(
                            "package content computes to {computed}, not the pinned {witness_cid} \
                             -- broken oracle / tampered package; rust recomputed the CID and refused"
                        ));
                    } else {
                        // The oracle re-ran honestly and got a different result:
                        // the witness no longer reproduces. The resolver was
                        // honest; the proof is stale (code/runtime DRIFTED).
                        drift.get_or_insert(format!(
                            "witness did not reproduce (re-run drifted): computed {computed} != \
                             pinned {witness_cid} -- the oracle was honest, the proof is stale"
                        ));
                    }
                }
                Err(e) => errors.push(e),
            }
        }
        if let Some((resolved_by, c)) = verified {
            out.push(WitnessVerifyResult {
                witness_cid: witness_cid.clone(),
                verdict: "verified".to_string(),
                checks: c,
                reason: format!(
                    "oracle resolved via {resolved_by}; rust recomputed the CID and it matched"
                ),
            });
        } else if let Some(reason) = broken_oracle {
            out.push(WitnessVerifyResult {
                witness_cid: witness_cid.clone(),
                verdict: "broken-oracle".to_string(),
                checks,
                reason,
            });
        } else if let Some(reason) = drift {
            out.push(WitnessVerifyResult {
                witness_cid: witness_cid.clone(),
                verdict: "refused".to_string(),
                checks,
                reason,
            });
        } else {
            out.push(WitnessVerifyResult {
                witness_cid: witness_cid.clone(),
                verdict: "refused".to_string(),
                checks,
                reason: format!("could not resolve witness body: {}", errors.join("; ")),
            });
        }
    }
    out
}

/// True when the pool carries at least one `witness-memento`.
pub fn has_witnesses(pool: &MementoPool) -> bool {
    pool.mementos
        .values()
        .any(|e| e.pointer("/header/kind").and_then(|v| v.as_str()) == Some("witness-memento"))
}

/// Scan `.provekit/lift/*/manifest.toml` for EVERY kit that declares a
/// `resolve_witness_command`, returning each as (argv, working_dir, method). We
/// return all of them, not the first found, because the resolver is not chosen
/// blindly by directory order: each witness picks the resolver whose body BLAKE3's
/// to its pinned CID (see `verify_witnesses`). A wrong kit cannot forge a hashing
/// body, so trying each is sound and removes the order-dependence a single
/// first-found scan would have in a multi-kit project.
fn find_resolvers(project_root: &Path) -> Vec<(Vec<String>, Option<PathBuf>, String)> {
    let lift_dir = project_root.join(".provekit").join("lift");
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(&lift_dir) else {
        return found;
    };
    for entry in entries.flatten() {
        let manifest = entry.path().join("manifest.toml");
        if !manifest.exists() {
            continue;
        }
        if let Some(r) = parse_resolve_command(&manifest, project_root) {
            found.push(r);
        }
    }
    found
}

/// Minimal TOML read for the `resolve_witness_command` array + `working_dir`,
/// mirroring cmd_prove's manifest parser (multi-line arrays, `#` comments).
fn parse_resolve_command(
    manifest: &Path,
    project_root: &Path,
) -> Option<(Vec<String>, Option<PathBuf>, String)> {
    let text = std::fs::read_to_string(manifest).ok()?;
    let strip = |l: &str| -> String {
        match l.find('#') {
            Some(p) => l[..p].to_string(),
            None => l.to_string(),
        }
    };
    let raw: Vec<String> = text.lines().map(|l| strip(l).trim().to_string()).collect();
    let mut argv: Vec<String> = Vec::new();
    let mut working_dir: Option<PathBuf> = None;
    let mut method: Option<String> = None;
    let mut i = 0;
    while i < raw.len() {
        let line = raw[i].clone();
        i += 1;
        if line.is_empty() || line.starts_with('[') {
            continue;
        }
        let Some(eq) = line.find('=') else { continue };
        let key = line[..eq].trim().to_string();
        let mut val = line[eq + 1..].trim().to_string();
        if val.starts_with('[') && !val.contains(']') {
            while i < raw.len() && !val.contains(']') {
                val.push(' ');
                val.push_str(&raw[i]);
                i += 1;
            }
        }
        match key.as_str() {
            "resolve_witness_command" => {
                let inner = val.trim_matches(|c| c == '[' || c == ']');
                argv = inner
                    .split(',')
                    .map(|s| s.trim().trim_matches('"').to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            "working_dir" => {
                let p = PathBuf::from(val.trim_matches('"'));
                working_dir = Some(if p.is_absolute() {
                    p
                } else {
                    project_root.join(p)
                });
            }
            "resolve_witness_method" => {
                method = Some(val.trim_matches('"').to_string());
            }
            _ => {}
        }
    }
    if argv.is_empty() {
        return None;
    }
    Some((
        argv,
        working_dir.or_else(|| Some(project_root.to_path_buf())),
        // Honor the manifest's declared method; default to the canonical one.
        method.unwrap_or_else(|| "provekit.plugin.resolve_witness".to_string()),
    ))
}

/// Spawn the kit oracle and call `provekit.plugin.resolve_witness`, returning
/// (resolved_by, body_bytes). The oracle returns CONTENT, not a verdict; the
/// caller recomputes the CID.
fn resolve_body(
    argv: &[String],
    working_dir: Option<&Path>,
    method: &str,
    project_root: &Path,
    memento_body: &Value,
) -> Result<(String, Vec<u8>), String> {
    if argv.is_empty() {
        return Err("empty resolver argv".to_string());
    }
    let abs_root = std::fs::canonicalize(project_root)
        .unwrap_or_else(|_| project_root.to_path_buf())
        .display()
        .to_string();
    let package_dir = project_root.join(".provekit").join("witnesses");
    let mut params = json!({
        "memento": memento_body,
        "workspace_root": abs_root,
    });
    if package_dir.exists() {
        params["package_dir"] = json!(package_dir.display().to_string());
    }
    let req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    });

    let mut cmd = Command::new(&argv[0]);
    cmd.args(&argv[1..]);
    cmd.arg("--rpc");
    if let Some(wd) = working_dir {
        cmd.current_dir(wd);
    }
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::null());
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("spawn resolver {}: {e}", argv[0]))?;
    {
        // TAKE stdin and DROP it after writing, so the kit's read loop sees EOF,
        // breaks, and closes stdout -- otherwise rust blocks reading to EOF while
        // the kit blocks reading the next request line (deadlock).
        let mut stdin = child.stdin.take().ok_or("resolver stdin unavailable")?;
        let line = serde_json::to_string(&req).map_err(|e| e.to_string())?;
        stdin
            .write_all(line.as_bytes())
            .and_then(|_| stdin.write_all(b"\n"))
            .map_err(|e| format!("write resolver stdin: {e}"))?;
        // stdin dropped here -> EOF to the kit.
    }
    // Read stdout on a worker thread and bound the wait with a TIMEOUT: even with
    // stdin dropped (EOF), a misbehaving kit could ignore it and hang. On timeout
    // we kill the child and refuse, rather than block verify forever.
    let stdout = child.stdout.take().ok_or("resolver stdout unavailable")?;
    let (tx, rx) = std::sync::mpsc::channel::<Option<Value>>();
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        let mut last_reply: Option<Value> = None;
        for line in reader.lines().map_while(Result::ok) {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
                if v.get("result").is_some() || v.get("error").is_some() {
                    last_reply = Some(v);
                }
            }
        }
        let _ = tx.send(last_reply);
    });
    const RESOLVER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);
    let reply = match rx.recv_timeout(RESOLVER_TIMEOUT) {
        Ok(r) => r,
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "resolver `{}` timed out after {}s",
                argv[0],
                RESOLVER_TIMEOUT.as_secs()
            ));
        }
    };
    let _ = child.wait();
    let reply = reply.ok_or("resolver produced no JSON-RPC reply")?;
    if let Some(err) = reply.get("error") {
        let msg = err
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        return Err(format!("oracle refused resolution: {msg}"));
    }
    let result = reply.get("result").ok_or("reply missing `result`")?;
    let body_b64 = result
        .get("body_b64")
        .and_then(|v| v.as_str())
        .ok_or("resolve_witness result missing `body_b64`")?;
    let resolved_by = result
        .get("resolved_by")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let bytes = B64
        .decode(body_b64)
        .map_err(|e| format!("decode body_b64: {e}"))?;
    Ok((resolved_by, bytes))
}
