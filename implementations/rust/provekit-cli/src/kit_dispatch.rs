// SPDX-License-Identifier: Apache-2.0
//
// Kit-agnostic dispatcher for the realize / emit / exam-manifest surfaces.
// cmd_materialize / cmd_emit / cmd_bind call into here to invoke per-language
// plugins via PEP 1.7.0 (`2026-05-12-plugin-protocol.md`); none of those
// commands carry language-specific code, no `if source_lang == "rust"` and
// no `TargetStyle::*` arms.
//
// Three surfaces:
//
//   1. `dispatch_realize(target_lang, library_tag, request)`
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
//   3. `dispatch_emit(workspace_root, target_lang, framework, plan)`
//      Resolves a `kind = "emit"` plugin by `.provekit/emit/<surface>/manifest.toml`,
//      where surfaces are target/framework packages such as `go-testing`.
//      Invokes `provekit.plugin.invoke` with the neutral EmitPlan. The kit owns
//      all target/framework syntax; the CLI owns only dispatch/composition.
//
// Kit unavailability is a `kit-plugin-unavailable` gap, not a hidden error.
// Per Supra omnia, rectum the dispatcher refuses loudly with a gap record
// the caller turns into a `GapRecord` and propagates downstream.

use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};

#[allow(unused_imports)]
pub use libprovekit::core::RealizeContractWitness;
use libprovekit::core::RealizeTransport;
pub use libprovekit::core::{RealizeRequest, RealizedSource};
use libprovekit::ExamManifestKit;
use provekit_canonicalizer::blake3_512_of;
use provekit_ir_types::{Cid, ExamManifestMemento};
use provekit_plugin_loader::{
    cid::compute_plugin_cid, write_plugin_registry_memento, PluginEnvelope, PluginHeader,
    PluginMemento, PluginMetadata, PluginRegistry, PluginRegistryMemento,
};

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

const REGISTRY_SEALED_AT: &str = "1970-01-01T00:00:00.000Z";
const REGISTRY_MANIFEST_KINDS: &[&str] = &["lift", "realize", "emit", "exam-manifest"];

static RUN_PLUGIN_REGISTRIES: OnceLock<Mutex<BTreeMap<PathBuf, RunPluginRegistry>>> =
    OnceLock::new();
static KIT_DISPATCH_DIAGNOSTICS: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct SealedPluginRegistry {
    pub memento: PluginRegistryMemento,
    pub path: PathBuf,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct RunPluginRegistry {
    sealed: SealedPluginRegistry,
    plugins: Vec<ManifestPluginRegistration>,
}

#[derive(Debug, Clone)]
struct ManifestPluginRegistration {
    kind: String,
    surface: String,
    source: String,
    manifest_path: PathBuf,
    parsed: ParsedManifest,
    memento: PluginMemento,
}

#[allow(dead_code)]
pub fn ensure_sealed_plugin_registry_for_project(
    workspace_root: &Path,
) -> Result<SealedPluginRegistry, String> {
    run_plugin_registry_for_project(workspace_root).map(|registry| registry.sealed)
}

#[allow(dead_code)]
pub fn reset_kit_dispatch_registry_cache_for_tests() {
    if let Some(cache) = RUN_PLUGIN_REGISTRIES.get() {
        cache.lock().expect("registry cache lock").clear();
    }
    let _ = drain_kit_dispatch_diagnostics();
}

#[allow(dead_code)]
pub fn drain_kit_dispatch_diagnostics() -> Vec<String> {
    let diagnostics = KIT_DISPATCH_DIAGNOSTICS.get_or_init(|| Mutex::new(Vec::new()));
    let mut diagnostics = diagnostics.lock().expect("diagnostics lock");
    std::mem::take(&mut *diagnostics)
}

fn run_plugin_registry_for_project(workspace_root: &Path) -> Result<RunPluginRegistry, String> {
    let key = registry_cache_key(workspace_root);
    let cache = RUN_PLUGIN_REGISTRIES.get_or_init(|| Mutex::new(BTreeMap::new()));
    if let Some(registry) = cache
        .lock()
        .expect("registry cache lock")
        .get(&key)
        .cloned()
    {
        return Ok(registry);
    }

    let registry = build_run_plugin_registry(&key)?;
    cache
        .lock()
        .expect("registry cache lock")
        .insert(key, registry.clone());
    Ok(registry)
}

fn registry_cache_key(workspace_root: &Path) -> PathBuf {
    std::fs::canonicalize(workspace_root).unwrap_or_else(|_| {
        if workspace_root.is_absolute() {
            workspace_root.to_path_buf()
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(workspace_root)
        }
    })
}

fn build_run_plugin_registry(workspace_root: &Path) -> Result<RunPluginRegistry, String> {
    let plugins = scan_manifest_plugins(workspace_root)?;
    let mut registry = PluginRegistry::new();
    for plugin in &plugins {
        registry
            .register(plugin.memento.clone(), &plugin.source)
            .map_err(|error| format!("register {}: {error}", plugin.source))?;
    }
    let memento = registry.emit_registry_memento_with_exam_manifest(
        REGISTRY_SEALED_AT,
        Some(configured_exam_manifest_cid(workspace_root)),
        None,
    );
    let path = write_plugin_registry_memento(workspace_root, &memento)
        .map_err(|error| format!("write sealed PluginRegistryMemento: {error}"))?;
    Ok(RunPluginRegistry {
        sealed: SealedPluginRegistry { memento, path },
        plugins,
    })
}

fn scan_manifest_plugins(workspace_root: &Path) -> Result<Vec<ManifestPluginRegistration>, String> {
    let mut plugins = Vec::new();
    let configured_emit_surfaces = configured_emit_surface_names(workspace_root);
    for kind in REGISTRY_MANIFEST_KINDS {
        let kind_dir = workspace_root.join(".provekit").join(kind);
        let Ok(entries) = std::fs::read_dir(&kind_dir) else {
            continue;
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
        for (surface, path) in surfaces {
            if *kind == "emit" && !configured_emit_surfaces.contains(&surface) {
                continue;
            }
            let manifest_path = path.join("manifest.toml");
            if !manifest_path.exists() {
                continue;
            }
            let parsed = parse_manifest(&manifest_path)?;
            let source = registry_source(workspace_root, &manifest_path);
            let memento = manifest_plugin_memento(
                workspace_root,
                *kind,
                &surface,
                &source,
                &manifest_path,
                &parsed,
            )?;
            plugins.push(ManifestPluginRegistration {
                kind: (*kind).to_string(),
                surface,
                source,
                manifest_path,
                parsed,
                memento,
            });
        }
    }
    Ok(plugins)
}

fn registry_source(workspace_root: &Path, path: &Path) -> String {
    path.strip_prefix(workspace_root)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn manifest_plugin_memento(
    workspace_root: &Path,
    kind: &str,
    surface: &str,
    source: &str,
    manifest_path: &Path,
    parsed: &ParsedManifest,
) -> Result<PluginMemento, String> {
    let manifest_bytes = std::fs::read(manifest_path)
        .map_err(|error| format!("read {}: {error}", manifest_path.display()))?;
    let working_dir = parsed
        .working_dir
        .as_ref()
        .map(|path| path.display().to_string());
    let content = json!({
        "kind": "manifest-plugin",
        "plugin_kind": kind,
        "surface": surface,
        "manifest_path": source,
        "manifest_cid": blake3_512_of(&manifest_bytes),
        "name": parsed.name.clone(),
        "command": parsed.command.clone(),
        "working_dir": working_dir,
        "library_tag": parsed.library_tag.clone(),
        "capability_kind": parsed.capability_kind.clone(),
        "exam_manifest_schema_version": parsed.exam_manifest_schema_version.clone(),
        "workspace_relative": manifest_path.starts_with(workspace_root),
    });
    let mut protocol_versions = parsed.protocol_versions.clone();
    if protocol_versions.is_empty() {
        protocol_versions.push(PEP_1_7_0.to_string());
    }
    protocol_versions.sort();
    protocol_versions.dedup();
    let mut header = PluginHeader {
        cid: String::new(),
        content,
        critical: false,
        kind: kind.to_string(),
        protocol_versions,
        provenance_cid: blake3_512_of(&manifest_bytes),
        schema_version: "1".to_string(),
        version: "0.1.0".to_string(),
    };
    header.cid = compute_plugin_cid(&header);
    Ok(PluginMemento {
        envelope: PluginEnvelope {
            declared_at: REGISTRY_SEALED_AT.to_string(),
            signature: "ed25519:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string(),
            signer: "ed25519:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".to_string(),
        },
        header,
        metadata: PluginMetadata::default(),
    })
}

fn registry_authorizes_plugin(
    registry: &RunPluginRegistry,
    plugin: &ManifestPluginRegistration,
) -> bool {
    registry
        .sealed
        .memento
        .header
        .load_order
        .iter()
        .any(|entry| {
            entry.kind == plugin.kind
                && entry.cid == plugin.memento.cid()
                && entry.source == plugin.source
        })
}

/// #1364 / #1355: Return the explicit concept-coverage declaration of
/// the realize manifest matching `(target_lang, library_tag)`. cmd_materialize
/// can defensively refuse-loudly when a consumer @boundary asks for a
/// concept not in the returned list — making per-kit coverage gaps
/// surface-explicit instead of falling through to the realize plugin's
/// `is_stub` fallback.
///
/// Returns an empty Vec when no manifest matches OR when the matched
/// manifest declares no `provides_concepts`. Empty is the substrate-honest
/// signal for "no explicit declaration — fall back to dispatch-time
/// is_stub behavior" (today's default).
pub fn provides_concepts_for_realize(
    workspace_root: &Path,
    target_lang: &str,
    library_tag: &str,
) -> Vec<String> {
    let Ok(candidates) = registry_realize_candidates(workspace_root, target_lang) else {
        return Vec::new();
    };
    for cand in &candidates {
        if cand.tag == library_tag {
            return read_provides_concepts_from_manifest_source(workspace_root, &cand.source);
        }
    }
    Vec::new()
}

fn read_provides_concepts_from_manifest_source(workspace_root: &Path, source: &str) -> Vec<String> {
    let candidate_path = PathBuf::from(source);
    let resolved = if candidate_path.is_absolute() {
        candidate_path
    } else {
        workspace_root.join(&candidate_path)
    };
    if !resolved.is_file() {
        return Vec::new();
    }
    match parse_manifest(&resolved) {
        Ok(parsed) => parsed.provides_concepts,
        Err(_) => Vec::new(),
    }
}

/// Ask every configured kit plugin for dependency `.proof` files resolved from
/// the project's package-manager graph. The substrate stays language-blind:
/// this function only iterates configured plugin commands, invokes the common
/// RPC verb, and returns proof file paths for the verifier to content-address.
pub fn dependency_proof_paths_via_rpc(workspace_root: &Path) -> Result<Vec<PathBuf>, String> {
    let registry = run_plugin_registry_for_project(workspace_root)?;
    let mut commands: BTreeMap<String, ResolvedCommand> = BTreeMap::new();
    for plugin in registry
        .plugins
        .iter()
        .filter(|plugin| registry_authorizes_plugin(&registry, plugin))
    {
        let command = resolved_command_from_manifest(workspace_root, &plugin.parsed);
        if command.argv.is_empty() {
            continue;
        }
        commands
            .entry(command_key(&command))
            .or_insert_with(|| command);
    }

    let mut proof_paths = BTreeSet::new();
    for command in commands.values() {
        let Some(paths) = dependency_proof_paths_for_command(workspace_root, command)? else {
            continue;
        };
        for path in paths {
            proof_paths.insert(path);
        }
    }
    Ok(proof_paths.into_iter().collect())
}

fn command_key(command: &ResolvedCommand) -> String {
    format!("{:?}\u{0}{:?}", command.argv, command.working_dir)
}

fn dependency_proof_paths_for_command(
    workspace_root: &Path,
    cmd_spec: &ResolvedCommand,
) -> Result<Option<Vec<PathBuf>>, String> {
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
    command.stderr(Stdio::inherit());

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            record_dependency_proof_diagnostic(format!(
                "dependency proof resolver unavailable for {:?}: {error}",
                cmd_spec.argv
            ));
            return Ok(None);
        }
    };
    let mut stdin = child
        .stdin
        .take()
        .ok_or("dependency proof kit stdin unavailable".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or("dependency proof kit stdout unavailable".to_string())?;
    let mut reader = BufReader::new(stdout);

    let req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "provekit.plugin.resolve_dependency_proofs",
        "params": {
            "project_root": workspace_root.display().to_string(),
        },
    });
    writeln!(stdin, "{req}").map_err(|e| format!("write resolve_dependency_proofs: {e}"))?;

    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|e| format!("read resolve_dependency_proofs response: {e}"))?;

    let shutdown = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "provekit.plugin.shutdown",
    });
    let _ = writeln!(stdin, "{shutdown}");
    drop(stdin);
    let _ = child.wait();

    if line.trim().is_empty() {
        record_dependency_proof_diagnostic(format!(
            "dependency proof resolver {:?} closed without a response",
            cmd_spec.argv
        ));
        return Ok(None);
    }

    let response: Value = serde_json::from_str(line.trim()).map_err(|e| {
        format!(
            "resolve_dependency_proofs response not valid JSON: {e}; raw={}",
            line.trim()
        )
    })?;
    if let Some(error) = response.get("error") {
        if error.get("code").and_then(Value::as_i64) == Some(-32601) {
            record_dependency_proof_diagnostic(format!(
                "dependency proof resolver {:?} does not implement provekit.plugin.resolve_dependency_proofs",
                cmd_spec.argv
            ));
            return Ok(None);
        }
        return Err(format!("dependency proof resolver error: {error}"));
    }

    let paths = response
        .get("result")
        .and_then(|result| {
            result
                .get("proof_paths")
                .or_else(|| result.get("proofPaths"))
                .or_else(|| result.get("paths"))
        })
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut out = Vec::new();
    for path in paths {
        let Some(path) = path.as_str() else {
            record_dependency_proof_diagnostic(format!(
                "dependency proof resolver {:?} returned a non-string path: {path}",
                cmd_spec.argv
            ));
            continue;
        };
        if let Some(path) = normalize_dependency_proof_path(workspace_root, path) {
            out.push(path);
        }
    }
    out.sort();
    out.dedup();
    Ok(Some(out))
}

fn normalize_dependency_proof_path(workspace_root: &Path, path: &str) -> Option<PathBuf> {
    let path = PathBuf::from(path);
    let resolved = if path.is_absolute() {
        path
    } else {
        workspace_root.join(path)
    };
    if resolved.extension().and_then(|s| s.to_str()) != Some("proof") {
        record_dependency_proof_diagnostic(format!(
            "dependency proof resolver returned non-.proof path {}; skipping",
            resolved.display()
        ));
        return None;
    }
    if !resolved.is_file() {
        record_dependency_proof_diagnostic(format!(
            "dependency proof resolver returned missing path {}; skipping",
            resolved.display()
        ));
        return None;
    }
    Some(resolved)
}

fn record_dependency_proof_diagnostic(message: String) {
    eprintln!("{message}");
    KIT_DISPATCH_DIAGNOSTICS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .expect("diagnostics lock")
        .push(message);
}

/// #1360 / #1355: Return the per-target scope-bringings declared by
/// the realize manifest matching `(target_lang, library_tag)`. cmd_materialize
/// collects these across all materialized sites in a consumer file and
/// hoists them into the file's prelude so the spliced bodies compile.
///
/// Returns an empty Vec when no manifest matches OR when the matched
/// manifest declares no `scope_bringings`. This is the substrate-honest
/// signal for "no scope-bringings needed" (NOT an error condition).
pub fn scope_bringings_for_realize(
    workspace_root: &Path,
    target_lang: &str,
    library_tag: &str,
) -> Vec<String> {
    let Ok(candidates) = registry_realize_candidates(workspace_root, target_lang) else {
        return Vec::new();
    };
    // First try exact library_tag match.
    for cand in &candidates {
        if cand.tag == library_tag {
            // RealizeCandidate.source is the manifest path relative to
            // workspace_root (`.provekit/realize/<surface>/manifest.toml`)
            // OR an env-var / built-in / PATH source string. Resolve relative
            // paths against workspace_root before parsing; non-manifest
            // sources return empty.
            return read_scope_bringings_from_manifest_source(workspace_root, &cand.source);
        }
    }
    Vec::new()
}

fn read_scope_bringings_from_manifest_source(workspace_root: &Path, source: &str) -> Vec<String> {
    let candidate_path = PathBuf::from(source);
    let resolved = if candidate_path.is_absolute() {
        candidate_path
    } else {
        workspace_root.join(&candidate_path)
    };
    if !resolved.is_file() {
        return Vec::new();
    }
    match parse_manifest(&resolved) {
        Ok(parsed) => parsed.scope_bringings,
        Err(_) => Vec::new(),
    }
}

/// #1359 / #1355: Find all realize candidates that satisfy a
/// constraint set `(target_lang, family?, library_tag?, library_version?)`.
/// Candidates are filtered down progressively:
///
/// 1. target_lang: required (string equality on the manifest's surface-language).
/// 2. family: when given, candidate's manifest MUST declare a matching family.
///    When given but a candidate has no family declared → exclude (substrate-
///    honest: missing means floating, but the consumer pinned, so it can't
///    satisfy the constraint).
/// 3. library_tag: when given, candidate's `tag` must equal it.
/// 4. library_version: when given, candidate's `library_version` must equal it.
///
/// Each absent constraint axis is treated as "anything matches" (the axis floats
/// from the consumer's perspective). The result is the set of candidates eligible
/// for further dispatch ranking; the actual chosen candidate is the caller's
/// responsibility (chunk 3 wires this into resolve_realize_command).
///
/// Returns an empty Vec when no candidates satisfy. This is the substrate-honest
/// signal for "refuse with would_close_with reason" upstream.
#[allow(dead_code)] // wired into dispatch in #1359 chunk 3 (follow-up PR)
pub(crate) fn find_realize_candidates_for_constraints(
    workspace_root: &Path,
    target_lang: &str,
    family: Option<&str>,
    library_tag: Option<&str>,
    library_version: Option<&str>,
) -> Result<Vec<RealizeCandidate>, String> {
    let candidates = registry_realize_candidates(workspace_root, target_lang)?;
    Ok(candidates
        .into_iter()
        .filter(|c| match family {
            Some(want) => c.family.as_deref() == Some(want),
            None => true,
        })
        .filter(|c| match library_tag {
            Some(want) => c.tag == want,
            None => true,
        })
        .filter(|c| match library_version {
            Some(want) => c.library_version.as_deref() == Some(want),
            None => true,
        })
        .collect())
}

pub(crate) fn registry_realize_candidates(
    workspace_root: &Path,
    target_lang: &str,
) -> Result<Vec<RealizeCandidate>, String> {
    let registry = run_plugin_registry_for_project(workspace_root)?;
    let mut candidates = registry
        .plugins
        .iter()
        .filter(|plugin| registry_authorizes_plugin(&registry, plugin))
        .filter(|plugin| plugin.kind == "realize")
        .filter(|plugin| realize_surface_matches_target(&plugin.surface, target_lang))
        .map(|plugin| RealizeCandidate {
            tag: plugin
                .parsed
                .library_tag
                .clone()
                .unwrap_or_else(|| DEFAULT_LIBRARY_TAG.to_string()),
            command: resolved_command_from_manifest(workspace_root, &plugin.parsed),
            source: plugin.source.clone(),
            family: plugin.parsed.family.clone(),
            library_version: plugin.parsed.library_version.clone(),
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|a, b| a.tag.cmp(&b.tag).then(a.source.cmp(&b.source)));
    Ok(candidates)
}

fn registry_exam_manifest_command(
    workspace_root: &Path,
    plugin_name: &str,
) -> Result<Option<ResolvedCommand>, String> {
    let registry = run_plugin_registry_for_project(workspace_root)?;
    let Some(plugin) = registry
        .plugins
        .iter()
        .filter(|plugin| registry_authorizes_plugin(&registry, plugin))
        .find(|plugin| plugin.kind == EXAM_MANIFEST_KIND && plugin.surface == plugin_name)
    else {
        return Ok(None);
    };
    validate_exam_manifest_plugin_manifest(&plugin.manifest_path, &plugin.parsed)?;
    Ok(Some(resolved_command_from_manifest(
        workspace_root,
        &plugin.parsed,
    )))
}

fn resolved_command_from_manifest(
    workspace_root: &Path,
    parsed: &ParsedManifest,
) -> ResolvedCommand {
    let mut argv = parsed.command.clone();
    if let Some(program) = argv.first_mut() {
        let path = Path::new(program);
        if path.is_relative() && path.components().count() > 1 {
            *program = workspace_root.join(path).to_string_lossy().into_owned();
        }
    }
    let working_dir = parsed
        .working_dir
        .clone()
        .map(|wd| {
            if wd.is_absolute() {
                wd
            } else {
                workspace_root.join(wd)
            }
        })
        .or_else(|| Some(workspace_root.to_path_buf()));
    ResolvedCommand { argv, working_dir }
}

fn record_fallback_diagnostic(kind: &str, surface: &str) {
    let message =
        format!("deprecated kit_dispatch filesystem fallback: kind={kind} surface={surface}");
    eprintln!("{message}");
    KIT_DISPATCH_DIAGNOSTICS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .expect("diagnostics lock")
        .push(message);
}

/// Refusal raised when a kit (realize, emit, exam-manifest) cannot be reached
/// or returns an unusable response. Callers turn this into a
/// `kit-plugin-unavailable` gap record and proceed loudly-bounded-lossy per
/// `body-template-memento.md` §5.
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

#[derive(Debug, Clone)]
pub(crate) struct ResolvedCommand {
    pub(crate) argv: Vec<String>,
    pub(crate) working_dir: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct ParsedManifest {
    #[allow(dead_code)]
    name: String,
    command: Vec<String>,
    working_dir: Option<PathBuf>,
    library_tag: Option<String>,
    /// #1359 / #1355: optional `family` pin. When present, dispatch can
    /// resolve consumer @boundary requests with `family = X` against any
    /// shim manifest sharing that family (even if library tags differ).
    /// Absent ↔ family floats; dispatch falls back to library_tag-equality.
    #[allow(dead_code)]
    family: Option<String>,
    /// #1359 / #1355: optional `library_version` pin. Parallel to `family`.
    #[allow(dead_code)]
    library_version: Option<String>,
    /// #1360 / #1355: per-target scope-bringings the consumer crate needs
    /// in its prelude when bodies from this realize plugin are spliced.
    /// E.g. `use std::io::{self, BufRead, Write};` for the rust-stdio
    /// shim. cmd_materialize collects these across all materialized sites
    /// and hoists them into the consumer file's `use` section.
    /// Empty vec when manifest omits the key (back-compat).
    scope_bringings: Vec<String>,
    /// #1364 / #1355: optional declaration of which concept_names this
    /// realize plugin CAN handle. cmd_materialize can defensively refuse-
    /// loudly when a consumer's @boundary asks for a concept not in the
    /// chosen manifest's provides_concepts list. Empty vec means "no
    /// declaration" — current cross-kit coverage is implicit (the
    /// dispatcher tries and the binary returns is_stub on miss); future
    /// per-kit declarations make the coverage gap surface-explicit.
    #[allow(dead_code)] // wired into cmd_materialize in #1364 chunk 2 (follow-up)
    provides_concepts: Vec<String>,
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
    let mut family: Option<String> = None;
    let mut library_version: Option<String> = None;
    let mut scope_bringings: Vec<String> = Vec::new();
    let mut provides_concepts: Vec<String> = Vec::new();
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
            // #1359 / #1355: optional realization-axis pins on realize manifests.
            // Parsed but not yet wired into dispatch resolution (that's chunk 2).
            ("", "family") => family = Some(val.trim_matches('"').to_string()),
            ("", "library_version") => library_version = Some(val.trim_matches('"').to_string()),
            ("", "scope_bringings") => scope_bringings = parse_toml_string_array(val),
            ("", "provides_concepts") => provides_concepts = parse_toml_string_array(val),
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
        family,
        library_version,
        scope_bringings,
        provides_concepts,
        protocol_versions,
        capability_kind,
        exam_manifest_schema_version,
    })
}

/// Parse a TOML inline string array like `["a", "b", "c"]`.
///
/// Quote-aware: commas inside `"..."` are NOT separators (needed for
/// #1360's `scope_bringings = ["use std::io::{self, BufRead, Write};"]`
/// where the value contains commas inside the quoted use-statement).
fn parse_toml_string_array(value: &str) -> Vec<String> {
    let inner = value.trim().trim_matches(|c| c == '[' || c == ']');
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    let mut escape = false;
    for ch in inner.chars() {
        if escape {
            current.push(ch);
            escape = false;
            continue;
        }
        match ch {
            '\\' if in_quote => {
                current.push(ch);
                escape = true;
            }
            '"' => {
                in_quote = !in_quote;
                current.push(ch);
            }
            ',' if !in_quote => {
                let trimmed = current.trim().trim_matches('"').to_string();
                if !trimmed.is_empty() {
                    out.push(trimmed);
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    let trimmed = current.trim().trim_matches('"').to_string();
    if !trimmed.is_empty() {
        out.push(trimmed);
    }
    out
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
    // #1359 chunk 3 / #1355: read the family + library_version pins from
    // the RealizeRequest (propagated by #1357 from @sugar / @boundary
    // annotations + carrier payload + shim binding entry). resolve_realize_command
    // uses these to perform family-aware constraint satisfaction when the
    // library_tag-equality lookup is ambiguous or empty.
    let resolved = resolve_realize_command(
        workspace_root,
        target_lang,
        library_tag,
        request.family.as_deref(),
        request.library_version.as_deref(),
    )?;
    // Inject the dispatched library_tag into the request so the plugin can
    // disambiguate body-template entries when multiple libraries ship templates
    // for the same concept. Without this, the multi-library body-template cache
    // is load-order-dependent.
    let mut request_with_tag = request.clone();
    if request_with_tag.target_library_tag.is_empty() {
        if let Some(tag) = library_tag {
            request_with_tag.target_library_tag = tag.to_string();
        }
    }
    invoke_realize(target_lang, &resolved, &request_with_tag).map_err(|e| KitUnavailable {
        kit_kind: "realize",
        language: target_lang.to_string(),
        detail: e,
    })
}

fn resolve_realize_command(
    workspace_root: &Path,
    target_lang: &str,
    library_tag: Option<&str>,
    family: Option<&str>,
    library_version: Option<&str>,
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

    let requested = library_tag.unwrap_or(DEFAULT_LIBRARY_TAG);
    let registry_candidates =
        registry_realize_candidates(workspace_root, target_lang).map_err(|detail| {
            KitUnavailable {
                kit_kind: "realize",
                language: target_lang.to_string(),
                detail,
            }
        })?;
    if let Some(candidate) = registry_candidates
        .iter()
        .find(|candidate| candidate.tag == requested)
    {
        return Ok(candidate.command.clone());
    }

    // #1359 chunk 3 / #1355: family-aware fallback. When library_tag-equality
    // didn't match (or library floated, signaled by library_tag == DEFAULT
    // and no manifest is tagged "default"), try matching by family +
    // library_version constraints. If exactly one candidate matches, use
    // it. Multiple matches → return clear refusal listing the candidates
    // so the consumer can pin further.
    if family.is_some() || library_version.is_some() {
        let family_matches: Vec<&RealizeCandidate> = registry_candidates
            .iter()
            .filter(|c| match family {
                Some(want) => c.family.as_deref() == Some(want),
                None => true,
            })
            .filter(|c| match library_version {
                Some(want) => c.library_version.as_deref() == Some(want),
                None => true,
            })
            .collect();
        if family_matches.len() == 1 {
            return Ok(family_matches[0].command.clone());
        }
        if !family_matches.is_empty() {
            let tags = family_matches
                .iter()
                .map(|c| format!("{} from {}", c.tag, c.source))
                .collect::<Vec<_>>()
                .join(", ");
            return Err(KitUnavailable {
                kit_kind: "realize",
                language: target_lang.to_string(),
                detail: format!(
                    "ambiguous realize dispatch for language `{target_lang}` with \
                     family={family:?} library_version={library_version:?}: {} \
                     candidates satisfy the constraints. registered: {tags}",
                    family_matches.len()
                ),
            });
        }
    }

    if library_tag.is_none() {
        if registry_candidates.len() == 1 {
            return Ok(registry_candidates[0].command.clone());
        }
        if !registry_candidates.is_empty() {
            let tags = registry_candidates
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

    record_fallback_diagnostic("realize", target_lang);
    let candidates = live_realize_candidates(workspace_root, target_lang)?;
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
             looked in .provekit/realize/*/manifest.toml. registered: {registered}"
        ),
    })
}

#[derive(Debug, Clone)]
pub(crate) struct RealizeCandidate {
    pub(crate) tag: String,
    pub(crate) command: ResolvedCommand,
    pub(crate) source: String,
    /// #1359 / #1355: realization-tuple axes the manifest declared.
    /// Used by the family-aware candidate query
    /// (`find_realize_candidates_for_family`) so dispatchers can resolve
    /// boundary requests of the form `(family = X, library = floating)`
    /// to any shim manifest sharing that family.
    pub(crate) family: Option<String>,
    pub(crate) library_version: Option<String>,
}

/// Live (unsealed) realize candidates: the `.provekit/realize/*/manifest.toml`
/// scan used when the sealed plugin registry is absent (dev/test). The
/// pre-registry env-var / built-in-binary / PATH discovery was removed — every
/// realize kit ships a manifest, so the manifest is the only realize source.
pub(crate) fn live_realize_candidates(
    workspace_root: &Path,
    target_lang: &str,
) -> Result<Vec<RealizeCandidate>, KitUnavailable> {
    project_realize_candidates(workspace_root, target_lang).map_err(|e| KitUnavailable {
        kit_kind: "realize",
        language: target_lang.to_string(),
        detail: e,
    })
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
            family: parsed.family.clone(),
            library_version: parsed.library_version.clone(),
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
    let observation_wrapper_emission_record =
        result.get("observation_wrapper_emission_record").cloned();
    // #1374: extract realization-fragment context if the realize plugin
    // emitted it. Legacy plugins omit these fields; default-empty/None.
    let imports = result
        .get("imports")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();
    let helpers = result
        .get("helpers")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let dependencies = result
        .get("dependencies")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let diagnostics = result
        .get("diagnostics")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let compile_unit_requirements = result.get("compile_unit_requirements").cloned();
    Ok(RealizedSource {
        extension,
        source,
        is_stub,
        emitted_artifact_cid,
        contract_cid: None,
        observed_loss_record,
        used_sugars,
        observation_wrapper_emission_record,
        imports,
        helpers,
        dependencies,
        diagnostics,
        compile_unit_requirements,
    })
}

// Emit dispatch (PEP 1.7.0 kind = "emit", method `provekit.plugin.invoke`)
// ============================================================================

/// Result of dispatching to an emit kit. The raw kit result is intentionally
/// preserved: emitter packages own their result schema beyond the common
/// `{source, extension, emitted_artifact_cid, ...}` fields consumed by the CLI.
#[derive(Debug, Clone)]
pub struct EmitDispatchResult {
    pub surface: String,
    pub source: String,
    pub result: Value,
}

#[derive(Debug, Clone)]
struct EmitCandidate {
    surface: String,
    command: ResolvedCommand,
    source: String,
}

/// Dispatch a neutral EmitPlan to a target/framework emitter kit.
///
/// This is deliberately not a Go/Java/Python parser. The substrate resolves an
/// emit manifest and sends JSON to the kit; the kit owns framework syntax and
/// target-language source generation.
pub fn dispatch_emit(
    workspace_root: &Path,
    target_lang: &str,
    framework: &str,
    plan: &Value,
) -> Result<EmitDispatchResult, KitUnavailable> {
    let candidate = resolve_emit_command(workspace_root, target_lang, framework)?;
    invoke_emit(target_lang, framework, &candidate, plan).map_err(|detail| KitUnavailable {
        kit_kind: "emit",
        language: target_lang.to_string(),
        detail,
    })
}

/// Ask the same configured emit kit to validate the emitted artifact using
/// its native ecosystem. The CLI supplies paths and normalized context over
/// RPC; the selected kit owns every target-language and framework decision.
pub fn dispatch_emit_check(
    workspace_root: &Path,
    target_lang: &str,
    framework: &str,
    plan: &Value,
    out_dir: &Path,
    artifact_path: &Path,
    emit_result: &Value,
) -> Result<Value, KitUnavailable> {
    let candidate = resolve_emit_command(workspace_root, target_lang, framework)?;
    invoke_emit_check(
        target_lang,
        framework,
        &candidate,
        plan,
        out_dir,
        artifact_path,
        emit_result,
    )
    .map_err(|detail| KitUnavailable {
        kit_kind: "emit",
        language: target_lang.to_string(),
        detail,
    })
}

fn resolve_emit_command(
    workspace_root: &Path,
    target_lang: &str,
    framework: &str,
) -> Result<EmitCandidate, KitUnavailable> {
    let registry_candidates = registry_emit_candidates(workspace_root, target_lang, framework)
        .map_err(|detail| KitUnavailable {
            kit_kind: "emit",
            language: target_lang.to_string(),
            detail,
        })?;
    if let Some(candidate) = select_emit_candidate(&registry_candidates, target_lang, framework) {
        return Ok(candidate);
    }

    record_fallback_diagnostic("emit", &format!("{target_lang}-{framework}"));
    let live_candidates =
        project_emit_candidates(workspace_root, target_lang, framework).map_err(|detail| {
            KitUnavailable {
                kit_kind: "emit",
                language: target_lang.to_string(),
                detail,
            }
        })?;
    if let Some(candidate) = select_emit_candidate(&live_candidates, target_lang, framework) {
        return Ok(candidate);
    }

    let registered = live_candidates
        .iter()
        .chain(registry_candidates.iter())
        .map(|candidate| format!("{} from {}", candidate.surface, candidate.source))
        .collect::<Vec<_>>();
    let registered = if registered.is_empty() {
        "none".to_string()
    } else {
        registered.join(", ")
    };
    Err(KitUnavailable {
        kit_kind: "emit",
        language: target_lang.to_string(),
        detail: format!(
            "no emit plugin for target `{target_lang}` and framework `{framework}`. \
             expected a project [[plugins]] registration in .provekit/config.toml \
             and a .provekit/emit/{target_lang}-{framework}/manifest.toml. \
             registered: {registered}"
        ),
    })
}

fn registry_emit_candidates(
    workspace_root: &Path,
    target_lang: &str,
    framework: &str,
) -> Result<Vec<EmitCandidate>, String> {
    let configured = configured_emit_surfaces(workspace_root, target_lang, framework);
    if configured.is_empty() {
        return Ok(Vec::new());
    }
    let registry = run_plugin_registry_for_project(workspace_root)?;
    let mut candidates = registry
        .plugins
        .iter()
        .filter(|plugin| registry_authorizes_plugin(&registry, plugin))
        .filter(|plugin| plugin.kind == "emit")
        .filter(|plugin| configured.contains(&plugin.surface))
        .filter(|plugin| emit_surface_matches(&plugin.surface, target_lang, framework))
        .map(|plugin| EmitCandidate {
            surface: plugin.surface.clone(),
            command: resolved_command_from_manifest(workspace_root, &plugin.parsed),
            source: plugin.source.clone(),
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|a, b| a.surface.cmp(&b.surface).then(a.source.cmp(&b.source)));
    Ok(candidates)
}

fn project_emit_candidates(
    workspace_root: &Path,
    target_lang: &str,
    framework: &str,
) -> Result<Vec<EmitCandidate>, String> {
    let configured = configured_emit_surfaces(workspace_root, target_lang, framework);
    if configured.is_empty() {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
    for surface in configured {
        let path = workspace_root.join(".provekit").join("emit").join(&surface);
        let manifest = path.join("manifest.toml");
        if !manifest.exists() {
            continue;
        }
        let parsed = parse_manifest(&manifest)?;
        out.push(EmitCandidate {
            surface,
            command: resolved_command_from_manifest(workspace_root, &parsed),
            source: manifest.display().to_string(),
        });
    }
    Ok(out)
}

fn configured_emit_surface_names(workspace_root: &Path) -> BTreeSet<String> {
    read_project_config(workspace_root)
        .plugins
        .into_iter()
        .filter_map(|plugin| {
            let surface = plugin.surface.trim().to_string();
            (!surface.is_empty()).then_some(surface)
        })
        .collect()
}

fn configured_emit_surfaces(
    workspace_root: &Path,
    target_lang: &str,
    framework: &str,
) -> Vec<String> {
    let exact_surface = format!("{target_lang}-{framework}");
    let mut surfaces = read_project_config(workspace_root)
        .plugins
        .into_iter()
        .filter_map(|plugin| {
            let surface = plugin.surface.trim().to_string();
            if surface.is_empty() || !emit_surface_matches(&surface, target_lang, framework) {
                return None;
            }
            if let Some(emit) = plugin.emit.as_deref() {
                let emit = emit.trim();
                if emit != framework && emit != exact_surface && emit != surface {
                    return None;
                }
            }
            Some(surface)
        })
        .collect::<Vec<_>>();
    surfaces.sort();
    surfaces.dedup();
    surfaces
}

fn select_emit_candidate(
    candidates: &[EmitCandidate],
    target_lang: &str,
    framework: &str,
) -> Option<EmitCandidate> {
    let exact_surface = format!("{target_lang}-{framework}");
    candidates
        .iter()
        .find(|candidate| candidate.surface == exact_surface)
        .cloned()
        .or_else(|| {
            if candidates.len() == 1 {
                candidates.first().cloned()
            } else {
                None
            }
        })
}

fn emit_surface_matches(surface: &str, target_lang: &str, framework: &str) -> bool {
    if framework.is_empty() {
        return surface == target_lang;
    }
    surface == format!("{target_lang}-{framework}")
}

fn invoke_emit(
    target_lang: &str,
    framework: &str,
    candidate: &EmitCandidate,
    plan: &Value,
) -> Result<EmitDispatchResult, String> {
    if candidate.command.argv.is_empty() {
        return Err("empty command".to_string());
    }
    let mut command = Command::new(&candidate.command.argv[0]);
    if candidate.command.argv.len() > 1 {
        command.args(&candidate.command.argv[1..]);
    }
    if !candidate.command.argv.iter().any(|arg| arg == "--rpc") {
        command.arg("--rpc");
    }
    if let Some(wd) = &candidate.command.working_dir {
        command.current_dir(wd);
    }
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::null());

    let mut child = command
        .spawn()
        .map_err(|e| format!("spawn emit kit: {e}"))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "emit kit stdin unavailable".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "emit kit stdout unavailable".to_string())?;
    let mut reader = BufReader::new(stdout);

    let req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "provekit.plugin.invoke",
        "params": plan,
    });
    writeln!(stdin, "{req}").map_err(|e| format!("write emit request: {e}"))?;
    let result = read_response(&mut reader, 1).map_err(|e| {
        format!(
            "emit kit `{}` for {target_lang}/{framework}: {e}",
            candidate.surface
        )
    })?;

    let shutdown = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "provekit.plugin.shutdown",
    });
    let _ = writeln!(stdin, "{shutdown}");
    drop(stdin);
    let _ = child.wait();

    let source = result
        .get("source")
        .and_then(Value::as_str)
        .ok_or_else(|| "emit response missing result.source".to_string())?
        .to_string();
    Ok(EmitDispatchResult {
        surface: candidate.surface.clone(),
        source,
        result,
    })
}

fn invoke_emit_check(
    target_lang: &str,
    framework: &str,
    candidate: &EmitCandidate,
    plan: &Value,
    out_dir: &Path,
    artifact_path: &Path,
    emit_result: &Value,
) -> Result<Value, String> {
    if candidate.command.argv.is_empty() {
        return Err("empty command".to_string());
    }
    let mut command = Command::new(&candidate.command.argv[0]);
    if candidate.command.argv.len() > 1 {
        command.args(&candidate.command.argv[1..]);
    }
    if !candidate.command.argv.iter().any(|arg| arg == "--rpc") {
        command.arg("--rpc");
    }
    if let Some(wd) = &candidate.command.working_dir {
        command.current_dir(wd);
    }
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::null());

    let mut child = command
        .spawn()
        .map_err(|e| format!("spawn emit kit check: {e}"))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "emit kit stdin unavailable".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "emit kit stdout unavailable".to_string())?;
    let mut reader = BufReader::new(stdout);

    let req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "provekit.plugin.check",
        "params": {
            "plan": plan,
            "out_dir": out_dir,
            "artifact_path": artifact_path,
            "emit_result": emit_result,
        },
    });
    writeln!(stdin, "{req}").map_err(|e| format!("write emit check request: {e}"))?;
    let result = read_response(&mut reader, 1).map_err(|e| {
        format!(
            "emit kit `{}` check for {target_lang}/{framework}: {e}",
            candidate.surface
        )
    })?;

    let shutdown = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "provekit.plugin.shutdown",
    });
    let _ = writeln!(stdin, "{shutdown}");
    drop(stdin);
    let _ = child.wait();

    Ok(result)
}

/// #1375 Milestone C: one file in a target-owned compilation-unit emission.
#[derive(Debug, Clone)]
pub struct AssembledFile {
    pub path: String,
    pub content: String,
}

/// #1388: result of an assemble RPC call. Carries the emitted files AND
/// the classpath the kit declares the materialized code needs to compile.
/// The CLI aggregates this kit-owned metadata and passes it back to the
/// selected kit for materialize checks.
#[derive(Debug, Clone, Default)]
pub struct AssembleResult {
    pub files: Vec<AssembledFile>,
    pub compile_classpath: Vec<String>,
}

/// Substrate-honest error from dispatch_assemble. Carries a discriminator so
/// the caller can distinguish "plugin doesn't implement assemble" (fall back
/// to legacy concat) from "plugin errored" (surface to user).
#[derive(Debug)]
pub enum AssembleError {
    /// The plugin returned -32601 (method not found). Caller should fall
    /// back to substrate's legacy concatenation logic.
    MethodNotSupported,
    /// Something else broke. Caller should treat as a transport error.
    Failed(String),
}

/// #1375 Milestone C: route compilation-unit assembly to the target kit.
///
/// The substrate sends the kit a batch of fragments (one per @boundary site
/// in a source file) + a destination hint, and the kit decides file
/// layout (package, imports, class wrapping, helper placement).
///
/// `fragments_json` is the raw JSON array of fragment objects (one per
/// site); each entry should have at least `source`, `imports`, etc., per
/// the realization-fragment shape (#1374).
pub fn dispatch_assemble(
    workspace_root: &Path,
    target_lang: &str,
    library_tag: Option<&str>,
    fragments_json: &str,
    file_basename: &str,
    package_hint: Option<&str>,
) -> Result<AssembleResult, AssembleError> {
    let resolved = resolve_realize_command(workspace_root, target_lang, library_tag, None, None)
        .map_err(|e| AssembleError::Failed(e.detail))?;
    if resolved.argv.is_empty() {
        return Err(AssembleError::Failed("empty command".to_string()));
    }
    let mut command = Command::new(&resolved.argv[0]);
    if resolved.argv.len() > 1 {
        command.args(&resolved.argv[1..]);
    }
    if let Some(wd) = &resolved.working_dir {
        command.current_dir(wd);
    }
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::null());

    let mut child = command
        .spawn()
        .map_err(|e| AssembleError::Failed(format!("spawn assemble kit: {e}")))?;

    let fragments_value: Value = serde_json::from_str(fragments_json)
        .map_err(|e| AssembleError::Failed(format!("fragments_json not valid JSON: {e}")))?;
    let mut params = serde_json::Map::new();
    params.insert(
        "target_lang".to_string(),
        Value::String(target_lang.to_string()),
    );
    params.insert(
        "file_basename".to_string(),
        Value::String(file_basename.to_string()),
    );
    if let Some(pkg) = package_hint {
        params.insert("package_hint".to_string(), Value::String(pkg.to_string()));
    }
    params.insert("fragments".to_string(), fragments_value);

    let req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "provekit.plugin.assemble",
        "params": Value::Object(params),
    });

    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| AssembleError::Failed("assemble kit stdin unavailable".to_string()))?;
        let req_str = serde_json::to_string(&req).expect("serialize assemble request");
        stdin
            .write_all(req_str.as_bytes())
            .and_then(|()| stdin.write_all(b"\n"))
            .map_err(|e| AssembleError::Failed(format!("write assemble request: {e}")))?;
    }

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AssembleError::Failed("assemble kit stdout unavailable".to_string()))?;
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|e| AssembleError::Failed(format!("read assemble response: {e}")))?;
    let _ = child.kill();
    let _ = child.wait();

    let v: Value = serde_json::from_str(line.trim())
        .map_err(|e| AssembleError::Failed(format!("assemble response not valid JSON: {e}")))?;
    if let Some(err) = v.get("error") {
        // -32601 is JSON-RPC's "method not found" — legacy kits without
        // the assemble RPC return this. Caller falls back to legacy concat.
        let code = err.get("code").and_then(Value::as_i64).unwrap_or(0);
        if code == -32601 {
            return Err(AssembleError::MethodNotSupported);
        }
        return Err(AssembleError::Failed(format!("assemble kit error: {err}")));
    }
    let result = v
        .get("result")
        .ok_or_else(|| AssembleError::Failed("assemble response missing result".to_string()))?;
    let files_arr = result
        .get("files")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            AssembleError::Failed("assemble response missing result.files".to_string())
        })?;
    let mut out = Vec::with_capacity(files_arr.len());
    for f in files_arr {
        let path = f
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| AssembleError::Failed("assemble file missing path".to_string()))?
            .to_string();
        let content = f
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(|| AssembleError::Failed("assemble file missing content".to_string()))?
            .to_string();
        out.push(AssembledFile { path, content });
    }
    // #1388: collect classpath the kit declares its emitted code needs.
    let compile_classpath: Vec<String> = result
        .get("compile_classpath")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();
    Ok(AssembleResult {
        files: out,
        compile_classpath,
    })
}

/// Ask the selected realize kit to check materialized output. The CLI owns
/// dispatch only; compiler/test/build semantics stay behind the kit RPC seam.
pub fn dispatch_materialize_check(
    workspace_root: &Path,
    target_lang: &str,
    library_tag: Option<&str>,
    out_dir: &Path,
    compile_classpath: &[String],
) -> Result<Value, String> {
    let resolved = resolve_realize_command(workspace_root, target_lang, library_tag, None, None)
        .map_err(|e| e.detail)?;
    if resolved.argv.is_empty() {
        return Err("empty command".to_string());
    }
    let mut command = Command::new(&resolved.argv[0]);
    if resolved.argv.len() > 1 {
        command.args(&resolved.argv[1..]);
    }
    if let Some(wd) = &resolved.working_dir {
        command.current_dir(wd);
    }
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::null());

    let mut child = command
        .spawn()
        .map_err(|e| format!("spawn materialize check kit: {e}"))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "materialize check kit stdin unavailable".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "materialize check kit stdout unavailable".to_string())?;
    let mut reader = BufReader::new(stdout);

    let req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "provekit.plugin.check",
        "params": {
            "kind": "materialize",
            "target_lang": target_lang,
            "target_library_tag": library_tag.unwrap_or(DEFAULT_LIBRARY_TAG),
            "out_dir": out_dir,
            "compile_classpath": compile_classpath,
        },
    });
    writeln!(stdin, "{req}").map_err(|e| format!("write materialize check request: {e}"))?;
    let result = read_response(&mut reader, 1).map_err(|e| {
        format!(
            "materialize check kit for {target_lang}/{}: {e}",
            library_tag.unwrap_or(DEFAULT_LIBRARY_TAG)
        )
    })?;

    let shutdown = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "provekit.plugin.shutdown",
    });
    let _ = writeln!(stdin, "{shutdown}");
    drop(stdin);
    let _ = child.wait();

    Ok(result)
}

// Per #1270 Tier 1.4: configure_java_runtime + java_home_from_maven removed.
// JVM-runtime discovery is the user's environment concern (set JAVA_HOME),
// not the kit-agnostic dispatcher's job. The dispatcher does not know that
// "java" is a runtime invocation; it just executes whatever the kit's
// manifest declares as its launcher.

// ============================================================================
// Exam manifest dispatch (PEP 1.7.0 kind = "exam-manifest")
// ============================================================================

const EXAM_MANIFEST_KIND: &str = "exam-manifest";
const EXAM_MANIFEST_SCHEMA_VERSION: &str = "provekit-exam-manifest/v1.1";
const EXAM_MANIFEST_SCHEMA_VERSION_V1: &str = "provekit-exam-manifest/v1";
const PEP_1_7_0: &str = "pep/1.7.0";
pub const DEFAULT_EXAM_MANIFEST_CID: &str = "blake3-512:b38426ba10ee3a6c28e9e32cae9aa65cfb5b750950464d1e67e9d669956bd40288d25c247d0ec2d638fd63e2d235d944f419055c0374c78488b4be98da040451";
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
    if local.header.cid == remote.header.cid {
        return Ok(());
    }

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
    match registry_exam_manifest_command(workspace_root, plugin_name) {
        Ok(Some(command)) => return Ok(Some(command)),
        Ok(None) => record_fallback_diagnostic(EXAM_MANIFEST_KIND, plugin_name),
        Err(detail) => {
            return Err(KitUnavailable {
                kit_kind: EXAM_MANIFEST_KIND,
                language: plugin_name.to_string(),
                detail,
            })
        }
    }

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
    Ok(Some(resolved_command_from_manifest(
        workspace_root,
        &parsed,
    )))
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
    if parsed.exam_manifest_schema_version.as_deref() != Some(EXAM_MANIFEST_SCHEMA_VERSION)
        && parsed.exam_manifest_schema_version.as_deref() != Some(EXAM_MANIFEST_SCHEMA_VERSION_V1)
    {
        return Err(format!(
            "manifest {} must declare [capabilities].exam_manifest_schema_version = \"{}\" or \"{}\"",
            manifest.display(),
            EXAM_MANIFEST_SCHEMA_VERSION,
            EXAM_MANIFEST_SCHEMA_VERSION_V1
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
// Emit witness dispatch (ORP witness emitter)
// ============================================================================

pub fn dispatch_emit_witness(
    workspace_root: &Path,
    surface: &str,
    plan: &Value,
) -> Result<Value, String> {
    let resolved = resolve_emit_surface_command(workspace_root, surface)?;
    rpc_emit_witness(workspace_root, surface, &resolved, plan)
}

fn resolve_emit_surface_command(
    workspace_root: &Path,
    surface: &str,
) -> Result<ResolvedCommand, String> {
    let configured = configured_emit_surface_names(workspace_root);
    if !configured.contains(surface) {
        return Err(format!(
            "no emit plugin registration for surface `{surface}` in .provekit/config.toml"
        ));
    }

    let registry = run_plugin_registry_for_project(workspace_root)?;
    if let Some(plugin) = registry
        .plugins
        .iter()
        .filter(|plugin| registry_authorizes_plugin(&registry, plugin))
        .find(|plugin| plugin.kind == "emit" && plugin.surface == surface)
    {
        return Ok(resolved_command_from_manifest(
            workspace_root,
            &plugin.parsed,
        ));
    }

    record_fallback_diagnostic("emit", surface);
    let manifest = workspace_root
        .join(".provekit")
        .join("emit")
        .join(surface)
        .join("manifest.toml");
    if !manifest.exists() {
        return Err(format!(
            "no emit plugin for surface `{surface}`; expected {}",
            manifest.display()
        ));
    }
    let parsed = parse_manifest(&manifest)?;
    Ok(resolved_command_from_manifest(workspace_root, &parsed))
}

fn rpc_emit_witness(
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
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|e| format!("spawn emit kit: {e}"))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or("emit kit stdin unavailable".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or("emit kit stdout unavailable".to_string())?;
    let stderr = child.stderr.take();
    let stderr_handle = stderr.map(|mut h| {
        std::thread::spawn(move || {
            let mut buf = String::new();
            let _ = std::io::Read::read_to_string(&mut h, &mut buf);
            buf
        })
    });
    let mut reader = BufReader::new(stdout);

    let emit_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "provekit.plugin.invoke",
        "params": {
            "surface": surface,
            "workspace_root": workspace_root.display().to_string(),
            "plan": plan
        }
    });
    writeln!(stdin, "{emit_req}").map_err(|e| format!("write emit witness request: {e}"))?;
    let response = read_response(&mut reader, 1);
    if let Err(message) = &response {
        let _ = writeln!(
            stdin,
            "{{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"provekit.plugin.shutdown\"}}"
        );
        drop(stdin);
        let _ = child.wait();
        let stderr_text = stderr_handle
            .and_then(|h| h.join().ok())
            .unwrap_or_default();
        return Err(if stderr_text.is_empty() {
            message.clone()
        } else {
            format!("{message}\nemit kit stderr:\n{stderr_text}")
        });
    }
    let response = response?;

    let shutdown_req = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "provekit.plugin.shutdown"
    });
    let _ = writeln!(stdin, "{shutdown_req}");
    drop(stdin);
    let _ = child.wait();
    let _ = stderr_handle.and_then(|h| h.join().ok());
    Ok(response)
}

fn realize_request_params(request: &RealizeRequest) -> Value {
    serde_json::to_value(request).expect("serialize realize request params")
}

/// Filesystem-level extension fallback. NOT language semantics: cmd_transport
/// and cmd_bind use whatever the realize kit emits in `result.extension`
/// (per `body-template-memento.md` §3.2). The kit is the authority on its
/// own file extension; the dispatcher only sees a fallback when the kit
/// elides the field.
///
/// Substrate-uniform convention: extension = lang identifier. Kits whose
/// language name differs from the file extension (e.g., python -> "py",
/// typescript -> "ts", rust -> "rs") MUST declare `extension` in their
/// realize RPC response. The dispatcher does not enumerate kits.
///
/// Per #1270 Tier 1.3: removed the hardcoded `match` table that listed
/// "python", "ruby", "typescript", "csharp", "rust" extensions. Built-in
/// kits (rust, python, typescript) already declare `extension` in their
/// realize responses, so the table was dead code masking the substrate
/// violation.
fn extension_from_convention(lang: &str) -> String {
    lang.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;

    // -----------------------------------------------------------------
    // #1359 / #1355: realize manifest gains family + library_version
    // -----------------------------------------------------------------

    // -----------------------------------------------------------------
    // #1360 / #1355: scope_bringings on realize manifest (case-1
    // effect propagation — use/import items the materialized body needs
    // in the consumer crate's prelude). Parsed here; cmd_materialize
    // reads them at splice time and hoists into the target file.
    // -----------------------------------------------------------------

    // -----------------------------------------------------------------
    // #1364 / #1355: provides_concepts declaration on realize manifests
    // (per-kit concept coverage — chunk 1 plumbs the parse; chunk 2
    // wires defensive checks in cmd_materialize; chunk 3+ populates per
    // kit).
    // -----------------------------------------------------------------

    #[test]
    fn parse_manifest_accepts_provides_concepts_array() {
        let dir = std::env::temp_dir().join("provekit-test-1364-provides");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        let manifest_path = dir.join("manifest.toml");
        fs::write(
            &manifest_path,
            r#"
name = "rust-realize-shim-blake3"
library_tag = "blake3"
provides_concepts = ["concept:blake3-512-of", "concept:blake3-hasher-new", "concept:blake3-hasher-update", "concept:blake3-hasher-finalize-xof-64"]
command = ["/bin/true"]
"#,
        )
        .expect("write");
        let parsed = parse_manifest(&manifest_path).expect("parse");
        assert_eq!(parsed.provides_concepts.len(), 4);
        assert!(parsed
            .provides_concepts
            .iter()
            .any(|c| c == "concept:blake3-512-of"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_manifest_without_provides_concepts_yields_empty_vec() {
        let dir = std::env::temp_dir().join("provekit-test-1364-noprovides");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        let manifest_path = dir.join("manifest.toml");
        fs::write(
            &manifest_path,
            r#"
name = "rust-realize-default"
library_tag = "default"
command = ["/bin/true"]
"#,
        )
        .expect("write");
        let parsed = parse_manifest(&manifest_path).expect("parse");
        assert!(parsed.provides_concepts.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_manifest_accepts_scope_bringings_array() {
        let dir = std::env::temp_dir().join("provekit-test-1360-scope");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        let manifest_path = dir.join("manifest.toml");
        fs::write(
            &manifest_path,
            r#"
name = "rust-realize-shim-stdio"
library_tag = "provekit-shim-stdio-rust"
family = "concept:family:stdio-stream"
scope_bringings = ["use std::io::{self, BufRead, Write};"]
command = ["/bin/true"]
"#,
        )
        .expect("write");
        let parsed = parse_manifest(&manifest_path).expect("parse");
        assert_eq!(parsed.scope_bringings.len(), 1);
        assert_eq!(
            parsed.scope_bringings[0],
            "use std::io::{self, BufRead, Write};"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_manifest_without_scope_bringings_yields_empty_vec() {
        let dir = std::env::temp_dir().join("provekit-test-1360-no-scope");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        let manifest_path = dir.join("manifest.toml");
        fs::write(
            &manifest_path,
            r#"
name = "rust-realize-default"
library_tag = "default"
command = ["/bin/true"]
"#,
        )
        .expect("write");
        let parsed = parse_manifest(&manifest_path).expect("parse");
        assert!(parsed.scope_bringings.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_manifest_accepts_family_and_library_version_pins() {
        let dir = std::env::temp_dir().join("provekit-test-1359-family-version");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        let manifest_path = dir.join("manifest.toml");
        fs::write(
            &manifest_path,
            r#"
name = "rust-realize-rusqlite"
library_tag = "rusqlite"
family = "concept:family:sql"
library_version = "0.39.0"
command = ["/bin/true"]
"#,
        )
        .expect("write");
        let parsed = parse_manifest(&manifest_path).expect("parse");
        assert_eq!(parsed.library_tag.as_deref(), Some("rusqlite"));
        assert_eq!(parsed.family.as_deref(), Some("concept:family:sql"));
        assert_eq!(parsed.library_version.as_deref(), Some("0.39.0"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_manifest_without_family_and_version_parses_clean() {
        // Back-compat: existing manifests without the new axes still parse.
        let dir = std::env::temp_dir().join("provekit-test-1359-noaxes");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        let manifest_path = dir.join("manifest.toml");
        fs::write(
            &manifest_path,
            r#"
name = "rust-realize-default"
library_tag = "default"
command = ["/bin/true"]
"#,
        )
        .expect("write");
        let parsed = parse_manifest(&manifest_path).expect("parse");
        assert!(parsed.family.is_none());
        assert!(parsed.library_version.is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    // -----------------------------------------------------------------
    // #1359 chunk 2: family-aware candidate query
    // -----------------------------------------------------------------

    fn make_candidate(
        tag: &str,
        family: Option<&str>,
        library_version: Option<&str>,
    ) -> RealizeCandidate {
        RealizeCandidate {
            tag: tag.to_string(),
            command: ResolvedCommand {
                argv: vec!["/bin/true".to_string()],
                working_dir: None,
            },
            source: format!("test-fixture:{tag}"),
            family: family.map(str::to_string),
            library_version: library_version.map(str::to_string),
        }
    }

    fn filter_candidates(
        candidates: Vec<RealizeCandidate>,
        family: Option<&str>,
        library_tag: Option<&str>,
        library_version: Option<&str>,
    ) -> Vec<RealizeCandidate> {
        // Mirror the filter chain in find_realize_candidates_for_constraints
        // without the registry-load step, so we can unit-test the matching
        // logic against synthetic candidates.
        candidates
            .into_iter()
            .filter(|c| match family {
                Some(want) => c.family.as_deref() == Some(want),
                None => true,
            })
            .filter(|c| match library_tag {
                Some(want) => c.tag == want,
                None => true,
            })
            .filter(|c| match library_version {
                Some(want) => c.library_version.as_deref() == Some(want),
                None => true,
            })
            .collect()
    }

    #[test]
    fn family_constraint_filters_candidates_to_same_family() {
        let candidates = vec![
            make_candidate("rusqlite", Some("concept:family:sql"), Some("0.39.0")),
            make_candidate("postgres-rs", Some("concept:family:sql"), Some("0.19")),
            make_candidate("blake3", Some("concept:family:hash"), Some("1")),
        ];
        let matches = filter_candidates(candidates, Some("concept:family:sql"), None, None);
        assert_eq!(matches.len(), 2);
        let tags: Vec<_> = matches.iter().map(|c| c.tag.as_str()).collect();
        assert!(tags.contains(&"rusqlite"));
        assert!(tags.contains(&"postgres-rs"));
    }

    #[test]
    fn family_plus_library_tag_narrows_to_one_when_both_pinned() {
        let candidates = vec![
            make_candidate("rusqlite", Some("concept:family:sql"), Some("0.39.0")),
            make_candidate("postgres-rs", Some("concept:family:sql"), Some("0.19")),
        ];
        let matches = filter_candidates(
            candidates,
            Some("concept:family:sql"),
            Some("rusqlite"),
            None,
        );
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tag, "rusqlite");
    }

    #[test]
    fn version_constraint_excludes_non_matching_version() {
        let candidates = vec![
            make_candidate("rusqlite", Some("concept:family:sql"), Some("0.39.0")),
            make_candidate("rusqlite", Some("concept:family:sql"), Some("0.40.0-rc1")),
        ];
        let matches = filter_candidates(candidates, None, Some("rusqlite"), Some("0.39.0"));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].library_version.as_deref(), Some("0.39.0"));
    }

    #[test]
    fn floating_family_yields_all_candidates_when_other_axes_unconstrained() {
        let candidates = vec![
            make_candidate("rusqlite", Some("concept:family:sql"), Some("0.39.0")),
            make_candidate("blake3", Some("concept:family:hash"), Some("1")),
        ];
        let matches = filter_candidates(candidates, None, None, None);
        assert_eq!(
            matches.len(),
            2,
            "all candidates pass when no constraints pinned"
        );
    }

    #[test]
    fn family_constraint_excludes_candidate_with_no_declared_family() {
        // Substrate-honest: if the CONSUMER pins family, but a candidate
        // doesn't declare one, the candidate cannot prove it's in that
        // family. Exclude it (don't silently default to "anything matches").
        let candidates = vec![
            make_candidate("default", None, None),
            make_candidate("rusqlite", Some("concept:family:sql"), Some("0.39.0")),
        ];
        let matches = filter_candidates(candidates, Some("concept:family:sql"), None, None);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tag, "rusqlite");
    }

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
