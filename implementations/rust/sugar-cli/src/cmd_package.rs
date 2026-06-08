// SPDX-License-Identifier: Apache-2.0
//
// `sugar package`: package-shaped supply-chain receipt helpers.

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use owo_colors::OwoColorize;

use crate::lift_plugin::{self, LiftPluginError, LiftPluginOptions};
use crate::project_config::{read_project_config, read_user_config};
use crate::{OutputFlags, EXIT_OK, EXIT_USER_ERROR, EXIT_VERIFY_FAIL};

#[derive(Parser, Debug, Clone)]
pub struct PackageArgs {
    #[command(subcommand)]
    pub cmd: PackageCmd,
}

#[derive(Subcommand, Debug, Clone)]
pub enum PackageCmd {
    /// Inspect a package through the configured lift plugin.
    Inspect(PackageInspectArgs),
}

#[derive(Parser, Debug, Clone)]
pub struct PackageInspectArgs {
    /// Package project root containing .sugar/config.toml.
    pub project: PathBuf,
    #[command(flatten)]
    pub out: OutputFlags,
}

pub fn run(args: PackageArgs) -> u8 {
    match args.cmd {
        PackageCmd::Inspect(args) => run_inspect(args),
    }
}

fn run_inspect(args: PackageInspectArgs) -> u8 {
    if !args.project.exists() {
        eprintln!(
            "{}: package project not found: {}",
            "error".red().bold(),
            args.project.display()
        );
        return EXIT_USER_ERROR;
    }

    let project_cfg = read_project_config(&args.project);
    let user_cfg = read_user_config();
    let surface = match project_cfg
        .surface_for("lift")
        .or_else(|| user_cfg.surface_for("lift"))
    {
        Some(surface) => surface,
        None => {
            eprintln!(
                "{}: no package inspection lifter configured. Set [authoring] surface or [authoring.lift] surface in .sugar/config.toml.",
                "error".red().bold()
            );
            return EXIT_USER_ERROR;
        }
    };

    match lift_plugin::dispatch_lift(
        &args.project,
        &surface,
        LiftPluginOptions {
            identify_only: true,
            library_bindings: false,
            ..Default::default()
        },
        args.out.quiet,
    ) {
        Ok(session) => {
            let response = session.response();
            let kind = response
                .get("kind")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown");
            if kind != "package-inspection-document" {
                eprintln!(
                    "{}: package inspect returned `{kind}`; expected `package-inspection-document` from identify-only lifter",
                    "error".red().bold()
                );
                return EXIT_VERIFY_FAIL;
            }
            if args.out.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(response).expect("serialize package inspection")
                );
            } else if !args.out.quiet {
                print_package_summary(response);
            }
            EXIT_OK
        }
        Err(LiftPluginError::MissingBinary { binary }) => {
            eprintln!(
                "{}: package inspection lifter binary `{binary}` not found",
                "error".red().bold()
            );
            EXIT_USER_ERROR
        }
        Err(LiftPluginError::Refused(refusal)) => {
            eprintln!(
                "{}: {}: {}",
                "error".red().bold(),
                refusal.header.failure_kind,
                refusal.header.failure_detail
            );
            EXIT_VERIFY_FAIL
        }
        Err(LiftPluginError::Failed(error)) => {
            eprintln!("{}: {error}", "error".red().bold());
            EXIT_VERIFY_FAIL
        }
    }
}

fn print_package_summary(report: &serde_json::Value) {
    println!("{}", "package inspect".green().bold());
    println!(
        "  ecosystem : {}",
        report
            .get("ecosystem")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown")
    );
    println!(
        "  name      : {}",
        report
            .pointer("/package/name")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown")
    );
    println!(
        "  version   : {}",
        report
            .pointer("/package/version")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown")
    );
    println!(
        "  binaryCid : {}",
        report
            .pointer("/artifact/binaryCid")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown")
    );
}
