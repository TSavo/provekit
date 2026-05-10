// SPDX-License-Identifier: Apache-2.0
//
// `provekit agent list`: enumerate installed plugins.
// `provekit agent describe <n>`: emit a tool descriptor for one.
//
// Discovery: walks `~/.config/provekit/agents/*/manifest.toml`. Each
// manifest declares: name, version, protocol_version, binary,
// capabilities. The bundled-in-Rust backends (stub, claude-code,
// openai) are reported alongside the manifest-based ones.

use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use provekit_agent::build_tool_descriptor;
use serde::{Deserialize, Serialize};

use crate::{OutputFlags, EXIT_OK, EXIT_USER_ERROR};

#[derive(Parser, Debug, Clone)]
pub struct AgentArgs {
    #[command(subcommand)]
    pub cmd: AgentCmd,
}

#[derive(Subcommand, Debug, Clone)]
pub enum AgentCmd {
    /// List all installed agent plugins (bundled + manifests).
    List(AgentListArgs),
    /// Emit a tool descriptor JSON for one agent (or for ProvekIt itself).
    Describe(AgentDescribeArgs),
}

#[derive(Parser, Debug, Clone)]
pub struct AgentListArgs {
    #[command(flatten)]
    pub out: OutputFlags,
}

#[derive(Parser, Debug, Clone)]
pub struct AgentDescribeArgs {
    /// Agent name. Use "provekit" to emit the descriptor of ProvekIt's
    /// own tools (the side that an external agent like Claude Code or
    /// Cursor would consume).
    pub name: String,
    #[command(flatten)]
    pub out: OutputFlags,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentManifest {
    pub name: String,
    pub version: String,
    pub protocol_version: String,
    pub binary: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub source: String,
}

pub fn run(args: AgentArgs) -> u8 {
    match args.cmd {
        AgentCmd::List(a) => list(a),
        AgentCmd::Describe(a) => describe(a),
    }
}

fn list(args: AgentListArgs) -> u8 {
    let mut all = bundled_manifests();
    all.extend(discover_manifests());
    if args.out.json {
        let j = serde_json::json!({ "agents": all });
        println!("{}", serde_json::to_string_pretty(&j).unwrap_or_default());
    } else if !args.out.quiet {
        if all.is_empty() {
            println!("no agents installed");
            return EXIT_OK;
        }
        println!("{:<20} {:<10} {:<25} source", "NAME", "VERSION", "PROTOCOL");
        for m in &all {
            println!(
                "{:<20} {:<10} {:<25} {}",
                m.name, m.version, m.protocol_version, m.source
            );
        }
    }
    EXIT_OK
}

fn describe(args: AgentDescribeArgs) -> u8 {
    if args.name == "provekit" {
        // The descriptor of ProvekIt's tool surface (what external
        // agents consume to drive ProvekIt).
        let d = build_tool_descriptor(
            env!("CARGO_PKG_VERSION"),
            "blake3-512:RECOMPUTE-after-protocol-catalog-v1.2.0",
        );
        println!("{}", serde_json::to_string_pretty(&d).unwrap_or_default());
        return EXIT_OK;
    }
    // Otherwise look up an installed agent and emit its manifest.
    let mut all = bundled_manifests();
    all.extend(discover_manifests());
    let found = all.into_iter().find(|m| m.name == args.name);
    match found {
        Some(m) => {
            println!("{}", serde_json::to_string_pretty(&m).unwrap_or_default());
            EXIT_OK
        }
        None => {
            eprintln!("error: no agent named `{}`", args.name);
            EXIT_USER_ERROR
        }
    }
}

fn bundled_manifests() -> Vec<AgentManifest> {
    vec![
        AgentManifest {
            name: "stub".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            protocol_version: "provekit-agent/1".into(),
            binary: "<bundled>".into(),
            capabilities: vec!["lift".into(), "must".into(), "fix".into()],
            source: "bundled".into(),
        },
        AgentManifest {
            name: "claude-code".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            protocol_version: "provekit-agent/1".into(),
            binary: "claude".into(),
            capabilities: vec!["lift".into(), "must".into(), "fix".into()],
            source: "bundled (skeleton; live mode behind `live` feature)".into(),
        },
        AgentManifest {
            name: "openai".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            protocol_version: "provekit-agent/1".into(),
            binary: "<HTTPS chat completions>".into(),
            capabilities: vec!["lift".into(), "must".into(), "fix".into()],
            source: "bundled (skeleton; live mode behind `live` feature)".into(),
        },
    ]
}

fn discover_manifests() -> Vec<AgentManifest> {
    let Some(root) = manifest_root() else {
        return vec![];
    };
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(&root) {
        Ok(e) => e,
        Err(_) => return out,
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let manifest_path = dir.join("manifest.toml");
        if let Some(m) = read_manifest(&manifest_path) {
            out.push(m);
        }
    }
    out
}

fn manifest_root() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return Some(PathBuf::from(xdg).join("provekit/agents"));
        }
    }
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".config/provekit/agents"))
}

fn read_manifest(path: &Path) -> Option<AgentManifest> {
    let text = std::fs::read_to_string(path).ok()?;
    parse_manifest(&text, path)
}

fn parse_manifest(text: &str, path: &Path) -> Option<AgentManifest> {
    // Tiny line-based TOML parser; same shape as project_config.
    let mut name = String::new();
    let mut version = String::new();
    let mut protocol_version = String::new();
    let mut binary = String::new();
    let mut capabilities: Vec<String> = Vec::new();
    for raw in text.lines() {
        let line = match raw.find('#') {
            Some(p) => &raw[..p],
            None => raw,
        }
        .trim();
        if line.is_empty() || line.starts_with('[') {
            continue;
        }
        let Some(eq) = line.find('=') else { continue };
        let key = line[..eq].trim();
        let val = line[eq + 1..].trim();
        if val.starts_with('[') {
            capabilities = val
                .trim_matches(|c: char| c == '[' || c == ']')
                .split(',')
                .map(|s| s.trim().trim_matches('"').to_string())
                .filter(|s| !s.is_empty())
                .collect();
            continue;
        }
        let val = val.trim_matches('"').to_string();
        match key {
            "name" => name = val,
            "version" => version = val,
            "protocol_version" => protocol_version = val,
            "binary" => binary = val,
            _ => {}
        }
    }
    if name.is_empty() {
        return None;
    }
    Some(AgentManifest {
        name,
        version,
        protocol_version,
        binary,
        capabilities,
        source: path.display().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_manifest_round_trip() {
        let raw = r#"
        name = "echo-agent"
        version = "0.1"
        protocol_version = "provekit-agent/1"
        binary = "/usr/local/bin/echo_agent.py"
        capabilities = ["must", "lift"]
        "#;
        let m = parse_manifest(raw, std::path::Path::new("/tmp/manifest.toml")).expect("parsed");
        assert_eq!(m.name, "echo-agent");
        assert_eq!(m.version, "0.1");
        assert_eq!(m.protocol_version, "provekit-agent/1");
        assert_eq!(m.binary, "/usr/local/bin/echo_agent.py");
        assert!(m.capabilities.contains(&"must".to_string()));
        assert!(m.capabilities.contains(&"lift".to_string()));
    }

    #[test]
    fn bundled_manifests_include_stub() {
        let bundled = bundled_manifests();
        assert!(bundled.iter().any(|m| m.name == "stub"));
        assert!(bundled.iter().any(|m| m.name == "claude-code"));
        assert!(bundled.iter().any(|m| m.name == "openai"));
    }
}
