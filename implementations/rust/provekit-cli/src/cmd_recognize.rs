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
use libprovekit::core::emit_obligation::{
    build_bridge_body, build_implication_contract_body, member_envelope_canonical,
};
use owo_colors::OwoColorize;
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

    let surface = args
        .surface
        .clone()
        .unwrap_or_else(|| "rust-bind".to_string());
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
        println!(
            "{}",
            serde_json::to_string_pretty(&out).unwrap_or_else(|_| out.to_string())
        );
    } else {
        println!("recognize: {} tag(s) emitted", tags.len());
        for (idx, tag) in tags.iter().enumerate() {
            let concept = tag
                .get("concept_name")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let file = tag.get("file").and_then(|v| v.as_str()).unwrap_or("?");
            let span = tag.get("span").cloned().unwrap_or(Value::Null);
            let start_line = span.get("start_line").and_then(|v| v.as_u64()).unwrap_or(0);
            let tier = tag
                .get("match_tier")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
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
            // One bridge + one implication contract per tag with a
            // function_name (the bridge falls back to its sibling
            // contract when no shim contract matches by ctor name).
            let bridge_count = tags
                .iter()
                .filter(|t| {
                    t.get("function_name")
                        .and_then(|v| v.as_str())
                        .is_some_and(|s| !s.is_empty())
                })
                .count();
            println!(
                "{}: minted {} bridge(s) + {} implication contract(s) into {}",
                "write".green().bold(),
                bridge_count,
                bridge_count,
                path.display()
            );
        }
    }

    EXIT_OK
}

// recognize_bridge_body was removed (#1579). The single-target form is
// no longer needed since the emit_bridge_envelope path always uses
// recognize_bridge_body_with_target with either a shim-matched
// contract CID or a sibling implication-contract CID. Both go through
// the shared libprovekit::core::emit_obligation::build_bridge_body.

/// Build the implication contract memento for a recognize tag.
/// Delegates to libprovekit's shared `build_implication_contract_body`
/// (the same function the materialize and rust-tests lift lanes will
/// eventually call). Per #1579: one canonical authoring path for the
/// substrate's implication-contract shape across all verbs.
fn recognize_implication_body(tag: &Value) -> Option<Value> {
    let function_name = tag.get("function_name").and_then(|v| v.as_str())?;
    if function_name.is_empty() {
        return None;
    }
    let concept_name = tag.get("concept_name").and_then(|v| v.as_str());
    // Collect param source-text strings into Vec<String> so we can hand
    // out &str references to the shared builder.
    let param_texts: Vec<String> = tag
        .get("param_bindings")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|b| {
                    b.get("source_text")
                        .and_then(|v| v.as_str())
                        .unwrap_or("_")
                        .to_string()
                })
                .collect()
        })
        .unwrap_or_default();
    let param_refs: Vec<&str> = param_texts.iter().map(|s| s.as_str()).collect();
    Some(build_implication_contract_body(
        "recognize",
        function_name,
        concept_name,
        &param_refs,
    ))
}

/// Mint a `.proof` envelope containing one bridge memento + one
/// implication contract memento per recognize tag. Written under
/// `<project>/.provekit/recognize/<cid>.proof`.
///
/// Two members per tag (bridge + contract):
///   - The contract memento is the ENUMERATE half: its post atomic
///     contains a ctor term named after the user's function.
///     enumerate_callsites walks contract formulas, finds the ctor,
///     looks up the bridge by name, emits a CallSite.
///   - The bridge memento is the RESOLVE half: sourceSymbol →
///     targetContractCid. The discharger resolves through the bridge
///     to a contract memento and composes pre/post.
///
/// Bridge target resolution order:
///   1. The tag's contract_cid if it cites a contract that exists in
///      the loaded shim .proof's (via the ctor-name index in the
///      binding loader). This is the production linkage when shims
///      mint contracts covering their sugar functions.
///   2. Fallback to the recognize-emitted SIBLING contract (the one
///      this same call just minted). Self-resolution keeps the loop
///      closed even when the shim mints no contract for that function —
///      the verdict is `discharged: vacuous` against the trivial
///      identity post the recognize lane emits.
///
/// The fallback isn't a hack; it's the lift-equivalent of "if there's
/// no upstream contract, the test author's own assertion is the
/// contract." Recognize's claim is "I see this user function
/// alpha-matches the sugar shape" — that IS a claim, content-addressed,
/// signed, and admissible by the substrate.
fn emit_bridge_envelope(
    project_root: &Path,
    tags: &[Value],
    target_language: &str,
) -> Result<Option<std::path::PathBuf>, String> {
    let proof_dir = project_root.join(".provekit").join("recognize");
    let mut members: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    // First pass: mint each tag's implication contract so we know its
    // CID. Build a map from function_name -> recognize-contract CID for
    // bridge fallback resolution.
    let mut sibling_contract_by_function: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for tag in tags {
        if let Some(body) = recognize_implication_body(tag) {
            let (cid, bytes) = member_envelope_canonical("contract", &body)?;
            members.entry(cid.clone()).or_insert(bytes);
            if let Some(fn_name) = tag.get("function_name").and_then(|v| v.as_str()) {
                sibling_contract_by_function.insert(fn_name.to_string(), cid);
            }
        }
    }
    // Second pass: mint the bridge memento for each tag with
    // targetContractCid resolved against the production binding's
    // contract_cid first (shim-minted contracts), falling back to the
    // sibling recognize-emitted contract from pass 1.
    for tag in tags {
        let function_name = match tag.get("function_name").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => s,
            _ => continue,
        };
        // Resolve target: shim's contract_cid (set by the binding loader
        // via ctor-name index) if it differs from the sugar entry's own
        // signature_shape_cid fallback; otherwise the sibling contract.
        let target_cid = tag
            .get("contract_cid")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .or_else(|| sibling_contract_by_function.get(function_name).cloned());
        let Some(target_cid) = target_cid else {
            continue;
        };
        let body = recognize_bridge_body_with_target(tag, target_language, &target_cid);
        let (cid, bytes) = member_envelope_canonical("bridge", &body)?;
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
    std::fs::write(&path, &proof.bytes).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(Some(path))
}

/// Variant of recognize_bridge_body that takes an explicit
/// targetContractCid. Delegates to libprovekit's shared
/// `build_bridge_body` so cmd_materialize and the rust-tests lifter
/// produce byte-identical bridge bodies for the same inputs (#1579).
fn recognize_bridge_body_with_target(
    tag: &Value,
    target_language: &str,
    target_cid: &str,
) -> Value {
    let function_name = tag
        .get("function_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let library_tag = tag
        .get("library_tag")
        .and_then(|v| v.as_str())
        .unwrap_or(target_language);
    build_bridge_body(
        "recognize",
        function_name,
        target_language,
        library_tag,
        target_cid,
    )
}

// flat_member_canonical + canonical_value_of_json were moved to
// libprovekit::core::emit_obligation as member_envelope_canonical +
// canonical_value_of_json (#1579). cmd_recognize now imports them.

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
    Ok(PluginManifest {
        command,
        working_dir,
    })
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

    // First pass: walk all members, build an index from ctor function
    // names to the CIDs of contract mementos whose formulas reference
    // that ctor. The linkage between a sugar binding and its contract
    // memento is BY CTOR NAME (the shim mints contract witnesses whose
    // inv/pre/post formulas contain `ctor(name="<source_function_name>")`
    // terms, e.g. `unwrap(json_parse(s)) = original`); the bridge then
    // resolves to that contract memento and the discharger composes.
    let candidates: Vec<(String, Value)> = collect_member_records(&env);
    let mut contract_by_function: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for (cid_key, raw) in &candidates {
        let record = unwrap_envelope(raw);
        // Contract mementos can be wrapped in either an "evidence" shape
        // (body / kind both nested) or have `kind` at the top of `header`
        // (the layered v1.2 envelope shape). Try both.
        let is_contract = record
            .get("kind")
            .and_then(|v| v.as_str())
            .map(|k| k == "contract")
            .unwrap_or_else(|| {
                record.pointer("/header/kind").and_then(|v| v.as_str()) == Some("contract")
            });
        if !is_contract {
            continue;
        }
        if cid_key.is_empty() {
            continue;
        }
        // Walk every formula slot looking for ctor names. The shim's
        // contracts live under `header` in the layered shape.
        let formula_root = record.get("header").unwrap_or(record);
        for slot in ["pre", "post", "inv"] {
            if let Some(f) = formula_root.get(slot) {
                collect_ctor_names(f, &mut |name| {
                    contract_by_function
                        .entry(name.to_string())
                        .or_insert_with(|| cid_key.clone());
                });
            }
        }
    }

    // Second pass: emit sugar binding templates with contract_cid
    // resolved via the function-name index.
    let mut out: Vec<Value> = Vec::new();
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
            _ => continue,
        };
        let template_cid = body.get("template_cid").cloned().unwrap_or(Value::Null);
        let param_names = body.get("param_names").cloned().unwrap_or(Value::Null);
        let function_name = record
            .get("source_function_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        // Resolve contract_cid: only an actual contract memento counts.
        //   1. explicit contract_cid field on the sugar entry (rare)
        //   2. matching contract memento via ctor-name index (the
        //      production linkage the shim mint actually uses).
        // If neither is found, contract_cid stays NULL — the bridge
        // emission then falls back to recognize's own sibling contract
        // (the implication memento it mints alongside each bridge).
        // This is the substrate-honest path: a bridge resolves to a
        // contract memento and nothing else; signature_shape_cid and
        // sugar-entry-CID are neither.
        let contract_cid = record
            .get("contract_cid")
            .filter(|v| !v.is_null())
            .cloned()
            .or_else(|| {
                contract_by_function
                    .get(&function_name)
                    .map(|c| Value::String(c.clone()))
            })
            .unwrap_or(Value::Null);
        let _ = cid_key; // intentionally unused: sugar entry CID is not a contract
        let entry = json!({
            "concept_name": record.get("concept_name").cloned().unwrap_or(Value::Null),
            "library_tag": record.get("target_library_tag").cloned().unwrap_or(Value::Null),
            "family": record.get("family").cloned().unwrap_or(Value::Null),
            "ast_template": template,
            "template_cid": template_cid,
            "param_names": param_names,
            "contract_cid": contract_cid,
            "source_function_name": function_name,
        });
        out.push(entry);
    }
    Ok(out)
}

/// Recursively walk a formula/term tree invoking `on_name` for every
/// ctor term encountered. The discharger looks for ctor terms whose
/// name matches a bridge sourceSymbol to enumerate callsites; here we
/// use the same walk to find which contract memento "is about" a given
/// function (because its formulas reference that function's ctor).
fn collect_ctor_names<F: FnMut(&str)>(node: &Value, on_name: &mut F) {
    if !node.is_object() {
        return;
    }
    if node.get("kind").and_then(|v| v.as_str()) == Some("ctor") {
        if let Some(name) = node.get("name").and_then(|v| v.as_str()) {
            on_name(name);
        }
    }
    if let Some(args) = node.get("args").and_then(|v| v.as_array()) {
        for a in args {
            collect_ctor_names(a, on_name);
        }
    }
    if let Some(ops) = node.get("operands").and_then(|v| v.as_array()) {
        for o in ops {
            collect_ctor_names(o, on_name);
        }
    }
    if let Some(body) = node.get("body") {
        collect_ctor_names(body, on_name);
    }
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
            let hex: String = b.iter().map(|byte| format!("{:02x}", byte)).collect();
            Value::String(hex)
        }
        CborValue::Array(items) => Value::Array(items.iter().map(cbor_to_json).collect()),
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
        assert_eq!(entries[0]["library_tag"], "provekit-shim-serde-json-rust");
        assert_eq!(entries[0]["template_cid"], "blake3-512:abc");
        // explicit contract_cid honored when present
        assert_eq!(entries[0]["contract_cid"], "blake3-512:def");
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn load_binding_leaves_contract_cid_null_when_no_ctor_match() {
        // Sugar entries from real shims don't carry an explicit
        // contract_cid. The loader resolves contract_cid via the
        // ctor-name index across the envelope's contract mementos. If
        // no contract memento references the sugar's source_function_name
        // via a ctor term, contract_cid stays NULL — the
        // emit_bridge_envelope path then falls back to the sibling
        // implication contract (substrate-honest: only an actual
        // contract memento counts; signature_shape_cid is not a contract).
        let env = json!({
            "members": [{
                "kind": "library-sugar-binding-entry",
                "concept_name": "concept:json-parse",
                "target_library_tag": "shim",
                "source_function_name": "json_parse",
                "signature_shape_cid": "blake3-512:sig",
                "body_source": {
                    "ast_template": {"kind": "block"},
                    "template_cid": "blake3-512:tpl",
                    "param_names": ["s"]
                }
            }]
        });
        let tmp = std::env::temp_dir().join(format!(
            "provekit-recognize-test-nullfallback-{}",
            std::process::id()
        ));
        std::fs::write(&tmp, env.to_string()).unwrap();
        let entries = load_binding_templates_from_proof(&tmp).expect("load");
        assert_eq!(entries.len(), 1);
        // contract_cid stays null — no contract memento matched.
        // signature_shape_cid is NOT a contract memento and must not
        // bleed into the bridge target field.
        assert!(
            entries[0]["contract_cid"].is_null(),
            "contract_cid must be null when no ctor-name match: {:?}",
            entries[0]["contract_cid"]
        );
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
        assert!(
            entries.is_empty(),
            "legacy entries without ast_template skipped"
        );
        std::fs::remove_file(&tmp).ok();
    }
}
