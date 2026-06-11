// SPDX-License-Identifier: Apache-2.0
//
// `sugar package`: package-shaped supply-chain receipt helpers.

use std::path::{Path, PathBuf};

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
    /// Attest + verify every artifact declared in a release manifest.
    Release(PackageReleaseArgs),
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

#[derive(Parser, Debug, Clone)]
pub struct PackageReleaseArgs {
    /// Release manifest (TOML) declaring the shippable artifacts to pin.
    #[arg(long)]
    pub manifest: PathBuf,
    /// Base directory for resolving relative artifact paths. Defaults to the
    /// manifest's own directory.
    #[arg(long)]
    pub root: Option<PathBuf>,
    /// Directory to read/write per-artifact receipts. Defaults to
    /// `<manifest-dir>/.sugar/release`.
    #[arg(long)]
    pub receipts: Option<PathBuf>,
    /// Verify against existing receipts only; do not (re)attest. This is the
    /// consumer/gate side: fail if any artifact's bytes no longer match its
    /// pinned binaryCid.
    #[arg(long)]
    pub verify_only: bool,
    #[command(flatten)]
    pub out_flags: OutputFlags,
}

/// A release manifest: the declared set of a project's shippable artifacts.
/// This is the config that arms the coarse pin -- the durable record of what a
/// project pins about itself, rather than an ad-hoc per-invocation flag.
#[derive(Debug, serde::Deserialize)]
struct ReleaseManifest {
    /// Optional default version applied to artifacts without their own.
    version: Option<String>,
    #[serde(default)]
    artifact: Vec<ManifestArtifact>,
}

#[derive(Debug, serde::Deserialize)]
struct ManifestArtifact {
    name: String,
    path: PathBuf,
    version: Option<String>,
}

pub fn run(args: PackageArgs) -> u8 {
    match args.cmd {
        PackageCmd::Inspect(args) => run_inspect(args),
        PackageCmd::Attest(args) => run_attest(args),
        PackageCmd::Release(args) => run_release(args),
    }
}

/// The JSON PackageReleaseReceipt the admission gate consumes (top-level
/// `binaryCid`). Shared by single `attest` and manifest `release`.
fn release_receipt(name: &str, version: &str, binary_cid: &str, bytes: usize) -> serde_json::Value {
    serde_json::json!({
        "kind": "PackageReleaseReceipt",
        "package": {"name": name, "version": version},
        "binaryCid": binary_cid,
        "bytes": bytes,
    })
}

/// `sugar package release --manifest M`: the config-driven dogfood. Reads the
/// manifest, and for each declared artifact either attests it (content-address
/// its bytes, write a receipt) or, with `--verify-only`, checks its current
/// bytes against the pinned binaryCid in an existing receipt. Fail-closed: a
/// missing/unreadable artifact or receipt, or any binaryCid mismatch, fails
/// the whole release. This is the producer that ARMS the artifact rail from a
/// declared manifest -- a sound gate with no producer is a silence read wrong.
fn run_release(args: PackageReleaseArgs) -> u8 {
    let manifest_text = match std::fs::read_to_string(&args.manifest) {
        Ok(t) => t,
        Err(e) => {
            eprintln!(
                "{}: read manifest {}: {e}",
                "error".red().bold(),
                args.manifest.display()
            );
            return EXIT_USER_ERROR;
        }
    };
    let manifest: ReleaseManifest = match toml::from_str(&manifest_text) {
        Ok(m) => m,
        Err(e) => {
            eprintln!(
                "{}: parse manifest {}: {e}",
                "error".red().bold(),
                args.manifest.display()
            );
            return EXIT_USER_ERROR;
        }
    };
    if manifest.artifact.is_empty() {
        eprintln!(
            "{}: manifest {} declares no [[artifact]] entries",
            "error".red().bold(),
            args.manifest.display()
        );
        return EXIT_USER_ERROR;
    }

    let manifest_dir = args
        .manifest
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let root = args.root.clone().unwrap_or_else(|| manifest_dir.clone());
    let receipts = args
        .receipts
        .clone()
        .unwrap_or_else(|| manifest_dir.join(".sugar").join("release"));

    if !args.verify_only {
        if let Err(e) = std::fs::create_dir_all(&receipts) {
            eprintln!(
                "{}: mkdir {}: {e}",
                "error".red().bold(),
                receipts.display()
            );
            return EXIT_USER_ERROR;
        }
    }

    let mut results = Vec::new();
    let mut all_ok = true;
    for art in &manifest.artifact {
        let version = art
            .version
            .clone()
            .or_else(|| manifest.version.clone())
            .unwrap_or_else(|| "unversioned".to_string());
        let artifact_path = root.join(&art.path);
        let receipt_path = receipts.join(format!("{}.release.json", art.name));

        let outcome = release_one(&art.name, &version, &artifact_path, &receipt_path, args.verify_only);
        if let Err(reason) = &outcome {
            all_ok = false;
            if !args.out_flags.json && !args.out_flags.quiet {
                eprintln!("  {} {}: {}", "FAIL".red().bold(), art.name, reason);
            }
        } else if !args.out_flags.json && !args.out_flags.quiet {
            let verb = if args.verify_only { "verified" } else { "attested" };
            println!("  {} {} ({verb})", "ok".green(), art.name);
        }
        results.push(serde_json::json!({
            "name": art.name,
            "ok": outcome.is_ok(),
            "binaryCid": outcome.as_ref().ok(),
            "reason": outcome.err(),
        }));
    }

    if args.out_flags.json {
        println!(
            "{}",
            serde_json::json!({
                "ok": all_ok,
                "mode": if args.verify_only { "verify" } else { "attest" },
                "artifacts": results,
            })
        );
    } else if all_ok && !args.out_flags.quiet {
        let verb = if args.verify_only { "verified" } else { "attested + verified" };
        println!(
            "{}: {} artifact(s) {verb}",
            "package release".green().bold(),
            manifest.artifact.len()
        );
    }

    if all_ok {
        EXIT_OK
    } else {
        EXIT_VERIFY_FAIL
    }
}

/// Attest (or verify-only) one manifest artifact. Returns its binaryCid on
/// success. Attest mode also verifies the receipt it just wrote by re-reading
/// the bytes, so a `release` run proves the produce->consume round-trip.
fn release_one(
    name: &str,
    version: &str,
    artifact_path: &Path,
    receipt_path: &Path,
    verify_only: bool,
) -> Result<String, String> {
    let bytes = std::fs::read(artifact_path)
        .map_err(|e| format!("read artifact {}: {e}", artifact_path.display()))?;
    let observed = blake3_512_of(&bytes);

    if verify_only {
        let receipt_text = std::fs::read_to_string(receipt_path)
            .map_err(|e| format!("read receipt {}: {e}", receipt_path.display()))?;
        let receipt: serde_json::Value = serde_json::from_str(&receipt_text)
            .map_err(|e| format!("parse receipt {}: {e}", receipt_path.display()))?;
        let pinned = receipt
            .get("binaryCid")
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("receipt {} missing binaryCid", receipt_path.display()))?;
        if pinned != observed {
            return Err(format!(
                "binaryCid mismatch (pinned {}, observed {})",
                &pinned[..pinned.len().min(23)],
                &observed[..observed.len().min(23)]
            ));
        }
        return Ok(observed);
    }

    let receipt = release_receipt(name, version, &observed, bytes.len());
    std::fs::write(
        receipt_path,
        serde_json::to_string_pretty(&receipt).expect("serialize release receipt"),
    )
    .map_err(|e| format!("write receipt {}: {e}", receipt_path.display()))?;
    // Round-trip: re-read the bytes and confirm the gate would accept.
    let reread = std::fs::read(artifact_path)
        .map_err(|e| format!("re-read artifact {}: {e}", artifact_path.display()))?;
    if blake3_512_of(&reread) != observed {
        return Err("artifact changed during attestation".to_string());
    }
    Ok(observed)
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

    let receipt = release_receipt(&args.name, &args.version, &binary_cid, bytes.len());

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
