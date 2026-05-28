// SPDX-License-Identifier: Apache-2.0
//
// `provekit recognize`: kit-owned source-level recognition per protocol §4.2.5.
//
// Walks user source files, asks the language kit (via the lift plugin manifest
// for the requested surface) to match function bodies' identifier-canonical
// AST templates against supplied `binding_templates`, emits tier-`exact` tags
// for matches. Bindings come from one or more `.proof` envelopes the caller
// passes via `--binding`; each envelope is loaded and its
// `library-sugar-binding-entry` records become recognize-side templates.
//
// This is the Tron-named verb: each kit's recognizer walks the grid for
// shapes that belong to its system. The substrate stays language-blind; only
// the kit reads AST shapes. See:
//   protocol/specs/2026-05-12-plugin-protocol.md §4.2.5
//   implementations/rust/provekit-walk/src/bin/walk_rpc.rs `recognize` handler

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use std::collections::BTreeMap;

use clap::Parser;
use owo_colors::OwoColorize;
use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value as CanonicalValue};
use provekit_ir_types::{BridgeHeaderV14, BridgeTarget};
use provekit_proof_envelope::cbor_decode::{decode as cbor_decode, CborValue};
use provekit_proof_envelope::{
    build_proof_envelope, ed25519_pubkey_string, Ed25519Seed, ProofEnvelopeInput,
};
use serde_json::{json, Value};

use crate::{OutputFlags, EXIT_OK, EXIT_USER_ERROR, EXIT_VERIFY_FAIL};

// Distinct signer seed for recognize-emitted bridges. Different from
// materialize's so the verifier can audit which lane authored each
// bridge in a mixed pool (both lanes emit the same body shape; the
// signer identity carries provenance).
const RECOGNIZE_BRIDGE_SIGNER_SEED: Ed25519Seed = [0x72; 32]; // 'r' for recognize
const RECOGNIZE_BRIDGE_DECLARED_AT: &str = "2026-05-28T00:00:00.000Z";

/// Arguments accepted by `provekit recognize`.
#[derive(Parser, Debug, Clone, Default)]
pub struct RecognizeArgs {
    /// Project root containing `.provekit/lift/<surface>/manifest.toml`.
    /// Defaults to current directory.
    #[arg(long)]
    pub project: Option<PathBuf>,
    /// Source paths (relative to project root) to scan for matches.
    /// Repeatable. e.g. `--source src/lib.rs --source src/ingest.rs`.
    #[arg(long = "source")]
    pub source_paths: Vec<String>,
    /// Paths to `.proof` envelopes that carry the binding templates.
    /// Repeatable; bindings from all envelopes union into one pool.
    #[arg(long = "binding")]
    pub bindings: Vec<PathBuf>,
    /// Lift surface name (default `rust-bind`). Resolves to
    /// `<project>/.provekit/lift/<surface>/manifest.toml`.
    #[arg(long)]
    pub surface: Option<String>,
    /// Mint bridge mementos from recognize tags into the project's
    /// `.provekit/recognize/<cid>.proof`. Without this flag, the verb
    /// is a dry-run that only prints tags. With it, the bridges land
    /// in the proof pool and become first-class citizens in `provekit
    /// prove` — same shape as materialize-authored bridges.
    #[arg(long)]
    pub write: bool,
    /// Source language (target of the kit). Today defaults to "rust".
    /// Reserved for the polyglot case once Java/Python/TS/Go kits get
    /// their own ast_template lifters.
    #[arg(long = "target", default_value = "rust")]
    pub target_language: String,
    #[command(flatten)]
    pub out: OutputFlags,
}

pub fn run(args: RecognizeArgs) -> u8 {
    let project_root = match args
        .project
        .clone()
        .map(|p| p.canonicalize().unwrap_or(p))
        .or_else(|| std::env::current_dir().ok())
    {
        Some(p) => p,
        None => {
            eprintln!("{}: cannot resolve project root", "error".red().bold());
            return EXIT_USER_ERROR;
        }
    };

    let surface = args.surface.clone().unwrap_or_else(|| "rust-bind".to_string());
    let manifest = match find_plugin_manifest(&project_root, &surface) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{}: {e}", "error".red().bold());
            return EXIT_USER_ERROR;
        }
    };

    // Build the binding_templates pool by loading each .proof envelope and
    // extracting its library-sugar-binding-entry records.
    let mut binding_templates: Vec<Value> = Vec::new();
    for path in &args.bindings {
        match load_binding_templates_from_proof(path) {
            Ok(mut entries) => binding_templates.append(&mut entries),
            Err(e) => {
                eprintln!(
                    "{}: load binding `{}`: {e}",
                    "error".red().bold(),
                    path.display()
                );
                return EXIT_USER_ERROR;
            }
        }
    }

    if !args.out.json && !args.out.quiet {
        eprintln!(
            "{}: surface=`{}` bindings={} sources={}",
            "dispatch".green().bold(),
            surface,
            binding_templates.len(),
            args.source_paths.len(),
        );
    }

    // Dispatch the recognize RPC.
    let req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "provekit.plugin.recognize",
        "params": {
            "project_root": project_root.to_string_lossy(),
            "source_paths": args.source_paths,
            "binding_templates": binding_templates,
        }
    });

    let tags = match invoke_plugin(&manifest, &project_root, &req) {
        Ok(resp) => match resp.get("result").and_then(|r| r.get("tags")).cloned() {
            Some(Value::Array(a)) => a,
            _ => {
                eprintln!(
                    "{}: plugin response missing result.tags: {resp}",
                    "error".red().bold()
                );
                return EXIT_VERIFY_FAIL;
            }
        },
        Err(e) => {
            eprintln!("{}: recognize dispatch failed: {e}", "error".red().bold());
            return EXIT_VERIFY_FAIL;
        }
    };

    // Mint bridge mementos from tags when --write is set. Same shape as
    // cmd_materialize's bridge emission, written under .provekit/recognize/
    // (sibling to .provekit/materialize/ so the lanes are distinguishable
    // but the verifier picks both up via load_all_proofs).
    let mut written_proof: Option<std::path::PathBuf> = None;
    if args.write {
        match emit_bridge_envelope(&project_root, &tags, &args.target_language) {
            Ok(Some(path)) => {
                written_proof = Some(path);
            }
            Ok(None) => {
                if !args.out.quiet && !args.out.json {
                    eprintln!(
                        "{}: --write requested but no tags carry a contract_cid; nothing minted",
                        "note".yellow().bold()
                    );
                }
            }
            Err(e) => {
                eprintln!("{}: emit bridge envelope: {e}", "error".red().bold());
                return EXIT_VERIFY_FAIL;
            }
        }
    }

    if args.out.json {
        let out = json!({
            "tags": tags,
            "bridge_proof": written_proof
                .as_ref()
                .map(|p| Value::String(p.display().to_string()))
                .unwrap_or(Value::Null),
        });
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_else(|_| out.to_string()));
    } else {
        println!("recognize: {} tag(s) emitted", tags.len());
        for (idx, tag) in tags.iter().enumerate() {
            let concept = tag.get("concept_name").and_then(|v| v.as_str()).unwrap_or("?");
            let file = tag.get("file").and_then(|v| v.as_str()).unwrap_or("?");
            let span = tag.get("span").cloned().unwrap_or(Value::Null);
            let start_line = span.get("start_line").and_then(|v| v.as_u64()).unwrap_or(0);
            let tier = tag.get("match_tier").and_then(|v| v.as_str()).unwrap_or("?");
            let fn_name = tag
                .get("function_name")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            println!(
                "  [{idx}] {} @ {}:{} (fn={}, {})",
                concept.green(),
                file,
                start_line,
                fn_name.cyan(),
                tier.dimmed()
            );
        }
        if let Some(path) = &written_proof {
            println!(
                "{}: minted {} bridge(s) into {}",
                "write".green().bold(),
                tags.iter().filter(|t| !t["contract_cid"].is_null()).count(),
                path.display()
            );
        }
    }

    EXIT_OK
}

/// Build the materialize-shape bridge body from a recognize tag. The
/// shape is identical to cmd_materialize's `materialize_bridge_body`
/// so the verifier and prove machinery treat recognize-authored bridges
/// the same as materialize-authored ones. The signer identity differs
/// (RECOGNIZE_BRIDGE_SIGNER_SEED vs MATERIALIZE_BRIDGE_SIGNER_SEED) so
/// provenance is auditable.
fn recognize_bridge_body(tag: &Value, target_language: &str) -> Option<Value> {
    let contract_cid = tag.get("contract_cid").and_then(|v| v.as_str())?;
    if contract_cid.is_empty() {
        return None;
    }
    let function_name = tag
        .get("function_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let library_tag = tag
        .get("library_tag")
        .and_then(|v| v.as_str())
        .unwrap_or(target_language)
        .to_string();
    let header = BridgeHeaderV14 {
        schema_version: "1".to_string(),
        kind: "bridge".to_string(),
        name: format!("recognize:{}:{}", target_language, function_name),
        source_symbol: function_name,
        source_layer: target_language.to_string(),
        source_contract_cid: contract_cid.to_string(),
        target: BridgeTarget::Contract {
            cid: contract_cid.to_string(),
        },
    };
    let mut value = serde_json::to_value(header).ok()?;
    if let Value::Object(map) = &mut value {
        map.insert(
            "targetContractCid".to_string(),
            Value::String(contract_cid.to_string()),
        );
        map.insert(
            "targetLayer".to_string(),
            Value::String(library_tag),
        );
    }
    Some(value)
}

/// Mint a `.proof` envelope containing one bridge memento per
/// recognize tag with a non-null contract_cid. Written under
/// `<project>/.provekit/recognize/<cid>.proof`. Returns the path
/// when bridges are minted; Ok(None) when no tags carried contract_cids.
fn emit_bridge_envelope(
    project_root: &Path,
    tags: &[Value],
    target_language: &str,
) -> Result<Option<std::path::PathBuf>, String> {
    let proof_dir = project_root.join(".provekit").join("recognize");
    let mut members: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for tag in tags {
        let Some(body) = recognize_bridge_body(tag, target_language) else {
            continue;
        };
        let envelope = json!({ "evidence": { "kind": "bridge", "body": body } });
        let (cid, bytes) = flat_member_canonical(&envelope)?;
        members.entry(cid).or_insert(bytes);
    }
    if members.is_empty() {
        return Ok(None);
    }
    std::fs::create_dir_all(&proof_dir)
        .map_err(|e| format!("create {}: {e}", proof_dir.display()))?;
    let signer = ed25519_pubkey_string(&RECOGNIZE_BRIDGE_SIGNER_SEED);
    let proof = build_proof_envelope(&ProofEnvelopeInput {
        name: "@provekit/recognize-bridges".to_string(),
        version: "0.1.0".to_string(),
        binary_cid: None,
        metadata: None,
        members,
        signer_cid: signer,
        signer_seed: RECOGNIZE_BRIDGE_SIGNER_SEED,
        declared_at: RECOGNIZE_BRIDGE_DECLARED_AT.to_string(),
    });
    let path = proof_dir.join(format!("{}.proof", proof.cid));
    std::fs::write(&path, &proof.bytes)
        .map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(Some(path))
}

fn flat_member_canonical(envelope: &Value) -> Result<(String, Vec<u8>), String> {
    let canonical = canonical_value_of_json(envelope)?;
    let bytes = encode_jcs(canonical.as_ref());
    let cid = blake3_512_of(bytes.as_bytes());
    Ok((cid, bytes.into_bytes()))
}

fn canonical_value_of_json(value: &Value) -> Result<std::sync::Arc<CanonicalValue>, String> {
    match value {
        Value::Null => Ok(CanonicalValue::null()),
        Value::Bool(b) => Ok(CanonicalValue::boolean(*b)),
        Value::Number(n) => n
            .as_i64()
            .map(CanonicalValue::integer)
            .ok_or_else(|| format!("recognize bridge contains non-integer number `{n}`")),
        Value::String(s) => Ok(CanonicalValue::string(s)),
        Value::Array(values) => values
            .iter()
            .map(canonical_value_of_json)
            .collect::<Result<Vec<_>, _>>()
            .map(CanonicalValue::array),
        Value::Object(entries) => entries
            .iter()
            .map(|(k, v)| canonical_value_of_json(v).map(|v| (k.clone(), v)))
            .collect::<Result<Vec<_>, _>>()
            .map(CanonicalValue::object),
    }
}

/// Manifest discovery: project-local then user-global. Mirrors lift_plugin's
/// `find_manifest` (which is module-private). Kept local here so recognize
/// can ship independently.
struct PluginManifest {
    command: Vec<PathBuf>,
    working_dir: Option<PathBuf>,
}

fn find_plugin_manifest(project_root: &Path, surface: &str) -> Result<PluginManifest, String> {
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

fn parse_manifest(path: &Path) -> Result<PluginManifest, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("read manifest {}: {e}", path.display()))?;
    let mut command: Vec<PathBuf> = Vec::new();
    let mut working_dir: Option<PathBuf> = None;
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("command") {
            // command = ["binary", "--flag"]
            if let Some(arr_text) = rest.split_once('=').map(|(_, v)| v.trim()) {
                command = parse_toml_string_array(arr_text);
            }
        } else if let Some(rest) = line.strip_prefix("working_dir") {
            if let Some(val) = rest.split_once('=').map(|(_, v)| v.trim()) {
                if let Some(s) = strip_quotes(val) {
                    working_dir = Some(PathBuf::from(s));
                }
            }
        }
    }
    if command.is_empty() {
        return Err(format!("manifest {} declares no command", path.display()));
    }
    Ok(PluginManifest { command, working_dir })
}

fn parse_toml_string_array(text: &str) -> Vec<PathBuf> {
    let trimmed = text.trim().trim_start_matches('[').trim_end_matches(']');
    trimmed
        .split(',')
        .filter_map(|s| {
            let t = s.trim();
            strip_quotes(t).map(PathBuf::from)
        })
        .collect()
}

fn strip_quotes(s: &str) -> Option<&str> {
    s.strip_prefix('"').and_then(|s| s.strip_suffix('"'))
}

/// Spawn the plugin binary with the manifest's command + working_dir, send
/// the JSON-RPC request, read one JSON line response, shutdown. The lift
/// binary's dispatch (initialize / lift / shutdown / provekit.plugin.recognize)
/// accepts the recognize method directly; no preceding initialize required.
fn invoke_plugin(
    manifest: &PluginManifest,
    project_root: &Path,
    request: &Value,
) -> Result<Value, String> {
    let (program, args) = manifest
        .command
        .split_first()
        .ok_or("plugin manifest command is empty")?;

    let mut cmd = Command::new(program);
    cmd.args(args);
    if let Some(working_dir) = &manifest.working_dir {
        let resolved = if working_dir.is_absolute() {
            working_dir.clone()
        } else {
            project_root.join(working_dir)
        };
        cmd.current_dir(resolved);
    } else {
        cmd.current_dir(project_root);
    }
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| format!("spawn plugin: {e}"))?;
    {
        let stdin = child.stdin.as_mut().ok_or("plugin stdin closed")?;
        let req_line = serde_json::to_string(request).map_err(|e| e.to_string())?;
        writeln!(stdin, "{req_line}").map_err(|e| format!("write request: {e}"))?;
        // Send a shutdown so the plugin exits cleanly after answering.
        let shutdown = json!({"jsonrpc":"2.0","id":2,"method":"shutdown","params":{}});
        writeln!(stdin, "{}", serde_json::to_string(&shutdown).unwrap())
            .map_err(|e| format!("write shutdown: {e}"))?;
    }

    let stdout = child.stdout.take().ok_or("plugin stdout closed")?;
    let mut reader = BufReader::new(stdout);
    let mut response_line = String::new();
    reader
        .read_line(&mut response_line)
        .map_err(|e| format!("read response: {e}"))?;
    let _ = child.wait();
    if response_line.trim().is_empty() {
        return Err("plugin response was empty".to_string());
    }
    serde_json::from_str(&response_line).map_err(|e| {
        format!(
            "parse response: {e}; raw={}",
            response_line.trim_end_matches('\n')
        )
    })
}

/// Load a `.proof` envelope and extract its sugar-binding entries into the
/// shape the recognize RPC expects.
fn load_binding_templates_from_proof(path: &Path) -> Result<Vec<Value>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read .proof: {e}"))?;
    // The .proof envelope is CBOR-encoded (proof-envelope canonical wire
    // form per the trinity envelope spec). Decode + convert to JSON for the
    // member-walking helpers below.
    let env = if bytes.first().map(|b| *b as char) == Some('{') {
        // Some test fixtures and legacy tools emit JSON-encoded .proof; tolerate.
        serde_json::from_slice::<Value>(&bytes)
            .map_err(|e| format!("parse .proof (JSON fallback): {e}"))?
    } else {
        let cbor = cbor_decode(&bytes).map_err(|e| format!("decode .proof CBOR: {e}"))?;
        cbor_to_json(&cbor)
    };

    // Walk the envelope's member set. The proof-envelope canonical shape
    // wraps each member as `{body: <decl>, header: {...}, schemaVersion}`,
    // and `members` may be either a CID-keyed map (.proof on disk) or an
    // array (lift response). collect_member_records normalizes that and
    // returns the (cid, value) pair; unwrap_envelope then peels the body
    // wrapping if present.
    let mut out: Vec<Value> = Vec::new();
    let candidates: Vec<(String, Value)> = collect_member_records(&env);
    for (cid_key, raw) in &candidates {
        let record = unwrap_envelope(raw);
        if record.get("kind").and_then(|v| v.as_str()) != Some("library-sugar-binding-entry") {
            continue;
        }
        let body = match record.get("body_source") {
            Some(b) => b,
            None => continue,
        };
        let template = match body.get("ast_template") {
            Some(t) if !t.is_null() => t.clone(),
            _ => continue, // skip entries minted before ast_template existed
        };
        let template_cid = body
            .get("template_cid")
            .cloned()
            .unwrap_or(Value::Null);
        let param_names = body
            .get("param_names")
            .cloned()
            .unwrap_or(Value::Null);
        // contract_cid resolution: prefer the binding's explicit
        // contract_cid (vendor minted a separate contract memento and
        // linked it); fall back to signature_shape_cid (the binding's
        // own signature-shape anchor); finally fall back to the member's
        // CID-key (the sugar entry's content address — the bridge says
        // "this user function alpha-matches this sugar binding"). All
        // three are content-addressed; the discharger composes against
        // whichever the bridge cites.
        let contract_cid = record
            .get("contract_cid")
            .filter(|v| !v.is_null())
            .cloned()
            .or_else(|| {
                record
                    .get("signature_shape_cid")
                    .filter(|v| !v.is_null())
                    .cloned()
            })
            .unwrap_or_else(|| {
                if cid_key.is_empty() {
                    Value::Null
                } else {
                    Value::String(cid_key.clone())
                }
            });
        let entry = json!({
            "concept_name": record.get("concept_name").cloned().unwrap_or(Value::Null),
            "library_tag": record.get("target_library_tag").cloned().unwrap_or(Value::Null),
            "family": record.get("family").cloned().unwrap_or(Value::Null),
            "ast_template": template,
            "template_cid": template_cid,
            "param_names": param_names,
            "contract_cid": contract_cid,
        });
        out.push(entry);
    }
    Ok(out)
}

/// Peel the proof-envelope wrapping (`{body, header, schemaVersion}`) and
/// return a reference to the inner decl. If the value doesn't look like an
/// envelope, return it as-is. The recognize-side never reads header fields
/// — those are signature/integrity concerns owned by the verifier.
fn unwrap_envelope(v: &Value) -> &Value {
    if let Some(body) = v.get("body") {
        if v.get("schemaVersion").is_some() || v.get("header").is_some() {
            return body;
        }
    }
    v
}

/// Convert a CBOR value into serde_json::Value. The trinity envelope's
/// catalog + members are pure data shapes (no floats, no negative ints —
/// those are rejected at decode time per the deterministic encoding
/// rules), so the mapping is clean: Uint→Number, Tstr→String, Array→Array,
/// Map→Object. Bstr is base64-encoded into a String so members carrying
/// embedded byte payloads survive the round trip (rare in sugar binding
/// entries, which are text-shaped).
fn cbor_to_json(v: &CborValue) -> Value {
    match v {
        CborValue::Uint(n) => json!(*n),
        CborValue::Tstr(s) => Value::String(s.clone()),
        CborValue::Bstr(b) => {
            // Sugar-binding entries don't typically carry bstr, but cover
            // the case: hex-encode so the JSON consumer can read it. This
            // is asymmetric with the canonical decoder (which keeps bytes)
            // but the recognize side only reads tstr/array/map shapes anyway.
            let hex: String = b
                .iter()
                .map(|byte| format!("{:02x}", byte))
                .collect();
            Value::String(hex)
        }
        CborValue::Array(items) => {
            Value::Array(items.iter().map(cbor_to_json).collect())
        }
        CborValue::Map(m) => Value::Object(
            m.iter()
                .map(|(k, v)| (k.clone(), cbor_to_json(v)))
                .collect(),
        ),
    }
}

/// Collect every JSON object that could be a member record. Walks the
/// envelope at the canonical roots: `members` (proof-envelope shape) and
/// `ir` (lift response shape, always an array). Best-effort; harmless when
/// a key is absent.
///
/// Crucial detail about the .proof catalog layout: each member's value
/// is the CBOR-canonical bytes of the member envelope, stored as a Bstr.
/// We re-decode those bytes here so callers see the structured envelope
/// rather than an opaque hex blob. (The hex form is what cbor_to_json
/// would emit for a Bstr; we want the structured form instead.)
fn collect_member_records(env: &Value) -> Vec<(String, Value)> {
    let mut out: Vec<(String, Value)> = Vec::new();
    if let Some(members) = env.get("members") {
        match members {
            Value::Array(arr) => {
                for v in arr {
                    out.push((String::new(), decode_embedded_member_if_hex(v)));
                }
            }
            Value::Object(map) => {
                for (cid, v) in map {
                    out.push((cid.clone(), decode_embedded_member_if_hex(v)));
                }
            }
            _ => {}
        }
    }
    if let Some(arr) = env.get("ir").and_then(|v| v.as_array()) {
        for v in arr {
            out.push((String::new(), v.clone()));
        }
    }
    out
}

/// If `v` is a hex-string (a Bstr round-tripped through cbor_to_json),
/// decode the hex back to bytes and parse them as JCS-canonical JSON to
/// recover the structured envelope. The proof-envelope catalog stores
/// each member as opaque JCS-JSON bytes keyed by CID (per cmd_mint's
/// `mint_library_sugar_binding_entry` -> `encode_jcs(envelope)` flow);
/// this peels that wrapping for the recognize-side reader.
fn decode_embedded_member_if_hex(v: &Value) -> Value {
    let s = match v.as_str() {
        Some(s) => s,
        None => return v.clone(),
    };
    // Hex if every char is ASCII hex AND length is even.
    if s.len() % 2 != 0 || !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return v.clone();
    }
    let bytes: Result<Vec<u8>, _> = (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
        .collect();
    let bytes = match bytes {
        Ok(b) => b,
        Err(_) => return v.clone(),
    };
    // The embedded bytes are JCS-canonical JSON. Parse them as such.
    match serde_json::from_slice::<Value>(&bytes) {
        Ok(j) => j,
        Err(_) => v.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_manifest_extracts_command_and_working_dir() {
        let tmp = std::env::temp_dir().join(format!(
            "provekit-recognize-test-manifest-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&tmp).unwrap();
        let manifest_path = tmp.join("manifest.toml");
        std::fs::write(
            &manifest_path,
            r#"name = "rust-bind-lift"
command = ["../../implementations/rust/target/debug/provekit-walk-rpc", "--rpc"]
working_dir = "."
"#,
        )
        .unwrap();
        let m = parse_manifest(&manifest_path).expect("parse");
        assert_eq!(m.command.len(), 2);
        assert!(m.command[0]
            .to_string_lossy()
            .ends_with("provekit-walk-rpc"));
        assert_eq!(m.command[1].to_string_lossy(), "--rpc");
        assert_eq!(m.working_dir, Some(PathBuf::from(".")));
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn load_binding_extracts_sugar_entries_from_envelope() {
        let env = json!({
            "members": [
                {
                    "kind": "library-sugar-binding-entry",
                    "concept_name": "concept:json-parse",
                    "target_library_tag": "provekit-shim-serde-json-rust",
                    "family": "concept:family:json",
                    "body_source": {
                        "ast_template": {"kind":"block","stmts":[]},
                        "template_cid": "blake3-512:abc",
                        "param_names": ["s"],
                    },
                    "contract_cid": "blake3-512:def"
                },
                { "kind": "something-else" }
            ]
        });
        let tmp = std::env::temp_dir().join(format!(
            "provekit-recognize-test-binding-{}",
            std::process::id()
        ));
        std::fs::write(&tmp, env.to_string()).unwrap();
        let entries = load_binding_templates_from_proof(&tmp).expect("load");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["concept_name"], "concept:json-parse");
        assert_eq!(
            entries[0]["library_tag"],
            "provekit-shim-serde-json-rust"
        );
        assert_eq!(entries[0]["template_cid"], "blake3-512:abc");
        // explicit contract_cid honored when present
        assert_eq!(entries[0]["contract_cid"], "blake3-512:def");
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn load_binding_falls_back_to_signature_shape_cid_when_no_contract_cid() {
        // Sugar entries from real shims don't carry an explicit
        // contract_cid (the contract memento is minted separately);
        // they DO carry signature_shape_cid as their identity anchor.
        // The loader must surface that as the bridge's contract_cid
        // fallback so recognize can emit bridges.
        let env = json!({
            "members": [{
                "kind": "library-sugar-binding-entry",
                "concept_name": "concept:json-parse",
                "target_library_tag": "shim",
                "signature_shape_cid": "blake3-512:sig",
                "body_source": {
                    "ast_template": {"kind": "block"},
                    "template_cid": "blake3-512:tpl",
                    "param_names": ["s"]
                }
            }]
        });
        let tmp = std::env::temp_dir().join(format!(
            "provekit-recognize-test-sigfallback-{}",
            std::process::id()
        ));
        std::fs::write(&tmp, env.to_string()).unwrap();
        let entries = load_binding_templates_from_proof(&tmp).expect("load");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["contract_cid"], "blake3-512:sig");
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn load_binding_skips_entries_without_ast_template() {
        // Backward-compat: sugar entries minted before ast_template existed
        // should be skipped, not error out.
        let env = json!({
            "members": [
                {
                    "kind": "library-sugar-binding-entry",
                    "concept_name": "concept:json-parse",
                    "target_library_tag": "old-shim",
                    "body_source": {
                        "body_text": "old body without ast_template",
                    }
                }
            ]
        });
        let tmp = std::env::temp_dir().join(format!(
            "provekit-recognize-test-legacy-{}",
            std::process::id()
        ));
        std::fs::write(&tmp, env.to_string()).unwrap();
        let entries = load_binding_templates_from_proof(&tmp).expect("load");
        assert!(entries.is_empty(), "legacy entries without ast_template skipped");
        std::fs::remove_file(&tmp).ok();
    }
}
