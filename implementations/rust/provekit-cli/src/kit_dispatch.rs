// SPDX-License-Identifier: Apache-2.0
//
// Kit-agnostic dispatcher for the eight-verb bind pipeline and the realize
// surface. cmd_bind and cmd_transport call into here to invoke per-language
// lift and realize plugins via PEP 1.7.0 (`2026-05-12-plugin-protocol.md`);
// neither command has any language-specific code, no `if source_lang ==
// "rust"` and no `TargetStyle::*` arms.
//
// Two surfaces:
//
//   1. `dispatch_bind_lift(workspace_root, source_lang)`
//      Resolves a `kind = "lift"` plugin for `source_lang` via convention
//      (`.provekit/lift/<lang>/manifest.toml`, then a workspace built-in
//      under `implementations/<lang>/`, then PATH). Invokes the
//      legacy-retained `initialize` / `lift` / `shutdown` JSON-RPC shape
//      and decodes `ir-document.ir[]` into `BindLiftEntry` records per
//      `2026-05-13-bind-ir-lift-result.md`.
//
//   2. `dispatch_realize(target_lang, request)`
//      Resolves a `kind = "realize"` (sugar/body-template) plugin for
//      `target_lang` via convention (`.provekit/realize/<lang>/manifest.toml`
//      or a built-in path; the Java built-in path is
//      `implementations/java/provekit-realize-java-core/target/...`).
//      Invokes the PEP 1.7.0 `provekit.plugin.invoke` method and returns
//      `{ source, is_stub }`.
//
// Kit unavailability is a `kit-plugin-unavailable` gap, not a hidden error.
// Per Supra omnia, rectum the dispatcher refuses loudly with a gap record
// the caller turns into a `GapRecord` and propagates downstream.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

// ============================================================================
// Bind lift dispatch (PEP 1.7.0 kind = "lift", legacy-retained method `lift`)
// ============================================================================

/// One bind-IR lift entry produced by a lift plugin per
/// `2026-05-13-bind-ir-lift-result.md` §1.1. The `term_shape` field is opaque
/// here; cmd_bind clusters on `term_shape_cid` and consults the catalog
/// downstream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindLiftEntry {
    #[serde(default)]
    pub kind: String,
    pub file: String,
    pub fn_name: String,
    #[serde(default)]
    pub fn_line: u64,
    #[serde(default)]
    pub attr_pre: Option<String>,
    #[serde(default)]
    pub attr_post: Option<String>,
    #[serde(default)]
    pub concept_annotation: Option<String>,
    #[serde(default)]
    pub param_names: Vec<String>,
    #[serde(default)]
    pub param_types: Vec<String>,
    #[serde(default)]
    pub return_type: String,
    #[serde(default)]
    pub term_shape: Value,
    #[serde(default)]
    pub term_shape_cid: String,
    #[serde(default)]
    pub witnesses: Vec<BindContractWitness>,
}

/// One contract witness carried by a bind lift entry.
///
/// `source_kind` intentionally reuses the existing `EvidenceMemento`
/// vocabulary (`annotation`, `test-assertion`, `type-signature`, `docstring`,
/// `native-surface`, ...). cmd_bind promotes these entries directly into
/// `EvidenceMemento`s instead of maintaining a parallel bind-only taxonomy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindContractWitness {
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub predicate: Option<Value>,
    #[serde(default)]
    pub predicate_text: Option<String>,
    #[serde(default)]
    pub source_kind: String,
    #[serde(default)]
    pub confidence_basis_points: Option<u16>,
    #[serde(default)]
    pub line: Option<u64>,
    #[serde(default)]
    pub col: Option<u64>,
    #[serde(default)]
    pub extension_fields: BTreeMap<String, Value>,
}

/// Result of `dispatch_bind_lift`. Carries the lift entries plus any
/// diagnostics the kit emitted (`ir-document.diagnostics[]`).
#[derive(Debug, Clone)]
pub struct BindLiftResult {
    pub entries: Vec<BindLiftEntry>,
    pub diagnostics: Vec<Value>,
}

/// Refusal raised when a lift kit cannot be reached. The caller MUST emit a
/// `kit-plugin-unavailable` gap record and proceed (loudly-bounded-lossy)
/// per `body-template-memento.md` §5 and `2026-05-13-bind-ir-lift-result.md`
/// §5.
#[derive(Debug)]
pub struct KitUnavailable {
    pub kit_kind: &'static str,
    pub language: String,
    pub detail: String,
}

impl std::fmt::Display for KitUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "kit-plugin-unavailable: no {} plugin for language `{}` ({})",
            self.kit_kind, self.language, self.detail
        )
    }
}

/// Dispatch the bind-lift surface for `source_lang` and decode the response.
///
/// Resolution order:
///   1. `<workspace_root>/.provekit/lift/<source_lang>-bind/manifest.toml`
///   2. `<workspace_root>/.provekit/lift/<source_lang>/manifest.toml`
///   3. Built-in for the language (workspace-relative compile-time path,
///      env-var-overridable per `PROVEKIT_BIND_LIFT_<LANG>_BIN`).
///   4. `provekit-bind-lift-<source_lang>` on PATH.
///
/// Returns `Err(KitUnavailable)` when none of the above resolve.
pub fn dispatch_bind_lift(
    workspace_root: &Path,
    source_lang: &str,
) -> Result<BindLiftResult, KitUnavailable> {
    let command = resolve_lift_command(workspace_root, source_lang)?;
    let response = rpc_lift(workspace_root, source_lang, &command).map_err(|e| KitUnavailable {
        kit_kind: "lift",
        language: source_lang.to_string(),
        detail: e,
    })?;
    decode_bind_lift_response(response).map_err(|e| KitUnavailable {
        kit_kind: "lift",
        language: source_lang.to_string(),
        detail: e,
    })
}

fn resolve_lift_command(
    workspace_root: &Path,
    source_lang: &str,
) -> Result<ResolvedCommand, KitUnavailable> {
    // 1 + 2: project-local manifest under .provekit/lift/<surface>/manifest.toml.
    for surface in [&format!("{source_lang}-bind"), source_lang] {
        let manifest = workspace_root
            .join(".provekit")
            .join("lift")
            .join(surface)
            .join("manifest.toml");
        if manifest.exists() {
            if let Ok(parsed) = parse_manifest(&manifest) {
                let working_dir = parsed
                    .working_dir
                    .map(|wd| {
                        if wd.is_absolute() {
                            wd
                        } else {
                            workspace_root.join(wd)
                        }
                    })
                    .or_else(|| Some(workspace_root.to_path_buf()));
                return Ok(ResolvedCommand {
                    argv: parsed.command,
                    working_dir,
                });
            }
        }
    }

    // 3: env-var override.
    let env_var = format!("PROVEKIT_BIND_LIFT_{}_BIN", source_lang.to_uppercase());
    if let Ok(bin) = std::env::var(&env_var) {
        return Ok(ResolvedCommand {
            argv: vec![bin, "--rpc".to_string()],
            working_dir: Some(workspace_root.to_path_buf()),
        });
    }

    // 4: built-in convention for known kits. These resolve relative to the
    // workspace root and are not language knowledge in cmd_bind — they're
    // the byte-stable substrate convention "per-language kit lives under
    // implementations/<lang>/". The dispatcher consults the FILESYSTEM, not
    // a hard-coded language list.
    for candidate in builtin_lift_candidates(workspace_root, source_lang) {
        if candidate.exists() {
            return Ok(ResolvedCommand {
                argv: vec![candidate.display().to_string(), "--rpc".to_string()],
                working_dir: Some(workspace_root.to_path_buf()),
            });
        }
    }

    // 5: PATH probe.
    let bin = format!("provekit-bind-lift-{source_lang}");
    if which_on_path(&bin).is_some() {
        return Ok(ResolvedCommand {
            argv: vec![bin, "--rpc".to_string()],
            working_dir: Some(workspace_root.to_path_buf()),
        });
    }

    Err(KitUnavailable {
        kit_kind: "lift",
        language: source_lang.to_string(),
        detail: format!(
            "no manifest at .provekit/lift/{source_lang}-bind/ or .provekit/lift/{source_lang}/, \
             no env {env_var}, no built-in binary under implementations/{source_lang}/, \
             no `provekit-bind-lift-{source_lang}` on PATH"
        ),
    })
}

/// Substrate-convention built-in binaries per language. Each row is a
/// workspace-relative path; the dispatcher tries them in order and picks
/// the first that exists on disk. This list MUST stay tiny: it is NOT
/// language knowledge in the CLI core. It is the wiring that lets the
/// substrate's standard kit layout (`implementations/<lang>/...`) be
/// discovered without the operator hand-rolling a manifest. Any kit not
/// listed here is still resolvable via a manifest or via PATH; this is
/// best-effort discovery, not policy.
fn builtin_lift_candidates(workspace_root: &Path, source_lang: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let impl_dir = workspace_root.join("implementations").join(source_lang);
    // Rust convention: the provekit-walk-rpc binary speaks `initialize`/`lift`
    // returning bind-IR per `2026-05-13-bind-ir-lift-result.md`.
    out.push(impl_dir.join("target/release/provekit-walk-rpc"));
    out.push(impl_dir.join("target/debug/provekit-walk-rpc"));
    // Per-language conventional name as a fallback (each kit's Makefile
    // installs the binary under target/{release,debug}/...).
    out.push(
        impl_dir
            .join("target/release")
            .join(format!("provekit-bind-lift-{source_lang}")),
    );
    out.push(
        impl_dir
            .join("target/debug")
            .join(format!("provekit-bind-lift-{source_lang}")),
    );
    // Sibling-of-current-executable convention: when `provekit` is launched
    // from a cargo target dir, `provekit-walk-rpc` and other kit binaries
    // live next to it. This lets `cargo test` and `cargo run` resolve the
    // built-in Rust kit without an env-var override or a manifest at the
    // workspace_root (which is often a tempdir under tests).
    if source_lang == "rust" {
        if let Ok(current) = std::env::current_exe() {
            if let Some(parent) = current.parent() {
                out.push(parent.join("provekit-walk-rpc"));
                out.push(parent.join(format!("provekit-bind-lift-{source_lang}")));
            }
        }
    } else if let Ok(current) = std::env::current_exe() {
        if let Some(parent) = current.parent() {
            out.push(parent.join(format!("provekit-bind-lift-{source_lang}")));
        }
    }
    out
}

fn which_on_path(bin: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).find_map(|dir| {
            let candidate = dir.join(bin);
            if candidate.is_file() {
                Some(candidate)
            } else {
                None
            }
        })
    })
}

#[derive(Debug, Clone)]
struct ResolvedCommand {
    argv: Vec<String>,
    working_dir: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct ParsedManifest {
    #[allow(dead_code)]
    name: String,
    command: Vec<String>,
    working_dir: Option<PathBuf>,
}

fn parse_manifest(path: &Path) -> Result<ParsedManifest, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let mut name = String::new();
    let mut command: Vec<String> = Vec::new();
    let mut working_dir: Option<PathBuf> = None;
    for line in text.lines() {
        let line = match line.find('#') {
            Some(pos) => &line[..pos],
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
            "name" => name = val.trim_matches('"').to_string(),
            "working_dir" => working_dir = Some(PathBuf::from(val.trim_matches('"'))),
            "command" => {
                let inner = val.trim_matches(|c| c == '[' || c == ']');
                command = inner
                    .split(',')
                    .map(|s| s.trim().trim_matches('"').to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            _ => {}
        }
    }
    if command.is_empty() {
        return Err(format!("manifest {} has no `command`", path.display()));
    }
    Ok(ParsedManifest {
        name,
        command,
        working_dir,
    })
}

fn rpc_lift(
    workspace_root: &Path,
    source_lang: &str,
    cmd_spec: &ResolvedCommand,
) -> Result<Value, String> {
    if cmd_spec.argv.is_empty() {
        return Err("empty command".to_string());
    }
    let mut command = Command::new(&cmd_spec.argv[0]);
    if cmd_spec.argv.len() > 1 {
        command.args(&cmd_spec.argv[1..]);
    }
    if !cmd_spec.argv.iter().any(|a| a == "--rpc") {
        command.arg("--rpc");
    }
    if let Some(wd) = &cmd_spec.working_dir {
        command.current_dir(wd);
    }
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::null());

    let mut child = command
        .spawn()
        .map_err(|e| format!("spawn lift kit: {e}"))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or("lift kit stdin unavailable".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or("lift kit stdout unavailable".to_string())?;
    let mut reader = BufReader::new(stdout);

    // initialize
    let init_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "client": {"name": "provekit-cli/bind", "version": env!("CARGO_PKG_VERSION")},
            "protocol_version": "pep/1.7.0",
            "workspace_root": workspace_root.display().to_string(),
            "config_path": ".provekit/config.toml"
        }
    });
    writeln!(stdin, "{init_req}").map_err(|e| format!("write initialize: {e}"))?;
    let _ = read_response(&mut reader, 1)?;

    // lift
    let lift_req = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "lift",
        "params": {
            "surface": source_lang,
            "workspace_root": workspace_root.display().to_string(),
            "source_paths": ["."],
            "options": { "layer": "all" }
        }
    });
    writeln!(stdin, "{lift_req}").map_err(|e| format!("write lift: {e}"))?;
    let response = read_response(&mut reader, 2)?;

    // shutdown (best-effort)
    let shutdown_req = json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "shutdown"
    });
    let _ = writeln!(stdin, "{shutdown_req}");
    drop(stdin);
    let _ = child.wait();
    Ok(response)
}

fn read_response(reader: &mut impl BufRead, id: i64) -> Result<Value, String> {
    let mut line = String::new();
    let n = reader
        .read_line(&mut line)
        .map_err(|e| format!("read response: {e}"))?;
    if n == 0 {
        return Err("lift kit closed stdout before responding".to_string());
    }
    let value: Value = serde_json::from_str(line.trim())
        .map_err(|e| format!("parse JSON-RPC response: {e}; raw={}", line.trim()))?;
    if value.get("id").and_then(Value::as_i64) != Some(id) {
        return Err(format!(
            "response id mismatch: expected {id}, got {value:?}"
        ));
    }
    if let Some(err) = value.get("error") {
        return Err(format!("kit returned error: {err}"));
    }
    value
        .get("result")
        .cloned()
        .ok_or_else(|| "response missing `result`".to_string())
}

fn decode_bind_lift_response(response: Value) -> Result<BindLiftResult, String> {
    let kind = response.get("kind").and_then(Value::as_str).unwrap_or("");
    if kind != "ir-document" {
        return Err(format!(
            "expected `kind = \"ir-document\"`, got `{kind}` (lift kit returned the wrong shape; \
             bind expects bind-IR per 2026-05-13-bind-ir-lift-result.md)"
        ));
    }
    let ir = response
        .get("ir")
        .and_then(Value::as_array)
        .ok_or_else(|| "ir-document missing `ir` array".to_string())?;
    let mut entries: Vec<BindLiftEntry> = Vec::new();
    for v in ir {
        let entry_kind = v.get("kind").and_then(Value::as_str).unwrap_or("");
        if entry_kind != "bind-lift-entry" {
            continue;
        }
        match serde_json::from_value::<BindLiftEntry>(v.clone()) {
            Ok(e) => entries.push(e),
            Err(err) => {
                eprintln!(
                    "bind-lift: skipping malformed entry: {err} raw={}",
                    serde_json::to_string(v).unwrap_or_default()
                );
            }
        }
    }
    let diagnostics: Vec<Value> = response
        .get("diagnostics")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(BindLiftResult {
        entries,
        diagnostics,
    })
}

// ============================================================================
// Realize dispatch (PEP 1.7.0 kind = "realize", method `provekit.plugin.invoke`)
// ============================================================================

/// Request shape for the realize surface. Mirrors what
/// `body-template-memento.md` §3 specifies: the realize plugin receives a
/// canonical clause + concept binding + signature info and returns a
/// language-specific source string plus an `is_stub` flag.
#[derive(Debug, Clone, Serialize)]
pub struct RealizeRequest {
    pub function: String,
    pub params: Vec<String>,
    pub param_types: Vec<String>,
    pub return_type: String,
    pub concept_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract: Option<RealizeContractPayload>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sugar_cids: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sugar_plugins: Vec<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RealizeContractPayload {
    pub concept_site_cid: String,
    pub local_contract_cid: String,
    pub origin: String,
    pub discharge_verdict: String,
    pub witnesses: Vec<RealizeContractWitness>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RealizeContractWitness {
    pub role: String,
    pub predicate: Value,
    pub predicate_text: String,
    pub source_kind: String,
}

#[derive(Debug, Clone)]
pub struct RealizedSource {
    pub extension: String,
    pub source: String,
    pub is_stub: bool,
    pub emitted_artifact_cid: Option<String>,
    pub observed_loss_record: Value,
    pub used_sugars: Vec<Value>,
    /// Raw `observation_wrapper_emission_record` from the kit response, present
    /// when mode ∈ {witness, monitor, dispatcher} and the kit emitted a wrapper
    /// FCM. Expected fields: wrapper_fcm_cid, observer_effects,
    /// preservation_claim_cid.
    pub observation_wrapper_emission_record: Option<Value>,
}

/// Dispatch a realize call for `target_lang`. Returns `Err(KitUnavailable)`
/// when no realize plugin exists. Callers turn this into a
/// `kit-plugin-unavailable` gap record so the run is loudly-bounded-lossy
/// at the realize boundary rather than silently empty.
pub fn dispatch_realize(
    workspace_root: &Path,
    target_lang: &str,
    request: &RealizeRequest,
) -> Result<RealizedSource, KitUnavailable> {
    let resolved = resolve_realize_command(workspace_root, target_lang)?;
    invoke_realize(target_lang, &resolved, request).map_err(|e| KitUnavailable {
        kit_kind: "realize",
        language: target_lang.to_string(),
        detail: e,
    })
}

fn resolve_realize_command(
    workspace_root: &Path,
    target_lang: &str,
) -> Result<ResolvedCommand, KitUnavailable> {
    // 1: manifest at .provekit/realize/<lang>/manifest.toml.
    let manifest = workspace_root
        .join(".provekit")
        .join("realize")
        .join(target_lang)
        .join("manifest.toml");
    if manifest.exists() {
        if let Ok(parsed) = parse_manifest(&manifest) {
            let working_dir = parsed
                .working_dir
                .map(|wd| {
                    if wd.is_absolute() {
                        wd
                    } else {
                        workspace_root.join(wd)
                    }
                })
                .or_else(|| Some(workspace_root.to_path_buf()));
            return Ok(ResolvedCommand {
                argv: parsed.command,
                working_dir,
            });
        }
    }

    // 2: env-var override.
    let env_var = format!("PROVEKIT_REALIZE_{}_BIN", target_lang.to_uppercase());
    if let Ok(bin) = std::env::var(&env_var) {
        return Ok(ResolvedCommand {
            argv: vec![bin, "--rpc".to_string()],
            working_dir: Some(workspace_root.to_path_buf()),
        });
    }

    // 3: substrate-convention built-in binaries. Same shape as lift:
    // the dispatcher consults the FILESYSTEM, not a hard-coded list.
    for candidate in builtin_realize_candidates(workspace_root, target_lang) {
        if candidate.path.exists() {
            return Ok(ResolvedCommand {
                argv: candidate.argv,
                working_dir: Some(workspace_root.to_path_buf()),
            });
        }
    }

    // 4: PATH probe.
    let bin = format!("provekit-realize-{target_lang}");
    if which_on_path(&bin).is_some() {
        return Ok(ResolvedCommand {
            argv: vec![bin, "--rpc".to_string()],
            working_dir: Some(workspace_root.to_path_buf()),
        });
    }

    Err(KitUnavailable {
        kit_kind: "realize",
        language: target_lang.to_string(),
        detail: format!(
            "no manifest at .provekit/realize/{target_lang}/, no env {env_var}, \
             no built-in binary under implementations/{target_lang}/, \
             no `provekit-realize-{target_lang}` on PATH"
        ),
    })
}

struct RealizeBuiltin {
    path: PathBuf,
    argv: Vec<String>,
}

fn builtin_realize_candidates(workspace_root: &Path, target_lang: &str) -> Vec<RealizeBuiltin> {
    let mut out = Vec::new();
    // Java: special-case in the SUBSTRATE CONVENTION (Maven build product), not
    // in the CLI semantics. The convention is "every Java realize kit ships a
    // jar at provekit-realize-java-core/target/provekit-realize-java.jar".
    // We register it here as a filesystem path, just like Rust's
    // target/{release,debug}/provekit-walk-rpc. Per "Rust isn't special"
    // (and "Java isn't special either"), this is filesystem discovery, not
    // a switch on `target_lang == "java"`.
    let impl_dir = workspace_root.join("implementations").join(target_lang);
    let realize_subdir = impl_dir.join(format!("provekit-realize-{target_lang}-core"));
    let jar = realize_subdir
        .join("target")
        .join(format!("provekit-realize-{target_lang}.jar"));
    if jar.exists() || jar.parent().map(|p| p.exists()).unwrap_or(false) {
        // Java/JVM jar convention.
        out.push(RealizeBuiltin {
            path: jar.clone(),
            argv: vec![
                "java".to_string(),
                "-jar".to_string(),
                jar.display().to_string(),
                "--rpc".to_string(),
            ],
        });
    }
    // Native binary convention (mirrors lift discovery).
    out.push(RealizeBuiltin {
        path: impl_dir
            .join("target/release")
            .join(format!("provekit-realize-{target_lang}")),
        argv: vec![
            impl_dir
                .join("target/release")
                .join(format!("provekit-realize-{target_lang}"))
                .display()
                .to_string(),
            "--rpc".to_string(),
        ],
    });
    out.push(RealizeBuiltin {
        path: impl_dir
            .join("target/debug")
            .join(format!("provekit-realize-{target_lang}")),
        argv: vec![
            impl_dir
                .join("target/debug")
                .join(format!("provekit-realize-{target_lang}"))
                .display()
                .to_string(),
            "--rpc".to_string(),
        ],
    });
    out
}

fn invoke_realize(
    target_lang: &str,
    cmd_spec: &ResolvedCommand,
    request: &RealizeRequest,
) -> Result<RealizedSource, String> {
    if cmd_spec.argv.is_empty() {
        return Err("empty command".to_string());
    }
    let mut command = Command::new(&cmd_spec.argv[0]);
    if cmd_spec.argv.len() > 1 {
        command.args(&cmd_spec.argv[1..]);
    }
    if let Some(wd) = &cmd_spec.working_dir {
        command.current_dir(wd);
    }
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::null());

    let mut child = command
        .spawn()
        .map_err(|e| format!("spawn realize kit: {e}"))?;

    let params = realize_request_params(request);
    let req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "provekit.plugin.invoke",
        "params": params,
    });

    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or("realize kit stdin unavailable".to_string())?;
        let req_str = serde_json::to_string(&req).expect("serialize realize request");
        stdin
            .write_all(req_str.as_bytes())
            .and_then(|()| stdin.write_all(b"\n"))
            .map_err(|e| format!("write realize request: {e}"))?;
    }

    let stdout = child
        .stdout
        .take()
        .ok_or("realize kit stdout unavailable".to_string())?;
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|e| format!("read realize response: {e}"))?;
    let _ = child.kill();
    let _ = child.wait();

    let v: Value = serde_json::from_str(line.trim())
        .map_err(|e| format!("realize response not valid JSON: {e}; raw={}", line.trim()))?;
    if let Some(err) = v.get("error") {
        return Err(format!("realize kit error: {err}"));
    }
    let result = v
        .get("result")
        .ok_or_else(|| format!("realize response missing result; raw={}", line.trim()))?;
    let source = result
        .get("source")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            format!(
                "realize response missing result.source; raw={}",
                line.trim()
            )
        })?
        .to_string();
    let is_stub = result
        .get("is_stub")
        .and_then(Value::as_bool)
        .ok_or_else(|| {
            format!(
                "realize response missing or non-boolean result.is_stub; raw={}",
                line.trim()
            )
        })?;
    let extension = result
        .get("extension")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| extension_from_convention(target_lang));
    let emitted_artifact_cid = result
        .get("emitted_artifact_cid")
        .and_then(Value::as_str)
        .map(str::to_string);
    let observed_loss_record = result
        .get("observed_loss_record")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let used_sugars = result
        .get("used_sugars")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    // Nit: used_sugars ⊆ cited_sugar_cids subset check.
    // If the kit returns a sugar CID that was not cited in the request, the
    // call is unauthorized. Fail with a descriptive error so the caller can
    // emit a CompositionRefusalMemento with failure_kind "ext:unauthorized-sugar".
    for used in &used_sugars {
        if let Some(used_cid) = used
            .get("header")
            .and_then(|h| h.get("cid"))
            .and_then(Value::as_str)
            .or_else(|| used.as_str())
        {
            if !request.sugar_cids.iter().any(|c| c == used_cid) {
                return Err(format!(
                    "ext:unauthorized-sugar: kit returned sugar CID {used_cid:?} \
                     not in cited set {:?}",
                    request.sugar_cids
                ));
            }
        }
    }
    let observation_wrapper_emission_record = result
        .get("observation_wrapper_emission_record")
        .cloned();
    Ok(RealizedSource {
        extension,
        source,
        is_stub,
        emitted_artifact_cid,
        observed_loss_record,
        used_sugars,
        observation_wrapper_emission_record,
    })
}

fn realize_request_params(request: &RealizeRequest) -> Value {
    serde_json::to_value(request).expect("serialize realize request params")
}

/// Filesystem-level extension hint. NOT language semantics: cmd_transport
/// and cmd_bind use whatever the realize kit emits in `result.extension`
/// (per `body-template-memento.md` §3.2). This fallback is only consulted
/// when the kit elides the field, in which case we use the conventional
/// short identifier of the language. Adding a row here is filesystem
/// convention, not CLI policy.
fn extension_from_convention(lang: &str) -> String {
    // Mirror the per-language manifest convention. This list lives in the
    // dispatcher (one file), not scattered across cmd_bind / cmd_transport.
    // A kit MAY override its extension via `result.extension`.
    match lang {
        "python" => "py",
        "ruby" => "rb",
        "typescript" => "ts",
        "csharp" => "cs",
        "rust" => "rs",
        other => other,
    }
    .to_string()
}

// ============================================================================
// Language detection (filesystem-level, NOT language semantics)
// ============================================================================

/// Probe the workspace for any source language a registered lift kit can
/// handle. Returns the first kit whose manifest resolves successfully.
/// This is a FILESYSTEM probe, not a hard-coded extension list.
pub fn detect_lift_language(workspace_root: &Path) -> Option<String> {
    // 1. Scan .provekit/lift/*/manifest.toml — the operator's declared kits.
    let lift_dir = workspace_root.join(".provekit").join("lift");
    if let Ok(entries) = std::fs::read_dir(&lift_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let manifest = path.join("manifest.toml");
                if manifest.exists() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        let lang = name.trim_end_matches("-bind").to_string();
                        return Some(lang);
                    }
                }
            }
        }
    }
    // 2. Scan implementations/*/ for built-in kits.
    let impl_dir = workspace_root.join("implementations");
    if let Ok(entries) = std::fs::read_dir(&impl_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let lang = match path.file_name().and_then(|n| n.to_str()) {
                Some(s) => s.to_string(),
                None => continue,
            };
            if !builtin_lift_candidates(workspace_root, &lang)
                .iter()
                .any(|p| p.exists())
            {
                continue;
            }
            return Some(lang);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn realize_request_params_include_contract_mode_and_loss_payload() {
        let request = RealizeRequest {
            function: "lookup".to_string(),
            params: vec!["name".to_string()],
            param_types: vec!["String".to_string()],
            return_type: "String".to_string(),
            concept_name: "concept:lookup".to_string(),
            mode: Some("monitor".to_string()),
            contract: Some(RealizeContractPayload {
                concept_site_cid: "blake3-512:site".to_string(),
                local_contract_cid: "blake3-512:compound".to_string(),
                origin: "evidence-lift[type-signature]".to_string(),
                discharge_verdict: "exact".to_string(),
                witnesses: vec![RealizeContractWitness {
                    role: "pre".to_string(),
                    predicate: json!({
                        "args": [
                            {"kind": "var", "name": "name"},
                            {"kind": "const", "sort": {"kind": "primitive", "name": "Ref"}, "value": null}
                        ],
                        "kind": "atomic",
                        "name": "neq"
                    }),
                    predicate_text: "non_null(name)".to_string(),
                    source_kind: "type-signature".to_string(),
                }],
            }),
            sugar_cids: vec!["blake3-512:sugar".to_string()],
            sugar_plugins: vec![json!({"header": {"kind": "sugar"}})],
        };

        let params = realize_request_params(&request);

        assert_eq!(params["mode"], "monitor");
        assert!(params.get("total_loss_record").is_none());
        assert_eq!(params["contract"]["concept_site_cid"], "blake3-512:site");
        assert_eq!(
            params["contract"]["local_contract_cid"],
            "blake3-512:compound"
        );
        assert_eq!(params["contract"]["witnesses"][0]["role"], "pre");
        assert_eq!(
            params["contract"]["witnesses"][0]["predicate_text"],
            "non_null(name)"
        );
        assert_eq!(params["sugar_plugins"][0]["header"]["kind"], "sugar");
        assert_eq!(params["sugar_cids"][0], "blake3-512:sugar");
    }
}
