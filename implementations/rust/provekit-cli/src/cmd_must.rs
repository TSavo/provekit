// SPDX-License-Identifier: Apache-2.0
//
// `provekit must <file> "<english>"` — translate English to a verified
// contract via the configured agent. Drives the propose / validate /
// mint loop and writes the minted contract memento.

use std::path::PathBuf;

use clap::Parser;
use provekit_agent::{run_must_loop, MustContext, MustLoopOptions, ProvekitAgent, StubAgent};

use crate::project_config::{read_project_config, read_user_config};
use crate::prompts::{resolve_prompt, substitute, PromptCommand, PromptOverrides};
use crate::{OutputFlags, EXIT_OK, EXIT_USER_ERROR, EXIT_VERIFY_FAIL};

#[derive(Parser, Debug, Clone)]
pub struct MustArgs {
    /// Source file the agent should read.
    pub file: PathBuf,
    /// English description of the desired guarantee.
    pub description: String,
    /// Agent backend (overrides config).
    #[arg(long)]
    pub agent: Option<String>,
    /// Maximum agent retries when validation rejects.
    #[arg(long, default_value_t = 3)]
    pub max_retries: u32,
    /// Override the prompt sent to the agent.
    #[arg(long)]
    pub prompt_file: Option<PathBuf>,
    /// Force a specific authoring surface (overrides config).
    #[arg(long)]
    pub surface: Option<String>,
    #[command(flatten)]
    pub out: OutputFlags,
}

pub fn run(args: MustArgs) -> u8 {
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

    // Resolve surface + agent: CLI flag > project config > user config > default.
    let project_cfg = read_project_config(&project_root);
    let user_cfg = read_user_config();
    let surface = args
        .surface
        .clone()
        .or_else(|| project_cfg.surface_for("must"))
        .or_else(|| user_cfg.surface_for("must"));
    let agent_name = args
        .agent
        .clone()
        .or_else(|| project_cfg.agent_for("must"))
        .or_else(|| user_cfg.agent_for("must"))
        .unwrap_or_else(|| "stub".to_string());

    let overrides = PromptOverrides {
        explicit_file: args.prompt_file.as_deref(),
        project: Some(&project_root),
        agent: Some(&agent_name),
        surface: surface.as_deref(),
    };
    let prompt = resolve_prompt(PromptCommand::Must, &overrides);
    let file_path_str = args.file.display().to_string();
    let rendered = substitute(
        &prompt.body,
        &[
            ("user_input", args.description.as_str()),
            ("source_file_path", file_path_str.as_str()),
            ("source_file_contents", source_text.as_str()),
            ("previous_rejection", ""),
            ("existing_contracts", ""),
            ("ir_grammar", ""),
        ],
    );

    // Build the agent. v1: only the stub is bundled in-tree; other
    // backends are expected to be installed as plugins via
    // ~/.config/provekit/agents/<name>/manifest.toml.
    let agent: Box<dyn ProvekitAgent> = match agent_name.as_str() {
        "stub" => Box::new(StubAgent::new()),
        other => {
            eprintln!(
                "error: agent `{other}` is not bundled. v1 ships with `stub`; \
                 configure other backends via the plugin manifest \
                 (~/.config/provekit/agents/<name>/manifest.toml). \
                 See protocol/specs/2026-04-30-agent-plugin-protocol.md."
            );
            return EXIT_USER_ERROR;
        }
    };

    let ctx = MustContext {
        source_path: args.file.clone(),
        source_text,
        description: args.description.clone(),
        authoring_api_doc: rendered,
        previous_rejection: None,
    };

    let opts = MustLoopOptions {
        max_retries: args.max_retries,
        ..Default::default()
    };

    let outcome = match run_must_loop(&*agent, ctx, &opts) {
        Ok(o) => o,
        Err(e) => {
            if args.out.json {
                let j = serde_json::json!({
                    "ok": false,
                    "error": e.to_string(),
                });
                println!("{}", serde_json::to_string_pretty(&j).unwrap_or_default());
            } else {
                eprintln!("error: {e}");
            }
            return EXIT_VERIFY_FAIL;
        }
    };

    if args.out.json {
        let j = serde_json::json!({
            "ok": true,
            "minted_cid": outcome.minted.cid,
            "name": outcome.candidate.name,
            "rejected": outcome.rejected.len(),
            "agent_calls": outcome.agent_calls,
            "prompt_source": prompt.source,
            "surface": surface,
            "agent": agent_name,
        });
        println!("{}", serde_json::to_string_pretty(&j).unwrap_or_default());
    } else if !args.out.quiet {
        println!("minted contract: {}", outcome.candidate.name);
        println!("  cid:           {}", outcome.minted.cid);
        println!("  agent:         {agent_name}");
        println!("  agent calls:   {}", outcome.agent_calls);
        println!("  rejected:      {}", outcome.rejected.len());
        println!("  prompt source: {}", prompt.source);
        if let Some(s) = surface.as_deref() {
            println!("  surface:       {s}");
        }
    }
    EXIT_OK
}
