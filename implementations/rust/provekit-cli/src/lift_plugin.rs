// SPDX-License-Identifier: Apache-2.0
//
// Lift-plugin RPC client.
//
// This module owns exactly one protocol boundary: read a lift surface manifest,
// spawn the configured lifter, speak initialize/lift/shutdown, and return the
// raw lift result. Callers decide what to do with that result.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use owo_colors::OwoColorize;
use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub(crate) struct LiftPluginManifest {
    pub name: String,
    pub command: Vec<String>,
    pub working_dir: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub(crate) struct LiftPluginSession {
    pub response: Value,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct LiftPluginOptions {
    pub identify_only: bool,
}

#[derive(Debug, Clone)]
pub(crate) enum LiftPluginError {
    MissingBinary { binary: String },
    Failed(String),
}

impl std::fmt::Display for LiftPluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingBinary { binary } => write!(f, "lifter binary `{binary}` not found"),
            Self::Failed(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for LiftPluginError {}

pub(crate) fn dispatch_lift(
    project_root: &Path,
    surface: &str,
    options: LiftPluginOptions,
    quiet: bool,
) -> Result<LiftPluginSession, LiftPluginError> {
    let started = Instant::now();
    let manifest = find_manifest(project_root, surface).map_err(LiftPluginError::Failed)?;
    trace_log(format!(
        "lift rpc start surface={surface} project={} plugin={} command={:?}",
        project_root.display(),
        manifest.name,
        manifest.command
    ));
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

    trace_log(format!(
        "lift rpc spawn surface={surface} command={:?}",
        manifest.command
    ));
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(LiftPluginError::MissingBinary {
                binary: manifest.command[0].clone(),
            });
        }
        Err(error) => {
            return Err(LiftPluginError::Failed(format!(
                "spawn {:?}: {error}",
                manifest.command
            )))
        }
    };
    trace_log(format!(
        "lift rpc spawned surface={surface} pid={}",
        child.id()
    ));

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| LiftPluginError::Failed("lift plugin stdin unavailable".into()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| LiftPluginError::Failed("lift plugin stdout unavailable".into()))?;
    let mut reader = BufReader::new(stdout);

    let init_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "client": {"name": "provekit-cli", "version": env!("CARGO_PKG_VERSION")},
            "protocol_version": "provekit-lift/1",
            "workspace_root": project_root.canonicalize().unwrap_or_else(|_| project_root.to_path_buf()),
            "config_path": ".provekit/config.toml"
        }
    });
    trace_log(format!("lift rpc send initialize surface={surface} id=1"));
    writeln!(stdin, "{init_req}")
        .map_err(|e| LiftPluginError::Failed(format!("write lift initialize: {e}")))?;

    trace_log(format!(
        "lift rpc wait initialize response surface={surface} id=1"
    ));
    let init_resp = read_response(&mut reader, 1).map_err(LiftPluginError::Failed)?;
    trace_log(format!(
        "lift rpc got initialize response surface={surface} elapsed={:?}",
        started.elapsed()
    ));
    if !quiet {
        if let Some(name) = init_resp.get("name").and_then(|v| v.as_str()) {
            println!("{}: plugin `{}` ready", "ok".green().bold(), name);
        }
    }

    let lift_params = build_lift_params(project_root, surface, options);
    let lift_req = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "lift",
        "params": lift_params
    });
    trace_log(format!("lift rpc send lift surface={surface} id=2"));
    writeln!(stdin, "{lift_req}")
        .map_err(|e| LiftPluginError::Failed(format!("write lift request: {e}")))?;
    trace_log(format!(
        "lift rpc wait lift response surface={surface} id=2"
    ));
    let response = read_response(&mut reader, 2).map_err(LiftPluginError::Failed)?;
    trace_log(format!(
        "lift rpc got lift response surface={surface} elapsed={:?}",
        started.elapsed()
    ));

    let shutdown_req = json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "shutdown"
    });
    trace_log(format!("lift rpc send shutdown surface={surface} id=3"));
    let _ = writeln!(stdin, "{shutdown_req}");
    drop(stdin);
    trace_log(format!("lift rpc wait child exit surface={surface}"));
    let status = child
        .wait()
        .map_err(|e| LiftPluginError::Failed(format!("wait lift plugin: {e}")))?;
    trace_log(format!(
        "lift rpc child exit surface={surface} status={status:?} elapsed={:?}",
        started.elapsed()
    ));
    if !status.success() {
        return Err(LiftPluginError::Failed(format!(
            "lift plugin exited {status} after {:?}",
            started.elapsed()
        )));
    }

    Ok(LiftPluginSession { response })
}

fn parse_manifest(path: &Path) -> Result<LiftPluginManifest, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let mut manifest = LiftPluginManifest {
        name: String::new(),
        command: Vec::new(),
        working_dir: None,
    };
    for line in text.lines() {
        let line = match line.find('#') {
            Some(pos) => &line[..pos],
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
            "name" => manifest.name = val.trim_matches('"').to_string(),
            "working_dir" => manifest.working_dir = Some(PathBuf::from(val.trim_matches('"'))),
            "command" => {
                let inner = val.trim_matches(|c| c == '[' || c == ']');
                manifest.command = inner
                    .split(',')
                    .map(|s| s.trim().trim_matches('"').to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            _ => {}
        }
    }
    if manifest.command.is_empty() {
        return Err(format!("manifest {} has no `command`", path.display()));
    }
    Ok(manifest)
}

fn find_manifest(project_root: &Path, surface: &str) -> Result<LiftPluginManifest, String> {
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

pub(crate) fn build_lift_params(
    project_root: &Path,
    surface: &str,
    options: LiftPluginOptions,
) -> Value {
    let workspace_root = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    let layer = if options.identify_only {
        "identify-only"
    } else {
        "all"
    };
    json!({
        "surface": surface,
        "workspace_root": workspace_root,
        "config_path": ".provekit/config.toml",
        "source_paths": ["."],
        "options": {
            "layer": layer,
            "identifyOnly": options.identify_only,
        }
    })
}

fn read_response(reader: &mut impl BufRead, id: i64) -> Result<Value, String> {
    let mut line = String::new();
    let n = reader
        .read_line(&mut line)
        .map_err(|e| format!("read lift response: {e}"))?;
    if n == 0 {
        return Err("lift plugin closed stdout before responding".into());
    }
    let value: Value = serde_json::from_str(line.trim())
        .map_err(|e| format!("parse lift JSON-RPC response: {e}\n  raw: {line}"))?;
    if value.get("id").and_then(Value::as_i64) != Some(id) {
        return Err(format!(
            "lift response id mismatch: expected {id}, got {value:?}"
        ));
    }
    if let Some(error) = value.get("error") {
        return Err(format!("lift plugin returned error: {error}"));
    }
    value
        .get("result")
        .cloned()
        .ok_or_else(|| "lift response missing `result`".into())
}

fn trace_enabled() -> bool {
    std::env::var_os("PROVEKIT_CLI_TRACE").is_some()
}

fn trace_log(message: impl std::fmt::Display) {
    if trace_enabled() {
        eprintln!("provekit trace: {message}");
    }
}
