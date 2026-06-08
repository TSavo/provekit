// SPDX-License-Identifier: Apache-2.0
//
// `sugar prove` / `sugar verify`: runs the six-stage pipeline.
//
// The witness-discharge path resolves each lift surface's manifest (the SAME
// dispatch lift uses) to export SUGAR_WITNESS_DISCHARGE_<TOOL> per tool, so
// witness recompute rides the manifest with no bespoke config.

use std::path::{Path, PathBuf};

use owo_colors::OwoColorize;
use serde_json::{json, Value};
use sugar_canonicalizer::blake3_512_of;

use sugar_verifier::{Runner, RunnerConfig};

use crate::project_config::read_project_config;
use crate::report_fmt;
use crate::ProveArgs;

// The witness-discharge path loads the lift surface manifest at
// `<project>/.sugar/lift/<surface>/manifest.toml` to read its
// `discharge_command` + `witness_tool`. No hardcoded `sugar-lift-<kit>`.

// ---------------------------------------------------------------------------
// Plugin manifest (mirrors cmd_mint: kept local to avoid coupling)
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct PluginManifest {
    name: String,
    command: Vec<String>,
    working_dir: Option<PathBuf>,
    /// Execution-witness discharge command the kit ships (recompute entry).
    /// Declared alongside `command` so witness discharge rides the SAME manifest
    /// dispatch as lift -- no bespoke config. `prove` exports it as
    /// `SUGAR_WITNESS_DISCHARGE_<witness_tool>` for the verifier's witness arm.
    discharge_command: Vec<String>,
    /// The `tool` value this surface stamps on its witness certificates (e.g.
    /// `pytest`). Keys the per-tool discharge registry so a proof carrying
    /// witnesses from multiple kits routes each to its own recompute.
    witness_tool: Option<String>,
}

fn parse_manifest(path: &std::path::Path) -> Result<PluginManifest, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let mut m = PluginManifest::default();
    let strip = |l: &str| -> String {
        match l.find('#') {
            Some(p) => l[..p].to_string(),
            None => l.to_string(),
        }
    };
    let raw: Vec<String> = text.lines().map(|l| strip(l).trim().to_string()).collect();
    let mut i = 0;
    while i < raw.len() {
        let line = raw[i].clone();
        i += 1;
        if line.is_empty() || line.starts_with('[') {
            continue;
        }
        let Some(eq) = line.find('=') else { continue };
        let key = line[..eq].trim().to_string();
        let mut val = line[eq + 1..].trim().to_string();
        // Multi-line array value: accumulate continuation lines until the
        // closing `]` (TOML allows `key = [` then elements on later lines).
        if val.starts_with('[') && !val.contains(']') {
            while i < raw.len() && !val.contains(']') {
                val.push(' ');
                val.push_str(&raw[i]);
                i += 1;
            }
        }
        let key = key.as_str();
        let val = val.as_str();
        match key {
            "name" => m.name = val.trim_matches('"').to_string(),
            "working_dir" => m.working_dir = Some(PathBuf::from(val.trim_matches('"'))),
            "witness_tool" => m.witness_tool = Some(val.trim_matches('"').to_string()),
            "command" | "discharge_command" => {
                let inner = val.trim_matches(|c| c == '[' || c == ']');
                let parsed: Vec<String> = inner
                    .split(',')
                    .map(|s| s.trim().trim_matches('"').to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                if key == "command" {
                    m.command = parsed;
                } else {
                    m.discharge_command = parsed;
                }
            }
            _ => {}
        }
    }
    if m.command.is_empty() {
        return Err(format!("manifest {} has no `command`", path.display()));
    }
    Ok(m)
}

fn find_manifest(project_root: &std::path::Path, surface: &str) -> Result<PluginManifest, String> {
    let project_local = project_root
        .join(".sugar")
        .join("lift")
        .join(surface)
        .join("manifest.toml");
    if project_local.exists() {
        return parse_manifest(&project_local);
    }
    if let Some(home) = std::env::var_os("HOME") {
        let user_global = PathBuf::from(home)
            .join(".config")
            .join("sugar")
            .join("lift")
            .join(surface)
            .join("manifest.toml");
        if user_global.exists() {
            return parse_manifest(&user_global);
        }
    }
    Err(format!(
        "no plugin manifest for surface `{surface}` (looked in .sugar/lift/{surface}/manifest.toml and ~/.config/sugar/lift/{surface}/manifest.toml)"
    ))
}

// ---------------------------------------------------------------------------
// pub fn run: entry point from main.rs
// ---------------------------------------------------------------------------

pub fn run(args: ProveArgs) -> u8 {
    if args.artifact.is_some() || args.proof.is_some() || args.policy.is_some() {
        return run_admission_gate(&args);
    }

    // Run the six-stage verifier pipeline.
    let project_root: PathBuf = args.project.unwrap_or_else(|| PathBuf::from("."));
    if !project_root.exists() {
        eprintln!(
            "{}: project root does not exist: {}",
            "error".red().bold(),
            project_root.display()
        );
        return crate::EXIT_USER_ERROR;
    }

    let cfg_doc = read_project_config(&project_root);

    configure_witness_discharge_env(&project_root, &cfg_doc);

    // Resolve `--with` paths relative to project_root unless absolute,
    // matching how `[verify].callees` is resolved (project-root-anchored).
    // Without this, `--with foo` depends on CWD and breaks when prove is
    // invoked outside the project root.
    let mut extra_projects: Vec<PathBuf> = args
        .with
        .iter()
        .map(|s| {
            let p = PathBuf::from(s);
            if p.is_absolute() {
                p
            } else {
                project_root.join(p)
            }
        })
        .collect();

    for callee in &cfg_doc.callees {
        let p = project_root.join(callee);
        if p.exists() {
            extra_projects.push(p);
        }
    }

    let dependency_proofs = match crate::kit_dispatch::dependency_proofs_via_rpc(&project_root) {
        Ok(proofs) => proofs,
        Err(error) => {
            eprintln!(
                "{}: dependency proof resolution skipped: {error}",
                "warning".yellow().bold()
            );
            Vec::new()
        }
    };

    let cfg = RunnerConfig {
        project_root: project_root.clone(),
        z3_path: args.z3,
        extra_projects,
        extra_proofs: dependency_proofs,
        ..Default::default()
    };
    let runner = Runner::new(cfg);
    let run_artifact = match runner.run_with_proof_run() {
        Ok(artifact) => artifact,
        Err(error) => {
            eprintln!("{}: {error}", "error".red().bold());
            return crate::EXIT_USER_ERROR;
        }
    };
    let report = run_artifact.report;

    if args.out.json {
        let j = report_fmt::report_to_json(&report);
        match serde_json::to_string_pretty(&j) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("{}: serialize JSON: {e}", "error".red().bold());
                return crate::EXIT_USER_ERROR;
            }
        }
    } else {
        report_fmt::print_report_pretty(&report, args.out.quiet);
    }

    report_fmt::report_exit_code(&report)
}

// WITNESS DISCHARGE defaults: so `sugar prove <project>` and artifact-mode
// `sugar verify --project <project>` settle execution witnesses by recompute
// WITHOUT the caller exporting env vars. The discharge command is declared in
// the KIT'S MANIFEST (alongside its lift `command`) and resolved here through
// the SAME `find_manifest` dispatch lift uses -- no bespoke config.
pub(crate) fn configure_witness_discharge_env(
    project_root: &Path,
    cfg_doc: &crate::project_config::ProjectConfig,
) {
    if std::env::var_os("SUGAR_WITNESS_PROJECT_DIR").is_none() {
        let p = project_root
            .canonicalize()
            .unwrap_or_else(|_| project_root.to_path_buf());
        std::env::set_var("SUGAR_WITNESS_PROJECT_DIR", &p);
    }
    for plugin in cfg_doc.plugins.iter().filter(|p| p.is_lift_plugin()) {
        let manifest = match find_manifest(project_root, &plugin.surface) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if manifest.discharge_command.is_empty() {
            continue;
        }
        let Some(tool) = manifest.witness_tool.as_deref() else {
            continue;
        };
        let key = format!(
            "SUGAR_WITNESS_DISCHARGE_{}",
            tool.to_uppercase()
                .replace(|c: char| !c.is_ascii_alphanumeric(), "_")
        );
        if std::env::var_os(&key).is_none() {
            std::env::set_var(&key, manifest.discharge_command.join(" "));
        }
    }
}

fn run_admission_gate(args: &ProveArgs) -> u8 {
    run_admission_gate_with(
        &args.artifact,
        &args.proof,
        &args.policy,
        args.out.json,
        args.out.quiet,
    )
}

/// Shared admission-gate entry point. The supply-chain artifact/policy
/// verification logic is owned here (it predates the keystone `verify`
/// verb), but both `prove` (legacy alias) and `verify` (PR-9 / #1405)
/// surface the same `--artifact`/`--proof`/`--policy` flags. Threading the
/// three `Option<PathBuf>` values directly (rather than `&ProveArgs`) lets
/// `cmd_verify` reuse this without coupling to the prover's arg struct.
pub fn run_admission_gate_with(
    artifact: &Option<PathBuf>,
    proof: &Option<PathBuf>,
    policy: &Option<PathBuf>,
    json: bool,
    quiet: bool,
) -> u8 {
    match verify_artifact_or_policy(artifact, proof, policy) {
        Ok(report) => {
            let ok = report["ok"].as_bool().unwrap_or(false);
            if json {
                println!("{}", serde_json::to_string_pretty(&report).unwrap());
            } else if !quiet {
                let verdict = report["verdict"].as_str().unwrap_or("unknown");
                println!("verify admission: {verdict}");
                if let Some(reason) = report.get("reason").and_then(Value::as_str) {
                    println!("  reason: {reason}");
                }
            }
            if ok {
                crate::EXIT_OK
            } else {
                crate::EXIT_VERIFY_FAIL
            }
        }
        Err(error) => {
            eprintln!("{}: {error}", "error".red().bold());
            crate::EXIT_USER_ERROR
        }
    }
}

fn verify_artifact_or_policy(
    artifact: &Option<PathBuf>,
    proof: &Option<PathBuf>,
    policy: &Option<PathBuf>,
) -> Result<Value, String> {
    let proof_path = proof
        .as_ref()
        .ok_or_else(|| "--proof is required for admission verification".to_string())?;
    let proof = read_json_value(proof_path)?;

    let policy_report = policy
        .as_ref()
        .map(|policy_path| verify_policy_receipt(&proof, policy_path))
        .transpose()?;
    let artifact_report = artifact
        .as_ref()
        .map(|artifact_path| verify_artifact_receipt(&proof, artifact_path))
        .transpose()?;

    match (policy_report, artifact_report) {
        (Some(policy), Some(artifact)) => {
            let policy_ok = value_ok(&policy);
            let artifact_ok = value_ok(&artifact);
            let ok = policy_ok && artifact_ok;
            Ok(json!({
                "ok": ok,
                "verdict": if ok { "accepted" } else { "rejected" },
                "reason": combined_admission_reason(policy_ok, artifact_ok),
                "policy": policy,
                "artifact": artifact,
            }))
        }
        (Some(policy), None) => Ok(policy),
        (None, Some(artifact)) => Ok(artifact),
        (None, None) => Err("--artifact or --policy is required for admission verification".into()),
    }
}

fn verify_policy_receipt(proof: &Value, policy_path: &Path) -> Result<Value, String> {
    let policy = read_json_value(policy_path)?;
    let pinned = policy
        .get("policyCid")
        .and_then(Value::as_str)
        .ok_or_else(|| "policy receipt missing policyCid".to_string())?;
    let candidate = proof
        .get("policyCid")
        .and_then(Value::as_str)
        .ok_or_else(|| "proof receipt missing policyCid".to_string())?;
    let ok = pinned == candidate;
    Ok(json!({
        "ok": ok,
        "verdict": if ok { "accepted" } else { "rejected" },
        "reason": if ok { "policyCid matched" } else { "policyCid mismatch" },
        "pinnedPolicyCid": pinned,
        "candidatePolicyCid": candidate,
    }))
}

fn verify_artifact_receipt(proof: &Value, artifact_path: &Path) -> Result<Value, String> {
    let artifact_bytes = std::fs::read(artifact_path)
        .map_err(|e| format!("read artifact {}: {e}", artifact_path.display()))?;
    let observed_binary_cid = blake3_512_of(&artifact_bytes);
    let attested_binary_cid = proof
        .get("binaryCid")
        .and_then(Value::as_str)
        .ok_or_else(|| "proof receipt missing binaryCid".to_string())?;
    let ok = observed_binary_cid == attested_binary_cid;
    Ok(json!({
        "ok": ok,
        "verdict": if ok { "accepted" } else { "rejected" },
        "reason": if ok { "binaryCid matched" } else { "binaryCid mismatch" },
        "artifact": artifact_path,
        "attestedBinaryCid": attested_binary_cid,
        "observedBinaryCid": observed_binary_cid,
    }))
}

fn value_ok(value: &Value) -> bool {
    value.get("ok").and_then(Value::as_bool).unwrap_or(false)
}

fn combined_admission_reason(policy_ok: bool, artifact_ok: bool) -> &'static str {
    match (policy_ok, artifact_ok) {
        (true, true) => "policyCid and binaryCid matched",
        (false, true) => "policyCid mismatch",
        (true, false) => "binaryCid mismatch",
        (false, false) => "policyCid and binaryCid mismatch",
    }
}

fn read_json_value(path: &Path) -> Result<Value, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("parse {}: {e}", path.display()))
}
