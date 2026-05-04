// SPDX-License-Identifier: Apache-2.0
//
// `provekit mint` — the lift-plugin protocol dispatcher.
//
// Architecture (substrate-as-only-mint-pipeline):
//
//   One Rust CLI; N language kits. The CLI is the sole mint pipeline for
//   every kit — including the rust kit itself. Rust is NOT special-cased.
//   Every kit exposes a lifter binary that speaks the lift-protocol RPC
//   (`initialize` + `lift`). The CLI drives that RPC, receives a
//   `proof-envelope` response, and then:
//
//     1. Writes the `.proof` file to the output directory (same as before).
//     2. Signs a self-contracts attestation (letter-envelope format, per
//        spec #94 / `protocol/specs/2026-05-02-bundle-attestation-protocol.md`)
//        and writes it to
//        `<repo-root>/.provekit/self-contracts-attestations/<kit>.json`.
//
//   The dogfood invariant: ProvekIt's `prove` verifies each kit satisfies
//   the canonical contracts minted by the rust kit. The substrate proves
//   the kits; the kits prove the substrate.
//
//   The lift protocol (`initialize` + `lift`) is distinct from the LSP
//   parse protocol (`initialize` + `parse`). The former is for mint; the
//   latter is for editor diagnostics. This dispatcher calls the lifter,
//   NOT the LSP.
//
// Spec: protocol/specs/2026-04-30-lift-plugin-protocol.md (draft for v1.2.0).
//       protocol/specs/2026-05-02-bundle-attestation-protocol.md
//       spec #94 §2 (contractSetCid in signed body)
//
// Response shapes: only `proof-envelope` (shape c) is supported in v1.
// Shapes (a) `ir-document` and (b) `signed-mementos` are spec'd but
// unimplemented; adding them is additive, requires no client breakage.
//
// Missing-lifter behavior: when a manifest declares a binary that does
// not exist yet (ENOENT on spawn), mint produces a well-formed
// attestation with contractSetCid = EMPTY_SET_CID (the BLAKE3-512 of
// JCS(`[]`)). This surfaces the gap at the per-kit lifter level without
// failing the substrate pipeline. Any other spawn failure (wrong
// permissions, exit > 0) is a hard error.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

use base64::Engine;
use clap::Parser;
use owo_colors::OwoColorize;
use serde_json::{json, Value};

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value as CValue};
use provekit_claim_envelope::{compute_contract_set_cid, mint_contract, Authoring, MintContractArgs, contract_cid};
use provekit_proof_envelope::{build_proof_envelope, ProofEnvelopeInput, ed25519_pubkey_string, ed25519_sign_string, Ed25519Seed};

use crate::project_config::{read_project_config, read_user_config};
use crate::OutputFlags;
use crate::{EXIT_OK, EXIT_USER_ERROR, EXIT_VERIFY_FAIL};

// ---------------------------------------------------------------------------
// Foundation signing constants
// ---------------------------------------------------------------------------

/// The v0 foundation seed. PUBLICLY KNOWN. Same seed used by foundation-keygen.
const FOUNDATION_V0_SEED: Ed25519Seed = [0x42u8; 32];

/// Pinned `declaredAt` for self-contracts attestations minted under the
/// unified pipeline. Matches v1.4.1 catalog declared_at for consistency.
const SELF_CONTRACTS_DECLARED_AT: &str = "2026-05-03T18:00:00Z";


/// Canonical mapping from `--kit=<name>` to (project_subdir, lift_surface, lang_key).
///
/// * `project_subdir` — path segment under `implementations/` (the project root passed to the lifter)
/// * `lift_surface` — subdirectory name under `.provekit/lift/<surface>/` (the manifest to load)
/// * `lang_key` — the `lang` field in the signed attestation JSON (and the
///   key for the `.provekit/self-contracts-attestations/<lang>.json` filename)
///
/// Naming diverges for several kits:
///   `ts`     → project dir `typescript`,  surface `typescript`,             lang `ts`
///   `csharp` → project dir `csharp`,      surface `csharp`,                 lang `csharp`
///   `rust`   → project dir `rust`,        surface `rust-self-contracts`,    lang `rust`
///   `go`     → project dir `go`,          surface `go-self-contracts`,      lang `go`
///
/// `--kit=rust` and `--kit=go` route to their self-contracts surfaces (which
/// invoke the slab-walking mint binaries) rather than the generic
/// workspace lifters (`provekit-lift` for rust, the test-fixture lifter for go).
/// Without this, `make mint-rust` / `make mint-go` walk the wrong source and
/// produce content-empty CIDs. The `--project=implementations/<lang>
/// --surface=<lang>` form still reaches the workspace/test-fixture lifters
/// for tooling that needs them.
/// Fix: issue #176 Tier 1, option (c) — route every kit to its
/// self-contracts lifter (PR #180 for go, PR #183 for rust).
const KIT_TABLE: &[(&str, &str, &str, &str)] = &[
    // (kit_alias, project_subdir, lift_surface,           lang_key)
    ("rust",       "rust",        "rust-self-contracts",  "rust"),
    ("go",         "go",          "go-self-contracts",    "go"),
    ("cpp",        "cpp",         "cpp-self-contracts",    "cpp"),
    ("ts",         "typescript",  "typescript-self-contracts", "ts"),
    ("csharp",     "csharp",      "csharp",               "csharp"),
    ("swift",      "swift",       "swift-self-contracts", "swift"),
    ("java",       "java",        "java-self-contracts",  "java"),
    ("python",     "python",      "python-self-contracts", "python"),
    ("ruby",       "ruby",        "ruby-self-contracts",  "ruby"),
    ("zig",        "zig",         "zig-self-contracts",   "zig"),
    ("c",          "c",           "c-self-contracts",     "c"),
];

/// Resolve `--kit=<name>` to the canonical project path, lift surface, and lang key.
/// Returns `(project_path, surface, lang_key)` relative to the CWD at
/// which `provekit` is invoked (expected to be repo root).
fn resolve_kit(kit: &str) -> Option<(PathBuf, String, String)> {
    KIT_TABLE.iter().find(|(alias, _, _, _)| *alias == kit).map(|(_, subdir, surface, lang)| {
        (
            PathBuf::from("implementations").join(subdir),
            surface.to_string(),
            lang.to_string(),
        )
    })
}

// ---------------------------------------------------------------------------
// Plugin manifest
// ---------------------------------------------------------------------------

/// Plugin manifest read from `.../lift/<name>/manifest.toml`.
#[derive(Debug, Default)]
struct PluginManifest {
    name: String,
    command: Vec<String>,
    working_dir: Option<PathBuf>,
}

fn parse_manifest(path: &Path) -> Result<PluginManifest, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("read {}: {e}", path.display()))?;
    let mut m = PluginManifest::default();
    for line in text.lines() {
        let line = match line.find('#') {
            Some(p) => &line[..p],
            None => line,
        }
        .trim();
        if line.is_empty() || line.starts_with('[') {
            continue;
        }
        let Some(eq) = line.find('=') else { continue };
        let key = line[..eq].trim();
        let val = line[eq + 1..].trim();
        match key {
            "name" => m.name = val.trim_matches('"').to_string(),
            "working_dir" => m.working_dir = Some(PathBuf::from(val.trim_matches('"'))),
            "command" => {
                let inner = val.trim_matches(|c| c == '[' || c == ']');
                m.command = inner
                    .split(',')
                    .map(|s| s.trim().trim_matches('"').to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            _ => {}
        }
    }
    if m.command.is_empty() {
        return Err(format!("manifest {} has no `command`", path.display()));
    }
    Ok(m)
}

fn find_manifest(project_root: &Path, surface: &str) -> Result<PluginManifest, String> {
    let project_local = project_root
        .join(".provekit")
        .join("lift")
        .join(surface)
        .join("manifest.toml");
    if project_local.exists() {
        return parse_manifest(&project_local);
    }
    if let Some(home) = std::env::var_os("HOME") {
        let user_global = PathBuf::from(home)
            .join(".config")
            .join("provekit")
            .join("lift")
            .join(surface)
            .join("manifest.toml");
        if user_global.exists() {
            return parse_manifest(&user_global);
        }
    }
    Err(format!(
        "no plugin manifest for surface `{surface}` (looked in .provekit/lift/{surface}/manifest.toml and ~/.config/provekit/lift/{surface}/manifest.toml)"
    ))
}

// ---------------------------------------------------------------------------
// Lift-protocol dispatch
// ---------------------------------------------------------------------------

/// Build the `params` object for the lift JSON-RPC request.
///
/// Extracted as a pure function so unit tests can assert the C3 invariant
/// (non-empty `source_paths`) without spawning a subprocess.
fn build_lift_params(surface: &str) -> Value {
    json!({
        "surface": surface,
        "source_paths": ["."],
        "options": {"layer": "all"}
    })
}

/// Result of a successful lift dispatch.
struct DispatchResult {
    filename_cid: String,
    contract_set_cid: String,
    bytes_written: usize,
}

/// Dispatch the lift-protocol RPC.
///
/// On ENOENT (lifter binary not found), returns `Ok` with
/// `contract_set_cid = compute_contract_set_cid([])` and writes no .proof.
/// The caller then signs an empty-set attestation. This surfaces the gap
/// at the per-kit lifter level without failing the substrate pipeline.
///
/// All other spawn failures (permission denied, RPC errors) are hard errors.
fn dispatch(
    project_root: &Path,
    surface: &str,
    out_dir: &Path,
    quiet: bool,
) -> Result<DispatchResult, String> {
    let manifest = find_manifest(project_root, surface)?;
    if !quiet {
        println!(
            "{}: surface=`{}` plugin=`{}` command={:?}",
            "dispatch".green().bold(),
            surface,
            manifest.name,
            manifest.command
        );
    }

    let mut cmd = Command::new(&manifest.command[0]);
    if manifest.command.len() > 1 {
        cmd.args(&manifest.command[1..]);
    }
    // Append --rpc only if the manifest doesn't already include it.
    // Several manifests (e.g. typescript) hard-code --rpc in their command
    // array; appending unconditionally produces duplicate args, which some
    // lifters reject. (Review feedback: PR #165 / Copilot.)
    if !manifest.command.iter().any(|a| a == "--rpc") {
        cmd.arg("--rpc");
    }
    if let Some(wd) = &manifest.working_dir {
        let resolved = if wd.is_absolute() {
            wd.clone()
        } else {
            project_root.join(wd)
        };
        cmd.current_dir(resolved);
    }
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::inherit());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Lifter binary not installed yet. Surface as empty-set attestation.
            if !quiet {
                println!(
                    "{}: lifter binary `{}` not found — producing empty-set attestation",
                    "warn".yellow().bold(),
                    manifest.command[0]
                );
            }
            let empty_cid = compute_contract_set_cid(vec![]);
            return Ok(DispatchResult {
                filename_cid: String::new(),
                contract_set_cid: empty_cid,
                bytes_written: 0,
            });
        }
        Err(e) => return Err(format!("spawn {:?}: {e}", manifest.command)),
    };

    let mut stdin = child.stdin.take().ok_or("no stdin")?;
    let stdout = child.stdout.take().ok_or("no stdout")?;
    let mut reader = BufReader::new(stdout);

    // 1. initialize
    let init_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "client": {"name": "provekit-cli", "version": env!("CARGO_PKG_VERSION")},
            "protocol_version": "provekit-lift/1",
            "workspace_root": project_root.canonicalize().unwrap_or_else(|_| project_root.to_path_buf()),
            "config_path": ".provekit/config.toml"
        }
    });
    writeln!(stdin, "{init_req}").map_err(|e| format!("write initialize: {e}"))?;

    let init_resp = read_response(&mut reader, 1)?;
    if !quiet {
        if let Some(name) = init_resp.get("name").and_then(|v| v.as_str()) {
            println!("{}: plugin `{}` ready", "ok".green().bold(), name);
        }
    }

    // 2. lift — send source_paths:["."] to satisfy C3 non-empty invariant.
    //    Most lifters walk their own working directory regardless of source_paths,
    //    but C3 (`verify_c3_lift_request_well_formed`) requires the array to be
    //    non-empty. Mirror the pattern from cmd_prove::capture_rpc.
    let lift_params = build_lift_params(surface);
    let lift_req = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "lift",
        "params": lift_params
    });
    writeln!(stdin, "{lift_req}").map_err(|e| format!("write lift: {e}"))?;
    let lift_resp = read_response(&mut reader, 2)?;

    // 3. shutdown
    let shutdown_req = json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "shutdown"
    });
    let _ = writeln!(stdin, "{shutdown_req}");
    drop(stdin);
    let _ = child.wait();

    // Process response: shape `proof-envelope` or `ir-document`
    let kind = lift_resp.get("kind").and_then(|v| v.as_str()).ok_or(
        "lift response missing `kind` field",
    )?;
    match kind {
        "proof-envelope" => {
            let filename_cid = lift_resp
                .get("filename_cid")
                .and_then(|v| v.as_str())
                .ok_or("missing filename_cid")?
                .to_string();
            let contract_set_cid = lift_resp
                .get("contract_set_cid")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let bytes_b64 = lift_resp
                .get("bytes_base64")
                .and_then(|v| v.as_str())
                .ok_or("missing bytes_base64")?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(bytes_b64)
                .map_err(|e| format!("decode bytes_base64: {e}"))?;

            std::fs::create_dir_all(out_dir)
                .map_err(|e| format!("mkdir {}: {e}", out_dir.display()))?;
            let out_path = out_dir.join(format!("{filename_cid}.proof"));
            std::fs::write(&out_path, &bytes)
                .map_err(|e| format!("write {}: {e}", out_path.display()))?;

            if !quiet {
                for d in lift_resp
                    .get("diagnostics")
                    .and_then(|v| v.as_array())
                    .into_iter()
                    .flatten()
                {
                    let s = d.as_str().unwrap_or("");
                    if !s.is_empty() {
                        println!("{}: {s}", "note".dimmed());
                    }
                }
            }

            Ok(DispatchResult {
                filename_cid,
                contract_set_cid,
                bytes_written: bytes.len(),
            })
        }
        "ir-document" => {
            let ir = lift_resp
                .get("ir")
                .and_then(|v| v.as_array())
                .ok_or("ir-document response missing `ir` array")?;

            let (bytes, filename_cid, contract_set_cid) =
                mint_from_ir_document(ir, &project_root)?;

            std::fs::create_dir_all(out_dir)
                .map_err(|e| format!("mkdir {}: {e}", out_dir.display()))?;
            let out_path = out_dir.join(format!("{filename_cid}.proof"));
            std::fs::write(&out_path, &bytes)
                .map_err(|e| format!("write {}: {e}", out_path.display()))?;

            if !quiet {
                let diags = lift_resp
                    .get("diagnostics")
                    .and_then(|v| v.as_array());

                if let Some(diags) = diags {
                    for d in diags {
                        let s = d.as_str().unwrap_or("");
                        if !s.is_empty() {
                            println!("{}: {s}", "note".dimmed());
                        }
                    }
                }
            }

            Ok(DispatchResult {
                filename_cid,
                contract_set_cid,
                bytes_written: bytes.len(),
            })
        }
        other => Err(format!(
            "unsupported response shape `{other}`; expected `proof-envelope` or `ir-document`",
        )),
    }
}

fn read_response(reader: &mut impl BufRead, id: i64) -> Result<Value, String> {
    let mut line = String::new();
    let n = reader
        .read_line(&mut line)
        .map_err(|e| format!("read response: {e}"))?;
    if n == 0 {
        return Err("plugin closed stdout before responding".to_string());
    }
    let v: Value = serde_json::from_str(line.trim())
        .map_err(|e| format!("parse JSON-RPC response: {e}\n  raw: {line}"))?;
    if v.get("id").and_then(|v| v.as_i64()) != Some(id) {
        return Err(format!("response id mismatch: expected {id}, got {v:?}"));
    }
    if let Some(err) = v.get("error") {
        return Err(format!("plugin returned error: {err}"));
    }
    v.get("result")
        .cloned()
        .ok_or_else(|| "response missing `result`".to_string())
}

// ---------------------------------------------------------------------------
// ir-document → proof-envelope minting
// ---------------------------------------------------------------------------

fn mint_from_ir_document(
    ir: &[Value],
    _project_root: &Path,
) -> Result<(Vec<u8>, String, String), String> {
    use std::collections::BTreeMap;

    let mut members: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut content_cids: Vec<String> = Vec::new();
    let signer_seed: Ed25519Seed = FOUNDATION_V0_SEED;
    let produced_at = "2026-05-03T18:00:00Z".to_string();

    for decl in ir {
        let kind = decl
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if kind != "contract" {
            continue;
        }

        let name = decl
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unnamed")
            .to_string();
        let out_binding = decl
            .get("outBinding")
            .and_then(|v| v.as_str())
            .unwrap_or("out")
            .to_string();
        let pre = decl.get("pre").map(json_to_cvalue);
        let post = decl.get("post").map(json_to_cvalue);
        let inv = decl.get("inv").map(json_to_cvalue);

        if pre.is_none() && post.is_none() && inv.is_none() {
            continue;
        }

        let args = MintContractArgs {
            contract_name: name,
            pre,
            post,
            inv,
            out_binding,
            produced_by: "provekit-cli".to_string(),
            produced_at: produced_at.clone(),
            input_cids: vec![],
            authoring: Authoring::Lift {
                lifter: "ir-document".to_string(),
                evidence: "minted from ir-document RPC response".to_string(),
                source_cid: None,
            },
            signer_seed,
        };

        let ccid = contract_cid(&args);
        content_cids.push(ccid);

        let m = mint_contract(&args)
            .map_err(|e| format!("mint contract: {e}"))?;

        members.entry(m.cid.clone()).or_insert(m.canonical_bytes);
    }

    if members.is_empty() {
        return Err("no contracts to mint".to_string());
    }

    let contract_set_cid = compute_contract_set_cid(content_cids);

    let signer_pubkey = ed25519_pubkey_string(&signer_seed);
    let signer_cid = blake3_512_of(signer_pubkey.as_bytes());

    let proof_input = ProofEnvelopeInput {
        name: "ir-document".to_string(),
        version: "1.0.0".to_string(),
        binary_cid: None,
        metadata: None,
        members,
        signer_cid,
        signer_seed,
        declared_at: produced_at,
    };

    let built = build_proof_envelope(&proof_input);

    Ok((built.bytes, built.cid, contract_set_cid))
}

/// Convert `serde_json::Value` to `provekit_canonicalizer::Value`.
fn json_to_cvalue(j: &Value) -> Arc<CValue> {
    match j {
        Value::Null => CValue::null(),
        Value::Bool(b) => CValue::boolean(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                CValue::integer(i)
            } else if let Some(u) = n.as_u64() {
                CValue::integer(u as i64)
            } else if let Some(f) = n.as_f64() {
                CValue::integer(f as i64)
            } else {
                CValue::integer(0)
            }
        }
        Value::String(s) => CValue::string(s.clone()),
        Value::Array(items) => {
            let v: Vec<_> = items.iter().map(|x| json_to_cvalue(x)).collect();
            CValue::array(v)
        }
        Value::Object(map) => {
            let entries: Vec<(String, Arc<CValue>)> = map
                .iter()
                .map(|(k, v)| (k.clone(), json_to_cvalue(v)))
                .collect();
            CValue::object(entries)
        }
    }
}

// ---------------------------------------------------------------------------
// Self-contracts attestation signing
// ---------------------------------------------------------------------------

/// Build the signed self-contracts attestation JSON for a kit.
///
/// The signed body (per spec #94 §2) is the seven-field object without
/// `signature`. JCS encoding of that body is what the foundation key signs.
///
/// When `bundle_cid` is empty (lifter binary not found), the attestation
/// records `cid: ""` — callers can detect the empty-lifter case via this
/// field. The `contractSetCid` is still valid (it's the empty-set CID).
fn build_signed_attestation(
    lang: &str,
    bundle_cid: &str,
    contract_set_cid: &str,
) -> Value {
    let signer_pubkey = ed25519_pubkey_string(&FOUNDATION_V0_SEED);

    // Build the seven-field message body (no `signature`).
    // JCS sorts keys by code point; we build as a canonicalizer object in
    // the SAME field order as foundation-keygen does so the bytes are
    // byte-identical to what sign-self-contracts produces.
    let entries: Vec<(String, Arc<CValue>)> = vec![
        ("schemaVersion".to_string(), CValue::string("1".to_string())),
        ("kind".to_string(), CValue::string("self-contracts-attestation".to_string())),
        ("lang".to_string(), CValue::string(lang.to_string())),
        ("cid".to_string(), CValue::string(bundle_cid.to_string())),
        ("contractSetCid".to_string(), CValue::string(contract_set_cid.to_string())),
        ("declaredAt".to_string(), CValue::string(SELF_CONTRACTS_DECLARED_AT.to_string())),
        ("signer".to_string(), CValue::string(signer_pubkey.clone())),
    ];
    let msg_obj = CValue::object(entries);
    let jcs_bytes = encode_jcs(&msg_obj).into_bytes();
    let signature = ed25519_sign_string(&FOUNDATION_V0_SEED, &jcs_bytes);

    json!({
        "schemaVersion": "1",
        "kind": "self-contracts-attestation",
        "lang": lang,
        "cid": bundle_cid,
        "contractSetCid": contract_set_cid,
        "declaredAt": SELF_CONTRACTS_DECLARED_AT,
        "signer": signer_pubkey,
        "signature": signature,
    })
}

/// Write the signed attestation to `<repo_root>/.provekit/self-contracts-attestations/<lang>.json`.
///
/// The repo root is located by ascending from the project root looking for
/// a `.provekit/self-contracts-attestations/` directory. Falls back to
/// searching from CWD if the project root doesn't resolve it.
fn write_attestation(
    project_root: &Path,
    lang: &str,
    bundle_cid: &str,
    contract_set_cid: &str,
    quiet: bool,
) -> Result<PathBuf, String> {
    let attestation = build_signed_attestation(lang, bundle_cid, contract_set_cid);
    let json_str = serde_json::to_string_pretty(&attestation)
        .map_err(|e| format!("serialize attestation: {e}"))?;

    let attest_dir = find_attestation_dir(project_root)?;
    std::fs::create_dir_all(&attest_dir)
        .map_err(|e| format!("mkdir {}: {e}", attest_dir.display()))?;
    let out_path = attest_dir.join(format!("{lang}.json"));
    std::fs::write(&out_path, json_str.as_bytes())
        .map_err(|e| format!("write {}: {e}", out_path.display()))?;
    if !quiet {
        println!("{}: wrote {}", "attest".green().bold(), out_path.display());
    }
    Ok(out_path)
}

/// Locate the `.provekit/self-contracts-attestations/` directory by
/// searching upward from `start`.
fn find_attestation_dir(start: &Path) -> Result<PathBuf, String> {
    // Walk up through the directory tree looking for the attestation dir.
    let abs = start
        .canonicalize()
        .unwrap_or_else(|_| start.to_path_buf());
    let mut cur = abs.as_path();
    loop {
        let candidate = cur.join(".provekit").join("self-contracts-attestations");
        if candidate.exists() {
            return Ok(candidate);
        }
        match cur.parent() {
            Some(p) => cur = p,
            None => break,
        }
    }
    // Fall back: construct from current working directory.
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    Ok(cwd.join(".provekit").join("self-contracts-attestations"))
}

// ---------------------------------------------------------------------------
// MintArgs + run
// ---------------------------------------------------------------------------

#[derive(Parser, Debug, Clone)]
pub struct MintArgs {
    /// Project root containing `.provekit/config.toml`. Defaults to current dir.
    #[arg(long)]
    pub project: Option<PathBuf>,
    /// Kit shortcut: maps `<kit>` to `implementations/<kit>`.
    /// Equivalent to `--project implementations/<kit>`.
    /// Known kits: rust, go, cpp, ts, csharp, swift, java, python, ruby, zig, c.
    #[arg(long, conflicts_with = "project")]
    pub kit: Option<String>,
    /// Override the authoring surface (otherwise read from config or derived from --kit).
    #[arg(long)]
    pub surface: Option<String>,
    /// Output directory for the produced `.proof` file. Defaults to current dir.
    #[arg(long)]
    pub out: Option<PathBuf>,
    /// Skip writing the signed attestation JSON.
    #[arg(long)]
    pub no_attest: bool,
    #[command(flatten)]
    pub flags: OutputFlags,
}

pub fn run(args: MintArgs) -> u8 {
    // Resolve (project_root, surface, lang_key) from --kit or --project.
    let (project_root, derived_surface, lang_key) = if let Some(kit) = &args.kit {
        match resolve_kit(kit) {
            Some((path, surface, lang)) => (path, Some(surface), Some(lang)),
            None => {
                let known: Vec<&str> = KIT_TABLE.iter().map(|(a, _, _, _)| *a).collect();
                eprintln!(
                    "{}: unknown kit `{}`; known kits: {}",
                    "error".red().bold(),
                    kit,
                    known.join(", ")
                );
                return EXIT_USER_ERROR;
            }
        }
    } else {
        let path = args.project.clone().unwrap_or_else(|| PathBuf::from("."));
        (path, None, None)
    };

    if !project_root.exists() {
        eprintln!("{}: project not found: {}", "error".red().bold(), project_root.display());
        return EXIT_USER_ERROR;
    }

    // Resolve surface: --surface > --kit derived > project config > user config.
    let surface = if let Some(s) = args.surface {
        s
    } else if let Some(s) = derived_surface {
        s
    } else {
        let project_cfg = read_project_config(&project_root);
        let user_cfg = read_user_config();
        match project_cfg
            .surface_for("must")
            .or_else(|| user_cfg.surface_for("must"))
        {
            Some(s) => s,
            None => {
                eprintln!(
                    "{}: no `[authoring] surface` in .provekit/config.toml. Pass --surface or --kit.",
                    "error".red().bold()
                );
                return EXIT_USER_ERROR;
            }
        }
    };

    let out_dir = args.out.unwrap_or_else(|| project_root.clone());

    match dispatch(&project_root, &surface, &out_dir, args.flags.quiet) {
        Ok(result) => {
            let contract_set_cid = if result.contract_set_cid.is_empty() {
                compute_contract_set_cid(vec![])
            } else {
                result.contract_set_cid.clone()
            };

            if !args.flags.quiet {
                println!();
                if !result.filename_cid.is_empty() {
                    println!("  catalog CID:        {}", result.filename_cid);
                }
                println!("  contractSetCid:     {contract_set_cid}");
                if result.bytes_written > 0 {
                    println!("  proof bytes:        {}", result.bytes_written);
                    println!(
                        "  .proof file:        {}",
                        out_dir.join(format!("{}.proof", result.filename_cid)).display()
                    );
                } else {
                    println!("  (no .proof written — lifter binary not found)");
                }
            } else {
                // Quiet mode: first line = bundle CID, second line = contractSetCid.
                // The Makefile captures contractSetCid via grep.
                if !result.filename_cid.is_empty() {
                    println!("{}", result.filename_cid);
                }
                println!("contractSetCid: {contract_set_cid}");
            }

            // Write attestation unless suppressed.
            if !args.no_attest {
                // Determine lang_key: use --kit derived value, else infer from surface.
                let lang = lang_key.as_deref().unwrap_or(&surface);
                if let Err(e) = write_attestation(
                    &project_root,
                    lang,
                    &result.filename_cid,
                    &contract_set_cid,
                    args.flags.quiet,
                ) {
                    eprintln!("{}: {e}", "warn".yellow().bold());
                    // Non-fatal: attestation write failure doesn't fail the mint.
                }
            }

            EXIT_OK
        }
        Err(e) => {
            eprintln!("{}: {e}", "error".red().bold());
            EXIT_VERIFY_FAIL
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_kit_ts_maps_to_typescript_dir() {
        // Issue #204: ts kit routes to typescript-self-contracts surface (not the
        // generic workspace lifter) so --kit=ts mints real self-contracts.
        let (path, surface, lang) = resolve_kit("ts").expect("ts must resolve");
        assert_eq!(path, PathBuf::from("implementations/typescript"));
        assert_eq!(surface, "typescript-self-contracts");
        assert_eq!(lang, "ts");
    }

    #[test]
    fn resolve_kit_rust_maps_to_rust_dir() {
        // Issue #176 Tier 1: rust kit maps to rust-self-contracts surface so the
        // attestation reflects the canonical self-contracts slab, not the generic
        // workspace lifter.
        let (path, surface, lang) = resolve_kit("rust").expect("rust must resolve");
        assert_eq!(path, PathBuf::from("implementations/rust"));
        assert_eq!(surface, "rust-self-contracts");
        assert_eq!(lang, "rust");
    }

    #[test]
    fn resolve_kit_all_11_kits() {
        let kits = ["rust", "go", "cpp", "ts", "csharp", "swift", "java", "python", "ruby", "zig", "c"];
        for kit in kits {
            assert!(resolve_kit(kit).is_some(), "kit `{kit}` must resolve");
        }
    }

    #[test]
    fn resolve_kit_unknown_returns_none() {
        assert!(resolve_kit("haskell").is_none());
    }

    #[test]
    fn build_signed_attestation_has_required_fields() {
        let a = build_signed_attestation("rust", "blake3-512:deadbeef", "blake3-512:cafebabe");
        assert_eq!(a["schemaVersion"].as_str(), Some("1"));
        assert_eq!(a["kind"].as_str(), Some("self-contracts-attestation"));
        assert_eq!(a["lang"].as_str(), Some("rust"));
        assert!(a["signature"].as_str().unwrap().starts_with("ed25519:"));
        assert!(a["signer"].as_str().unwrap().starts_with("ed25519:"));
    }

    #[test]
    fn build_signed_attestation_is_deterministic() {
        let a = build_signed_attestation("go", "blake3-512:aa", "blake3-512:bb");
        let b = build_signed_attestation("go", "blake3-512:aa", "blake3-512:bb");
        assert_eq!(a, b);
    }

    #[test]
    fn dispatch_lift_params_source_paths_non_empty() {
        // C3 (verify_c3_lift_request_well_formed) requires source_paths to be
        // a non-empty array. Sending [] was the bug fixed in issue #166.
        let params = build_lift_params("rust");
        let paths = params["source_paths"]
            .as_array()
            .expect("source_paths must be an array");
        assert!(
            !paths.is_empty(),
            "source_paths must not be empty — was C3 violation (issue #166)"
        );
        assert_eq!(paths[0].as_str(), Some("."), "first entry should be '.'");
    }

    #[test]
    fn dispatch_lift_params_has_surface_and_options() {
        let params = build_lift_params("go");
        assert_eq!(params["surface"].as_str(), Some("go"));
        assert_eq!(params["options"]["layer"].as_str(), Some("all"));
    }

    #[test]
    fn empty_set_cid_is_stable() {
        // Verify compute_contract_set_cid([]) is stable across calls.
        let a = compute_contract_set_cid(vec![]);
        let b = compute_contract_set_cid(vec![]);
        assert_eq!(a, b);
        assert!(a.starts_with("blake3-512:"));
        // Print so the integration test can verify against the pinned value.
        eprintln!("empty-set CID = {a}");
    }
}
