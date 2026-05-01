// SPDX-License-Identifier: Apache-2.0
//
// Prompt resolution + variable substitution.
//
// The prompt the CLI hands to a coding-agent backend is determined by
// walking a chain of override locations. First hit wins.
//
// Resolution order (most specific to most general):
//
//   1. CLI flag:    --prompt-file <path>
//   2. Project per-agent + per-surface:
//      <project>/.provekit/prompts/<cmd>/<surface>.<agent>.md
//   3. Project per-agent:
//      <project>/.provekit/prompts/<cmd>/<agent>.md
//   4. Project per-surface:
//      <project>/.provekit/prompts/<cmd>/<surface>.md
//   5. Project default:
//      <project>/.provekit/prompts/<cmd>/default.md
//      (legacy flat: <project>/.provekit/prompts/<cmd>.md)
//   6. User per-agent + per-surface:
//      ~/.config/provekit/prompts/<cmd>/<surface>.<agent>.md
//   7. User per-agent:
//      ~/.config/provekit/prompts/<cmd>/<agent>.md
//   8. User per-surface:
//      ~/.config/provekit/prompts/<cmd>/<surface>.md
//   9. User default:
//      ~/.config/provekit/prompts/<cmd>/default.md
//  10. Bundled default (compiled in via include_str!).
//
// Templates use `{{var_name}}` substitution. We use plain
// String::replace rather than dragging in handlebars; the variables
// the CLI passes are well-known and finite.

use std::path::{Path, PathBuf};

/// The three commands that drive a coding agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptCommand {
    Must,
    Lift,
    Fix,
}

impl PromptCommand {
    pub fn as_str(self) -> &'static str {
        match self {
            PromptCommand::Must => "must",
            PromptCommand::Lift => "lift",
            PromptCommand::Fix => "fix",
        }
    }
}

// ---------------------------------------------------------------------------
// Bundled defaults
// ---------------------------------------------------------------------------

const BUNDLED_MUST_DEFAULT: &str = include_str!("../prompts/must/default.md");
const BUNDLED_LIFT_DEFAULT: &str = include_str!("../prompts/lift/default.md");
const BUNDLED_FIX_DEFAULT: &str = include_str!("../prompts/fix/default.md");

fn bundled_default(cmd: PromptCommand) -> &'static str {
    match cmd {
        PromptCommand::Must => BUNDLED_MUST_DEFAULT,
        PromptCommand::Lift => BUNDLED_LIFT_DEFAULT,
        PromptCommand::Fix => BUNDLED_FIX_DEFAULT,
    }
}

// ---------------------------------------------------------------------------
// Resolution
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct PromptOverrides<'a> {
    /// `--prompt-file <path>` from the CLI; wins outright.
    pub explicit_file: Option<&'a Path>,
    /// Project root (typically cwd). When None, project layers are skipped.
    pub project: Option<&'a Path>,
    /// Agent name ("claude-code", "openai", "stub", ...).
    pub agent: Option<&'a str>,
    /// Authoring surface ("ts-zod", "rust-contracts-crate", ...).
    pub surface: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct ResolvedPrompt {
    /// Source of the prompt (path or "<bundled>").
    pub source: String,
    pub body: String,
}

pub fn resolve_prompt(cmd: PromptCommand, ov: &PromptOverrides<'_>) -> ResolvedPrompt {
    if let Some(p) = ov.explicit_file {
        if let Ok(s) = std::fs::read_to_string(p) {
            return ResolvedPrompt {
                source: p.display().to_string(),
                body: s,
            };
        }
    }

    let cmd_str = cmd.as_str();
    let mut tried: Vec<PathBuf> = Vec::new();

    // Build the candidate list, project layer first.
    if let Some(project) = ov.project {
        push_layer(&mut tried, &project.join(".provekit/prompts"), cmd_str, ov.agent, ov.surface);
        // Legacy flat file:
        tried.push(project.join(format!(".provekit/prompts/{cmd_str}.md")));
    }
    if let Some(home) = dirs_home_config() {
        push_layer(&mut tried, &home.join("provekit/prompts"), cmd_str, ov.agent, ov.surface);
    }

    for candidate in &tried {
        if let Ok(body) = std::fs::read_to_string(candidate) {
            return ResolvedPrompt {
                source: candidate.display().to_string(),
                body,
            };
        }
    }

    ResolvedPrompt {
        source: "<bundled>".into(),
        body: bundled_default(cmd).to_string(),
    }
}

fn push_layer(
    out: &mut Vec<PathBuf>,
    base: &Path,
    cmd: &str,
    agent: Option<&str>,
    surface: Option<&str>,
) {
    let dir = base.join(cmd);
    if let (Some(s), Some(a)) = (surface, agent) {
        out.push(dir.join(format!("{s}.{a}.md")));
    }
    if let Some(a) = agent {
        out.push(dir.join(format!("{a}.md")));
    }
    if let Some(s) = surface {
        out.push(dir.join(format!("{s}.md")));
    }
    out.push(dir.join("default.md"));
}

/// Best-effort `~/.config` lookup that does not pull in the `dirs` crate.
fn dirs_home_config() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return Some(PathBuf::from(xdg));
        }
    }
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".config"))
}

// ---------------------------------------------------------------------------
// Variable substitution
// ---------------------------------------------------------------------------

/// Apply `{{key}}` substitutions. Unknown keys are left as-is so that
/// missing variables are easy to spot in the rendered prompt.
pub fn substitute(template: &str, vars: &[(&str, &str)]) -> String {
    let mut out = template.to_string();
    for (k, v) in vars {
        let needle = format!("{{{{{k}}}}}");
        out = out.replace(&needle, v);
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    #[test]
    fn bundled_default_is_non_empty_for_each_command() {
        for cmd in [PromptCommand::Must, PromptCommand::Lift, PromptCommand::Fix] {
            let rp = resolve_prompt(cmd, &PromptOverrides::default());
            assert_eq!(rp.source, "<bundled>");
            assert!(!rp.body.is_empty(), "{:?} bundled empty", cmd);
            assert!(rp.body.contains("kit"), "{:?} prompt should mention kit", cmd);
        }
    }

    #[test]
    fn project_default_overrides_bundled() {
        let dir = tempdir();
        let prompts = dir.path().join(".provekit/prompts/must");
        fs::create_dir_all(&prompts).unwrap();
        let path = prompts.join("default.md");
        let mut f = fs::File::create(&path).unwrap();
        writeln!(f, "PROJECT-DEFAULT-MARKER").unwrap();
        let ov = PromptOverrides {
            project: Some(dir.path()),
            ..Default::default()
        };
        let rp = resolve_prompt(PromptCommand::Must, &ov);
        assert!(rp.body.contains("PROJECT-DEFAULT-MARKER"));
        assert_eq!(rp.source, path.display().to_string());
    }

    #[test]
    fn surface_overrides_default_when_present() {
        let dir = tempdir();
        let prompts = dir.path().join(".provekit/prompts/must");
        fs::create_dir_all(&prompts).unwrap();
        fs::write(prompts.join("default.md"), "DEFAULT").unwrap();
        fs::write(prompts.join("ts-zod.md"), "TS-ZOD-MARKER").unwrap();
        let ov = PromptOverrides {
            project: Some(dir.path()),
            surface: Some("ts-zod"),
            ..Default::default()
        };
        let rp = resolve_prompt(PromptCommand::Must, &ov);
        assert!(rp.body.contains("TS-ZOD-MARKER"));
    }

    #[test]
    fn agent_overrides_surface_when_both_present() {
        let dir = tempdir();
        let prompts = dir.path().join(".provekit/prompts/must");
        fs::create_dir_all(&prompts).unwrap();
        fs::write(prompts.join("default.md"), "DEFAULT").unwrap();
        fs::write(prompts.join("ts-zod.md"), "SURFACE").unwrap();
        fs::write(prompts.join("claude-code.md"), "AGENT").unwrap();
        fs::write(prompts.join("ts-zod.claude-code.md"), "BOTH").unwrap();
        let ov = PromptOverrides {
            project: Some(dir.path()),
            agent: Some("claude-code"),
            surface: Some("ts-zod"),
            ..Default::default()
        };
        let rp = resolve_prompt(PromptCommand::Must, &ov);
        // Most-specific wins.
        assert!(rp.body.contains("BOTH"), "got: {}", rp.body);
    }

    #[test]
    fn explicit_file_wins() {
        let dir = tempdir();
        let p = dir.path().join("custom.md");
        fs::write(&p, "CUSTOM-MARKER").unwrap();
        let ov = PromptOverrides {
            explicit_file: Some(&p),
            ..Default::default()
        };
        let rp = resolve_prompt(PromptCommand::Lift, &ov);
        assert!(rp.body.contains("CUSTOM-MARKER"));
    }

    #[test]
    fn substitute_replaces_known_keys() {
        let t = "Hello {{name}}! Source: {{source_file_path}}";
        let s = substitute(t, &[("name", "world"), ("source_file_path", "foo.ts")]);
        assert_eq!(s, "Hello world! Source: foo.ts");
    }

    #[test]
    fn bundled_must_placeholders_get_replaced() {
        let rp = resolve_prompt(PromptCommand::Must, &PromptOverrides::default());
        // Sanity: bundled prompt mentions the canonical placeholders.
        assert!(rp.body.contains("{{user_input}}"));
        assert!(rp.body.contains("{{source_file_path}}"));
        let rendered = substitute(
            &rp.body,
            &[
                ("user_input", "not lose money"),
                ("source_file_path", "doubleledger.ts"),
            ],
        );
        assert!(rendered.contains("not lose money"));
        assert!(rendered.contains("doubleledger.ts"));
        // The replaced sites are gone.
        assert!(!rendered.contains("{{user_input}}"));
        assert!(!rendered.contains("{{source_file_path}}"));
    }

    #[test]
    fn substitute_leaves_unknown_keys_visible() {
        let s = substitute("Got {{unknown}} and {{name}}", &[("name", "Sir")]);
        assert!(s.contains("{{unknown}}"));
        assert!(s.contains("Sir"));
    }

    // --- minimal tempdir helper to avoid pulling in `tempfile` ---
    struct TempDir(PathBuf);
    impl TempDir {
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn tempdir() -> TempDir {
        use std::time::{SystemTime, UNIX_EPOCH};
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("provekit-prompt-test-{n}"));
        fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
}
