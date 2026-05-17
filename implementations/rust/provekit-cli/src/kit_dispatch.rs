// SPDX-License-Identifier: Apache-2.0
//
// Kit-agnostic dispatcher for the eight-verb bind pipeline and the realize
// surface. cmd_bind and cmd_transport call into here to invoke per-language
// lift and realize plugins via PEP 1.7.0 (`2026-05-12-plugin-protocol.md`);
// neither command has any language-specific code, no `if source_lang ==
// "rust"` and no `TargetStyle::*` arms.
//
// Three surfaces:
//
//   1. `dispatch_bind_lift(workspace_root, source_lang)`
//      Resolves a `kind = "lift"` plugin for `source_lang` via convention
//      (`.provekit/lift/<lang>/manifest.toml`, then a workspace built-in
//      under `implementations/<lang>/`, then PATH). Invokes the
//      legacy-retained `initialize` / `lift` / `shutdown` JSON-RPC shape
//      and decodes `ir-document.ir[]` into `BindLiftEntry` records per
//      `2026-05-13-bind-ir-lift-result.md`.
//
//   2. `dispatch_realize(target_lang, library_tag, request)`
//      Resolves a `kind = "realize"` (sugar/body-template) plugin for
//      `(target_lang, library_tag.unwrap_or("default"))` via convention
//      (`.provekit/realize/<surface>/manifest.toml` or a built-in path; the
//      Java built-in path is
//      `implementations/java/provekit-realize-java-core/target/...`).
//      Invokes the PEP 1.7.0 `provekit.plugin.invoke` method and returns
//      `{ source, is_stub }`.
//
//   3. `dispatch_exam_manifest(workspace_root, plugin_name, path_or_cid)`
//      Resolves a `kind = "exam-manifest"` plugin via convention
//      (`.provekit/exam-manifest/<name>/manifest.toml`, then user config).
//      Invokes `provekit.plugin.invoke` with `{path}` or `{cid}` and returns
//      the validated ExamManifestMemento. If no plugin manifest exists, the
//      compiled-in default ExamManifestKit loads a local path or catalog CID.
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

#[allow(unused_imports)]
pub use libprovekit::core::RealizeContractWitness;
use libprovekit::core::RealizeTransport;
pub use libprovekit::core::{RealizeContractPayload, RealizeRequest, RealizedSource};
use libprovekit::ExamManifestKit;
use provekit_ir_types::{Cid, ExamManifestMemento};
use provekit_plugin_loader::{PluginRegistry, PluginRegistryMemento};

use crate::project_config::read_project_config;

#[derive(Debug, Clone, Copy)]
pub struct DispatchRealizeTransport;

impl RealizeTransport for DispatchRealizeTransport {
    fn dispatch_realize(
        &self,
        workspace_root: &Path,
        target_lang: &str,
        library_tag: Option<&str>,
        request: &RealizeRequest,
    ) -> Result<RealizedSource, String> {
        dispatch_realize(workspace_root, target_lang, library_tag, request)
            .map_err(|error| error.to_string())
    }
}

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
#[allow(dead_code)]
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
    // workspace root and are not language knowledge in cmd_bind; they are
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
    library_tag: Option<String>,
    protocol_versions: Vec<String>,
    capability_kind: Option<String>,
    exam_manifest_schema_version: Option<String>,
}

fn parse_manifest(path: &Path) -> Result<ParsedManifest, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let mut name = String::new();
    let mut command: Vec<String> = Vec::new();
    let mut working_dir: Option<PathBuf> = None;
    let mut library_tag: Option<String> = None;
    let mut protocol_versions: Vec<String> = Vec::new();
    let mut capability_kind: Option<String> = None;
    let mut exam_manifest_schema_version: Option<String> = None;
    let mut section = String::new();
    for line in text.lines() {
        let line = match line.find('#') {
            Some(pos) => &line[..pos],
            None => line,
        }
        .trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line
                .trim_start_matches('[')
                .trim_end_matches(']')
                .trim()
                .to_string();
            continue;
        }
        let Some(eq) = line.find('=') else { continue };
        let key = line[..eq].trim();
        let val = line[eq + 1..].trim();
        match (section.as_str(), key) {
            ("", "name") => name = val.trim_matches('"').to_string(),
            ("", "working_dir") => working_dir = Some(PathBuf::from(val.trim_matches('"'))),
            ("", "protocol_versions") => protocol_versions = parse_toml_string_array(val),
            ("", "library_tag") => {
                let tag = val.trim_matches('"').to_string();
                validate_library_tag(&tag).map_err(|detail| {
                    format!(
                        "manifest {} has invalid `library_tag` `{tag}`: {detail}",
                        path.display()
                    )
                })?;
                library_tag = Some(tag);
            }
            ("", "command") => command = parse_toml_string_array(val),
            ("capabilities", "kind") => capability_kind = Some(val.trim_matches('"').to_string()),
            ("capabilities", "exam_manifest_schema_version") => {
                exam_manifest_schema_version = Some(val.trim_matches('"').to_string())
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
        library_tag,
        protocol_versions,
        capability_kind,
        exam_manifest_schema_version,
    })
}

fn parse_toml_string_array(value: &str) -> Vec<String> {
    let inner = value.trim().trim_matches(|c| c == '[' || c == ']');
    inner
        .split(',')
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn validate_library_tag(tag: &str) -> Result<(), &'static str> {
    let mut chars = tag.chars();
    let Some(first) = chars.next() else {
        return Err("expected [a-z][a-z0-9-]*");
    };
    if !first.is_ascii_lowercase() {
        return Err("expected [a-z][a-z0-9-]*");
    }
    if !chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return Err("expected [a-z][a-z0-9-]*");
    }
    Ok(())
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
    configure_java_runtime(&mut command, &cmd_spec.argv[0]);
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

const DEFAULT_LIBRARY_TAG: &str = "default";

/// Dispatch a realize call for `(target_lang, library_tag)`. Returns
/// `Err(KitUnavailable)` when no realize plugin exists. Callers turn this into a
/// `kit-plugin-unavailable` gap record so the run is loudly-bounded-lossy
/// at the realize boundary rather than silently empty.
pub fn dispatch_realize(
    workspace_root: &Path,
    target_lang: &str,
    library_tag: Option<&str>,
    request: &RealizeRequest,
) -> Result<RealizedSource, KitUnavailable> {
    let resolved = resolve_realize_command(workspace_root, target_lang, library_tag)?;
    invoke_realize(target_lang, &resolved, request).map_err(|e| KitUnavailable {
        kit_kind: "realize",
        language: target_lang.to_string(),
        detail: e,
    })
}

fn resolve_realize_command(
    workspace_root: &Path,
    target_lang: &str,
    library_tag: Option<&str>,
) -> Result<ResolvedCommand, KitUnavailable> {
    if let Some(tag) = library_tag {
        if let Err(detail) = validate_library_tag(tag) {
            return Err(KitUnavailable {
                kit_kind: "realize",
                language: target_lang.to_string(),
                detail: format!("invalid requested library_tag `{tag}`: {detail}"),
            });
        }
    }

    let candidates = registered_realize_candidates(workspace_root, target_lang)?;
    let requested = library_tag.unwrap_or(DEFAULT_LIBRARY_TAG);
    if let Some(candidate) = candidates
        .iter()
        .find(|candidate| candidate.tag == requested)
    {
        return Ok(candidate.command.clone());
    }

    if library_tag.is_none() {
        if candidates.len() == 1 {
            return Ok(candidates[0].command.clone());
        }
        if !candidates.is_empty() {
            let tags = candidates
                .iter()
                .map(|candidate| format!("{} from {}", candidate.tag, candidate.source))
                .collect::<Vec<_>>()
                .join(", ");
            return Err(KitUnavailable {
                kit_kind: "realize",
                language: target_lang.to_string(),
                detail: format!(
                    "multiple realize plugins registered for language `{target_lang}` but none \
                     has library_tag `default`; pass an explicit library_tag. registered: {tags}"
                ),
            });
        }
    }

    let env_var = format!("PROVEKIT_REALIZE_{}_BIN", target_lang.to_uppercase());
    let registered = if candidates.is_empty() {
        "none".to_string()
    } else {
        candidates
            .iter()
            .map(|candidate| format!("{} from {}", candidate.tag, candidate.source))
            .collect::<Vec<_>>()
            .join(", ")
    };
    Err(KitUnavailable {
        kit_kind: "realize",
        language: target_lang.to_string(),
        detail: format!(
            "no realize plugin for language `{target_lang}` and library_tag `{requested}`. \
             looked in .provekit/realize/*/manifest.toml, env {env_var}, built-in binaries \
             under implementations/{target_lang}/, and `provekit-realize-{target_lang}` on \
             PATH. registered: {registered}"
        ),
    })
}

#[derive(Debug, Clone)]
struct RealizeCandidate {
    tag: String,
    command: ResolvedCommand,
    source: String,
}

fn registered_realize_candidates(
    workspace_root: &Path,
    target_lang: &str,
) -> Result<Vec<RealizeCandidate>, KitUnavailable> {
    let mut candidates =
        project_realize_candidates(workspace_root, target_lang).map_err(|e| KitUnavailable {
            kit_kind: "realize",
            language: target_lang.to_string(),
            detail: e,
        })?;

    // Env-var, built-in, and PATH fallbacks have no manifest tag, so they occupy
    // the back-compatible default slot.
    let env_var = format!("PROVEKIT_REALIZE_{}_BIN", target_lang.to_uppercase());
    if let Ok(bin) = std::env::var(&env_var) {
        candidates.push(RealizeCandidate {
            tag: DEFAULT_LIBRARY_TAG.to_string(),
            command: ResolvedCommand {
                argv: vec![bin, "--rpc".to_string()],
                working_dir: Some(workspace_root.to_path_buf()),
            },
            source: env_var,
        });
    }

    // Substrate-convention built-in binaries. Same shape as lift:
    // the dispatcher consults the FILESYSTEM, not a hard-coded list.
    for candidate in builtin_realize_candidates(workspace_root, target_lang) {
        if candidate.path.exists() {
            candidates.push(RealizeCandidate {
                tag: DEFAULT_LIBRARY_TAG.to_string(),
                command: ResolvedCommand {
                    argv: candidate.argv,
                    working_dir: Some(workspace_root.to_path_buf()),
                },
                source: candidate.path.display().to_string(),
            });
        }
    }

    // PATH probe.
    let bin = format!("provekit-realize-{target_lang}");
    if which_on_path(&bin).is_some() {
        candidates.push(RealizeCandidate {
            tag: DEFAULT_LIBRARY_TAG.to_string(),
            command: ResolvedCommand {
                argv: vec![bin.clone(), "--rpc".to_string()],
                working_dir: Some(workspace_root.to_path_buf()),
            },
            source: format!("PATH:{bin}"),
        });
    }

    Ok(candidates)
}

fn project_realize_candidates(
    workspace_root: &Path,
    target_lang: &str,
) -> Result<Vec<RealizeCandidate>, String> {
    let realize_dir = workspace_root.join(".provekit").join("realize");
    let Ok(entries) = std::fs::read_dir(&realize_dir) else {
        return Ok(Vec::new());
    };
    let mut surfaces = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_dir() {
                return None;
            }
            let surface = path.file_name()?.to_str()?.to_string();
            Some((surface, path))
        })
        .collect::<Vec<_>>();
    surfaces.sort_by(|a, b| a.0.cmp(&b.0));

    let mut out = Vec::new();
    for (surface, path) in surfaces {
        if !realize_surface_matches_target(&surface, target_lang) {
            continue;
        }
        let manifest = path.join("manifest.toml");
        if !manifest.exists() {
            continue;
        }
        let parsed = parse_manifest(&manifest)?;
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
        out.push(RealizeCandidate {
            tag: parsed
                .library_tag
                .unwrap_or_else(|| DEFAULT_LIBRARY_TAG.to_string()),
            command: ResolvedCommand {
                argv: parsed.command,
                working_dir,
            },
            source: manifest.display().to_string(),
        });
    }
    Ok(out)
}

fn realize_surface_matches_target(surface: &str, target_lang: &str) -> bool {
    surface == target_lang
        || surface
            .strip_prefix(target_lang)
            .and_then(|suffix| suffix.strip_prefix('-'))
            .is_some()
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
    configure_java_runtime(&mut command, &cmd_spec.argv[0]);
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
    let observation_wrapper_emission_record =
        result.get("observation_wrapper_emission_record").cloned();
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

fn configure_java_runtime(command: &mut Command, argv0: &str) {
    if argv0 != "java" || std::env::var_os("JAVA_HOME").is_some() {
        return;
    }
    if let Some(java_home) = java_home_from_maven() {
        command.env("JAVA_HOME", java_home);
    }
}

fn java_home_from_maven() -> Option<String> {
    let output = Command::new("mvn").arg("-version").output().ok()?;
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    for line in combined.lines() {
        if let Some((_, runtime)) = line.split_once("runtime: ") {
            let runtime = runtime.trim();
            if Path::new(runtime).join("bin").join("java").exists() {
                return Some(runtime.to_string());
            }
        }
    }
    None
}

// ============================================================================
// Exam manifest dispatch (PEP 1.7.0 kind = "exam-manifest")
// ============================================================================

const EXAM_MANIFEST_KIND: &str = "exam-manifest";
const EXAM_MANIFEST_SCHEMA_VERSION: &str = "provekit-exam-manifest/v1";
const PEP_1_7_0: &str = "pep/1.7.0";
pub const DEFAULT_EXAM_MANIFEST_CID: &str = "blake3-512:0e012db4ce35b235b8482344795ccbe8bccad51522825b5c495a862648736936497b11a940cf0ba9170ee6202849e9a8dc9eca5cb3021261ffa2f4ac4df6edc1";
#[allow(dead_code)]
pub const EXAM_MANIFEST_MISMATCH_REASON: &str = "exam-manifest-mismatch";

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KitDispatchError {
    ExamManifestMismatch {
        local_manifest_cid: Cid,
        remote_manifest_cid: Cid,
    },
}

#[allow(dead_code)]
impl KitDispatchError {
    pub fn refused_reason(&self) -> &'static str {
        match self {
            Self::ExamManifestMismatch { .. } => EXAM_MANIFEST_MISMATCH_REASON,
        }
    }

    pub fn refusal_payload(&self) -> Value {
        match self {
            Self::ExamManifestMismatch {
                local_manifest_cid,
                remote_manifest_cid,
            } => json!({
                "refused_reason": EXAM_MANIFEST_MISMATCH_REASON,
                "local_manifest_cid": local_manifest_cid,
                "remote_manifest_cid": remote_manifest_cid,
            }),
        }
    }
}

impl std::fmt::Display for KitDispatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExamManifestMismatch {
                local_manifest_cid,
                remote_manifest_cid,
            } => write!(
                f,
                "{}: local_manifest_cid={}, remote_manifest_cid={}",
                EXAM_MANIFEST_MISMATCH_REASON, local_manifest_cid, remote_manifest_cid
            ),
        }
    }
}

impl std::error::Error for KitDispatchError {}

pub fn configured_exam_manifest_cid(project_root: &Path) -> Cid {
    read_project_config(project_root)
        .exam_manifest_cid
        .filter(|cid| !cid.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_EXAM_MANIFEST_CID.to_string())
}

#[allow(dead_code)]
pub fn seal_plugin_registry_for_project(
    registry: &PluginRegistry,
    project_root: &Path,
    sealed_at: &str,
) -> PluginRegistryMemento {
    registry.emit_registry_memento_with_exam_manifest(
        sealed_at,
        Some(configured_exam_manifest_cid(project_root)),
        None,
    )
}

#[allow(dead_code)]
pub fn federate_plugin_registries(
    local: &PluginRegistryMemento,
    remote: &PluginRegistryMemento,
) -> Result<(), KitDispatchError> {
    let local_cids = registry_exam_manifest_cids(local);
    let remote_cids = registry_exam_manifest_cids(remote);
    if local_cids.iter().any(|cid| remote_cids.contains(cid)) {
        return Ok(());
    }

    Err(KitDispatchError::ExamManifestMismatch {
        local_manifest_cid: registry_primary_exam_manifest_cid(local),
        remote_manifest_cid: registry_primary_exam_manifest_cid(remote),
    })
}

#[allow(dead_code)]
fn registry_exam_manifest_cids(registry: &PluginRegistryMemento) -> Vec<Cid> {
    let mut cids = registry
        .header
        .exam_manifest_set
        .clone()
        .unwrap_or_default();
    if let Some(cid) = &registry.header.exam_manifest_cid {
        cids.push(cid.clone());
    }
    if cids.is_empty() {
        cids.push(DEFAULT_EXAM_MANIFEST_CID.to_string());
    }
    cids.sort();
    cids.dedup();
    cids
}

#[allow(dead_code)]
fn registry_primary_exam_manifest_cid(registry: &PluginRegistryMemento) -> Cid {
    registry
        .header
        .exam_manifest_cid
        .clone()
        .unwrap_or_else(|| DEFAULT_EXAM_MANIFEST_CID.to_string())
}

pub fn dispatch_exam_manifest(
    workspace_root: &Path,
    plugin_name: &str,
    path_or_cid: &str,
) -> Result<ExamManifestMemento, KitUnavailable> {
    if let Some(path) = plugin_name.strip_prefix("builtin-path:") {
        return load_builtin_exam_manifest(workspace_root, path).map_err(|detail| KitUnavailable {
            kit_kind: EXAM_MANIFEST_KIND,
            language: "builtin".to_string(),
            detail,
        });
    }

    match resolve_exam_manifest_command(workspace_root, plugin_name) {
        Ok(Some(resolved)) => {
            invoke_exam_manifest(&resolved, path_or_cid).map_err(|detail| KitUnavailable {
                kit_kind: EXAM_MANIFEST_KIND,
                language: plugin_name.to_string(),
                detail,
            })
        }
        Ok(None) => load_builtin_exam_manifest(workspace_root, path_or_cid).map_err(|detail| {
            KitUnavailable {
                kit_kind: EXAM_MANIFEST_KIND,
                language: plugin_name.to_string(),
                detail,
            }
        }),
        Err(error) => Err(error),
    }
}

fn resolve_exam_manifest_command(
    workspace_root: &Path,
    plugin_name: &str,
) -> Result<Option<ResolvedCommand>, KitUnavailable> {
    let project_manifest = workspace_root
        .join(".provekit")
        .join(EXAM_MANIFEST_KIND)
        .join(plugin_name)
        .join("manifest.toml");
    let user_manifest = std::env::var_os("HOME").map(|home| {
        PathBuf::from(home)
            .join(".config")
            .join("provekit")
            .join(EXAM_MANIFEST_KIND)
            .join(plugin_name)
            .join("manifest.toml")
    });

    let manifest = if project_manifest.exists() {
        Some(project_manifest)
    } else {
        user_manifest.filter(|path| path.exists())
    };
    let Some(manifest) = manifest else {
        return Ok(None);
    };

    let parsed = parse_manifest(&manifest).map_err(|detail| KitUnavailable {
        kit_kind: EXAM_MANIFEST_KIND,
        language: plugin_name.to_string(),
        detail,
    })?;
    validate_exam_manifest_plugin_manifest(&manifest, &parsed).map_err(|detail| {
        KitUnavailable {
            kit_kind: EXAM_MANIFEST_KIND,
            language: plugin_name.to_string(),
            detail,
        }
    })?;
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
    Ok(Some(ResolvedCommand {
        argv: parsed.command,
        working_dir,
    }))
}

fn validate_exam_manifest_plugin_manifest(
    manifest: &Path,
    parsed: &ParsedManifest,
) -> Result<(), String> {
    if !parsed
        .protocol_versions
        .iter()
        .any(|version| version == PEP_1_7_0)
    {
        return Err(format!(
            "manifest {} must declare protocol_versions = [\"{}\"]",
            manifest.display(),
            PEP_1_7_0
        ));
    }
    if parsed.capability_kind.as_deref() != Some(EXAM_MANIFEST_KIND) {
        return Err(format!(
            "manifest {} must declare [capabilities].kind = \"{}\"",
            manifest.display(),
            EXAM_MANIFEST_KIND
        ));
    }
    if parsed.exam_manifest_schema_version.as_deref() != Some(EXAM_MANIFEST_SCHEMA_VERSION) {
        return Err(format!(
            "manifest {} must declare [capabilities].exam_manifest_schema_version = \"{}\"",
            manifest.display(),
            EXAM_MANIFEST_SCHEMA_VERSION
        ));
    }
    Ok(())
}

fn invoke_exam_manifest(
    cmd_spec: &ResolvedCommand,
    path_or_cid: &str,
) -> Result<ExamManifestMemento, String> {
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
    configure_java_runtime(&mut command, &cmd_spec.argv[0]);
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::null());

    let mut child = command
        .spawn()
        .map_err(|error| format!("spawn exam-manifest kit: {error}"))?;
    let req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "provekit.plugin.invoke",
        "params": exam_manifest_request_params(path_or_cid)?,
    });

    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or("exam-manifest kit stdin unavailable".to_string())?;
        let req_str = serde_json::to_string(&req).expect("serialize exam-manifest request");
        stdin
            .write_all(req_str.as_bytes())
            .and_then(|()| stdin.write_all(b"\n"))
            .map_err(|error| format!("write exam-manifest request: {error}"))?;
    }

    let stdout = child
        .stdout
        .take()
        .ok_or("exam-manifest kit stdout unavailable".to_string())?;
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|error| format!("read exam-manifest response: {error}"))?;
    let _ = child.kill();
    let _ = child.wait();

    let value: Value = serde_json::from_str(line.trim()).map_err(|error| {
        format!(
            "exam-manifest response not valid JSON: {error}; raw={}",
            line.trim()
        )
    })?;
    if let Some(error) = value.get("error") {
        return Err(format!("exam-manifest kit error: {error}"));
    }
    let result = value
        .get("result")
        .cloned()
        .ok_or_else(|| format!("exam-manifest response missing result; raw={}", line.trim()))?;
    let manifest: ExamManifestMemento = serde_json::from_value(result)
        .map_err(|error| format!("decode ExamManifestMemento: {error}"))?;
    validate_exam_manifest_memento(&manifest)?;
    Ok(manifest)
}

fn exam_manifest_request_params(path_or_cid: &str) -> Result<Value, String> {
    if path_or_cid.is_empty() {
        return Err("exam manifest target is empty".to_string());
    }
    if path_or_cid.starts_with("blake3-512:") {
        Ok(json!({ "cid": path_or_cid }))
    } else {
        Ok(json!({ "path": path_or_cid }))
    }
}

fn load_builtin_exam_manifest(
    workspace_root: &Path,
    path_or_cid: &str,
) -> Result<ExamManifestMemento, String> {
    let target = path_or_cid
        .strip_prefix("builtin-path:")
        .unwrap_or(path_or_cid);
    let path = if target.starts_with("blake3-512:") {
        find_exam_manifest_cid(workspace_root, target)?
    } else {
        resolve_workspace_path(workspace_root, target)
    };
    ExamManifestKit::new()
        .load_path(&path)
        .map_err(|error| error.to_string())
}

fn resolve_workspace_path(workspace_root: &Path, raw: &str) -> PathBuf {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        path
    } else {
        workspace_root.join(path)
    }
}

fn find_exam_manifest_cid(workspace_root: &Path, cid: &str) -> Result<PathBuf, String> {
    let roots = [
        workspace_root.join("catalog"),
        workspace_root.join(".provekit").join("catalog"),
        workspace_root.to_path_buf(),
    ];
    for root in roots {
        let index = root.join("index.json");
        if index.exists() {
            if let Some(path) = exam_manifest_path_from_index(&root, &index, cid)? {
                return Ok(path);
            }
        }
        let exams = root.join("exams");
        if exams.is_dir() {
            for entry in std::fs::read_dir(&exams)
                .map_err(|error| format!("read {}: {error}", exams.display()))?
            {
                let entry =
                    entry.map_err(|error| format!("read {} entry: {error}", exams.display()))?;
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                    continue;
                }
                if let Ok(manifest) = ExamManifestKit::new().load_path(&path) {
                    if manifest.header.cid == cid {
                        return Ok(path);
                    }
                }
            }
        }
    }
    Err(format!(
        "exam manifest CID {cid} not found in catalog/index.json or exams/"
    ))
}

fn exam_manifest_path_from_index(
    catalog_root: &Path,
    index: &Path,
    cid: &str,
) -> Result<Option<PathBuf>, String> {
    let raw = std::fs::read_to_string(index)
        .map_err(|error| format!("read {}: {error}", index.display()))?;
    let value: Value = serde_json::from_str(&raw)
        .map_err(|error| format!("parse {}: {error}", index.display()))?;
    let Some(entry) = value
        .get("entries")
        .and_then(Value::as_object)
        .and_then(|entries| entries.get(cid))
    else {
        return Ok(None);
    };
    let kind = entry.get("kind").and_then(Value::as_str).unwrap_or("");
    if kind != "exam" {
        return Err(format!(
            "catalog entry {cid} in {} has kind `{kind}`, expected `exam`",
            index.display()
        ));
    }
    let path = entry
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("catalog entry {cid} in {} missing path", index.display()))?;
    Ok(Some(catalog_root.join(path)))
}

fn validate_exam_manifest_memento(manifest: &ExamManifestMemento) -> Result<(), String> {
    manifest
        .validate()
        .map_err(|error| format!("validate ExamManifestMemento: {error}"))?;
    let recomputed = manifest
        .recompute_header_cid()
        .map_err(|error| format!("recompute ExamManifestMemento CID: {error}"))?;
    if recomputed != manifest.header.cid {
        return Err(format!(
            "ExamManifestMemento header.cid mismatch: declared {}, recomputed {}",
            manifest.header.cid, recomputed
        ));
    }
    Ok(())
}

// ============================================================================
// Lower witness dispatch (ORP witness lowerer, legacy method `realize`)
// ============================================================================

pub fn dispatch_lower_witness(
    workspace_root: &Path,
    surface: &str,
    plan: &Value,
) -> Result<Value, String> {
    let resolved = resolve_lower_command(workspace_root, surface)?;
    rpc_lower_witness(workspace_root, surface, &resolved, plan)
}

fn resolve_lower_command(workspace_root: &Path, surface: &str) -> Result<ResolvedCommand, String> {
    let manifest = workspace_root
        .join(".provekit")
        .join("lower")
        .join(surface)
        .join("manifest.toml");
    if !manifest.exists() {
        return Err(format!(
            "no lower plugin for surface `{surface}`; expected {}",
            manifest.display()
        ));
    }
    let parsed = parse_manifest(&manifest)?;
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
    Ok(ResolvedCommand {
        argv: parsed.command,
        working_dir,
    })
}

fn rpc_lower_witness(
    workspace_root: &Path,
    surface: &str,
    cmd_spec: &ResolvedCommand,
    plan: &Value,
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
    configure_java_runtime(&mut command, &cmd_spec.argv[0]);
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::null());

    let mut child = command
        .spawn()
        .map_err(|e| format!("spawn lower kit: {e}"))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or("lower kit stdin unavailable".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or("lower kit stdout unavailable".to_string())?;
    let mut reader = BufReader::new(stdout);

    let init_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "client": {"name": "provekit-cli/lower", "version": env!("CARGO_PKG_VERSION")},
            "protocol_version": "provekit-orp/1",
            "workspace_root": workspace_root.display().to_string(),
            "config_path": ".provekit/config.toml"
        }
    });
    writeln!(stdin, "{init_req}").map_err(|e| format!("write lower initialize: {e}"))?;
    let _ = read_response(&mut reader, 1)?;

    let lower_req = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "realize",
        "params": {
            "surface": surface,
            "workspace_root": workspace_root.display().to_string(),
            "plan": plan
        }
    });
    writeln!(stdin, "{lower_req}").map_err(|e| format!("write lower realize: {e}"))?;
    let response = read_response(&mut reader, 2)?;

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
#[allow(dead_code)]
pub fn detect_lift_language(workspace_root: &Path) -> Option<String> {
    // 1. Scan .provekit/lift/*/manifest.toml, the operator's declared kits.
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
        let request: RealizeRequest = serde_json::from_value(json!({
            "function": "lookup",
            "params": ["name"],
            "param_types": ["String"],
            "return_type": "String",
            "concept_name": "concept:lookup",
            "mode": "monitor",
            "modes": ["monitor", "witness"],
            "contract": {
                "concept_site_cid": "blake3-512:site",
                "object_fcm_cid": "blake3-512:object",
                "local_contract_cid": "blake3-512:compound",
                "origin": "evidence-lift[type-signature]",
                "discharge_verdict": "exact",
                "witnesses": [{
                    "role": "pre",
                    "predicate": {
                        "args": [
                            {"kind": "var", "name": "name"},
                            {"kind": "const", "sort": {"kind": "primitive", "name": "Ref"}, "value": null}
                        ],
                        "kind": "atomic",
                        "name": "neq"
                    },
                    "predicate_text": "non_null(name)",
                    "source_kind": "type-signature"
                }]
            },
            "sugar_cids": ["blake3-512:sugar"],
            "sugar_plugins": [{"header": {"kind": "sugar"}}]
        }))
        .expect("request decodes");

        let params = realize_request_params(&request);

        assert_eq!(params["mode"], "monitor");
        assert_eq!(params["modes"][0], "monitor");
        assert_eq!(params["modes"][1], "witness");
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

    #[test]
    fn realize_request_params_include_named_term_tree() {
        let request: RealizeRequest = serde_json::from_value(json!({
            "function": "compose_tree",
            "params": ["value"],
            "param_types": ["int"],
            "return_type": "int",
            "concept_name": "UNNAMED-CONCEPT-1",
            "named_term_tree": {
                "conceptName": "concept:seq",
                "operationKind": "seq",
                "shapeCid": "blake3-512:seq",
                "args": [{
                    "conceptName": "concept:return",
                    "operationKind": "return",
                    "shapeCid": "blake3-512:return",
                    "args": []
                }]
            },
            "modes": [],
            "sugar_cids": [],
            "sugar_plugins": []
        }))
        .expect("request decodes");

        let params = realize_request_params(&request);

        assert_eq!(params["named_term_tree"]["conceptName"], "concept:seq");
        assert_eq!(
            params["named_term_tree"]["args"][0]["conceptName"],
            "concept:return"
        );
    }

    #[test]
    fn library_tag_validation_accepts_stable_identifier_shape() {
        assert!(validate_library_tag("urllib").is_ok());
        assert!(validate_library_tag("apache-httpclient").is_ok());
        assert!(validate_library_tag("httpx2").is_ok());
        assert!(validate_library_tag("").is_err());
        assert!(validate_library_tag("Requests").is_err());
        assert!(validate_library_tag("urllib.request").is_err());
        assert!(validate_library_tag("1requests").is_err());
    }
}
