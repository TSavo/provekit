// SPDX-License-Identifier: Apache-2.0
//
// `provekit prove` / `provekit verify`: runs the six-stage pipeline,
// or (when --kit is given) the lift-plugin-protocol conformance gate.
//
// Conformance gate (--kit=<alias>):
//
//   Resolves the alias from project/user config, then spawns the configured
//   lifter manifest via lift-plugin-protocol JSON-RPC (same dispatch as
//   `cmd_mint`). It captures the raw initialize request/response and lift
//   request/response, then runs every operational verifier from
//   `provekit_self_contracts::lift_plugin_protocol` (C1-C8) against those
//   captured messages.
//
//   Exit 0   all contracts hold.
//   Exit 1   one or more contracts violated (VERIFY_FAIL).
//   Exit 2   user error (unknown kit, lifter ENOENT, spawn failure).
//
//   The key difference from `cmd_mint`'s dispatch:
//   - ENOENT on the lifter binary is a HARD error here (we need the RPC
//     traffic to run the verifiers; an absent lifter means zero data).
//   - We capture the full JSON-RPC envelopes (with `jsonrpc`/`id`/`result`
//     keys) before stripping them, because the verifiers expect the
//     full-envelope form.
//   - The lift request uses `source_paths: ["."]` to satisfy C3's
//     non-empty-paths invariant (most lifters walk their own working
//     directory regardless of source_paths).

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use owo_colors::OwoColorize;
use provekit_canonicalizer::blake3_512_of;
use serde_json::{json, Value};

use provekit_self_contracts::lift_plugin_protocol::{
    verify_c1_initialize_protocol_version_match, verify_c2_initialize_capabilities_populated,
    verify_c3_lift_request_well_formed, verify_c4_surface_in_capabilities,
    verify_c5_response_kind_matches_layer, verify_c6_ir_document_array,
    verify_c7_diagnostics_field_is_array, verify_c8_call_edge_stream_present,
};
use provekit_verifier::{Runner, RunnerConfig};

use crate::project_config::read_project_config;
use crate::report_fmt;
use crate::ProveArgs;

// Surface + binary resolution lives in `cmd_mint`. The conformance gate must
// dispatch to the SAME (project, surface, lifter binary) tuple that `mint`
// uses, otherwise C4 fails (surface mismatch) or spawn fails (ENOENT on a
// hardcoded binary that doesn't exist). Issue #325.
//
// We delegate to `cmd_mint::resolve_kit` for surface resolution, then load
// the lift surface manifest at
// `<configured-project>/.provekit/lift/<surface>/manifest.toml` to get the
// actual binary command. No hardcoded `provekit-lift-<kit>`.
use crate::cmd_mint;

/// Adapter: drop the `lang_key` field from `cmd_mint::resolve_kit` since
/// the conformance gate doesn't write attestation files (it just runs
/// verifiers against captured RPC traffic).
fn resolve_kit(kit: &str) -> Option<(PathBuf, String)> {
    cmd_mint::resolve_kit(kit).map(|(path, surface, _lang)| (path, surface))
}

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
    /// `PROVEKIT_WITNESS_DISCHARGE_<witness_tool>` for the verifier's witness arm.
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
    for line in text.lines() {
        let line = match line.find('#') {
            Some(p) => &line[..p],
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

// ---------------------------------------------------------------------------
// RPC capture
// ---------------------------------------------------------------------------

/// Raw RPC messages captured during a lift-plugin-protocol session.
///
/// The verifiers in `lift_plugin_protocol` take:
/// - `init_params`: the `params` object from the initialize request (not the
///   full envelope, just params).
/// - `init_response`: the FULL initialize response envelope
///   (`{"jsonrpc":"2.0","id":1,"result":{...}}`).
/// - `lift_params`: the `params` object from the lift request.
/// - `lift_response`: the FULL lift response envelope.
struct CapturedRpc {
    init_params: Value,
    init_response: Value,
    lift_params: Value,
    lift_response: Value,
}

/// Read one line from `reader`, parse as JSON-RPC, assert the id matches,
/// and return the FULL response envelope (jsonrpc+id+result or error).
fn read_full_response(reader: &mut impl BufRead, expected_id: i64) -> Result<Value, String> {
    let mut line = String::new();
    let n = reader
        .read_line(&mut line)
        .map_err(|e| format!("read response id={expected_id}: {e}"))?;
    if n == 0 {
        return Err(format!(
            "plugin closed stdout before responding to id={expected_id}"
        ));
    }
    let v: Value = serde_json::from_str(line.trim())
        .map_err(|e| format!("parse JSON-RPC response id={expected_id}: {e}\n  raw: {line}"))?;
    if v.get("id").and_then(|id| id.as_i64()) != Some(expected_id) {
        return Err(format!(
            "response id mismatch: expected {expected_id}, got {:?}",
            v.get("id")
        ));
    }
    Ok(v)
}

/// Spawn the kit's lifter, drive initialize→lift→shutdown, and return the
/// captured RPC messages for verifier consumption.
///
/// Unlike `cmd_mint::dispatch`, ENOENT is a hard error here because we need
/// actual RPC traffic to run the verifiers.
fn capture_rpc(
    project_root: &std::path::Path,
    surface: &str,
    quiet: bool,
) -> Result<CapturedRpc, String> {
    let manifest = find_manifest(project_root, surface)?;
    if !quiet {
        println!(
            "{}: surface=`{}` plugin=`{}` command={:?}",
            "dispatch".green().bold(),
            surface,
            manifest.name,
            manifest.command
        );
    }

    let mut cmd = Command::new(&manifest.command[0]);
    if manifest.command.len() > 1 {
        cmd.args(&manifest.command[1..]);
    }
    // Append --rpc only if the manifest doesn't already include it.
    // Several manifests (e.g. typescript) hard-code --rpc in their command
    // array; appending unconditionally produces duplicate args, which some
    // lifters reject. (Review feedback: PR #165 / Copilot.)
    if !manifest.command.iter().any(|a| a == "--rpc") {
        cmd.arg("--rpc");
    }
    if let Some(wd) = &manifest.working_dir {
        let resolved = if wd.is_absolute() {
            wd.clone()
        } else {
            project_root.join(wd)
        };
        cmd.current_dir(resolved);
    }
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::inherit());

    let mut child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            format!(
                "lifter binary `{}` not found: cannot verify conformance without a running lifter",
                manifest.command[0]
            )
        } else {
            format!("spawn {:?}: {e}", manifest.command)
        }
    })?;

    let mut stdin = child.stdin.take().ok_or("no stdin")?;
    let stdout = child.stdout.take().ok_or("no stdout")?;
    let mut reader = BufReader::new(stdout);

    // 1. initialize
    let init_params = json!({
        "client": {"name": "provekit-cli", "version": env!("CARGO_PKG_VERSION")},
        "protocol_version": "pep/1.7.0",
        "workspace_root": project_root.canonicalize().unwrap_or_else(|_| project_root.to_path_buf()),
        "config_path": ".provekit/config.toml"
    });
    let init_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": init_params
    });
    writeln!(stdin, "{init_req}").map_err(|e| format!("write initialize: {e}"))?;
    let init_response = read_full_response(&mut reader, 1)?;

    if !quiet {
        if let Some(name) = init_response
            .get("result")
            .and_then(|r| r.get("name"))
            .and_then(|v| v.as_str())
        {
            println!("{}: plugin `{}` ready", "ok".green().bold(), name);
        }
    }

    // 2. lift: use source_paths: ["."] to satisfy C3 non-empty invariant.
    //    Most lifters ignore source_paths and walk their own working dir.
    let lift_params = json!({
        "surface": surface,
        "source_paths": ["."],
        "options": {"layer": "all"}
    });
    let lift_req = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "lift",
        "params": lift_params
    });
    writeln!(stdin, "{lift_req}").map_err(|e| format!("write lift: {e}"))?;
    let lift_response = read_full_response(&mut reader, 2)?;

    // 3. shutdown
    let shutdown_req = json!({"jsonrpc":"2.0","id":3,"method":"shutdown"});
    let _ = writeln!(stdin, "{shutdown_req}");
    drop(stdin);
    let _ = child.wait();

    Ok(CapturedRpc {
        init_params,
        init_response,
        lift_params,
        lift_response,
    })
}

// ---------------------------------------------------------------------------
// Verifier runner
// ---------------------------------------------------------------------------

/// One contract's verdict.
struct ContractResult {
    name: &'static str,
    result: Result<(), String>,
}

/// Run all 8 lift-plugin-protocol verifiers against the captured RPC messages.
fn run_verifiers(rpc: &CapturedRpc) -> Vec<ContractResult> {
    vec![
        ContractResult {
            name: "C1: initialize protocol_version_match",
            result: verify_c1_initialize_protocol_version_match(
                &rpc.init_params,
                &rpc.init_response,
            ),
        },
        ContractResult {
            name: "C2: initialize capabilities_populated",
            result: verify_c2_initialize_capabilities_populated(&rpc.init_response),
        },
        ContractResult {
            name: "C3: lift request well_formed",
            result: verify_c3_lift_request_well_formed(&rpc.lift_params),
        },
        ContractResult {
            name: "C4: lift surface_in_capabilities",
            result: verify_c4_surface_in_capabilities(&rpc.lift_params, &rpc.init_response),
        },
        ContractResult {
            name: "C5: lift response kind_matches_layer",
            result: verify_c5_response_kind_matches_layer(&rpc.lift_params, &rpc.lift_response),
        },
        ContractResult {
            name: "C6: lift response ir_document_array",
            result: verify_c6_ir_document_array(&rpc.lift_response),
        },
        ContractResult {
            name: "C7: diagnostics field_is_array",
            result: verify_c7_diagnostics_field_is_array(&rpc.lift_response),
        },
        ContractResult {
            name: "C8: call_edge_stream_present",
            result: verify_c8_call_edge_stream_present(&rpc.lift_response),
        },
    ]
}

// ---------------------------------------------------------------------------
// Kit conformance entry point
// ---------------------------------------------------------------------------

fn run_kit(kit: &str, quiet: bool, json_out: bool) -> u8 {
    let (project_root, surface) = match resolve_kit(kit) {
        Some(v) => v,
        None => {
            let aliases = cmd_mint::configured_kit_alias_names();
            eprintln!("{}", cmd_mint::format_unknown_kit_error(kit, &aliases));
            return crate::EXIT_USER_ERROR;
        }
    };

    if !project_root.exists() {
        eprintln!(
            "{}: kit project not found: {} (run from repo root)",
            "error".red().bold(),
            project_root.display()
        );
        return crate::EXIT_USER_ERROR;
    }

    // Pre-flight: print which contracts will be checked.
    if !quiet && !json_out {
        println!(
            "{}: kit=`{}` surface=`{}`: checking 8 lift-plugin-protocol contracts (C1-C8)",
            "provekit".cyan().bold(),
            kit,
            surface
        );
    }

    let rpc = match capture_rpc(&project_root, &surface, quiet) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}: {e}", "error".red().bold());
            if !quiet && !json_out {
                eprintln!();
                eprintln!("Pre-flight: the following contracts could not be evaluated because the");
                eprintln!("lifter binary was unavailable:");
                eprintln!("  C1: initialize protocol_version_match");
                eprintln!("  C2: initialize capabilities_populated");
                eprintln!("  C3: lift request well_formed");
                eprintln!("  C4: lift surface_in_capabilities");
                eprintln!("  C5: lift response kind_matches_layer");
                eprintln!("  C6: lift response ir_document_array");
                eprintln!("  C7: diagnostics field_is_array");
                eprintln!("  C8: call_edge_stream_present");
                eprintln!();
                eprintln!("Install the `{}` kit lifter and re-run.", kit);
            }
            return crate::EXIT_USER_ERROR;
        }
    };

    let results = run_verifiers(&rpc);

    if json_out {
        let arr: Vec<serde_json::Value> = results
            .iter()
            .map(|r| {
                json!({
                    "contract": r.name,
                    "pass": r.result.is_ok(),
                    "error": r.result.as_ref().err().map(|e| e.as_str()).unwrap_or(""),
                })
            })
            .collect();
        let out = json!({
            "kit": kit,
            "surface": surface,
            "contracts": arr,
        });
        match serde_json::to_string_pretty(&out) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("{}: serialize JSON: {e}", "error".red().bold());
                return crate::EXIT_USER_ERROR;
            }
        }
    } else {
        let mut all_pass = true;
        for r in &results {
            match &r.result {
                Ok(()) => {
                    if !quiet {
                        println!("  {} {}", "pass".green().bold(), r.name);
                    }
                }
                Err(msg) => {
                    all_pass = false;
                    println!("  {} {}: {}", "FAIL".red().bold(), r.name, msg);
                }
            }
        }
        if !quiet {
            println!();
            let pass_count = results.iter().filter(|r| r.result.is_ok()).count();
            let total = results.len();
            if all_pass {
                println!(
                    "{}: kit=`{}`: all {total} contracts hold",
                    "pass".green().bold(),
                    kit
                );
            } else {
                let fail_count = total - pass_count;
                println!(
                    "{}: kit=`{}`: {fail_count}/{total} contracts violated",
                    "FAIL".red().bold(),
                    kit
                );
            }
        }
    }

    let all_pass = results.iter().all(|r| r.result.is_ok());
    if all_pass {
        crate::EXIT_OK
    } else {
        crate::EXIT_VERIFY_FAIL
    }
}

// ---------------------------------------------------------------------------
// pub fn run: entry point from main.rs
// ---------------------------------------------------------------------------

pub fn run(args: ProveArgs) -> u8 {
    if args.artifact.is_some() || args.proof.is_some() || args.policy.is_some() {
        return run_admission_gate(&args);
    }

    // When --kit is given, run the conformance gate.
    if let Some(kit) = &args.kit {
        return run_kit(kit, args.out.quiet, args.out.json);
    }

    // Otherwise, run the six-stage verifier pipeline.
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

    // WITNESS DISCHARGE defaults: so `provekit prove <project>` settles execution
    // witnesses by recompute WITHOUT the caller exporting env vars. The discharge
    // command is declared in the KIT'S MANIFEST (alongside its lift `command`) and
    // resolved here through the SAME `find_manifest` dispatch lift uses -- no
    // bespoke config. Each lift surface's manifest may declare `discharge_command`
    // + `witness_tool`; we export PROVEKIT_WITNESS_DISCHARGE_<TOOL> per tool so a
    // proof carrying witnesses from multiple kits routes each to its own recompute.
    // Project dir defaults to the project being proven (the source the witness's
    // relative code paths resolve against). Explicit env vars still win.
    if std::env::var_os("PROVEKIT_WITNESS_PROJECT_DIR").is_none() {
        let p = project_root
            .canonicalize()
            .unwrap_or_else(|_| project_root.clone());
        std::env::set_var("PROVEKIT_WITNESS_PROJECT_DIR", &p);
    }
    for plugin in cfg_doc.plugins.iter().filter(|p| p.is_lift_plugin()) {
        let manifest = match find_manifest(&project_root, &plugin.surface) {
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
            "PROVEKIT_WITNESS_DISCHARGE_{}",
            tool.to_uppercase()
                .replace(|c: char| !c.is_ascii_alphanumeric(), "_")
        );
        if std::env::var_os(&key).is_none() {
            std::env::set_var(&key, manifest.discharge_command.join(" "));
        }
    }

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

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Helper: build a conformant initialize response envelope.
    fn good_init_response(surface: &str) -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "name": format!("{surface}-lifter"),
                "version": "1.0.0",
                "protocol_version": "pep/1.7.0",
                "capabilities": {
                    "authoring_surfaces": [surface],
                    "ir_version": "v1.1.0",
                },
            },
        })
    }

    // Helper: build a conformant lift response (ir-document shape).
    fn good_lift_response_ir() -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "result": {
                "kind": "ir-document",
                "ir": [{"kind": "contract", "name": "example_contract", "outBinding": "out"}],
                "diagnostics": [],
            },
        })
    }

    // Helper: build a conformant CapturedRpc with all 8 contracts passing.
    fn good_rpc(surface: &str) -> CapturedRpc {
        CapturedRpc {
            init_params: json!({
                "client": {"name": "provekit-cli", "version": "0.1.0"},
                "protocol_version": "pep/1.7.0",
                "workspace_root": "/tmp",
                "config_path": ".provekit/config.toml"
            }),
            init_response: good_init_response(surface),
            lift_params: json!({
                "surface": surface,
                "source_paths": ["."],
                "options": {"layer": "all"}
            }),
            lift_response: good_lift_response_ir(),
        }
    }

    #[test]
    fn all_8_verifiers_pass_on_conformant_rpc() {
        let rpc = good_rpc("rust");
        let results = run_verifiers(&rpc);
        assert_eq!(results.len(), 8, "must have exactly 8 contract results");
        for r in &results {
            assert!(
                r.result.is_ok(),
                "contract `{}` should pass on conformant RPC, got: {:?}",
                r.name,
                r.result
            );
        }
    }

    #[test]
    fn c1_violation_caught_wrong_protocol_version() {
        let mut rpc = good_rpc("rust");
        // Drift: response returns a different protocol version without error code.
        rpc.init_response = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "name": "bad-plugin",
                "version": "0.0.1",
                "protocol_version": "provekit-lift/0",
                "capabilities": {
                    "authoring_surfaces": ["rust"],
                    "ir_version": "v1.0.0",
                },
            },
        });
        let results = run_verifiers(&rpc);
        let c1 = &results[0];
        assert_eq!(c1.name, "C1: initialize protocol_version_match");
        assert!(
            c1.result.is_err(),
            "C1 should fail when protocol versions disagree silently"
        );
    }

    #[test]
    fn c2_violation_caught_empty_authoring_surfaces() {
        let mut rpc = good_rpc("rust");
        rpc.init_response = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "name": "bad-plugin",
                "version": "0.0.1",
                "protocol_version": "pep/1.7.0",
                "capabilities": {
                    "authoring_surfaces": [],
                    "ir_version": "v1.0.0",
                },
            },
        });
        let results = run_verifiers(&rpc);
        let c2 = &results[1];
        assert_eq!(c2.name, "C2: initialize capabilities_populated");
        assert!(
            c2.result.is_err(),
            "C2 should fail on empty authoring_surfaces"
        );
    }

    #[test]
    fn c3_violation_caught_empty_source_paths() {
        let mut rpc = good_rpc("rust");
        // Drift: empty source_paths (our CLI never sends this, but a buggy
        // caller might; the verifier catches it).
        rpc.lift_params = json!({
            "surface": "rust",
            "source_paths": [],
        });
        let results = run_verifiers(&rpc);
        let c3 = &results[2];
        assert_eq!(c3.name, "C3: lift request well_formed");
        assert!(c3.result.is_err(), "C3 should fail on empty source_paths");
    }

    #[test]
    fn c4_violation_caught_surface_not_in_capabilities() {
        let mut rpc = good_rpc("rust");
        // Drift: request asks for a surface not declared in init capabilities.
        rpc.lift_params = json!({
            "surface": "nonexistent-surface",
            "source_paths": ["."],
        });
        let results = run_verifiers(&rpc);
        let c4 = &results[3];
        assert_eq!(c4.name, "C4: lift surface_in_capabilities");
        assert!(
            c4.result.is_err(),
            "C4 should fail when surface not in capabilities"
        );
    }

    #[test]
    fn c5_violation_caught_unknown_kind() {
        let mut rpc = good_rpc("rust");
        rpc.lift_response = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "result": {
                "kind": "unknown-response-shape",
                "data": {},
            },
        });
        let results = run_verifiers(&rpc);
        let c5 = &results[4];
        assert_eq!(c5.name, "C5: lift response kind_matches_layer");
        assert!(c5.result.is_err(), "C5 should fail on unknown kind");
    }

    #[test]
    fn c6_violation_caught_ir_document_without_ir_array() {
        let mut rpc = good_rpc("rust");
        rpc.lift_response = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "result": {
                "kind": "ir-document",
                "ir": "not-an-array",
            },
        });
        let results = run_verifiers(&rpc);
        let c6 = &results[5];
        assert_eq!(c6.name, "C6: lift response ir_document_array");
        assert!(c6.result.is_err(), "C6 should fail when ir is not an array");
    }

    #[test]
    fn c7_violation_caught_diagnostics_not_array() {
        let mut rpc = good_rpc("rust");
        rpc.lift_response = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "result": {
                "kind": "ir-document",
                "ir": [],
                "diagnostics": "should-be-array",
            },
        });
        let results = run_verifiers(&rpc);
        let c7 = &results[6];
        assert_eq!(c7.name, "C7: diagnostics field_is_array");
        assert!(
            c7.result.is_err(),
            "C7 should fail when diagnostics is not an array"
        );
    }

    #[test]
    fn c8_violation_caught_proof_envelope_without_call_edges() {
        let mut rpc = good_rpc("rust");
        // proof-envelope with non-empty members but no call_edges.
        rpc.lift_response = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "result": {
                "kind": "proof-envelope",
                "members": {
                    "blake3-512:deadbeef": "base64-bytes",
                },
                "filename_cid": "blake3-512:aabbcc",
                // call_edges intentionally absent
            },
        });
        let results = run_verifiers(&rpc);
        let c8 = &results[7];
        assert_eq!(c8.name, "C8: call_edge_stream_present");
        assert!(
            c8.result.is_err(),
            "C8 should fail when proof-envelope has no call_edges"
        );
    }

    #[test]
    fn conformance_gate_alias_resolution_uses_configured_mint_resolver() {
        use crate::project_config::{KitAliasEntry, ProjectConfig};

        let project_cfg = ProjectConfig {
            kits: vec![KitAliasEntry {
                alias: "swift".to_string(),
                project: "implementations/swift".to_string(),
                surface: "swift-self-contracts".to_string(),
                lang: "swift".to_string(),
            }],
            ..ProjectConfig::default()
        };

        let resolved = cmd_mint::resolve_kit_from_configs(
            "swift",
            Path::new("/workspace"),
            &project_cfg,
            &ProjectConfig::default(),
        )
        .expect("configured swift kit alias must resolve");

        assert_eq!(
            resolved.project_root,
            PathBuf::from("/workspace/implementations/swift")
        );
        assert_eq!(
            resolved.surface, "swift-self-contracts",
            "issue #325: prove must use the configured surface so manifest lookup finds the real lifter binary"
        );
    }

    /// Issue #325 acceptance: the binary command for `--kit=swift` must come
    /// from the lift surface manifest, NOT a hardcoded `provekit-lift-swift`.
    /// We assert this by reading the actual manifest from the repo and
    /// checking it does not match the old hardcoded shape.
    #[test]
    fn swift_manifest_command_is_not_hardcoded_provekit_lift_swift() {
        // Walk up to repo root: tests run in implementations/rust/provekit-cli
        // (CARGO_MANIFEST_DIR), so go up three levels.
        let crate_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let repo_root = crate_dir
            .parent() // implementations/rust
            .and_then(|p| p.parent()) // implementations
            .and_then(|p| p.parent()) // repo root
            .expect("locate repo root from CARGO_MANIFEST_DIR");
        let manifest = repo_root
            .join("implementations/swift/.provekit/lift/swift-self-contracts/manifest.toml");
        if !manifest.exists() {
            // Be lenient: skip if running outside a full checkout.
            eprintln!(
                "skipping: swift manifest not present at {}",
                manifest.display()
            );
            return;
        }
        let parsed = parse_manifest(&manifest).expect("swift-self-contracts manifest must parse");
        assert!(
            !parsed.command.is_empty(),
            "swift manifest must declare a command"
        );
        assert_ne!(
            parsed.command[0], "provekit-lift-swift",
            "issue #325: swift binary must NOT be the hardcoded `provekit-lift-swift` (that binary is never built); the manifest declares the real binary"
        );
    }

    #[test]
    fn missing_one_contract_yields_verify_fail() {
        // A kit that passes C1-C7 but fails C8 (no call_edges in proof-envelope).
        // run_verifiers returns 8 results; one is Err; all_pass is false.
        let mut rpc = good_rpc("rust");
        rpc.lift_response = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "result": {
                "kind": "proof-envelope",
                "members": {"blake3-512:abc": "bytes"},
                "filename_cid": "blake3-512:def",
                // call_edges absent: C8 violation
            },
        });
        let results = run_verifiers(&rpc);
        let all_pass = results.iter().all(|r| r.result.is_ok());
        assert!(
            !all_pass,
            "overall gate must fail when one contract is violated"
        );
        let failures: Vec<&str> = results
            .iter()
            .filter(|r| r.result.is_err())
            .map(|r| r.name)
            .collect();
        assert!(
            failures.contains(&"C8: call_edge_stream_present"),
            "C8 must be in the failure list; got: {failures:?}"
        );
    }
}
