//! `cargo sugar`: behavioral semver for Rust crates.
//!
//! The package-manager wedge. `cargo sugar check` lifts the current crate and
//! the last published version, diffs their behavior with `sugar diff`, and fails
//! if the version bump in Cargo.toml is dishonest about what the code does.
//!
//! It needs nothing from crates.io or the crate author: it mints BOTH sides
//! itself, so it works on day one for any crate. That dissolves the bootstrap
//! problem -- you do not need an ecosystem of published proofs to start.
//!
//! Trojan-horse positioning: this is "cargo-semver-checks, but it catches a
//! behavior change, not just an API-signature change." Nobody adopts a worldview;
//! they add one line to a workflow and get a green or red check. The
//! content-addressed-contract machinery stays entirely under the hood.

use std::path::Path;
use std::process::{Command, Stdio};

use clap::{Parser, Subcommand};

mod crates_io;

#[derive(Parser)]
#[command(name = "cargo-sugar", bin_name = "cargo sugar", version, about)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Behavioral diff between two proof sets or git revisions. Passes through
    /// to `sugar diff` (so `cargo sugar diff ...` == `sugar diff ...`).
    Diff {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Behavioral semver check: diff the current crate against a baseline (the
    /// last crates.io release by default, or a git revision) and enforce the bump.
    Check(CheckArgs),
}

#[derive(clap::Args)]
struct CheckArgs {
    /// Baseline a git revision instead of the last crates.io release.
    #[arg(long, value_name = "REV")]
    rev: Option<String>,
    /// Baseline a specific crates.io version (default: the latest published).
    #[arg(long, value_name = "VERSION")]
    version: Option<String>,
    /// Fail unless the behavior delta fits within this bump (none|minor|major).
    #[arg(long, value_name = "BUMP")]
    require: Option<String>,
    /// With --rev, the project subdirectory within the revision's tree.
    #[arg(long, default_value = ".")]
    path: String,
}

fn sugar_bin() -> String {
    std::env::var("SUGAR_BIN").unwrap_or_else(|_| "sugar".to_string())
}

fn main() -> std::process::ExitCode {
    // cargo invokes `cargo sugar X` as `cargo-sugar sugar X`; drop the leading
    // subcommand token so clap sees the real arguments.
    let mut args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("sugar") {
        args.remove(1);
    }
    let cli = Cli::parse_from(args);
    match run(cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("cargo sugar: {e}");
            std::process::ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> Result<std::process::ExitCode, String> {
    match cli.cmd {
        Cmd::Diff { args } => {
            let status = Command::new(sugar_bin())
                .arg("diff")
                .args(&args)
                .status()
                .map_err(|e| format!("spawn `{} diff`: {e}", sugar_bin()))?;
            Ok(exit_from(status.code().unwrap_or(1)))
        }
        Cmd::Check(a) => check(a),
    }
}

fn exit_from(code: i32) -> std::process::ExitCode {
    std::process::ExitCode::from(u8::try_from(code).unwrap_or(1))
}

/// Read `name` and `version` out of `./Cargo.toml` (the `[package]` table).
fn crate_identity() -> Result<(String, String), String> {
    let text = std::fs::read_to_string("Cargo.toml")
        .map_err(|e| format!("read ./Cargo.toml: {e} (run from a crate root)"))?;
    parse_package_identity(&text).ok_or_else(|| "no [package] name/version in Cargo.toml".into())
}

fn parse_package_identity(toml: &str) -> Option<(String, String)> {
    let mut in_package = false;
    let (mut name, mut version) = (None, None);
    for line in toml.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_package = t == "[package]";
            continue;
        }
        if !in_package {
            continue;
        }
        if let Some(v) = t.strip_prefix("name").and_then(|r| toml_str_value(r)) {
            name = Some(v);
        } else if let Some(v) = t.strip_prefix("version").and_then(|r| toml_str_value(r)) {
            version = Some(v);
        }
    }
    Some((name?, version?))
}

/// `= "value"` -> `value`. Ignores workspace-inherited values (`= { ... }`).
fn toml_str_value(rest: &str) -> Option<String> {
    let after_eq = rest.trim_start().strip_prefix('=')?.trim();
    let inner = after_eq.strip_prefix('"')?;
    let end = inner.find('"')?;
    Some(inner[..end].to_string())
}

/// Extract a git `rev:path` subtree into `dst` via `git archive | tar`.
fn extract_git(rev: &str, path: &str, dst: &Path) -> Result<(), String> {
    let treeish = if path == "." || path.is_empty() {
        rev.to_string()
    } else {
        format!("{rev}:{path}")
    };
    let archive = Command::new("git")
        .args(["archive", "--format=tar", &treeish])
        .output()
        .map_err(|e| format!("git archive: {e}"))?;
    if !archive.status.success() {
        return Err(format!(
            "git archive {treeish}: {}",
            String::from_utf8_lossy(&archive.stderr).trim()
        ));
    }
    let mut tar = Command::new("tar")
        .args(["-x", "-C", &dst.to_string_lossy()])
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| format!("tar: {e}"))?;
    use std::io::Write;
    tar.stdin
        .take()
        .expect("tar stdin")
        .write_all(&archive.stdout)
        .map_err(|e| format!("tar stdin: {e}"))?;
    if !tar.wait().map_err(|e| format!("tar wait: {e}"))?.success() {
        return Err("tar extract failed".into());
    }
    Ok(())
}

/// The Rust lift config cargo-sugar stamps into a crate before minting. Two
/// surfaces drive the behavior fingerprint: `rust-walk-contracts` (body-derived
/// function contracts via sugar-walk-rpc) and `rust-tests` (harvested `#[test]`
/// assertions via sugar-lift -- the test *is* the contract). `{walk}`/`{lift}`
/// are filled with absolute lifter paths.
const RUST_CONFIG_TOML: &str = r#"[[plugins]]
name = "rust-walk-contracts"
surface = "rust-walk-contracts"
emit = "ir-document"
[[plugins]]
name = "rust-tests"
surface = "rust-tests"
emit = "ir-document"
[solvers]
default = "z3"
[solvers.dispatch]
default = "z3"
[solvers.z3]
binary = "z3"
flags = ["-smt2", "-in"]
"#;

/// Resolve a sibling lifter binary next to the `sugar` binary; fall back to the
/// bare name (assume it is on PATH, as it would be after `cargo install`).
fn lifter_bin(name: &str) -> String {
    let sugar = sugar_bin();
    if sugar.contains('/') {
        if let Some(dir) = Path::new(&sugar).parent() {
            let p = dir.join(name);
            if p.exists() {
                return p.to_string_lossy().into_owned();
            }
        }
    }
    name.to_string()
}

/// Write the Rust lift `.sugar/` config + manifests into `dir` (idempotent: a
/// crate that already declares a surface is left alone).
fn bootstrap_rust(dir: &Path) -> Result<(), String> {
    let s = dir.join(".sugar");
    if s.join("config.toml").exists() {
        return Ok(());
    }
    let manifest = |surface: &str, bin: &str| {
        format!("name = \"{surface}\"\ncommand = [\"{bin}\", \"--rpc\"]\nworking_dir = \".\"\n")
    };
    std::fs::create_dir_all(s.join("lift/rust-walk-contracts")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(s.join("lift/rust-tests")).map_err(|e| e.to_string())?;
    std::fs::write(s.join("config.toml"), RUST_CONFIG_TOML).map_err(|e| e.to_string())?;
    std::fs::write(
        s.join("lift/rust-walk-contracts/manifest.toml"),
        manifest("rust-walk-contracts", &lifter_bin("sugar-walk-rpc")),
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(
        s.join("lift/rust-tests/manifest.toml"),
        manifest("rust-tests", &lifter_bin("sugar-lift")),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Copy the current crate's lift-relevant sources into `dst` so we can bootstrap
/// and mint without writing `.sugar/` into the user's working tree.
fn copy_current_crate(dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for item in ["Cargo.toml", "src", "tests", "benches"] {
        if Path::new(item).exists() {
            let ok = Command::new("cp")
                .arg("-r")
                .arg(item)
                .arg(dst)
                .status()
                .map_err(|e| format!("cp {item}: {e}"))?
                .success();
            if !ok {
                return Err(format!("cp {item} failed"));
            }
        }
    }
    Ok(())
}

/// Bootstrap Rust lift config into `project` and mint its proofs into `out_dir`.
fn mint(project: &Path, out_dir: &Path) -> Result<(), String> {
    bootstrap_rust(project)?;
    let sugar = sugar_bin();
    let status = Command::new(&sugar)
        .args(["mint", "--project"])
        .arg(project)
        .arg("--out")
        .arg(out_dir)
        .status()
        .map_err(|e| format!("spawn `{sugar} mint`: {e}"))?;
    if !status.success() {
        return Err(format!(
            "`sugar mint --project {}` failed",
            project.display()
        ));
    }
    Ok(())
}

fn check(a: CheckArgs) -> Result<std::process::ExitCode, String> {
    let (name, version) = crate_identity()?;
    let scratch = std::env::temp_dir().join(format!("cargo-sugar-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    let base_src = scratch.join("baseline-src");
    let base_out = scratch.join("baseline-proofs");
    let cur_src = scratch.join("current-src");
    let cur_out = scratch.join("current-proofs");
    std::fs::create_dir_all(&base_src).map_err(|e| format!("mkdir: {e}"))?;

    // 1. resolve + materialize the baseline source.
    let baseline_label = if let Some(rev) = &a.rev {
        extract_git(rev, &a.path, &base_src)?;
        format!("git {rev}")
    } else {
        let v = match &a.version {
            Some(v) => v.clone(),
            None => crates_io::latest_version(&name)?,
        };
        crates_io::download_and_extract(&name, &v, &base_src)?;
        format!("crates.io {name} {v}")
    };

    // 2. mint both sides (current crate copied out so we never write into the
    // user's working tree).
    eprintln!("cargo sugar: current crate {name} {version} vs baseline {baseline_label}");
    copy_current_crate(&cur_src)?;
    mint(&cur_src, &cur_out)?;
    mint(&base_src, &base_out)?;

    // 3. diff the two proof sets and enforce the bump.
    let mut cmd = Command::new(sugar_bin());
    cmd.arg("diff").arg(&base_out).arg(&cur_out);
    if let Some(req) = &a.require {
        cmd.args(["--require", req]);
    }
    let status = cmd
        .status()
        .map_err(|e| format!("spawn `{} diff`: {e}", sugar_bin()))?;
    let _ = std::fs::remove_dir_all(&scratch);
    Ok(exit_from(status.code().unwrap_or(1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_package_name_and_version() {
        let toml = "[package]\nname = \"serde\"\nversion = \"1.0.203\"\nedition = \"2021\"\n";
        assert_eq!(
            parse_package_identity(toml),
            Some(("serde".to_string(), "1.0.203".to_string()))
        );
    }

    #[test]
    fn ignores_other_tables_and_workspace_inherited() {
        let toml = "[package]\nname = \"x\"\nversion.workspace = true\n\n[dependencies]\nname = \"not-this\"\n";
        // version is workspace-inherited (not a string literal), so identity is incomplete.
        assert_eq!(parse_package_identity(toml), None);
    }

    #[test]
    fn toml_str_value_extracts_quoted() {
        assert_eq!(toml_str_value(" = \"abc\"").as_deref(), Some("abc"));
        assert_eq!(toml_str_value(".workspace = true"), None);
    }
}
