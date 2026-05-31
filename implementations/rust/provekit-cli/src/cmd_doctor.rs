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

use clap::Parser;
use owo_colors::OwoColorize;
use serde_json::{json, Value};

use crate::lift_plugin::{parse_manifest_at, resolved_working_dir_for};
use crate::project_config::{read_project_config, PluginEntry};
use crate::{EXIT_OK, EXIT_USER_ERROR};

/// Status of one doctor check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
}

impl CheckStatus {
    fn as_str(&self) -> &'static str {
        match self {
            CheckStatus::Pass => "pass",
            CheckStatus::Warn => "warn",
            CheckStatus::Fail => "fail",
        }
    }
}

/// One check result.
#[derive(Debug, Clone)]
pub struct Check {
    pub name: String,
    pub status: CheckStatus,
    pub detail: String,
}

impl Check {
    fn pass(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Pass,
            detail: detail.into(),
        }
    }
    fn warn(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Warn,
            detail: detail.into(),
        }
    }
    fn fail(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Fail,
            detail: detail.into(),
        }
    }
}

#[derive(Parser, Debug, Clone)]
pub struct DoctorArgs {
    /// Kit directory to validate. Defaults to the current directory.
    /// Must contain a .provekit/config.toml file.
    #[arg(long)]
    pub target: Option<PathBuf>,

    /// Emit structured JSON instead of human-readable text.
    #[arg(long)]
    pub json: bool,
}

pub fn run(args: DoctorArgs) -> u8 {
    let target = match resolve_target(args.target.as_ref()) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{}: {e}", "error".red().bold());
            return EXIT_USER_ERROR;
        }
    };

    let checks = run_checks(&target);

    let any_hard_fail = checks
        .iter()
        .any(|c| c.status == CheckStatus::Fail);

    if args.json {
        print_json(&checks, !any_hard_fail);
    } else {
        print_human(&target, &checks, !any_hard_fail);
    }

    if any_hard_fail {
        EXIT_USER_ERROR
    } else {
        EXIT_OK
    }
}

/// Resolve the target kit directory from an optional CLI path.
fn resolve_target(target: Option<&PathBuf>) -> Result<PathBuf, String> {
    let path = match target {
        Some(p) => {
            if p.is_absolute() {
                p.clone()
            } else {
                std::env::current_dir()
                    .map_err(|e| format!("read current directory: {e}"))?
                    .join(p)
            }
        }
        None => std::env::current_dir()
            .map_err(|e| format!("read current directory: {e}"))?,
    };

    let canonical = path
        .canonicalize()
        .map_err(|e| format!("resolve target {}: {e}", path.display()))?;

    if !canonical.join(".provekit/config.toml").is_file() {
        return Err(format!(
            "target is not a provekit kit (missing .provekit/config.toml): {}",
            canonical.display()
        ));
    }

    Ok(canonical)
}

/// Run all checks against the target kit directory.
/// Pure function: suitable for testing without CLI overhead.
pub fn run_checks(kit_dir: &Path) -> Vec<Check> {
    let mut checks: Vec<Check> = Vec::new();

    // --- Check 1: config.toml and all manifest TOML files parse as valid TOML.
    let config_path = kit_dir.join(".provekit/config.toml");
    match std::fs::read_to_string(&config_path) {
        Err(e) => {
            checks.push(Check::fail(
                "config-toml-parse",
                format!("cannot read {}: {e}", config_path.display()),
            ));
            // Cannot enumerate plugins without a config; stop here.
            return checks;
        }
        Ok(text) => match text.parse::<toml::Value>() {
            Err(e) => {
                checks.push(Check::fail(
                    "config-toml-parse",
                    format!("{}: invalid TOML: {e}", config_path.display()),
                ));
                // Config invalid; cannot enumerate plugins.
                return checks;
            }
            Ok(_) => {
                checks.push(Check::pass(
                    "config-toml-parse",
                    format!("{} parses as valid TOML", config_path.display()),
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
                checks.push(Check::fail(
                    &check_name,
                    format!("cannot read {}: {e}", manifest_path.display()),
                ));
            }
            Ok(text) => match text.parse::<toml::Value>() {
                Err(e) => {
                    checks.push(Check::fail(
                        &check_name,
                        format!("{}: invalid TOML: {e}", manifest_path.display()),
                    ));
                }
                Ok(_) => {
                    checks.push(Check::pass(
                        &check_name,
                        format!("{} parses as valid TOML", manifest_path.display()),
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
                checks.push(Check::fail(
                    &check_name,
                    format!("cannot parse manifest: {e}"),
                ));
                continue;
            }
            Ok(m) => m,
        };
        if manifest.command.is_empty() {
            checks.push(Check::fail(
                &check_name,
                format!(
                    "manifest {} declares no command",
                    manifest_path.display()
                ),
            ));
            continue;
        }

        let cmd0 = &manifest.command[0];
        let resolved_wd = resolved_working_dir_for(kit_dir, &manifest);

        match resolve_binary(cmd0, resolved_wd.as_deref()) {
            BinaryResolution::Found(abs) => {
                checks.push(Check::pass(
                    &check_name,
                    format!(
                        "surface={surface} command={cmd0:?} -> {} (executable)",
                        abs.display()
                    ),
                ));
            }
            BinaryResolution::NotFound { resolved_path } => {
                let fix = binary_fix_hint(cmd0, resolved_wd.as_deref(), kit_dir);
                checks.push(Check::fail(
                    &check_name,
                    format!(
                        "surface={surface} command={cmd0:?}: binary not found at {}. {fix}",
                        resolved_path
                    ),
                ));
            }
            BinaryResolution::NotExecutable { abs } => {
                checks.push(Check::fail(
                    &check_name,
                    format!(
                        "surface={surface} command={cmd0:?}: file exists at {} but is not executable",
                        abs.display()
                    ),
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

    checks
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
            return Check::pass(name, "no declared plugins; imports dir not required");
        }
        return Check::warn(
            name,
            format!(
                ".provekit/imports/ does not exist. If this kit has dependencies, \
run their mints first and copy the resulting .proof files here \
({})",
                imports_dir.display()
            ),
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
        Check::warn(
            name,
            format!(
                ".provekit/imports/ is empty (0 .proof files). \
If this kit depends on others, mint them and place their .proof outputs here."
            ),
        )
    } else {
        Check::pass(
            name,
            format!(".provekit/imports/: {count} .proof file(s) present"),
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
        Some(Check::pass(
            "oracle-wiring",
            "PROVEKIT_RESOLVE_ORACLE=rust-analyzer: rust-analyzer and provekit-linkerd are both locatable",
        ))
    } else {
        Some(Check::warn(
            "oracle-wiring",
            format!(
                "PROVEKIT_RESOLVE_ORACLE=rust-analyzer is set but oracle prerequisites are missing: {}. \
self-check with --oracle will hard-fail on an inert oracle.",
                problems.join("; ")
            ),
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

// --- Output ---

fn print_human(kit_dir: &Path, checks: &[Check], ok: bool) {
    println!(
        "{} {}",
        "provekit doctor".bold(),
        kit_dir.display().to_string().dimmed()
    );
    println!();

    let mut passes = 0usize;
    let mut warns = 0usize;
    let mut fails = 0usize;

    for check in checks {
        let (label, colored_name) = match check.status {
            CheckStatus::Pass => {
                passes += 1;
                ("pass".green().bold().to_string(), check.name.green().to_string())
            }
            CheckStatus::Warn => {
                warns += 1;
                ("warn".yellow().bold().to_string(), check.name.yellow().to_string())
            }
            CheckStatus::Fail => {
                fails += 1;
                ("FAIL".red().bold().to_string(), check.name.red().to_string())
            }
        };
        println!("  [{label}] {colored_name}");
        if check.status != CheckStatus::Pass || !check.detail.is_empty() {
            println!("         {}", check.detail);
        }
    }

    println!();
    if ok {
        println!(
            "{}: {} passed, {} warned, {} failed",
            "ok".green().bold(),
            passes,
            warns,
            fails
        );
    } else {
        println!(
            "{}: {} passed, {} warned, {} failed",
            "FAIL".red().bold(),
            passes,
            warns,
            fails
        );
    }
}

fn print_json(checks: &[Check], ok: bool) {
    let checks_json: Vec<Value> = checks
        .iter()
        .map(|c| {
            json!({
                "name": c.name,
                "status": c.status.as_str(),
                "detail": c.detail,
            })
        })
        .collect();
    let out = json!({
        "checks": checks_json,
        "ok": ok,
    });
    println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
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
}
