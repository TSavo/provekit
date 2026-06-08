// SPDX-License-Identifier: Apache-2.0
//
// `provekit lift <PROJECT>`: dispatch the configured lift-plugin protocol
// and emit the raw lifted ProofIR response. Minting is a separate composition
// step owned by `provekit mint`.

use std::io::Write;
use std::path::PathBuf;

use owo_colors::OwoColorize;

use crate::lift_plugin::{self, LiftPluginError, LiftPluginOptions};
use crate::project_config::{read_project_config, read_user_config};
use crate::{LiftArgs, EXIT_OK, EXIT_USER_ERROR, EXIT_VERIFY_FAIL};

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
            ..Default::default()
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
) -> Result<String, libsugar::ProvekitError> {
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
    libsugar::canonical::json_jcs(&doc)
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
