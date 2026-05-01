// SPDX-License-Identifier: Apache-2.0
//
// Project / user configuration for the agent-driven subcommands.
//
// Two parallel declarative concerns, both written by `provekit init`
// and both edited by the user. ProvekIt does not auto-detect either:
//
//   1. **Authoring surface** — which annotation library the agent
//      should target ("ts-zod", "rust-contracts-crate", "default", ...).
//   2. **Agent backend** — which coding-agent drives the work
//      ("claude-code", "openai", "stub", ...).
//
// Resolution order for both is identical:
//
//   - CLI flag (`--surface`, `--agent`) — one-shot override.
//   - Project per-command override   `[authoring.must]`, `[agent.must]`.
//   - Project default                `[authoring]`,        `[agent]`.
//   - User per-command override      (in `~/.config/provekit/config.toml`).
//   - User default                   (in same).
//   - Bundled fallback (none for surface; "stub" for agent).
//
// Same shape as `.npmrc` / `.cargo/config.toml`: declarative files
// at known paths. The user is in charge.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct ProjectConfig {
    pub surface_default: Option<String>,
    pub surface_must: Option<String>,
    pub surface_lift: Option<String>,
    pub surface_fix: Option<String>,

    pub agent_default: Option<String>,
    pub agent_must: Option<String>,
    pub agent_lift: Option<String>,
    pub agent_fix: Option<String>,

    pub agent_model: Option<String>,
    pub agent_api_key_env: Option<String>,

    /// Solver configuration. v1 captures the shape; the verifier
    /// itself still runs Z3 only. Future work routes through this.
    pub solver_default: Option<String>,
    pub solver_chain: Vec<String>,
    pub solver_portfolio: Vec<String>,
    pub solver_mode: Option<String>, // "first-wins" | "consensus"
}

impl ProjectConfig {
    pub fn surface_for(&self, cmd: &str) -> Option<String> {
        let per_cmd = match cmd {
            "must" => &self.surface_must,
            "lift" => &self.surface_lift,
            "fix" => &self.surface_fix,
            _ => &None,
        };
        per_cmd.clone().or_else(|| self.surface_default.clone())
    }

    pub fn agent_for(&self, cmd: &str) -> Option<String> {
        let per_cmd = match cmd {
            "must" => &self.agent_must,
            "lift" => &self.agent_lift,
            "fix" => &self.agent_fix,
            _ => &None,
        };
        per_cmd.clone().or_else(|| self.agent_default.clone())
    }
}

pub fn read_project_config(project_root: &Path) -> ProjectConfig {
    read_config_file(&project_root.join(".provekit/config.toml"))
}

pub fn read_user_config() -> ProjectConfig {
    let p = match user_config_path() {
        Some(p) => p,
        None => return ProjectConfig::default(),
    };
    read_config_file(&p)
}

fn user_config_path() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return Some(PathBuf::from(xdg).join("provekit/config.toml"));
        }
    }
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".config/provekit/config.toml"))
}

fn read_config_file(path: &Path) -> ProjectConfig {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => return ProjectConfig::default(),
    };
    parse_config(&text)
}

fn parse_config(text: &str) -> ProjectConfig {
    let mut cfg = ProjectConfig::default();
    let mut section: Option<String> = None;
    for raw_line in text.lines() {
        let line = strip_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(s) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            section = Some(s.trim().to_lowercase());
            continue;
        }
        let Some(eq) = line.find('=') else { continue };
        let key = line[..eq].trim();
        let val = line[eq + 1..].trim().trim_matches('"').to_string();
        match (section.as_deref(), key) {
            (Some("authoring"), "surface") => cfg.surface_default = Some(val),
            (Some("authoring.must"), "surface") => cfg.surface_must = Some(val),
            (Some("authoring.lift"), "surface") => cfg.surface_lift = Some(val),
            (Some("authoring.fix"), "surface") => cfg.surface_fix = Some(val),
            (Some("agent"), "backend") => cfg.agent_default = Some(val),
            (Some("agent.must"), "backend") => cfg.agent_must = Some(val),
            (Some("agent.lift"), "backend") => cfg.agent_lift = Some(val),
            (Some("agent.fix"), "backend") => cfg.agent_fix = Some(val),
            (Some("agent"), "model") => cfg.agent_model = Some(val),
            (Some("agent"), "api_key_env") => cfg.agent_api_key_env = Some(val),
            (Some("solvers"), "default") => cfg.solver_default = Some(val),
            (Some("solvers"), "chain") => {
                cfg.solver_chain = parse_string_array(&val);
            }
            (Some("solvers"), "portfolio") => {
                cfg.solver_portfolio = parse_string_array(&val);
            }
            (Some("solvers"), "mode") => cfg.solver_mode = Some(val),
            _ => {}
        }
    }
    cfg
}

fn parse_string_array(raw: &str) -> Vec<String> {
    // raw shape: `["z3", "cvc5"]` (already trimmed of surrounding quotes
    // by the parser; we re-handle here because the array form skipped that).
    // Be tolerant: split on comma, strip brackets and quotes.
    raw.trim_matches(|c: char| c == '[' || c == ']' || c == '"')
        .split(',')
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn strip_comment(s: &str) -> &str {
    if let Some(pos) = s.find('#') {
        &s[..pos]
    } else {
        s
    }
}

/// Merge: `project` wins over `user`. Either side's `None` falls
/// through; both `None` becomes `None`.
pub fn merged_for_command(
    project: &ProjectConfig,
    user: &ProjectConfig,
    cmd: &str,
) -> (Option<String>, Option<String>) {
    let surface = project.surface_for(cmd).or_else(|| user.surface_for(cmd));
    let agent = project.agent_for(cmd).or_else(|| user.agent_for(cmd));
    (surface, agent)
}

/// Surface menu shown by `provekit init`.
pub const KNOWN_SURFACES: &[&str] = &[
    "default",
    "rust-provekit-decorator",
    "rust-invariant-file",
    "rust-contracts-crate",
    "rust-kani",
    "rust-prusti",
    "rust-creusot",
    "rust-flux",
    "rust-verus",
    "rust-proptest",
    "rust-quickcheck",
    "ts-zod",
    "ts-class-validator",
    "ts-fast-check",
    "ts-jsdoc",
    "ts-joi",
    "ts-ajv",
    "python-pydantic",
    "python-deal",
    "python-hypothesis",
    "java-bean-validation",
    "java-jml",
    "cpp-26-contracts",
    "cpp-boost-contract",
];

/// Agent menu shown by `provekit init`.
pub const KNOWN_AGENTS: &[&str] = &[
    "stub",
    "claude-code",
    "openai",
    "opencode",
    "codex",
    "ollama",
];

/// Solver menu shown by `provekit init`. v1 ships with single-solver
/// support (Z3); the chain / portfolio / consensus modes are
/// captured in the config schema but not yet implemented in the
/// verifier (TODO: see protocol/specs/2026-04-30-agent-plugin-protocol.md).
pub const KNOWN_SOLVERS: &[&str] = &["z3", "cvc5", "bitwuzla", "yices2", "mathsat"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_surface_default() {
        let cfg = parse_config("[authoring]\nsurface = \"ts-zod\"\n");
        assert_eq!(cfg.surface_for("must").as_deref(), Some("ts-zod"));
    }

    #[test]
    fn parses_agent_default_and_per_command() {
        let raw = "
        [agent]
        backend = \"claude-code\"
        model = \"claude-opus-4-7\"
        api_key_env = \"ANTHROPIC_API_KEY\"
        [agent.fix]
        backend = \"stub\"
        ";
        let cfg = parse_config(raw);
        assert_eq!(cfg.agent_for("must").as_deref(), Some("claude-code"));
        assert_eq!(cfg.agent_for("lift").as_deref(), Some("claude-code"));
        assert_eq!(cfg.agent_for("fix").as_deref(), Some("stub"));
        assert_eq!(cfg.agent_model.as_deref(), Some("claude-opus-4-7"));
        assert_eq!(cfg.agent_api_key_env.as_deref(), Some("ANTHROPIC_API_KEY"));
    }

    #[test]
    fn project_overrides_user_via_merge() {
        let project = parse_config("[agent]\nbackend = \"openai\"\n");
        let user = parse_config(
            "[authoring]\nsurface = \"ts-zod\"\n[agent]\nbackend = \"claude-code\"\n",
        );
        let (surface, agent) = merged_for_command(&project, &user, "must");
        assert_eq!(agent.as_deref(), Some("openai"));
        assert_eq!(surface.as_deref(), Some("ts-zod"));
    }

    #[test]
    fn comments_and_blanks_ignored() {
        let raw = "
        # comment
        [authoring]
        # mid
        surface = \"kani\"  # trailing
        ";
        let cfg = parse_config(raw);
        assert_eq!(cfg.surface_for("must").as_deref(), Some("kani"));
    }

    #[test]
    fn missing_file_yields_default() {
        let p = std::env::temp_dir().join("provekit-no-such-config.toml");
        let _ = std::fs::remove_file(&p);
        let cfg = read_config_file(&p);
        assert!(cfg.surface_default.is_none());
        assert!(cfg.agent_default.is_none());
    }

    #[test]
    fn known_lists_include_anchor_entries() {
        assert!(KNOWN_SURFACES.contains(&"default"));
        assert!(KNOWN_SURFACES.contains(&"ts-zod"));
        assert!(KNOWN_AGENTS.contains(&"stub"));
        assert!(KNOWN_AGENTS.contains(&"claude-code"));
    }
}
