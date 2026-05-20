// SPDX-License-Identifier: Apache-2.0
//
// `provekit lift <PROJECT>`: dispatch the configured lift-plugin protocol
// and emit the raw lifted ProofIR response. Minting is a separate composition
// step owned by `provekit mint`.

use std::io::Write;
use std::path::PathBuf;

use clap::Parser;
use owo_colors::OwoColorize;
use provekit_agent::{run_lift_loop, LiftLoopOptions, ProposeContext, ProvekitAgent, StubAgent};
use serde_json::json;

use crate::lift_plugin::{self, LiftPluginError, LiftPluginOptions};
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
    let project_root = args.project.unwrap_or_else(|| PathBuf::from("."));
    if !project_root.exists() {
        eprintln!(
            "{}: project not found: {}",
            "error".red().bold(),
            project_root.display()
        );
        return EXIT_USER_ERROR;
    }

    let project_cfg = read_project_config(&project_root);
    let user_cfg = read_user_config();
    let surface = match project_cfg
        .surface_for("lift")
        .or_else(|| user_cfg.surface_for("lift"))
    {
        Some(surface) => surface,
        None => {
            eprintln!(
                "{}: no lift surface configured. Set [authoring] surface or [authoring.lift] surface in .provekit/config.toml.",
                "error".red().bold()
            );
            return EXIT_USER_ERROR;
        }
    };

    match lift_plugin::dispatch_lift_path(
        &project_root,
        &surface,
        LiftPluginOptions {
            identify_only: args.identify_only,
            library_bindings: args.library_bindings,
        },
        true,
    ) {
        Ok(session) => {
            let response = session.response();
            if args.identify_only
                && response
                    .get("kind")
                    .and_then(|value| value.as_str())
                    .is_none_or(|kind| {
                        kind != "identity-document" && kind != "package-inspection-document"
                    })
            {
                let kind = response
                    .get("kind")
                    .and_then(|value| value.as_str())
                    .unwrap_or("unknown");
                eprintln!(
                    "{}: identify-only lift returned `{kind}`; expected `identity-document` or `package-inspection-document`",
                    "error".red().bold()
                );
                return EXIT_VERIFY_FAIL;
            }
            let output = match lift_output_document(&project_root, &surface, response) {
                Ok(output) => output,
                Err(error) => {
                    eprintln!(
                        "{}: canonicalize lift response: {error}",
                        "error".red().bold()
                    );
                    return EXIT_USER_ERROR;
                }
            };
            if let Err(error) = write_output(args.output.as_ref(), output.as_bytes()) {
                eprintln!("{}: {error}", "error".red().bold());
                return EXIT_USER_ERROR;
            }
            if !args.out.quiet
                && args
                    .output
                    .as_ref()
                    .is_some_and(|path| path.as_os_str() != "-")
            {
                eprintln!("lift: wrote ProofIR term JSON");
            }
            EXIT_OK
        }
        Err(LiftPluginError::MissingBinary { binary }) => {
            eprintln!(
                "{}: lifter binary `{binary}` not found",
                "error".red().bold()
            );
            EXIT_USER_ERROR
        }
        Err(LiftPluginError::Refused(refusal)) => {
            eprintln!(
                "{}: {}",
                "error".red().bold(),
                serde_json::to_string(&refusal).unwrap_or_else(|_| {
                    format!(
                        "{}: {}",
                        refusal.header.failure_kind, refusal.header.failure_detail
                    )
                })
            );
            EXIT_VERIFY_FAIL
        }
        Err(LiftPluginError::Failed(error)) => {
            eprintln!("{}: {error}", "error".red().bold());
            EXIT_VERIFY_FAIL
        }
    }
}

fn lift_output_document(
    project_root: &PathBuf,
    surface: &str,
    response: &serde_json::Value,
) -> Result<String, libprovekit::ProvekitError> {
    let mut doc = response.clone();
    if let Some(object) = doc.as_object_mut() {
        object
            .entry("sourceLanguage".to_string())
            .or_insert_with(|| serde_json::Value::String(surface.to_string()));
        object
            .entry("workspaceRoot".to_string())
            .or_insert_with(|| {
                serde_json::Value::String(
                    project_root
                        .canonicalize()
                        .unwrap_or_else(|_| project_root.to_path_buf())
                        .display()
                        .to_string(),
                )
            });
    }
    libprovekit::canonical::json_jcs(&doc)
}

fn write_output(path: Option<&PathBuf>, bytes: &[u8]) -> Result<(), String> {
    match path {
        Some(path) if path.as_os_str() != "-" => {
            std::fs::write(path, bytes).map_err(|e| format!("write {}: {e}", path.display()))
        }
        _ => {
            let mut stdout = std::io::stdout().lock();
            stdout
                .write_all(bytes)
                .map_err(|e| format!("write stdout: {e}"))
        }
    }
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
        println!(
            "lift: minted {}, rejected {}, agent calls {}",
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
            project: Some(PathBuf::from("/provekit/no/such/lift/project")),
            output: None,
            identify_only: false,
            library_bindings: false,
            out: OutputFlags::default(),
        };
        assert_eq!(run(args), crate::EXIT_USER_ERROR);
    }
}
