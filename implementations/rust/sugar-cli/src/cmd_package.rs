// SPDX-License-Identifier: Apache-2.0
//
// `sugar package`: package-shaped supply-chain receipt helpers.

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use owo_colors::OwoColorize;
use sugar_canonicalizer::blake3_512_of;

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
    /// Mint a release proof pinning a shippable artifact's binaryCid.
    Attest(PackageAttestArgs),
}

#[derive(Parser, Debug, Clone)]
pub struct PackageInspectArgs {
    /// Package project root containing .sugar/config.toml.
    pub project: PathBuf,
    #[command(flatten)]
    pub out: OutputFlags,
}

#[derive(Parser, Debug, Clone)]
pub struct PackageAttestArgs {
    /// The shippable artifact to pin (npm tarball, firmware image, ...).
    #[arg(long)]
    pub artifact: PathBuf,
    /// Package name recorded in the release proof.
    #[arg(long)]
    pub name: String,
    /// Package version recorded in the release proof.
    #[arg(long)]
    pub version: String,
    /// Where to write the `.proof` release attestation.
    #[arg(long)]
    pub out: PathBuf,
    #[command(flatten)]
    pub out_flags: OutputFlags,
}

pub fn run(args: PackageArgs) -> u8 {
    match args.cmd {
        PackageCmd::Inspect(args) => run_inspect(args),
        PackageCmd::Attest(args) => run_attest(args),
    }
}

/// `sugar package attest`: arm the coarse supply-chain pin. Reads the
/// shippable artifact, content-addresses its bytes, and writes a JSON
/// PackageReleaseReceipt whose top-level `binaryCid` the admission gate
/// (`sugar verify --artifact --proof`) checks. This is the production producer
/// of binaryCid-bearing receipts -- without it the artifact rail is sound but
/// unarmed (no receipt pins a binary), so contract-free byte changes pass
/// unnoticed. The receipt is the JSON shape the gate already consumes
/// (`run_admission_gate_with` reads `proof["binaryCid"]`), not the CBOR
/// `.proof` envelope.
fn run_attest(args: PackageAttestArgs) -> u8 {
    let bytes = match std::fs::read(&args.artifact) {
        Ok(b) => b,
        Err(e) => {
            eprintln!(
                "{}: read artifact {}: {e}",
                "error".red().bold(),
                args.artifact.display()
            );
            return EXIT_USER_ERROR;
        }
    };
    let binary_cid = blake3_512_of(&bytes);

    let receipt = serde_json::json!({
        "kind": "PackageReleaseReceipt",
        "package": {"name": args.name, "version": args.version},
        "binaryCid": binary_cid,
        "bytes": bytes.len(),
    });

    if let Some(parent) = args.out.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            eprintln!("{}: mkdir {}: {e}", "error".red().bold(), parent.display());
            return EXIT_USER_ERROR;
        }
    }
    if let Err(e) = std::fs::write(
        &args.out,
        serde_json::to_string_pretty(&receipt).expect("serialize release receipt"),
    ) {
        eprintln!(
            "{}: write {}: {e}",
            "error".red().bold(),
            args.out.display()
        );
        return EXIT_USER_ERROR;
    }

    if args.out_flags.json {
        println!(
            "{}",
            serde_json::json!({
                "ok": true,
                "binaryCid": binary_cid,
                "receipt": args.out,
            })
        );
    } else if !args.out_flags.quiet {
        println!("{}", "package attest".green().bold());
        println!("  binaryCid : {binary_cid}");
        println!("  receipt   : {}", args.out.display());
    }
    EXIT_OK
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
