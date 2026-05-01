// SPDX-License-Identifier: Apache-2.0
//
// `provekit fix <bug>` — agent produces patches; we apply them in a
// sandbox, verify, and either ship or feed back. v1: stub agent +
// trivial verifier (always-green when stub is used; the verifier hook
// is a closure callers can override).

use std::path::PathBuf;

use clap::Parser;
use provekit_agent::loop_fix::{
    run_fix_loop, FixLoopOptions, VerifyOutcome, Verifier,
};
use provekit_agent::{FilePatch, FixContext, ProvekitAgent, StubAgent};

use crate::project_config::{read_project_config, read_user_config};
use crate::prompts::{resolve_prompt, substitute, PromptCommand, PromptOverrides};
use crate::{OutputFlags, EXIT_OK, EXIT_USER_ERROR, EXIT_VERIFY_FAIL};

#[derive(Parser, Debug, Clone)]
pub struct FixArgs {
    /// English description of the bug.
    pub bug: String,
    /// Repository root (default: cwd).
    #[arg(long)]
    pub repo: Option<PathBuf>,
    /// Constrain edits to these paths (repeatable).
    #[arg(long)]
    pub allow: Vec<PathBuf>,
    /// Agent backend (overrides config).
    #[arg(long)]
    pub agent: Option<String>,
    /// Maximum agent retries.
    #[arg(long, default_value_t = 3)]
    pub max_retries: u32,
    /// Apply patches without prompting.
    #[arg(long)]
    pub auto_apply: bool,
    /// Override the prompt sent to the agent.
    #[arg(long)]
    pub prompt_file: Option<PathBuf>,
    /// Force a specific authoring surface.
    #[arg(long)]
    pub surface: Option<String>,
    #[command(flatten)]
    pub out: OutputFlags,
}

/// Default verifier — accepts any patch set. v1 placeholder; the real
/// hook will run cargo build / pnpm test in a sandbox. For the stub
/// agent path this is sufficient because the stub does not produce
/// breaking patches.
struct AlwaysGreenVerifier;
impl Verifier for AlwaysGreenVerifier {
    fn verify(&self, _: &[FilePatch]) -> VerifyOutcome {
        VerifyOutcome::Green
    }
}

pub fn run(args: FixArgs) -> u8 {
    let repo_root = args
        .repo
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let project_cfg = read_project_config(&repo_root);
    let user_cfg = read_user_config();
    let surface = args
        .surface
        .clone()
        .or_else(|| project_cfg.surface_for("fix"))
        .or_else(|| user_cfg.surface_for("fix"));
    let agent_name = args
        .agent
        .clone()
        .or_else(|| project_cfg.agent_for("fix"))
        .or_else(|| user_cfg.agent_for("fix"))
        .unwrap_or_else(|| "stub".to_string());

    let overrides = PromptOverrides {
        explicit_file: args.prompt_file.as_deref(),
        project: Some(&repo_root),
        agent: Some(&agent_name),
        surface: surface.as_deref(),
    };
    let prompt = resolve_prompt(PromptCommand::Fix, &overrides);
    let repo_str = repo_root.display().to_string();
    let allowed_str = args
        .allow
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(",");
    let _rendered_prompt = substitute(
        &prompt.body,
        &[
            ("user_input", args.bug.as_str()),
            ("repo_root", repo_str.as_str()),
            ("allowed_paths", allowed_str.as_str()),
            ("violated_contracts", ""),
            ("previous_rejection", ""),
        ],
    );
    // The fix loop does not yet thread a rendered prompt to the agent
    // (FixContext has no `authoring_api_doc` field). Templates are
    // rendered for parity with must/lift; v2 plumbs them through.

    let agent: Box<dyn ProvekitAgent> = match agent_name.as_str() {
        "stub" => Box::new(StubAgent::new()),
        other => {
            eprintln!(
                "error: agent `{other}` is not bundled. v1 ships with `stub`. \
                 See protocol/specs/2026-04-30-agent-plugin-protocol.md."
            );
            return EXIT_USER_ERROR;
        }
    };

    let ctx = FixContext {
        repo_root: repo_root.clone(),
        bug_description: args.bug.clone(),
        violated_contracts: vec![],
        allowed_paths: args.allow.clone(),
        previous_rejection: None,
    };
    let opts = FixLoopOptions {
        max_retries: args.max_retries,
        ..Default::default()
    };
    let outcome = match run_fix_loop(&*agent, ctx, &AlwaysGreenVerifier, &opts) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("error: {e}");
            return EXIT_VERIFY_FAIL;
        }
    };

    if args.out.json {
        let j = serde_json::json!({
            "ok": outcome.verified,
            "patches": outcome.patches.len(),
            "minted_contracts": outcome.minted_contracts.len(),
            "agent_calls": outcome.agent_calls,
            "commentary": outcome.commentary,
            "last_failure": outcome.last_failure,
            "applied": false, // v1: never auto-applies; CLI prints diff for review
            "prompt_source": prompt.source,
            "surface": surface,
            "agent": agent_name,
        });
        println!("{}", serde_json::to_string_pretty(&j).unwrap_or_default());
    } else if !args.out.quiet {
        if outcome.verified {
            println!("fix verified ({} agent calls)", outcome.agent_calls);
            println!("  patches:           {}", outcome.patches.len());
            println!("  minted contracts:  {}", outcome.minted_contracts.len());
            println!("  commentary:        {}", outcome.commentary);
            if !args.auto_apply {
                println!("(re-run with --auto-apply to write patches; v1 prints only)");
            }
        } else {
            println!(
                "fix did not verify after {} attempts",
                outcome.agent_calls
            );
            if let Some(r) = outcome.last_failure {
                println!("last failure: {r}");
            }
        }
    }

    if outcome.verified {
        EXIT_OK
    } else {
        EXIT_VERIFY_FAIL
    }
}
