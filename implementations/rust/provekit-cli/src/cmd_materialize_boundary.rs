// SPDX-License-Identifier: Apache-2.0
//
// `provekit materialize-boundary`: kit-owned by-reference boundary fill.
//
// The rust mirror of the python kit's `bind_rpc.py::materialize_impl`. Finds
// `#[provekit::boundary(library, call)]` stubs in consumer source, asks the
// language kit (over the SAME RPC transport `recognize` uses) to resolve each
// bound vendor function's REAL body through the SOURCE ORACLE — body returned
// IFF the on-disk vendor source recomputes to the pinned CIDs in the frozen
// vendor `.proof`, else REFUSED — and rewrites the stub body in place.
//
// This is DELIBERATELY a separate verb from `materialize` (the realize/
// LowerKit/concept-emit path): that path emits target source where no real
// source exists; this one only RESOLVES real source that already exists on
// disk, refusing when it does not. Exact-or-refuse, no silent loss.
//
// The CLI never reads `.proof` files or reconstructs bodies; proof/package/
// source resolution belongs to the kit behind the RPC seam. Mirrors
// `cmd_recognize.rs` transport exactly (manifest spawn → one JSON-RPC line).

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use clap::Parser;
use owo_colors::OwoColorize;
use serde_json::{json, Value};

use crate::{OutputFlags, EXIT_OK, EXIT_USER_ERROR, EXIT_VERIFY_FAIL};

/// Arguments accepted by `provekit materialize-boundary`.
#[derive(Parser, Debug, Clone, Default)]
pub struct MaterializeBoundaryArgs {
    /// Project root containing `.provekit/lift/<surface>/manifest.toml`, the
    /// `#[provekit::boundary]` stubs, AND the resolved vendor `.proof`s
    /// (`.provekit/imports/` + cargo deps). Defaults to current directory.
    #[arg(long)]
    pub project: Option<PathBuf>,
    /// Consumer source paths (relative to project root) to scan for boundary
    /// stubs. Repeatable. e.g. `--source src/lib.rs`.
    #[arg(long = "source")]
    pub source_paths: Vec<String>,
    /// Lift surface name (the kit serving `provekit.plugin.materialize`). If
    /// omitted, resolves from the single configured lift manifest.
    #[arg(long)]
    pub surface: Option<String>,
    /// Root the frozen memento's `file` path is relative to (where the LIVE
    /// vendor source lives on disk). Defaults to the project root. The oracle
    /// resolves the frozen pin against this live source; drift is REFUSED.
    #[arg(long = "vendor-root")]
    pub vendor_root: Option<PathBuf>,
    /// Rewrite the stub bodies in place. Omitted means dry-run (report only).
    #[arg(long)]
    pub write: bool,
    #[command(flatten)]
    pub out: OutputFlags,
}

pub fn run(args: MaterializeBoundaryArgs) -> u8 {
    let project_root = match args
        .project
        .clone()
        .map(|p| p.canonicalize().unwrap_or(p))
        .or_else(|| std::env::current_dir().ok())
    {
        Some(p) => p,
        None => {
            eprintln!("{}: cannot resolve project root", "error".red().bold());
            return EXIT_USER_ERROR;
        }
    };

    let surface = match resolve_surface(args.surface.as_deref(), &project_root) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}: {e}", "error".red().bold());
            return EXIT_USER_ERROR;
        }
    };
    let manifest = match find_plugin_manifest(&project_root, &surface) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{}: {e}", "error".red().bold());
            return EXIT_USER_ERROR;
        }
    };

    let vendor_root = args
        .vendor_root
        .clone()
        .map(|p| p.canonicalize().unwrap_or(p))
        .unwrap_or_else(|| project_root.clone());

    if !args.out.json && !args.out.quiet {
        eprintln!(
            "{}: surface=`{}` sources={} vendor_root={}",
            "materialize-boundary".green().bold(),
            surface,
            args.source_paths.len(),
            vendor_root.display(),
        );
    }

    let req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "provekit.plugin.materialize",
        "params": {
            "project_root": project_root.to_string_lossy(),
            "source_paths": args.source_paths,
            "vendor_root": vendor_root.to_string_lossy(),
            "write": args.write,
        }
    });

    let results = match invoke_plugin(&manifest, &project_root, &req) {
        Ok(resp) => {
            if let Some(err) = resp.get("error") {
                eprintln!(
                    "{}: materialize kit error: {err}",
                    "error".red().bold()
                );
                return EXIT_VERIFY_FAIL;
            }
            match resp
                .get("result")
                .and_then(|r| r.get("results"))
                .and_then(Value::as_array)
                .cloned()
            {
                Some(a) => a,
                None => {
                    eprintln!(
                        "{}: kit response missing result.results: {resp}",
                        "error".red().bold()
                    );
                    return EXIT_VERIFY_FAIL;
                }
            }
        }
        Err(e) => {
            eprintln!(
                "{}: materialize dispatch failed: {e}",
                "error".red().bold()
            );
            return EXIT_VERIFY_FAIL;
        }
    };

    let refused = results
        .iter()
        .filter(|r| r.get("outcome").and_then(Value::as_str) == Some("refused"))
        .count();
    let materialized = results
        .iter()
        .filter(|r| r.get("outcome").and_then(Value::as_str) == Some("materialized"))
        .count();

    if args.out.json {
        let out = json!({ "results": results });
        println!(
            "{}",
            serde_json::to_string_pretty(&out).unwrap_or_else(|_| out.to_string())
        );
    } else {
        for r in &results {
            let outcome = r.get("outcome").and_then(Value::as_str).unwrap_or("?");
            let file = r.get("file").and_then(Value::as_str).unwrap_or("?");
            match outcome {
                "materialized" => {
                    let syms: Vec<String> = r
                        .get("materialized")
                        .and_then(Value::as_array)
                        .map(|a| {
                            a.iter()
                                .filter_map(|m| {
                                    m.get("symbol").and_then(Value::as_str).map(str::to_string)
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    println!(
                        "  {} {} [{}]",
                        "materialized".green().bold(),
                        file,
                        syms.join(", ")
                    );
                }
                "refused" => {
                    let reason = r.get("reason").and_then(Value::as_str).unwrap_or("");
                    println!("  {} {}: {}", "REFUSED".red().bold(), file, reason);
                }
                other => println!("  {other} {file}"),
            }
        }
        println!(
            "materialize-boundary: {} materialized, {} refused",
            materialized, refused
        );
    }

    // A refusal is a loud, expected outcome (drift detected) — but it is NOT a
    // successful fill, so the verb returns non-zero so a caller/CI sees it.
    if refused > 0 {
        EXIT_VERIFY_FAIL
    } else {
        EXIT_OK
    }
}

fn resolve_surface(explicit: Option<&str>, project_root: &Path) -> Result<String, String> {
    if let Some(surface) = explicit.map(str::trim).filter(|s| !s.is_empty()) {
        return Ok(surface.to_string());
    }
    // Discover the single lift manifest under .provekit/lift/<surface>/.
    let lift_dir = project_root.join(".provekit").join("lift");
    let mut surfaces: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&lift_dir) {
        for entry in entries.flatten() {
            if entry.path().join("manifest.toml").is_file() {
                if let Some(name) = entry.file_name().to_str() {
                    surfaces.push(name.to_string());
                }
            }
        }
    }
    surfaces.sort();
    match surfaces.as_slice() {
        [one] => Ok(one.clone()),
        [] => Err(format!(
            "no lift manifest under {}; pass --surface",
            lift_dir.display()
        )),
        _ => Err(format!(
            "multiple lift surfaces ({}); pass --surface",
            surfaces.join(", ")
        )),
    }
}

struct PluginManifest {
    command: Vec<PathBuf>,
    working_dir: Option<PathBuf>,
}

fn find_plugin_manifest(project_root: &Path, surface: &str) -> Result<PluginManifest, String> {
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
        "no plugin manifest for surface `{surface}` (looked in .provekit/lift/{surface}/manifest.toml)"
    ))
}

fn parse_manifest(path: &Path) -> Result<PluginManifest, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("read manifest {}: {e}", path.display()))?;
    let mut command: Vec<PathBuf> = Vec::new();
    let mut working_dir: Option<PathBuf> = None;
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("command") {
            if let Some(arr_text) = rest.split_once('=').map(|(_, v)| v.trim()) {
                command = parse_toml_string_array(arr_text);
            }
        } else if let Some(rest) = line.strip_prefix("working_dir") {
            if let Some(val) = rest.split_once('=').map(|(_, v)| v.trim()) {
                if let Some(s) = strip_quotes(val) {
                    working_dir = Some(PathBuf::from(s));
                }
            }
        }
    }
    if command.is_empty() {
        return Err(format!("manifest {} declares no command", path.display()));
    }
    Ok(PluginManifest {
        command,
        working_dir,
    })
}

fn parse_toml_string_array(text: &str) -> Vec<PathBuf> {
    let trimmed = text.trim().trim_start_matches('[').trim_end_matches(']');
    trimmed
        .split(',')
        .filter_map(|s| strip_quotes(s.trim()).map(PathBuf::from))
        .collect()
}

fn strip_quotes(s: &str) -> Option<&str> {
    s.strip_prefix('"').and_then(|s| s.strip_suffix('"'))
}

/// Spawn the plugin binary with the manifest's command + working_dir, send the
/// JSON-RPC request, read one JSON line response, shutdown. Mirrors
/// `cmd_recognize::invoke_plugin`.
fn invoke_plugin(
    manifest: &PluginManifest,
    project_root: &Path,
    request: &Value,
) -> Result<Value, String> {
    let (program, args) = manifest
        .command
        .split_first()
        .ok_or("plugin manifest command is empty")?;

    let mut cmd = Command::new(program);
    cmd.args(args);
    if let Some(working_dir) = &manifest.working_dir {
        let resolved = if working_dir.is_absolute() {
            working_dir.clone()
        } else {
            project_root.join(working_dir)
        };
        cmd.current_dir(resolved);
    } else {
        cmd.current_dir(project_root);
    }
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| format!("spawn plugin: {e}"))?;
    {
        let stdin = child.stdin.as_mut().ok_or("plugin stdin closed")?;
        let req_line = serde_json::to_string(request).map_err(|e| e.to_string())?;
        writeln!(stdin, "{req_line}").map_err(|e| format!("write request: {e}"))?;
        let shutdown = json!({"jsonrpc":"2.0","id":2,"method":"shutdown","params":{}});
        writeln!(stdin, "{}", serde_json::to_string(&shutdown).unwrap())
            .map_err(|e| format!("write shutdown: {e}"))?;
    }

    let stdout = child.stdout.take().ok_or("plugin stdout closed")?;
    let mut reader = BufReader::new(stdout);
    let mut response_line = String::new();
    reader
        .read_line(&mut response_line)
        .map_err(|e| format!("read response: {e}"))?;
    let _ = child.wait();
    if response_line.trim().is_empty() {
        return Err("plugin response was empty".to_string());
    }
    serde_json::from_str(&response_line).map_err(|e| {
        format!(
            "parse response: {e}; raw={}",
            response_line.trim_end_matches('\n')
        )
    })
}
