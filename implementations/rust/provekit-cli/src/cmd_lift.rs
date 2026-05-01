// SPDX-License-Identifier: Apache-2.0
//
// `provekit lift <FILE>` — when `--agent` is passed, drives the
// LLM-assisted lift loop (Layer 3). When absent, falls through to the
// stub message; mechanical lift adapters (proptest, contracts, kani,
// etc.) live in their own subcommands and TS plugins.

use std::path::PathBuf;

use clap::Parser;
use owo_colors::OwoColorize;
use provekit_agent::{
    run_lift_loop, LiftLoopOptions, ProposeContext, ProvekitAgent, StubAgent,
};
use serde_json::json;

use crate::project_config::{read_project_config, read_user_config};
use crate::prompts::{resolve_prompt, substitute, PromptCommand, PromptOverrides};
use crate::{LiftArgs, OutputFlags, EXIT_OK, EXIT_USER_ERROR, EXIT_VERIFY_FAIL};

/// Extended args for the agent-driven path. The legacy `LiftArgs`
/// remains the wire shape so the existing CLI route is unchanged;
/// callers wanting the agent path pass `--agent` (handled below).
#[derive(Parser, Debug, Clone)]
pub struct AgentLiftArgs {
    pub file: PathBuf,
    /// Run the lift loop using this agent backend.
    #[arg(long)]
    pub agent: Option<String>,
    /// Restrict to one function name.
    #[arg(long)]
    pub function: Option<String>,
    #[arg(long, default_value_t = 3)]
    pub max_retries: u32,
    #[arg(long)]
    pub prompt_file: Option<PathBuf>,
    #[arg(long)]
    pub surface: Option<String>,
    #[command(flatten)]
    pub out: OutputFlags,
}

pub fn run(args: LiftArgs) -> u8 {
    let msg = "Lift v0 lives in TS. See implementations/typescript/src/proveLift/. \
               Pass --agent <name> to drive the LLM-assisted Rust path.";
    if args.out.json {
        let payload = json!({
            "status": "stub",
            "message": msg,
            "file": args.file.as_ref().map(|p| p.display().to_string()),
        });
        println!("{}", serde_json::to_string_pretty(&payload).unwrap());
    } else {
        println!("{} {}", "lift:".yellow().bold(), msg);
    }
    EXIT_OK
}

/// New-style agent-driven lift. Wired into main.rs as `Cmd::AgentLift`.
pub fn run_agent(args: AgentLiftArgs) -> u8 {
    if !args.file.exists() {
        eprintln!("error: source file not found: {}", args.file.display());
        return EXIT_USER_ERROR;
    }
    let source_text = match std::fs::read_to_string(&args.file) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: read {}: {e}", args.file.display());
            return EXIT_USER_ERROR;
        }
    };
    let project_root = args
        .file
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    let project_cfg = read_project_config(&project_root);
    let user_cfg = read_user_config();
    let surface = args
        .surface
        .clone()
        .or_else(|| project_cfg.surface_for("lift"))
        .or_else(|| user_cfg.surface_for("lift"));
    let agent_name = args
        .agent
        .clone()
        .or_else(|| project_cfg.agent_for("lift"))
        .or_else(|| user_cfg.agent_for("lift"))
        .unwrap_or_else(|| "stub".to_string());

    let overrides = PromptOverrides {
        explicit_file: args.prompt_file.as_deref(),
        project: Some(&project_root),
        agent: Some(&agent_name),
        surface: surface.as_deref(),
    };
    let prompt = resolve_prompt(PromptCommand::Lift, &overrides);
    let file_path_str = args.file.display().to_string();
    let function_str = args.function.clone().unwrap_or_default();
    let rendered = substitute(
        &prompt.body,
        &[
            ("user_input", ""),
            ("source_file_path", file_path_str.as_str()),
            ("source_file_contents", source_text.as_str()),
            ("function_name", function_str.as_str()),
            ("previous_rejection", ""),
            ("existing_contracts", ""),
            ("ir_grammar", ""),
        ],
    );

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

    let ctx = ProposeContext {
        source_path: args.file.clone(),
        source_text,
        function_name: args.function.clone(),
        authoring_api_doc: rendered,
        existing_contract_names: vec![],
        previous_rejection: None,
    };
    let opts = LiftLoopOptions {
        max_retries: args.max_retries,
        ..Default::default()
    };
    let outcome = match run_lift_loop(&*agent, ctx, &opts) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("error: {e}");
            return EXIT_VERIFY_FAIL;
        }
    };

    if args.out.json {
        let j = json!({
            "minted": outcome.minted.iter().map(|m| json!({
                "name": m.name,
                "cid":  m.cid,
            })).collect::<Vec<_>>(),
            "rejected": outcome.rejected.iter().map(|(c, r)| json!({
                "name": c.name,
                "reason": r,
            })).collect::<Vec<_>>(),
            "agent_calls": outcome.agent_calls,
            "prompt_source": prompt.source,
            "surface": surface,
            "agent": agent_name,
        });
        println!("{}", serde_json::to_string_pretty(&j).unwrap_or_default());
    } else if !args.out.quiet {
        println!("lift: minted {}, rejected {}, agent calls {}",
            outcome.minted.len(),
            outcome.rejected.len(),
            outcome.agent_calls,
        );
        for m in &outcome.minted {
            println!("  + {} {}", m.name, m.cid);
        }
        for (c, r) in &outcome.rejected {
            println!("  - {} (rejected: {})", c.name, r);
        }
    }
    if outcome.minted.is_empty() {
        EXIT_VERIFY_FAIL
    } else {
        EXIT_OK
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OutputFlags;

    #[test]
    fn lift_returns_ok() {
        let args = LiftArgs {
            file: None,
            out: OutputFlags::default(),
        };
        assert_eq!(run(args), crate::EXIT_OK);
    }
}
