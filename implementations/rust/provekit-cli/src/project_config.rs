// SPDX-License-Identifier: Apache-2.0
//
// Project / user configuration for the agent-driven subcommands.
//
// Two parallel declarative concerns, both written by `provekit init`
// and both edited by the user. ProvekIt does not auto-detect either:
//
//   1. **Authoring surface**: which annotation library the agent
//      should target ("ts-zod", "rust-contracts-crate", "default", ...).
//   2. **Agent backend**: which coding-agent drives the work
//      ("claude-code", "openai", "stub", ...).
//
// Resolution order for both is identical:
//
//   - CLI flag (`--surface`, `--agent`): one-shot override.
//   - Project per-command override   `[authoring.must]`, `[agent.must]`.
//   - Project default                `[authoring]`,        `[agent]`.
//   - User per-command override      (in `~/.config/provekit/config.toml`).
//   - User default                   (in same).
//   - Bundled fallback (none for surface; "stub" for agent).
//
// Same shape as `.npmrc` / `.cargo/config.toml`: declarative files
// at known paths. The user is in charge.

use std::path::{Path, PathBuf};

/// One entry in `.provekit/config.toml`'s `[[plugins]]` array.
///
/// Each entry declares one lift plugin this project enables. cmd_mint
/// reads the full list and dispatches each plugin via its manifest at
/// `.provekit/lift/<surface>/manifest.toml`, then merges all
/// ir-documents into one envelope. The substrate's "config-driven
/// multi-plugin orchestration" pattern: the user declares which kits
/// are active; the CLI invokes them.
#[derive(Debug, Clone, Default)]
pub struct PluginEntry {
    /// Human label for diagnostics. Optional; falls back to `surface`.
    pub name: Option<String>,
    /// Lift surface — resolves to `.provekit/lift/<surface>/manifest.toml`.
    pub surface: String,
    /// Optional absolute path the plugin should treat as its
    /// `workspace_root`. Use case: a shim that lifts from a
    /// cargo-resolved dependency's source instead of from its own
    /// project root. Defaults to the project root (the directory
    /// containing `.provekit/config.toml`).
    pub workspace_override: Option<String>,
    /// Optional `options.emit` value passed through to the plugin.
    /// `"ir-document"` opts a self-minting plugin (provekit-lift) into
    /// composable mode for multi-plugin merge; `"proof-envelope"` is
    /// the default self-mint shape.
    pub emit: Option<String>,
    /// Optional `options.layer` value passed through to the plugin.
    /// Used to opt the sugar/boundary lane into emitting
    /// `library-sugar-binding-entry` + `refusal-memento` (vs the
    /// default bind-IR shape). Recognized values per
    /// `lift-plugin-protocol.md`: "all", "library-bindings",
    /// "identify-only". Per-plugin so the sugar lane and the contract
    /// lane can request different layers from their respective
    /// language-kit lifters.
    pub layer: Option<String>,
}

impl PluginEntry {
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or(self.surface.as_str())
    }
}

/// #1358 / #1355: Per-shim or per-consumer-project platform profile.
/// Declares the five-axis realization tuple (language, family, library,
/// version) plus the concept-level binding the shim's @sugar / @boundary
/// annotations declare on a per-function basis. Any axis may be absent
/// (None) — absent means the axis FLOATS and resolves against the
/// consumer's profile at materialize time (per #1355 dispatch model).
///
/// Declared inline in `.provekit/config.toml`:
/// ```toml
/// [platform_profile]
/// language = "rust"
/// family = "concept:family:sql"
/// library = "rusqlite"
/// version = "0.39.0"
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlatformProfile {
    pub language: Option<String>,
    pub family: Option<String>,
    pub library: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ProjectConfig {
    pub exam_manifest_cid: Option<String>,

    /// Project's declared lift plugins, in array-of-tables form
    /// (`[[plugins]]` in TOML). Empty if the project still uses the
    /// legacy single-surface `[authoring] surface = ...` form.
    pub plugins: Vec<PluginEntry>,

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

    /// Extra contract directories loaded by `provekit prove`.
    /// e.g., an OpenAPI spec project whose .proof files are
    /// consumed alongside the main project.
    pub callees: Vec<String>,

    /// Serialized command path documents, keyed by command.
    pub path_mint: Option<String>,

    /// #1358 / #1355: realization tuple for this project. Used by
    /// `cmd_mint` to stamp every emitted memento and by `cmd_materialize`
    /// to resolve floating axes against the consumer's profile.
    pub platform_profile: Option<PlatformProfile>,
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

    pub fn path_for(&self, cmd: &str) -> Option<String> {
        match cmd {
            "mint" => self.path_mint.clone(),
            _ => None,
        }
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
    // In-flight `[[plugins]]` entry. Pushed to `cfg.plugins` on each new
    // `[[plugins]]` header or at end-of-file. Stays None while the parser
    // is in any other (single-bracket) section.
    let mut current_plugin: Option<PluginEntry> = None;
    for raw_line in text.lines() {
        let line = strip_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        // Array-of-tables header: `[[plugins]]`. Each occurrence starts a
        // new entry; the previous entry (if any) is finalized into
        // `cfg.plugins`. The substrate's intent: one [[plugins]] block per
        // declared lift kit.
        if let Some(inner) = line.strip_prefix("[[").and_then(|s| s.strip_suffix("]]")) {
            if let Some(prev) = current_plugin.take() {
                if !prev.surface.is_empty() {
                    cfg.plugins.push(prev);
                }
            }
            let header = inner.trim().to_lowercase();
            if header == "plugins" {
                current_plugin = Some(PluginEntry::default());
                section = Some("plugins.entry".to_string());
            } else {
                section = Some(header);
            }
            continue;
        }
        if let Some(s) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            if let Some(prev) = current_plugin.take() {
                if !prev.surface.is_empty() {
                    cfg.plugins.push(prev);
                }
            }
            section = Some(s.trim().to_lowercase());
            continue;
        }
        let Some(eq) = line.find('=') else { continue };
        let key = line[..eq].trim();
        let val = line[eq + 1..].trim().trim_matches('"').to_string();
        match (section.as_deref(), key) {
            (None, "exam_manifest_cid") => cfg.exam_manifest_cid = Some(val),
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
            (Some("verify"), "callees") => {
                cfg.callees = parse_string_array(&val);
            }
            (Some("paths.mint"), "file") => cfg.path_mint = Some(val),
            // #1358 / #1355: [platform_profile] section
            (Some("platform_profile"), "language") => {
                cfg.platform_profile
                    .get_or_insert_with(PlatformProfile::default)
                    .language = Some(val);
            }
            (Some("platform_profile"), "family") => {
                cfg.platform_profile
                    .get_or_insert_with(PlatformProfile::default)
                    .family = Some(val);
            }
            (Some("platform_profile"), "library") => {
                cfg.platform_profile
                    .get_or_insert_with(PlatformProfile::default)
                    .library = Some(val);
            }
            (Some("platform_profile"), "version") => {
                cfg.platform_profile
                    .get_or_insert_with(PlatformProfile::default)
                    .version = Some(val);
            }
            (Some("plugins.entry"), "name") => {
                if let Some(entry) = current_plugin.as_mut() {
                    entry.name = Some(val);
                }
            }
            (Some("plugins.entry"), "surface") => {
                if let Some(entry) = current_plugin.as_mut() {
                    entry.surface = val;
                }
            }
            (Some("plugins.entry"), "workspace_override") => {
                if let Some(entry) = current_plugin.as_mut() {
                    entry.workspace_override = Some(val);
                }
            }
            (Some("plugins.entry"), "emit") => {
                if let Some(entry) = current_plugin.as_mut() {
                    entry.emit = Some(val);
                }
            }
            (Some("plugins.entry"), "layer") => {
                if let Some(entry) = current_plugin.as_mut() {
                    entry.layer = Some(val);
                }
            }
            _ => {}
        }
    }
    // Flush the final in-flight entry.
    if let Some(prev) = current_plugin.take() {
        if !prev.surface.is_empty() {
            cfg.plugins.push(prev);
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
#[allow(dead_code)] // public API; consumed by `provekit init` interactive flow (TODO: wire up)
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
#[allow(dead_code)] // public API; menu data for `provekit init` interactive flow (TODO: wire up)
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
    "typescript-source",
    "ts-class-validator",
    "ts-fast-check",
    "ts-jsdoc",
    "ts-joi",
    "ts-ajv",
    "python-pydantic",
    "python-deal",
    "python-hypothesis",
    "python-source",
    "ruby-source",
    "php-source",
    "java-source",
    "java-bean-validation",
    "java-jml",
    "jvm-bytecode",
    "csharp",
    "csharp-source",
    "cpp-26-contracts",
    "cpp-boost-contract",
    "cpp-source",
    "clr-bytecode",
    "swift-source",
    "zig-source",
    "evm-bytecode",
];

/// Agent menu shown by `provekit init`.
#[allow(dead_code)] // public API; menu data for `provekit init` interactive flow (TODO: wire up)
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
#[allow(dead_code)] // public API; menu data for `provekit init` interactive flow (TODO: wire up)
pub const KNOWN_SOLVERS: &[&str] = &["z3", "cvc5", "bitwuzla", "yices2", "mathsat"];

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------
    // #1358 / #1355: [platform_profile] section parsing
    // -----------------------------------------------------------------

    #[test]
    fn parses_full_platform_profile_section() {
        let raw = r#"
[platform_profile]
language = "rust"
family = "concept:family:sql"
library = "rusqlite"
version = "0.39.0"
"#;
        let cfg = parse_config(raw);
        let profile = cfg.platform_profile.expect("platform_profile parsed");
        assert_eq!(profile.language.as_deref(), Some("rust"));
        assert_eq!(profile.family.as_deref(), Some("concept:family:sql"));
        assert_eq!(profile.library.as_deref(), Some("rusqlite"));
        assert_eq!(profile.version.as_deref(), Some("0.39.0"));
    }

    #[test]
    fn parses_partial_platform_profile_with_floating_axes() {
        // Per #1355: any axis may float. Profile must parse cleanly when
        // only some axes are pinned.
        let raw = r#"
[platform_profile]
language = "rust"
family = "concept:family:hash"
"#;
        let cfg = parse_config(raw);
        let profile = cfg.platform_profile.expect("platform_profile parsed");
        assert_eq!(profile.language.as_deref(), Some("rust"));
        assert_eq!(profile.family.as_deref(), Some("concept:family:hash"));
        assert!(profile.library.is_none(), "library floats");
        assert!(profile.version.is_none(), "version floats");
    }

    #[test]
    fn absent_platform_profile_section_yields_none() {
        // Back-compat: configs without [platform_profile] still parse.
        let cfg = parse_config("[authoring]\nsurface = \"kani\"\n");
        assert!(cfg.platform_profile.is_none());
    }

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
        let user =
            parse_config("[authoring]\nsurface = \"ts-zod\"\n[agent]\nbackend = \"claude-code\"\n");
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
    fn parses_mint_path_file() {
        let cfg = parse_config("[paths.mint]\nfile = \".provekit/paths/mint.json\"\n");
        assert_eq!(
            cfg.path_for("mint").as_deref(),
            Some(".provekit/paths/mint.json")
        );
        assert_eq!(cfg.path_for("prove"), None);
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
        assert!(KNOWN_SURFACES.contains(&"typescript-source"));
        assert!(KNOWN_SURFACES.contains(&"php-source"));
        assert!(KNOWN_SURFACES.contains(&"ruby-source"));
        assert!(KNOWN_SURFACES.contains(&"csharp-source"));
        assert!(KNOWN_SURFACES.contains(&"swift-source"));
        assert!(KNOWN_SURFACES.contains(&"zig-source"));
        assert!(KNOWN_SURFACES.contains(&"clr-bytecode"));
        assert!(KNOWN_SURFACES.contains(&"evm-bytecode"));
        assert!(KNOWN_AGENTS.contains(&"stub"));
        assert!(KNOWN_AGENTS.contains(&"claude-code"));
    }
}
