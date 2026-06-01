// SPDX-License-Identifier: Apache-2.0
//
// provekit doctor: validate a kit's config/manifest wiring up front.
//
// Catches the manifest-path footgun (a declared plugin command pointing at a
// binary that does not exist) BEFORE it silently produces an empty-set
// attestation. Every failure is loud, named, and counted. No language
// semantics embedded here: doctor is language-blind and validates generic kit
// wiring only.
//
// Checks performed:
//   1. (HARD) config.toml and all manifest TOML files parse as valid TOML.
//   2. (HARD) For each declared plugin command, the binary EXISTS and is
//      executable, resolved the same way mint does: relative to the plugin's
//      working dir when the command contains a path separator; via PATH when
//      the command is a bare name.
//   3. (WARN) .provekit/imports/ file count -- zero is a warning when the kit
//      declares plugins with a non-trivial surface list.
//   4. (WARN) When PROVEKIT_RESOLVE_ORACLE=rust-analyzer, check that the
//      rust-analyzer binary is locatable and PROVEKIT_LINKERD_BIN or
//      provekit-linkerd is on PATH.
//
// Exit codes:
//   0   all checks passed (warnings may be present)
//   2   at least one HARD check failed

use std::path::{Path, PathBuf};

use clap::ValueEnum;
use serde_json::{json, Value};

use crate::lift_plugin::{parse_manifest_at, resolved_working_dir_for};
use crate::project_config::{read_project_config, PluginEntry};

/// Status of one doctor check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
}

impl CheckStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            CheckStatus::Pass => "pass",
            CheckStatus::Warn => "warn",
            CheckStatus::Fail => "fail",
        }
    }
}

/// Severity of one doctor check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckSeverity {
    Advisory,
    Hard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum DoctorMode {
    Structural,
    Strict,
}

impl DoctorMode {
    pub fn as_str(self) -> &'static str {
        match self {
            DoctorMode::Structural => "structural",
            DoctorMode::Strict => "strict",
        }
    }
}

impl std::fmt::Display for DoctorMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DoctorContext {
    pub mode: DoctorMode,
}

impl DoctorContext {
    pub fn new(mode: DoctorMode) -> Self {
        Self { mode }
    }
}

impl Default for DoctorContext {
    fn default() -> Self {
        Self::new(DoctorMode::Structural)
    }
}

/// One check result.
#[derive(Debug, Clone)]
pub struct DoctorCheck {
    pub id: String,
    pub name: String,
    pub status: CheckStatus,
    pub severity: CheckSeverity,
    pub domain: String,
    pub detail: String,
    pub evidence: Value,
}

pub type Check = DoctorCheck;

#[derive(Debug, Clone)]
pub struct DoctorReport {
    pub target: PathBuf,
    pub mode: DoctorMode,
    pub checks: Vec<DoctorCheck>,
    pub ok: bool,
}

impl DoctorCheck {
    fn pass_with_evidence(
        name: impl Into<String>,
        detail: impl Into<String>,
        evidence: Value,
    ) -> Self {
        Self::with_status(name, CheckStatus::Pass, detail, evidence)
    }
    fn warn_with_evidence(
        name: impl Into<String>,
        detail: impl Into<String>,
        evidence: Value,
    ) -> Self {
        Self::with_status(name, CheckStatus::Warn, detail, evidence)
    }
    fn fail_with_evidence(
        name: impl Into<String>,
        detail: impl Into<String>,
        evidence: Value,
    ) -> Self {
        Self::with_status(name, CheckStatus::Fail, detail, evidence)
    }
    fn with_status(
        name: impl Into<String>,
        status: CheckStatus,
        detail: impl Into<String>,
        evidence: Value,
    ) -> Self {
        let name = name.into();
        let detail = detail.into();
        let (id, domain, severity, evidence) = check_metadata(&name, &detail, evidence);
        Self {
            id,
            name,
            status,
            severity,
            domain,
            detail,
            evidence,
        }
    }
}

fn check_metadata(
    name: &str,
    detail: &str,
    evidence: Value,
) -> (String, String, CheckSeverity, Value) {
    let (id, domain, severity) = if name == "config-toml-parse" {
        ("kit.config.parse", "kit.config", CheckSeverity::Hard)
    } else if name.starts_with("manifest-toml-parse:") {
        ("kit.manifest.parse", "kit.manifest", CheckSeverity::Hard)
    } else if name.starts_with("binary-exists:") {
        (
            "kit.plugin.command.available",
            "kit.plugin",
            CheckSeverity::Hard,
        )
    } else if name == "imports-present" {
        (
            "proof.import_pool.present",
            "proof.import_pool",
            CheckSeverity::Advisory,
        )
    } else if name == "oracle-wiring" {
        (
            "oracle.host.locatable",
            "oracle.host",
            CheckSeverity::Advisory,
        )
    } else if name.starts_with("consumer-wiring:") {
        (
            "kit.consumer_surface.contract",
            "kit.consumer_surface",
            CheckSeverity::Hard,
        )
    } else {
        ("doctor.check", "doctor", CheckSeverity::Hard)
    };
    let mut evidence = if evidence.is_null() {
        json!({ "detail": detail })
    } else {
        evidence
    };
    if let Some(obj) = evidence.as_object_mut() {
        obj.insert("legacyName".to_string(), Value::String(name.to_string()));
    }
    (id.to_string(), domain.to_string(), severity, evidence)
}

pub fn run_report(kit_dir: &Path) -> DoctorReport {
    let checks = run_checks(kit_dir);
    let ok = !checks.iter().any(|c| c.status == CheckStatus::Fail);
    DoctorReport {
        target: kit_dir.to_path_buf(),
        mode: DoctorMode::Structural,
        checks,
        ok,
    }
}

pub fn run_report_with_context(kit_dir: &Path, context: DoctorContext) -> DoctorReport {
    if context.mode == DoctorMode::Structural {
        return run_report(kit_dir);
    }
    let checks = run_checks_with_context(kit_dir, context);
    let ok = !checks.iter().any(|c| c.status == CheckStatus::Fail);
    DoctorReport {
        target: kit_dir.to_path_buf(),
        mode: context.mode,
        checks,
        ok,
    }
}

/// Run all checks against the target kit directory.
/// Pure function: suitable for testing without CLI overhead.
pub fn run_checks(kit_dir: &Path) -> Vec<Check> {
    run_checks_with_context(kit_dir, DoctorContext::default())
}

pub fn run_checks_with_context(kit_dir: &Path, context: DoctorContext) -> Vec<Check> {
    let _mode = context.mode;
    let mut checks: Vec<Check> = Vec::new();

    // --- Check 1: config.toml and all manifest TOML files parse as valid TOML.
    let config_path = kit_dir.join(".provekit/config.toml");
    match std::fs::read_to_string(&config_path) {
        Err(e) => {
            checks.push(Check::fail_with_evidence(
                "config-toml-parse",
                format!("cannot read {}: {e}", config_path.display()),
                json!({"path": config_path.display().to_string()}),
            ));
            // Cannot enumerate plugins without a config; stop here.
            return checks;
        }
        Ok(text) => match text.parse::<toml::Value>() {
            Err(e) => {
                checks.push(Check::fail_with_evidence(
                    "config-toml-parse",
                    format!("{}: invalid TOML: {e}", config_path.display()),
                    json!({"path": config_path.display().to_string()}),
                ));
                // Config invalid; cannot enumerate plugins.
                return checks;
            }
            Ok(_) => {
                checks.push(Check::pass_with_evidence(
                    "config-toml-parse",
                    format!("{} parses as valid TOML", config_path.display()),
                    json!({"path": config_path.display().to_string()}),
                ));
            }
        },
    }

    // Load the structured config for plugin enumeration.
    let config = read_project_config(kit_dir);

    // Enumerate manifest TOML files for all declared plugins (check 1 continued).
    // Map: (surface, kind) -> manifest dir name ("lift", "realize", "emit").
    let manifest_entries = collect_manifest_entries(kit_dir, &config.plugins);
    for (surface, kind_dir, manifest_path) in &manifest_entries {
        let check_name = format!("manifest-toml-parse:{kind_dir}:{surface}");
        match std::fs::read_to_string(manifest_path) {
            Err(e) => {
                checks.push(Check::fail_with_evidence(
                    &check_name,
                    format!("cannot read {}: {e}", manifest_path.display()),
                    json!({
                        "kind": kind_dir,
                        "surface": surface,
                        "path": manifest_path.display().to_string(),
                    }),
                ));
            }
            Ok(text) => match text.parse::<toml::Value>() {
                Err(e) => {
                    checks.push(Check::fail_with_evidence(
                        &check_name,
                        format!("{}: invalid TOML: {e}", manifest_path.display()),
                        json!({
                            "kind": kind_dir,
                            "surface": surface,
                            "path": manifest_path.display().to_string(),
                        }),
                    ));
                }
                Ok(_) => {
                    checks.push(Check::pass_with_evidence(
                        &check_name,
                        format!("{} parses as valid TOML", manifest_path.display()),
                        json!({
                            "kind": kind_dir,
                            "surface": surface,
                            "path": manifest_path.display().to_string(),
                        }),
                    ));
                }
            },
        }
    }

    // --- Check 2: binary existence for each declared plugin.
    for (surface, kind_dir, manifest_path) in &manifest_entries {
        let check_name = format!("binary-exists:{kind_dir}:{surface}");
        let manifest = match parse_manifest_at(manifest_path) {
            Err(e) => {
                checks.push(Check::fail_with_evidence(
                    &check_name,
                    format!("cannot parse manifest: {e}"),
                    json!({
                        "kind": kind_dir,
                        "surface": surface,
                        "path": manifest_path.display().to_string(),
                    }),
                ));
                continue;
            }
            Ok(m) => m,
        };
        if manifest.command.is_empty() {
            checks.push(Check::fail_with_evidence(
                &check_name,
                format!(
                    "manifest {} declares no command",
                    manifest_path.display()
                ),
                json!({
                    "kind": kind_dir,
                    "surface": surface,
                    "path": manifest_path.display().to_string(),
                }),
            ));
            continue;
        }

        let cmd0 = &manifest.command[0];
        let resolved_wd = resolved_working_dir_for(kit_dir, &manifest);

        match resolve_binary(cmd0, resolved_wd.as_deref()) {
            BinaryResolution::Found(abs) => {
                checks.push(Check::pass_with_evidence(
                    &check_name,
                    format!(
                        "surface={surface} command={cmd0:?} -> {} (executable)",
                        abs.display()
                    ),
                    json!({
                        "kind": kind_dir,
                        "surface": surface,
                        "path": manifest_path.display().to_string(),
                        "command": cmd0,
                        "resolvedPath": abs.display().to_string(),
                    }),
                ));
            }
            BinaryResolution::NotFound { resolved_path } => {
                let fix = binary_fix_hint(cmd0, resolved_wd.as_deref(), kit_dir);
                checks.push(Check::fail_with_evidence(
                    &check_name,
                    format!(
                        "surface={surface} command={cmd0:?}: binary not found at {}. {fix}",
                        resolved_path
                    ),
                    json!({
                        "kind": kind_dir,
                        "surface": surface,
                        "path": manifest_path.display().to_string(),
                        "command": cmd0,
                        "resolvedPath": resolved_path,
                    }),
                ));
            }
            BinaryResolution::NotExecutable { abs } => {
                checks.push(Check::fail_with_evidence(
                    &check_name,
                    format!(
                        "surface={surface} command={cmd0:?}: file exists at {} but is not executable",
                        abs.display()
                    ),
                    json!({
                        "kind": kind_dir,
                        "surface": surface,
                        "path": manifest_path.display().to_string(),
                        "command": cmd0,
                        "resolvedPath": abs.display().to_string(),
                    }),
                ));
            }
        }
    }

    // --- Check 3: .provekit/imports/ file count (advisory).
    let imports_dir = kit_dir.join(".provekit/imports");
    let imports_check = check_imports(&imports_dir, &config.plugins);
    checks.push(imports_check);

    // --- Check 4: oracle wiring (advisory).
    if let Some(oracle_check) = check_oracle_wiring() {
        checks.push(oracle_check);
    }

    // --- Check 5: consumer-surface method/phase wiring (HARD).
    // The manifest method/phase omission footgun: a consumer surface
    // (e.g. rust-implications) whose manifest omits `method`/`phase` silently
    // runs the default `lift` PRODUCER method, so its pass never fires and the
    // mint emits a degenerate attestation with no error. Five investigations
    // on 2026-05-31 lost a day to exactly this. The plugin SELF-DECLARES which
    // surfaces are consumers and what (method, phase) they require (via the
    // `initialize` capabilities) so this stays language-blind: doctor reads the
    // requirement from the kit's own plugin, not a hard-coded CLI table.
    for (surface, kind_dir, manifest_path) in &manifest_entries {
        let check_name = format!("consumer-wiring:{kind_dir}:{surface}");
        let manifest = match parse_manifest_at(manifest_path) {
            Ok(m) => m,
            Err(_) => continue, // already reported by checks 1/2
        };
        let resolved_wd = resolved_working_dir_for(kit_dir, &manifest);
        let consumer_reqs = match plugin_consumer_surfaces(&manifest, resolved_wd.as_deref()) {
            Ok(c) => c,
            Err(e) => {
                checks.push(Check::warn_with_evidence(
                    &check_name,
                    format!("could not query plugin capabilities for {surface}: {e}"),
                    json!({
                        "kind": kind_dir,
                        "surface": surface,
                        "path": manifest_path.display().to_string(),
                    }),
                ));
                continue;
            }
        };
        let Some(req) = consumer_reqs.get(surface.as_str()) else {
            // Not a consumer surface per the plugin: nothing to enforce.
            continue;
        };
        let method_ok = manifest.method.as_deref() == Some(req.0.as_str());
        let phase_ok = manifest.phase.as_deref() == Some(req.1.as_str());
        if method_ok && phase_ok {
            checks.push(Check::pass_with_evidence(
                &check_name,
                format!(
                    "consumer surface {surface} correctly wired (method={}, phase={})",
                    req.0, req.1
                ),
                json!({
                    "kind": kind_dir,
                    "surface": surface,
                    "path": manifest_path.display().to_string(),
                    "method": manifest.method,
                    "phase": manifest.phase,
                    "requiredMethod": req.0,
                    "requiredPhase": req.1,
                }),
            ));
        } else {
            checks.push(Check::fail_with_evidence(
                &check_name,
                format!(
                    "consumer surface {surface} is mis-wired: manifest {} has method={:?} phase={:?} \
                     but the plugin requires method=\"{}\" phase=\"{}\". Without these the surface \
                     silently runs the default `lift` producer and its pass never fires (degenerate \
                     attestation, no error). Add both lines to the manifest.",
                    manifest_path.display(),
                    manifest.method,
                    manifest.phase,
                    req.0,
                    req.1,
                ),
                json!({
                    "kind": kind_dir,
                    "surface": surface,
                    "path": manifest_path.display().to_string(),
                    "method": manifest.method,
                    "phase": manifest.phase,
                    "requiredMethod": req.0,
                    "requiredPhase": req.1,
                }),
            ));
        }
    }

    checks
}

/// Query a plugin's `initialize` capabilities and return its declared
/// consumer-surface requirements: `surface -> (required_method, required_phase)`.
/// Language-blind: the requirement is the PLUGIN's self-declaration, not a CLI
/// table. Spawns the plugin command, sends one `initialize` JSON-RPC line, reads
/// one response line, and parses `result.capabilities.consumer_surfaces`.
fn plugin_consumer_surfaces(
    manifest: &crate::lift_plugin::LiftPluginManifest,
    working_dir: Option<&Path>,
) -> Result<std::collections::HashMap<String, (String, String)>, String> {
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Command, Stdio};

    if manifest.command.is_empty() {
        return Err("manifest declares no command".into());
    }
    let mut cmd = Command::new(&manifest.command[0]);
    cmd.args(&manifest.command[1..]);
    if let Some(wd) = working_dir {
        cmd.current_dir(wd);
    }
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = cmd.spawn().map_err(|e| format!("spawn: {e}"))?;
    {
        let stdin = child.stdin.as_mut().ok_or("no stdin")?;
        let req = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}});
        let mut line = serde_json::to_vec(&req).map_err(|e| e.to_string())?;
        line.push(b'\n');
        stdin.write_all(&line).map_err(|e| format!("write: {e}"))?;
    }
    let stdout = child.stdout.take().ok_or("no stdout")?;
    let mut reader = BufReader::new(stdout);
    let mut resp_line = String::new();
    reader
        .read_line(&mut resp_line)
        .map_err(|e| format!("read: {e}"))?;
    let _ = child.kill();
    let _ = child.wait();
    let resp: Value =
        serde_json::from_str(resp_line.trim()).map_err(|e| format!("parse response: {e}"))?;
    let mut out = std::collections::HashMap::new();
    if let Some(map) = resp
        .pointer("/result/capabilities/consumer_surfaces")
        .and_then(|v| v.as_object())
    {
        for (surface, req) in map {
            let method = req.get("method").and_then(|v| v.as_str());
            let phase = req.get("phase").and_then(|v| v.as_str());
            if let (Some(m), Some(p)) = (method, phase) {
                out.insert(surface.clone(), (m.to_string(), p.to_string()));
            }
        }
    }
    Ok(out)
}

/// Collect all (surface, kind_dir, manifest_path) triples for declared plugins.
/// Manifests live at .provekit/<kind_dir>/<surface>/manifest.toml.
fn collect_manifest_entries(
    kit_dir: &Path,
    plugins: &[PluginEntry],
) -> Vec<(String, String, PathBuf)> {
    let mut entries = Vec::new();
    for plugin in plugins {
        let kind_dir = plugin_kind_dir(plugin);
        let manifest_path = kit_dir
            .join(".provekit")
            .join(&kind_dir)
            .join(&plugin.surface)
            .join("manifest.toml");
        entries.push((plugin.surface.clone(), kind_dir, manifest_path));
    }
    entries
}

/// Map a PluginEntry's declared kind to the manifest subdirectory name.
///
/// NOTE: `is_lift_plugin()` and `is_emit_plugin()` both return `true` when
/// `kind` is absent (legacy dual-use registration). Manifests for
/// legacy/lift plugins live under `.provekit/lift/`. Only an EXPLICIT
/// `kind = "emit"` or `kind = "realize"` should redirect to a different dir.
fn plugin_kind_dir(plugin: &PluginEntry) -> String {
    match plugin.kind.as_deref() {
        Some(k) if k.eq_ignore_ascii_case("realize") => "realize".to_string(),
        Some(k) if k.eq_ignore_ascii_case("emit") => "emit".to_string(),
        // Explicit "lift" or absent (legacy dual-use) -> lift dir.
        _ => "lift".to_string(),
    }
}

/// How a binary path is resolved: same logic as spawn in LiftPluginKit.
/// If command[0] contains a path separator, it is joined to the working dir
/// (the OS treats it as a relative path, not a PATH search).
/// If command[0] is a bare name (no separator), the OS PATH-searches it.
enum BinaryResolution {
    Found(PathBuf),
    NotFound { resolved_path: String },
    NotExecutable { abs: PathBuf },
}

fn resolve_binary(cmd0: &str, working_dir: Option<&Path>) -> BinaryResolution {
    if cmd0.contains('/') || std::path::Path::new(cmd0).is_absolute() {
        // Relative or absolute path: resolve against working_dir (fallback: cwd).
        let base = working_dir
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        let abs = if Path::new(cmd0).is_absolute() {
            PathBuf::from(cmd0)
        } else {
            base.join(cmd0)
        };
        // Canonicalize to resolve `..` components cleanly for reporting.
        let canonical = abs.canonicalize().unwrap_or_else(|_| abs.clone());
        if !canonical.exists() {
            return BinaryResolution::NotFound {
                resolved_path: canonical.display().to_string(),
            };
        }
        if !is_executable(&canonical) {
            return BinaryResolution::NotExecutable { abs: canonical };
        }
        BinaryResolution::Found(canonical)
    } else {
        // Bare name: PATH search via `which`.
        match which_binary(cmd0) {
            Some(found) => BinaryResolution::Found(found),
            None => BinaryResolution::NotFound {
                resolved_path: format!("{cmd0} (searched PATH)"),
            },
        }
    }
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// Locate a bare-name binary via PATH, mirroring how the OS would resolve it.
fn which_binary(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if candidate.is_file() && is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// Produce a human-readable fix hint for a missing binary.
fn binary_fix_hint(cmd0: &str, working_dir: Option<&Path>, kit_dir: &Path) -> String {
    if cmd0.contains('/') || Path::new(cmd0).is_absolute() {
        let base = working_dir.unwrap_or(kit_dir);
        let joined = base.join(cmd0);
        // Check if this looks like a Rust debug binary path.
        if cmd0.contains("target/debug") || cmd0.contains("target/release") {
            return format!(
                "The binary has not been built yet or the path depth is wrong. \
Run `cargo build` in the Rust workspace to produce the binary, then verify \
that the `..` count in {cmd0:?} matches the kit's depth relative to the \
workspace target/ directory (resolved base: {}).",
                base.display()
            );
        }
        format!(
            "Verify the path depth in {cmd0:?} is correct relative to working_dir \
(resolved: {}). If the binary has not been built, build it first.",
            joined.display()
        )
    } else {
        format!("Install or build `{cmd0}` and ensure it is on PATH.")
    }
}

/// Check .provekit/imports/ for dependency .proof files.
fn check_imports(imports_dir: &Path, plugins: &[PluginEntry]) -> Check {
    let name = "imports-present";
    if !imports_dir.exists() {
        if plugins.is_empty() {
            return Check::pass_with_evidence(
                name,
                "no declared plugins; imports dir not required",
                json!({
                    "path": imports_dir.display().to_string(),
                    "pluginCount": plugins.len(),
                }),
            );
        }
        return Check::warn_with_evidence(
            name,
            format!(
                ".provekit/imports/ does not exist. If this kit has dependencies, \
run their mints first and copy the resulting .proof files here \
({})",
                imports_dir.display()
            ),
            json!({
                "path": imports_dir.display().to_string(),
                "pluginCount": plugins.len(),
            }),
        );
    }

    let count = std::fs::read_dir(imports_dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| {
                    e.path()
                        .extension()
                        .map(|ext| ext == "proof")
                        .unwrap_or(false)
                })
                .count()
        })
        .unwrap_or(0);

    if count == 0 && !plugins.is_empty() {
        Check::warn_with_evidence(
            name,
            format!(
                ".provekit/imports/ is empty (0 .proof files). \
If this kit depends on others, mint them and place their .proof outputs here."
            ),
            json!({
                "path": imports_dir.display().to_string(),
                "pluginCount": plugins.len(),
                "proofCount": count,
            }),
        )
    } else {
        Check::pass_with_evidence(
            name,
            format!(".provekit/imports/: {count} .proof file(s) present"),
            json!({
                "path": imports_dir.display().to_string(),
                "pluginCount": plugins.len(),
                "proofCount": count,
            }),
        )
    }
}

/// Check oracle wiring (advisory only).
/// Only runs when PROVEKIT_RESOLVE_ORACLE=rust-analyzer.
fn check_oracle_wiring() -> Option<Check> {
    let oracle = std::env::var("PROVEKIT_RESOLVE_ORACLE").unwrap_or_default();
    if oracle != "rust-analyzer" {
        return None;
    }

    let ra_found = locate_rust_analyzer().is_some();
    let linkerd_found = locate_linkerd().is_some();

    let mut problems: Vec<String> = Vec::new();
    if !ra_found {
        problems.push(
            "rust-analyzer not found (checked PROVEKIT_RUST_ANALYZER, PATH, `rustup which rust-analyzer`)"
                .to_string(),
        );
    }
    if !linkerd_found {
        problems.push(
            "provekit-linkerd daemon not found (checked PROVEKIT_LINKERD_BIN and PATH)"
                .to_string(),
        );
    }

    if problems.is_empty() {
        Some(Check::pass_with_evidence(
            "oracle-wiring",
            "PROVEKIT_RESOLVE_ORACLE=rust-analyzer: rust-analyzer and provekit-linkerd are both locatable",
            json!({
                "oracle": "rust-analyzer",
                "rustAnalyzerFound": ra_found,
                "linkerdFound": linkerd_found,
            }),
        ))
    } else {
        Some(Check::warn_with_evidence(
            "oracle-wiring",
            format!(
                "PROVEKIT_RESOLVE_ORACLE=rust-analyzer is set but oracle prerequisites are missing: {}. \
self-check with --oracle will hard-fail on an inert oracle.",
                problems.join("; ")
            ),
            json!({
                "oracle": "rust-analyzer",
                "rustAnalyzerFound": ra_found,
                "linkerdFound": linkerd_found,
                "problems": problems,
            }),
        ))
    }
}

/// Mirror the oracle-locate logic from ra_oracle.rs.
fn locate_rust_analyzer() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("PROVEKIT_RUST_ANALYZER") {
        if !p.is_empty() {
            let pb = PathBuf::from(&p);
            if pb.exists() {
                return Some(pb);
            }
        }
    }
    // `rustup which rust-analyzer` gives the toolchain-resolved path.
    if let Ok(out) = std::process::Command::new("rustup")
        .args(["which", "rust-analyzer"])
        .output()
    {
        if out.status.success() {
            let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !path.is_empty() && Path::new(&path).exists() {
                return Some(PathBuf::from(path));
            }
        }
    }
    which_binary("rust-analyzer")
}

/// Locate the provekit-linkerd daemon binary.
fn locate_linkerd() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("PROVEKIT_LINKERD_BIN") {
        if !p.is_empty() {
            let pb = PathBuf::from(&p);
            if pb.exists() {
                return Some(pb);
            }
        }
    }
    which_binary("provekit-linkerd")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    /// Write a minimal kit config.toml with the given plugins section.
    fn write_kit(dir: &Path, plugins_toml: &str) {
        fs::create_dir_all(dir.join(".provekit/imports")).unwrap();
        fs::write(
            dir.join(".provekit/config.toml"),
            format!(
                "# test kit\n[authoring]\nsurface = \"test-surface\"\n{plugins_toml}"
            ),
        )
        .unwrap();
    }

    /// Write a manifest.toml for a surface under the given kind dir.
    fn write_manifest(
        kit_dir: &Path,
        kind: &str,
        surface: &str,
        command: &str,
        working_dir: &str,
    ) {
        let dir = kit_dir.join(".provekit").join(kind).join(surface);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("manifest.toml"),
            format!(
                "name = \"test-{surface}\"\ncommand = [{command}]\nworking_dir = \"{working_dir}\"\n"
            ),
        )
        .unwrap();
    }

    /// Create a dummy executable file.
    fn make_executable(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, b"#!/bin/sh\n").unwrap();
        let mut perms = fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).unwrap();
    }

    fn make_consumer_plugin(path: &Path, surface: &str, method: &str, phase: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let response = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "capabilities": {
                    "consumer_surfaces": {
                        surface: {
                            "method": method,
                            "phase": phase,
                        }
                    }
                }
            }
        });
        fs::write(
            path,
            format!("#!/bin/sh\nread _line\nprintf '%s\\n' '{}'\n", response),
        )
        .unwrap();
        let mut perms = fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).unwrap();
    }

    #[test]
    fn known_good_kit_passes() {
        let td = TempDir::new().unwrap();
        let kit = td.path();

        // Create a fake binary.
        let bin = kit.join("fake-binary");
        make_executable(&bin);

        // Write manifest referencing the binary by relative path from kit root.
        write_kit(
            kit,
            "[[plugins]]\nname = \"test\"\nkind = \"lift\"\nsurface = \"test-surface\"\n",
        );
        write_manifest(kit, "lift", "test-surface", "\"./fake-binary\"", ".");

        let checks = run_checks(kit);

        let any_fail = checks.iter().any(|c| c.status == CheckStatus::Fail);
        assert!(
            !any_fail,
            "expected no FAIL checks but got: {:#?}",
            checks
                .iter()
                .filter(|c| c.status == CheckStatus::Fail)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn missing_binary_fails_loudly_with_path() {
        let td = TempDir::new().unwrap();
        let kit = td.path();

        // Binary intentionally NOT created (just document where it would be for the fix hint).

        write_kit(
            kit,
            "[[plugins]]\nname = \"test\"\nkind = \"lift\"\nsurface = \"broken-surface\"\n",
        );
        write_manifest(
            kit,
            "lift",
            "broken-surface",
            "\"../target/debug/nonexistent-binary\"",
            ".",
        );

        let checks = run_checks(kit);

        let fail_checks: Vec<&Check> = checks
            .iter()
            .filter(|c| c.status == CheckStatus::Fail)
            .collect();

        assert!(
            !fail_checks.is_empty(),
            "expected at least one FAIL check for missing binary"
        );

        // The fail detail must name the resolved absolute path.
        let binary_fail = fail_checks
            .iter()
            .find(|c| c.name.contains("binary-exists"))
            .expect("expected a binary-exists FAIL check");

        // The resolved path should appear in the detail.
        assert!(
            binary_fail.detail.contains("nonexistent-binary"),
            "FAIL detail should name the missing binary; got: {}",
            binary_fail.detail
        );

        // The detail must not be silent (no empty detail).
        assert!(
            !binary_fail.detail.is_empty(),
            "FAIL detail must not be empty"
        );

        // Confirm overall exit code would be nonzero.
        let any_hard_fail = checks.iter().any(|c| c.status == CheckStatus::Fail);
        assert!(any_hard_fail, "any_hard_fail must be true -> exit nonzero");
    }

    #[test]
    fn invalid_config_toml_fails() {
        let td = TempDir::new().unwrap();
        let kit = td.path();
        fs::create_dir_all(kit.join(".provekit")).unwrap();
        fs::write(kit.join(".provekit/config.toml"), b"not valid toml = [[[").unwrap();

        let checks = run_checks(kit);

        let config_fail = checks
            .iter()
            .find(|c| c.name == "config-toml-parse" && c.status == CheckStatus::Fail);
        assert!(
            config_fail.is_some(),
            "expected config-toml-parse FAIL for invalid TOML"
        );
    }

    #[test]
    fn run_report_defaults_to_structural_mode() {
        let td = TempDir::new().unwrap();
        let kit = td.path();
        fs::create_dir_all(kit.join(".provekit")).unwrap();

        let report = run_report(kit);

        assert_eq!(report.mode, DoctorMode::Structural);
    }

    #[test]
    fn doctor_report_mode_reflects_requested_mode() {
        let td = TempDir::new().unwrap();
        let kit = td.path();
        fs::create_dir_all(kit.join(".provekit")).unwrap();

        let strict = run_report_with_context(kit, DoctorContext::new(DoctorMode::Strict));

        assert_eq!(strict.mode, DoctorMode::Strict);
    }

    #[test]
    fn run_checks_with_context_preserves_default_engine_results() {
        let td = TempDir::new().unwrap();
        let kit = td.path();
        fs::create_dir_all(kit.join(".provekit")).unwrap();

        let default_checks = run_checks(kit);
        let structural_checks =
            run_checks_with_context(kit, DoctorContext::new(DoctorMode::Structural));

        assert_eq!(default_checks.len(), structural_checks.len());
        assert_eq!(default_checks[0].id, structural_checks[0].id);
        assert_eq!(default_checks[0].status, structural_checks[0].status);
        assert_eq!(default_checks[0].severity, structural_checks[0].severity);
        assert_eq!(default_checks[0].evidence, structural_checks[0].evidence);
    }

    #[test]
    fn modes_preserve_config_check_output() {
        let td = TempDir::new().unwrap();
        fs::create_dir_all(td.path().join(".provekit")).unwrap();

        assert_modes_match_for_check(td.path(), "kit.config.parse");
    }

    #[test]
    fn modes_preserve_manifest_check_output() {
        let td = TempDir::new().unwrap();
        let kit = td.path();
        write_kit(
            kit,
            "[[plugins]]\nname = \"test\"\nkind = \"lift\"\nsurface = \"broken-surface\"\n",
        );
        let manifest_dir = kit.join(".provekit/lift/broken-surface");
        fs::create_dir_all(&manifest_dir).unwrap();
        fs::write(
            manifest_dir.join("manifest.toml"),
            b"name = \"broken\"\ncommand = [[[\n",
        )
        .unwrap();

        assert_modes_match_for_check(kit, "kit.manifest.parse");
    }

    #[test]
    fn modes_preserve_binary_check_output() {
        let td = TempDir::new().unwrap();
        let kit = td.path();
        write_kit(
            kit,
            "[[plugins]]\nname = \"test\"\nkind = \"lift\"\nsurface = \"broken-surface\"\n",
        );
        write_manifest(
            kit,
            "lift",
            "broken-surface",
            "\"../target/debug/nonexistent-binary\"",
            ".",
        );

        assert_modes_match_for_check(kit, "kit.plugin.command.available");
    }

    #[test]
    fn modes_preserve_consumer_surface_check_output() {
        let td = TempDir::new().unwrap();
        let kit = td.path();
        let plugin = kit.join("consumer-plugin");
        make_consumer_plugin(
            &plugin,
            "consumer-surface",
            "provekit.plugin.lift_implications",
            "consumer",
        );
        write_kit(
            kit,
            "[[plugins]]\nname = \"consumer\"\nkind = \"lift\"\nsurface = \"consumer-surface\"\n",
        );
        write_manifest(
            kit,
            "lift",
            "consumer-surface",
            "\"./consumer-plugin\"",
            ".",
        );

        assert_modes_match_for_check(kit, "kit.consumer_surface.contract");
    }

    #[test]
    fn invalid_manifest_toml_fails_with_substrate_id() {
        let td = TempDir::new().unwrap();
        let kit = td.path();
        write_kit(
            kit,
            "[[plugins]]\nname = \"test\"\nkind = \"lift\"\nsurface = \"broken-surface\"\n",
        );
        let manifest_dir = kit.join(".provekit/lift/broken-surface");
        fs::create_dir_all(&manifest_dir).unwrap();
        fs::write(
            manifest_dir.join("manifest.toml"),
            b"name = \"broken\"\ncommand = [[[\n",
        )
        .unwrap();

        let report = run_report(kit);

        let manifest = report
            .checks
            .iter()
            .find(|check| check.id == "kit.manifest.parse")
            .expect("manifest parse check");
        assert_eq!(manifest.status, CheckStatus::Fail);
        assert_eq!(manifest.severity, CheckSeverity::Hard);
        assert!(
            manifest.detail.contains("invalid TOML"),
            "manifest parse failure should carry parse detail: {}",
            manifest.detail
        );
    }

    #[test]
    fn nonexecutable_binary_fails() {
        let td = TempDir::new().unwrap();
        let kit = td.path();

        // Create a non-executable file.
        let bin = kit.join("non-exec");
        fs::write(&bin, b"not a script\n").unwrap();
        let mut perms = fs::metadata(&bin).unwrap().permissions();
        perms.set_mode(0o644); // not executable
        fs::set_permissions(&bin, perms).unwrap();

        write_kit(
            kit,
            "[[plugins]]\nname = \"test\"\nkind = \"lift\"\nsurface = \"test-surface\"\n",
        );
        write_manifest(kit, "lift", "test-surface", "\"./non-exec\"", ".");

        let checks = run_checks(kit);

        let fail = checks
            .iter()
            .find(|c| c.name.contains("binary-exists") && c.status == CheckStatus::Fail);
        assert!(
            fail.is_some(),
            "expected binary-exists FAIL for non-executable file"
        );
    }

    #[test]
    fn consumer_method_phase_mismatch_fails_with_fix_hint() {
        let td = TempDir::new().unwrap();
        let kit = td.path();
        let plugin = kit.join("consumer-plugin");
        make_consumer_plugin(
            &plugin,
            "consumer-surface",
            "provekit.plugin.lift_implications",
            "consumer",
        );
        write_kit(
            kit,
            "[[plugins]]\nname = \"consumer\"\nkind = \"lift\"\nsurface = \"consumer-surface\"\n",
        );
        write_manifest(
            kit,
            "lift",
            "consumer-surface",
            "\"./consumer-plugin\"",
            ".",
        );

        let report = run_report(kit);

        let consumer = report
            .checks
            .iter()
            .find(|check| check.id == "kit.consumer_surface.contract")
            .expect("consumer surface contract check");
        assert_eq!(consumer.status, CheckStatus::Fail);
        assert_eq!(consumer.severity, CheckSeverity::Hard);
        assert!(
            consumer.detail.contains("Add both lines to the manifest"),
            "consumer mismatch should preserve the fix hint: {}",
            consumer.detail
        );
        assert_eq!(
            consumer
                .evidence
                .get("requiredMethod")
                .and_then(Value::as_str),
            Some("provekit.plugin.lift_implications")
        );
    }

    #[test]
    fn doctor_report_ok_aggregates_check_statuses() {
        let td = TempDir::new().unwrap();
        let kit = td.path();
        let bin = kit.join("fake-binary");
        make_executable(&bin);
        write_kit(
            kit,
            "[[plugins]]\nname = \"test\"\nkind = \"lift\"\nsurface = \"test-surface\"\n",
        );
        write_manifest(kit, "lift", "test-surface", "\"./fake-binary\"", ".");

        let report = run_report(kit);

        assert!(report.ok, "warn-only doctor report should be ok: {report:#?}");

        let missing = TempDir::new().unwrap();
        fs::create_dir_all(missing.path().join(".provekit")).unwrap();

        let report = run_report(missing.path());

        assert!(!report.ok, "hard-fail doctor report must not be ok");
        assert!(
            report
                .checks
                .iter()
                .any(|check| check.status == CheckStatus::Fail),
            "hard-fail report must contain a failing check: {report:#?}"
        );
    }

    #[test]
    fn doctor_checks_populate_substrate_fields() {
        let td = TempDir::new().unwrap();
        let kit = td.path();
        let bin = kit.join("fake-binary");
        make_executable(&bin);
        write_kit(
            kit,
            "[[plugins]]\nname = \"test\"\nkind = \"lift\"\nsurface = \"test-surface\"\n",
        );
        write_manifest(kit, "lift", "test-surface", "\"./fake-binary\"", ".");

        let report = run_report(kit);

        let config = report
            .checks
            .iter()
            .find(|check| check.name == "config-toml-parse")
            .expect("config parse check");
        assert_eq!(config.id, "kit.config.parse");
        assert_eq!(config.domain, "kit.config");
        assert_eq!(config.severity, CheckSeverity::Hard);
        let config_path = config
            .evidence
            .get("path")
            .and_then(Value::as_str)
            .expect("config path evidence");
        assert!(
            config_path.ends_with(".provekit/config.toml"),
            "config check should carry config path evidence: {config:#?}"
        );

        let binary = report
            .checks
            .iter()
            .find(|check| check.name.contains("binary-exists"))
            .expect("binary availability check");
        assert_eq!(binary.id, "kit.plugin.command.available");
        assert_eq!(binary.domain, "kit.plugin");
        assert_eq!(binary.severity, CheckSeverity::Hard);
        assert_eq!(
            binary.evidence.get("command").and_then(Value::as_str),
            Some("./fake-binary"),
            "binary check should carry exact command evidence: {binary:#?}"
        );
    }

    #[test]
    fn missing_config_is_hard_report_check() {
        let td = TempDir::new().unwrap();
        fs::create_dir_all(td.path().join(".provekit")).unwrap();

        let report = run_report(td.path());

        let config = report
            .checks
            .iter()
            .find(|check| check.id == "kit.config.parse")
            .expect("config parse check");
        assert_eq!(config.status, CheckStatus::Fail);
        assert_eq!(config.severity, CheckSeverity::Hard);
        assert!(
            config.detail.contains(".provekit/config.toml"),
            "missing-config detail should name the file: {}",
            config.detail
        );
    }

    fn check_by_id<'a>(report: &'a DoctorReport, id: &str) -> &'a DoctorCheck {
        report
            .checks
            .iter()
            .find(|check| check.id == id)
            .unwrap_or_else(|| panic!("{id} check in {report:#?}"))
    }

    fn assert_modes_match_for_check(kit: &Path, id: &str) {
        let structural =
            run_report_with_context(kit, DoctorContext::new(DoctorMode::Structural));
        let strict = run_report_with_context(kit, DoctorContext::new(DoctorMode::Strict));

        let structural_check = check_by_id(&structural, id);
        let strict_check = check_by_id(&strict, id);

        assert_eq!(structural_check.status, strict_check.status, "{id} status");
        assert_eq!(
            structural_check.severity, strict_check.severity,
            "{id} severity"
        );
        assert_eq!(structural_check.detail, strict_check.detail, "{id} detail");
        assert_eq!(
            structural_check.evidence, strict_check.evidence,
            "{id} evidence"
        );
    }

    #[test]
    fn cmd_doctor_is_cli_shell_after_refactor() {
        let source = include_str!("cmd_doctor.rs");
        for forbidden in [
            "pub fn run_checks",
            "fn plugin_consumer_surfaces",
            "fn resolve_binary",
            "fn check_imports",
            "fn check_oracle_wiring",
        ] {
            assert!(
                !source.contains(forbidden),
                "cmd_doctor.rs should not contain check logic after refactor: found {forbidden}"
            );
        }
    }
}
