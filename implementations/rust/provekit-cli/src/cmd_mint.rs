// SPDX-License-Identifier: Apache-2.0
//
// `provekit mint` — the lift-plugin protocol dispatcher.
//
// Reads `.provekit/config.toml` for the configured authoring surface,
// resolves the matching plugin manifest, spawns the plugin's binary
// with `--rpc`, exchanges NDJSON-over-stdio JSON-RPC, and writes the
// resulting `.proof` to disk.
//
// Spec: protocol/specs/2026-04-30-lift-plugin-protocol.md (draft for v1.2.0).
//
// Three response shapes are defined in the spec; this v1 implements
// only `proof-envelope` (shape c) — the plugin owns the full pipeline
// and returns a complete .proof. Shapes (a) ir-document and (b)
// signed-mementos are spec'd but unimplemented in this dispatcher;
// adding them is additive, requires no client breakage.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use base64::Engine;
use clap::Parser;
use owo_colors::OwoColorize;
use serde_json::{json, Value};

use crate::project_config::{read_project_config, read_user_config};
use crate::OutputFlags;
use crate::{EXIT_OK, EXIT_USER_ERROR, EXIT_VERIFY_FAIL};

#[derive(Parser, Debug, Clone)]
pub struct MintArgs {
    /// Project root containing `.provekit/config.toml`. Defaults to current dir.
    #[arg(long)]
    pub project: Option<PathBuf>,
    /// Override the authoring surface (otherwise read from config).
    #[arg(long)]
    pub surface: Option<String>,
    /// Output directory for the produced `.proof` file. Defaults to current dir.
    #[arg(long)]
    pub out: Option<PathBuf>,
    #[command(flatten)]
    pub flags: OutputFlags,
}

/// Plugin manifest read from `.../lift/<name>/manifest.toml`.
#[derive(Debug, Default)]
struct PluginManifest {
    name: String,
    command: Vec<String>,
    working_dir: Option<PathBuf>,
}

fn parse_manifest(path: &Path) -> Result<PluginManifest, String> {
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

fn find_manifest(project_root: &Path, surface: &str) -> Result<PluginManifest, String> {
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

fn dispatch(
    project_root: &Path,
    surface: &str,
    out_dir: &Path,
    quiet: bool,
) -> Result<(String, usize), String> {
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

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("spawn {:?}: {e}", manifest.command))?;
    let mut stdin = child.stdin.take().ok_or("no stdin")?;
    let stdout = child.stdout.take().ok_or("no stdout")?;
    let mut reader = BufReader::new(stdout);

    // 1. initialize
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
    writeln!(stdin, "{init_req}").map_err(|e| format!("write initialize: {e}"))?;

    let init_resp = read_response(&mut reader, 1)?;
    if !quiet {
        if let Some(name) = init_resp.get("name").and_then(|v| v.as_str()) {
            println!("{}: plugin `{}` ready", "ok".green().bold(), name);
        }
    }

    // 2. lift
    let lift_req = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "lift",
        "params": {
            "surface": surface,
            "source_paths": [],
            "options": {"layer": "all"}
        }
    });
    writeln!(stdin, "{lift_req}").map_err(|e| format!("write lift: {e}"))?;
    let lift_resp = read_response(&mut reader, 2)?;

    // 3. shutdown
    let shutdown_req = json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "shutdown"
    });
    let _ = writeln!(stdin, "{shutdown_req}");
    drop(stdin);
    let _ = child.wait();

    // Process response: shape `proof-envelope`
    let kind = lift_resp.get("kind").and_then(|v| v.as_str()).ok_or(
        "lift response missing `kind` field; only proof-envelope shape supported in v1",
    )?;
    if kind != "proof-envelope" {
        return Err(format!(
            "v1 dispatcher only supports `proof-envelope` shape; got `{kind}`. The lift-plugin spec defines shapes (a) `ir-document` and (b) `signed-mementos`; this dispatcher version doesn't implement them yet.",
        ));
    }
    let filename_cid = lift_resp
        .get("filename_cid")
        .and_then(|v| v.as_str())
        .ok_or("missing filename_cid")?
        .to_string();
    let bytes_b64 = lift_resp
        .get("bytes_base64")
        .and_then(|v| v.as_str())
        .ok_or("missing bytes_base64")?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(bytes_b64)
        .map_err(|e| format!("decode bytes_base64: {e}"))?;

    std::fs::create_dir_all(out_dir)
        .map_err(|e| format!("mkdir {}: {e}", out_dir.display()))?;
    let out_path = out_dir.join(format!("{filename_cid}.proof"));
    std::fs::write(&out_path, &bytes)
        .map_err(|e| format!("write {}: {e}", out_path.display()))?;

    Ok((filename_cid, bytes.len()))
}

fn read_response(reader: &mut impl BufRead, id: i64) -> Result<Value, String> {
    let mut line = String::new();
    let n = reader
        .read_line(&mut line)
        .map_err(|e| format!("read response: {e}"))?;
    if n == 0 {
        return Err("plugin closed stdout before responding".to_string());
    }
    let v: Value = serde_json::from_str(line.trim())
        .map_err(|e| format!("parse JSON-RPC response: {e}\n  raw: {line}"))?;
    if v.get("id").and_then(|v| v.as_i64()) != Some(id) {
        return Err(format!("response id mismatch: expected {id}, got {v:?}"));
    }
    if let Some(err) = v.get("error") {
        return Err(format!("plugin returned error: {err}"));
    }
    v.get("result")
        .cloned()
        .ok_or_else(|| "response missing `result`".to_string())
}

pub fn run(args: MintArgs) -> u8 {
    let project_root: PathBuf = args.project.unwrap_or_else(|| PathBuf::from("."));
    if !project_root.exists() {
        eprintln!("{}: project not found: {}", "error".red().bold(), project_root.display());
        return EXIT_USER_ERROR;
    }

    // Resolve surface: --surface > project config > user config.
    let surface = if let Some(s) = args.surface {
        s
    } else {
        let project_cfg = read_project_config(&project_root);
        let user_cfg = read_user_config();
        match project_cfg
            .surface_for("must")
            .or_else(|| user_cfg.surface_for("must"))
        {
            Some(s) => s,
            None => {
                eprintln!(
                    "{}: no `[authoring] surface` in .provekit/config.toml. Pass --surface or run `provekit init`.",
                    "error".red().bold()
                );
                return EXIT_USER_ERROR;
            }
        }
    };

    let out_dir = args.out.unwrap_or_else(|| project_root.clone());

    match dispatch(&project_root, &surface, &out_dir, args.flags.quiet) {
        Ok((cid, n)) => {
            if !args.flags.quiet {
                println!();
                println!("  catalog CID:        {cid}");
                println!("  proof bytes:        {n}");
                println!("  .proof file:        {}", out_dir.join(format!("{cid}.proof")).display());
            } else {
                println!("{cid}");
            }
            EXIT_OK
        }
        Err(e) => {
            eprintln!("{}: {e}", "error".red().bold());
            EXIT_VERIFY_FAIL
        }
    }
}
