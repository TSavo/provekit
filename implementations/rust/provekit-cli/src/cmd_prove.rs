// SPDX-License-Identifier: Apache-2.0
//
// `provekit prove` / `provekit verify` — runs the six-stage pipeline,
// or (when --kit is given) the lift-plugin-protocol conformance gate.
//
// Conformance gate (--kit=<kit>):
//
//   Spawns the kit's lifter via the lift-plugin-protocol JSON-RPC (same
//   dispatch as `cmd_mint`), captures the raw initialize request/response
//   and lift request/response, then runs every operational verifier from
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
use std::path::PathBuf;
use std::process::{Command, Stdio};

use owo_colors::OwoColorize;
use serde_json::{json, Value};

use provekit_self_contracts::lift_plugin_protocol::{
    verify_c1_initialize_protocol_version_match,
    verify_c2_initialize_capabilities_populated,
    verify_c3_lift_request_well_formed,
    verify_c4_surface_in_capabilities,
    verify_c5_response_kind_in_set,
    verify_c6_ir_document_array,
    verify_c7_diagnostics_field_is_array,
    verify_c8_call_edge_stream_present,
};
use provekit_verifier::{Runner, RunnerConfig};

use crate::report_fmt;
use crate::ProveArgs;

// Re-use the same KIT_TABLE from cmd_mint for consistency.
// The table is duplicated here to avoid a cross-module dependency; any
// change to one must be reflected in the other.
const KIT_TABLE: &[(&str, &str, &str)] = &[
    // (kit_alias, project_subdir, surface)
    ("rust",       "rust",       "rust"),
    ("go",         "go",         "go"),
    ("cpp",        "cpp",        "cpp"),
    ("ts",         "typescript", "typescript"),
    ("csharp",     "csharp",     "csharp"),
    ("swift",      "swift",      "swift"),
    ("java",       "java",       "java"),
    ("python",     "python",     "python"),
    ("ruby",       "ruby",       "ruby"),
    ("zig",        "zig",        "zig"),
    ("c",          "c",          "c"),
];

fn resolve_kit(kit: &str) -> Option<(PathBuf, String)> {
    KIT_TABLE
        .iter()
        .find(|(alias, _, _)| *alias == kit)
        .map(|(_, subdir, surface)| {
            (
                PathBuf::from("implementations").join(subdir),
                surface.to_string(),
            )
        })
}

// ---------------------------------------------------------------------------
// Plugin manifest (mirrors cmd_mint — kept local to avoid coupling)
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct PluginManifest {
    name: String,
    command: Vec<String>,
    working_dir: Option<PathBuf>,
}

fn parse_manifest(path: &std::path::Path) -> Result<PluginManifest, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("read {}: {e}", path.display()))?;
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
            "command" => {
                let inner = val.trim_matches(|c| c == '[' || c == ']');
                m.command = inner
                    .split(',')
                    .map(|s| s.trim().trim_matches('"').to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
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
/// - `init_params`  — the `params` object from the initialize request (not the
///   full envelope, just params).
/// - `init_response` — the FULL initialize response envelope
///   (`{"jsonrpc":"2.0","id":1,"result":{...}}`).
/// - `lift_params` — the `params` object from the lift request.
/// - `lift_response` — the FULL lift response envelope.
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
    let v: Value = serde_json::from_str(line.trim()).map_err(|e| {
        format!(
            "parse JSON-RPC response id={expected_id}: {e}\n  raw: {line}"
        )
    })?;
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
    cmd.arg("--rpc");
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
                "lifter binary `{}` not found — cannot verify conformance without a running lifter",
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
        "protocol_version": "provekit-lift/1",
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

    // 2. lift — use source_paths: ["."] to satisfy C3 non-empty invariant.
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
            name: "C5: lift response kind_in_set",
            result: verify_c5_response_kind_in_set(&rpc.lift_response),
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
            let known: Vec<&str> = KIT_TABLE.iter().map(|(a, _, _)| *a).collect();
            eprintln!(
                "{}: unknown kit `{}`; known kits: {}",
                "error".red().bold(),
                kit,
                known.join(", ")
            );
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
            "{}: kit=`{}` surface=`{}` — checking 8 lift-plugin-protocol contracts (C1-C8)",
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
                eprintln!("  C5: lift response kind_in_set");
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
                    "{}: kit=`{}` — all {total} contracts hold",
                    "pass".green().bold(),
                    kit
                );
            } else {
                let fail_count = total - pass_count;
                println!(
                    "{}: kit=`{}` — {fail_count}/{total} contracts violated",
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
// pub fn run — entry point from main.rs
// ---------------------------------------------------------------------------

pub fn run(args: ProveArgs) -> u8 {
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

    let cfg = RunnerConfig {
        project_root: project_root.clone(),
        z3_path: args.z3,
        ..Default::default()
    };
    let runner = Runner::new(cfg);
    let report = runner.run();

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
                "protocol_version": "provekit-lift/1",
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
                "protocol_version": "provekit-lift/1",
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
                "protocol_version": "provekit-lift/1",
                "capabilities": {
                    "authoring_surfaces": [],
                    "ir_version": "v1.0.0",
                },
            },
        });
        let results = run_verifiers(&rpc);
        let c2 = &results[1];
        assert_eq!(c2.name, "C2: initialize capabilities_populated");
        assert!(c2.result.is_err(), "C2 should fail on empty authoring_surfaces");
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
        assert!(c4.result.is_err(), "C4 should fail when surface not in capabilities");
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
        assert_eq!(c5.name, "C5: lift response kind_in_set");
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
        assert!(c7.result.is_err(), "C7 should fail when diagnostics is not an array");
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
        assert!(c8.result.is_err(), "C8 should fail when proof-envelope has no call_edges");
    }

    #[test]
    fn resolve_kit_rust_resolves() {
        let (path, surface) = resolve_kit("rust").expect("rust must resolve");
        assert_eq!(path, PathBuf::from("implementations/rust"));
        assert_eq!(surface, "rust");
    }

    #[test]
    fn resolve_kit_ts_resolves() {
        let (path, surface) = resolve_kit("ts").expect("ts must resolve");
        assert_eq!(path, PathBuf::from("implementations/typescript"));
        assert_eq!(surface, "typescript");
    }

    #[test]
    fn resolve_kit_all_11_kits() {
        let kits = [
            "rust", "go", "cpp", "ts", "csharp", "swift", "java", "python", "ruby", "zig", "c",
        ];
        for kit in kits {
            assert!(resolve_kit(kit).is_some(), "kit `{kit}` must resolve");
        }
    }

    #[test]
    fn resolve_kit_unknown_returns_none() {
        assert!(resolve_kit("haskell").is_none());
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
                // call_edges absent — C8 violation
            },
        });
        let results = run_verifiers(&rpc);
        let all_pass = results.iter().all(|r| r.result.is_ok());
        assert!(!all_pass, "overall gate must fail when one contract is violated");
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
