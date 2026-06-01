// SPDX-License-Identifier: Apache-2.0
//
// provekit doctor: validate a kit's config/manifest wiring up front.

use std::path::PathBuf;

use clap::Parser;
use owo_colors::OwoColorize;
use serde_json::{json, Value};

use crate::doctor::{run_report, CheckStatus, DoctorCheck, DoctorReport};
use crate::{EXIT_OK, EXIT_USER_ERROR};

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

    let report = run_report(&target);

    if args.json {
        print_json(&report);
    } else {
        print_human(&report);
    }

    if report.ok {
        EXIT_OK
    } else {
        EXIT_USER_ERROR
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

fn print_human(report: &DoctorReport) {
    println!(
        "{} {}",
        "provekit doctor".bold(),
        report.target.display().to_string().dimmed()
    );
    println!();

    let mut passes = 0usize;
    let mut warns = 0usize;
    let mut fails = 0usize;

    for check in &report.checks {
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
    if report.ok {
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

fn print_json(report: &DoctorReport) {
    println!(
        "{}",
        serde_json::to_string_pretty(&legacy_report_json(report)).unwrap_or_default()
    );
}

fn legacy_report_json(report: &DoctorReport) -> Value {
    let checks_json: Vec<Value> = report.checks.iter().map(legacy_check_json).collect();
    json!({
        "checks": checks_json,
        "ok": report.ok,
    })
}

fn legacy_check_json(check: &DoctorCheck) -> Value {
    let _ = (&check.id, &check.severity, &check.domain, &check.evidence);
    json!({
        "name": check.name,
        "status": check.status.as_str(),
        "detail": check.detail,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn json_output_keeps_legacy_shape() {
        let td = TempDir::new().unwrap();
        let kit = td.path();
        fs::create_dir_all(kit.join(".provekit/imports")).unwrap();
        fs::write(
            kit.join(".provekit/config.toml"),
            "# test kit\n[authoring]\nsurface = \"test-surface\"\n",
        )
        .unwrap();

        let report = run_report(kit);
        let value = legacy_report_json(&report);
        let first_check = value["checks"][0]
            .as_object()
            .expect("first check object");

        assert!(value.get("ok").is_some());
        assert!(first_check.contains_key("name"));
        assert!(first_check.contains_key("status"));
        assert!(first_check.contains_key("detail"));
        assert!(!first_check.contains_key("id"));
        assert!(!first_check.contains_key("domain"));
        assert!(!first_check.contains_key("severity"));
        assert!(!first_check.contains_key("evidence"));
    }
}
