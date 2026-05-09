// SPDX-License-Identifier: Apache-2.0
//
// `provekit ci ...` — CICP reference admission surface.
//
// Language libraries may emit CICP JSON bodies natively. The Rust CLI
// is the universal checker: it parses the body, runs the reference
// libprovekit validator, and reports the body CID.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand, ValueEnum};
use libprovekit::canonical::json_cid;
use libprovekit::ci::{
    admit_identical_reuse, check_ci_body, CIBlastRadius, CIBlastRadiusInput, CIJobResultBodyClaim,
    CIJobResultInput, CINondeterminism, CINondeterminismMode, CIProducer,
};
use owo_colors::OwoColorize;
use serde_json::{json, Value as Json};
use walkdir::WalkDir;

use crate::cmd_mint::{resolve_kit, KIT_TABLE};
use crate::protocol::EXPECTED_CATALOG_CID;
use crate::OutputFlags;

#[derive(Parser, Debug, Clone)]
pub struct CiArgs {
    #[command(subcommand)]
    pub cmd: CiCmd,
}

#[derive(Subcommand, Debug, Clone)]
pub enum CiCmd {
    /// Check a CICP body claim and print its canonical CID.
    Check(CiCheckArgs),
    /// Emit a CIJobResultBodyClaim for a completed job.
    Result(CiResultArgs),
    /// Compute CICP blast-radius body claims without skipping any CI work yet.
    Shadow(CiShadowArgs),
    /// Admit an identical-input-closure reuse witness for an accepted prior result.
    Reuse(CiReuseArgs),
    /// Generate or check checked-in CICP accepted result witnesses.
    Accept(CiAcceptArgs),
}

#[derive(Parser, Debug, Clone)]
pub struct CiCheckArgs {
    /// CICP JSON body claim to validate.
    #[arg(long)]
    pub body: PathBuf,
    #[command(flatten)]
    pub out: OutputFlags,
}

#[derive(Parser, Debug, Clone)]
pub struct CiResultArgs {
    /// CIBlastRadius JSON body for the completed job.
    #[arg(long)]
    pub blast_radius: PathBuf,
    /// File path for the emitted CIJobResultBodyClaim.
    #[arg(long)]
    pub out: PathBuf,
    /// Job result to record.
    #[arg(long, value_enum, default_value = "pass")]
    pub result: CiResultArg,
    /// CID of the job output artifact. Defaults to a deterministic checked-in marker CID.
    #[arg(long)]
    pub output_cid: Option<String>,
    /// CID of the job log artifact. Defaults to a deterministic checked-in marker CID.
    #[arg(long)]
    pub log_cid: Option<String>,
    /// Start time carried by the result witness.
    #[arg(long, default_value = "2026-05-07T00:00:00Z")]
    pub started_at: String,
    /// Finish time carried by the result witness.
    #[arg(long, default_value = "2026-05-07T00:00:00Z")]
    pub finished_at: String,
    /// Producer kind carried by the result witness.
    #[arg(long, default_value = "ci-runner")]
    pub producer_kind: String,
    /// Producer name carried by the result witness.
    #[arg(long, default_value = "provekit-ci")]
    pub producer_name: String,
    /// Producer version carried by the result witness.
    #[arg(long, default_value = "checked-in")]
    pub producer_version: String,
    #[command(flatten)]
    pub flags: OutputFlags,
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum CiResultArg {
    Pass,
    Fail,
    Flaky,
}

#[derive(Parser, Debug, Clone)]
pub struct CiShadowArgs {
    /// Repository root whose paths define the blast-radius closures.
    #[arg(long, default_value = ".")]
    pub repo: PathBuf,
    /// Kit to profile. Known kits match `provekit mint --kit`.
    #[arg(long, conflicts_with = "all_kits")]
    pub kit: Option<String>,
    /// Emit a shadow blast radius for every known kit.
    #[arg(long)]
    pub all_kits: bool,
    /// Directory for generated shadow bodies and summaries.
    #[arg(long)]
    pub out_dir: Option<PathBuf>,
    /// Stable runner identity label included in each blast radius.
    #[arg(long, default_value = "provekit-ci-shadow/local")]
    pub runner_identity: String,
    #[command(flatten)]
    pub out: OutputFlags,
}

#[derive(Parser, Debug, Clone)]
pub struct CiReuseArgs {
    /// Current CIBlastRadius JSON body.
    #[arg(long)]
    pub current_blast_radius: PathBuf,
    /// Prior CIJobResultBodyClaim JSON body to consider for reuse.
    #[arg(
        long,
        conflicts_with = "accepted_dir",
        required_unless_present = "accepted_dir"
    )]
    pub previous_result: Option<PathBuf>,
    /// Root directory of checked-in accepted job-result witnesses.
    #[arg(long, conflicts_with = "previous_result")]
    pub accepted_dir: Option<PathBuf>,
    /// File path for the emitted CIReuseBodyClaim skip witness.
    #[arg(long)]
    pub reuse_out: PathBuf,
    #[command(flatten)]
    pub out: OutputFlags,
}

#[derive(Parser, Debug, Clone)]
pub struct CiAcceptArgs {
    /// Repository root whose paths define the blast-radius closures.
    #[arg(long, default_value = ".")]
    pub repo: PathBuf,
    /// Kit to accept. Known kits match `provekit mint --kit`.
    #[arg(long, conflicts_with = "all_kits")]
    pub kit: Option<String>,
    /// Accept witnesses for the checked-in CICP prove-job kit set.
    #[arg(long)]
    pub all_kits: bool,
    /// Root directory for checked-in accepted job-result witnesses.
    #[arg(long, default_value = ".provekit/ci/accepted")]
    pub out: PathBuf,
    /// Candidate CIJobResultBodyClaim to accept. Use with a single --kit.
    #[arg(long, conflicts_with = "results_dir")]
    pub result: Option<PathBuf>,
    /// Directory containing per-kit candidate job results as <kit>/job-result.json.
    #[arg(long)]
    pub results_dir: Option<PathBuf>,
    /// Bootstrap mode: create a passing accepted result without importing a candidate result.
    #[arg(long, conflicts_with_all = ["result", "results_dir"])]
    pub assume_pass: bool,
    /// Stable runner identity label. Defaults to GitHub Linux, except Swift macOS.
    #[arg(long)]
    pub runner_identity: Option<String>,
    /// Refuse stale witnesses and print the missing set without writing files.
    #[arg(long)]
    pub check: bool,
    /// Compute blast radii from a detached clean git worktree at HEAD.
    #[arg(long)]
    pub clean: bool,
    #[command(flatten)]
    pub flags: OutputFlags,
}

pub fn run(args: CiArgs) -> u8 {
    match args.cmd {
        CiCmd::Check(args) => match run_check(&args) {
            Ok(payload) => {
                if args.out.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&payload)
                            .unwrap_or_else(|_| payload.to_string())
                    );
                } else if !args.out.quiet {
                    println!("{}", "ProvekIt CI body check".bold());
                    println!(
                        "  body kind: {}",
                        payload["bodyKind"].as_str().unwrap_or("")
                    );
                    println!("  body CID : {}", payload["bodyCid"].as_str().unwrap_or(""));
                    println!("  status   : {}", "admitted".green().bold());
                }
                crate::EXIT_OK
            }
            Err(e) => {
                eprintln!("{}: {e}", "error".red().bold());
                crate::EXIT_USER_ERROR
            }
        },
        CiCmd::Result(args) => match run_result(&args) {
            Ok(payload) => {
                if args.flags.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&payload)
                            .unwrap_or_else(|_| payload.to_string())
                    );
                } else if !args.flags.quiet {
                    print_result_human(&payload);
                }
                crate::EXIT_OK
            }
            Err(e) => {
                eprintln!("{}: {e}", "error".red().bold());
                crate::EXIT_USER_ERROR
            }
        },
        CiCmd::Shadow(args) => match run_shadow(&args) {
            Ok(payload) => {
                if args.out.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&payload)
                            .unwrap_or_else(|_| payload.to_string())
                    );
                } else if !args.out.quiet {
                    print_shadow_human(&payload);
                }
                crate::EXIT_OK
            }
            Err(e) => {
                eprintln!("{}: {e}", "error".red().bold());
                crate::EXIT_USER_ERROR
            }
        },
        CiCmd::Reuse(args) => match run_reuse(&args) {
            Ok(payload) => {
                if args.out.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&payload)
                            .unwrap_or_else(|_| payload.to_string())
                    );
                } else if !args.out.quiet {
                    print_reuse_human(&payload);
                }
                crate::EXIT_OK
            }
            Err(e) => {
                eprintln!("{}: {e}", "admission refused".red().bold());
                crate::EXIT_VERIFY_FAIL
            }
        },
        CiCmd::Accept(args) => match run_accept(&args) {
            Ok(payload) => {
                if args.flags.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&payload)
                            .unwrap_or_else(|_| payload.to_string())
                    );
                } else if !args.flags.quiet {
                    print_accept_human(&payload);
                }
                crate::EXIT_OK
            }
            Err(e) => {
                eprintln!("{e}");
                crate::EXIT_VERIFY_FAIL
            }
        },
    }
}

fn run_check(args: &CiCheckArgs) -> Result<Json, String> {
    let body = read_json_file(&args.body)?;
    let check = check_ci_body(&body).map_err(|e| e.to_string())?;

    Ok(json!({
        "kind": "CICheck",
        "ok": true,
        "bodyKind": check.kind,
        "bodyCid": check.cid,
        "bodyPath": args.body.display().to_string(),
    }))
}

fn run_result(args: &CiResultArgs) -> Result<Json, String> {
    let blast_body = read_json_file(&args.blast_radius)?;
    let blast_check = check_ci_body(&blast_body).map_err(|e| e.to_string())?;
    if blast_check.kind != "CIBlastRadius" {
        return Err(format!(
            "blast radius body must be CIBlastRadius, got {}",
            blast_check.kind
        ));
    }
    let blast: CIBlastRadius =
        serde_json::from_value(blast_body).map_err(|e| format!("parse CIBlastRadius: {e}"))?;
    let result_json = build_result_body(
        &blast,
        &blast_check.cid,
        args.result,
        args.output_cid.clone(),
        args.log_cid.clone(),
        &args.started_at,
        &args.finished_at,
        &args.producer_kind,
        &args.producer_name,
        &args.producer_version,
    )?;
    let result_check = check_ci_body(&result_json).map_err(|e| e.to_string())?;
    write_json_file(&args.out, &result_json)?;

    Ok(json!({
        "kind": "CIResult",
        "ok": true,
        "result": ci_result_name(args.result),
        "jobKey": blast.job_key,
        "blastRadiusCid": blast_check.cid,
        "bodyCid": result_check.cid,
        "bodyPath": args.out.display().to_string(),
        "body": result_json,
    }))
}

fn run_accept(args: &CiAcceptArgs) -> Result<Json, String> {
    let source_repo = args
        .repo
        .canonicalize()
        .map_err(|e| format!("canonicalize repo {}: {e}", args.repo.display()))?;
    let accepted_dir = absolutize_under(&source_repo, &args.out);
    let kits = selected_accept_kits(args)?;
    if args.result.is_some() && kits.len() != 1 {
        return Err("--result accepts exactly one --kit; use --results-dir with --all-kits".into());
    }
    let mut guard = None;
    let working_repo = if args.clean {
        let clean = CleanWorktree::create(&source_repo)?;
        let path = clean.path.clone();
        guard = Some(clean);
        path
    } else {
        source_repo.clone()
    };

    let scratch = AcceptScratch::new()?;
    let mut results = Vec::new();
    let mut missing = Vec::new();
    let mut added_count = 0usize;
    let mut existing_count = 0usize;
    let mut verified_count = 0usize;

    for kit in kits {
        let runner_identity = accept_runner_identity(args, &kit);
        let shadow_dir = scratch.path.join(&kit);
        build_shadow_for_kit(&working_repo, &shadow_dir, &kit, &runner_identity)?;
        let blast_path = shadow_dir.join("blast-radius.json");
        let blast_body = read_json_file(&blast_path)?;
        let blast_check = check_ci_body(&blast_body).map_err(|e| e.to_string())?;
        let blast: CIBlastRadius = serde_json::from_value(blast_body)
            .map_err(|e| format!("parse CIBlastRadius for {kit}: {e}"))?;
        let accepted_path = accepted_result_path(&accepted_dir, &blast, &blast_check.cid)?;

        if accepted_path.exists() {
            let witness_cid = verify_accepted_result(&blast, &blast_check.cid, &accepted_path)?;
            existing_count += 1;
            verified_count += 1;
            results.push(json!({
                "kit": kit,
                "jobKey": blast.job_key,
                "runnerIdentity": runner_identity,
                "blastRadiusCid": blast_check.cid,
                "acceptedResultPath": accepted_path.display().to_string(),
                "witnessCid": witness_cid,
                "status": "existing",
            }));
            continue;
        }

        missing.push((kit.clone(), blast_check.cid.clone(), accepted_path.clone()));
        if args.check {
            results.push(json!({
                "kit": kit,
                "jobKey": blast.job_key,
                "runnerIdentity": runner_identity,
                "blastRadiusCid": blast_check.cid,
                "acceptedResultPath": accepted_path.display().to_string(),
                "status": "missing",
            }));
            continue;
        }

        let (result_json, result_cid, source_kind, candidate_result_path) =
            accept_result_input(args, &source_repo, &kit, &blast, &blast_check.cid)?;
        write_json_file(&accepted_path, &result_json)?;
        let witness_cid = verify_accepted_result(&blast, &blast_check.cid, &accepted_path)?;
        added_count += 1;
        verified_count += 1;
        results.push(json!({
            "kit": kit,
            "jobKey": blast.job_key,
            "runnerIdentity": runner_identity,
            "blastRadiusCid": blast_check.cid,
            "acceptedResultPath": accepted_path.display().to_string(),
            "witnessCid": witness_cid,
            "source": source_kind,
            "candidateResultPath": candidate_result_path.map(|path| path.display().to_string()),
            "status": "added",
        }));
        debug_assert_eq!(result_cid, witness_cid);
    }

    drop(guard);

    if args.check && !missing.is_empty() {
        return Err(stale_accept_error(args, &missing));
    }

    let missing_count = if args.check { missing.len() } else { 0 };

    Ok(json!({
        "kind": "CIAccept",
        "ok": true,
        "mode": if args.check { "check" } else { "write" },
        "clean": args.clean,
        "repo": source_repo.display().to_string(),
        "acceptedDir": accepted_dir.display().to_string(),
        "addedCount": added_count,
        "existingCount": existing_count,
        "missingCount": missing_count,
        "verifiedCount": verified_count,
        "results": results,
    }))
}

fn run_reuse(args: &CiReuseArgs) -> Result<Json, String> {
    let current_body = read_json_file(&args.current_blast_radius)?;
    let current_check = check_ci_body(&current_body).map_err(|e| e.to_string())?;
    if current_check.kind != "CIBlastRadius" {
        return Err(format!(
            "current blast radius body must be CIBlastRadius, got {}",
            current_check.kind
        ));
    }
    let current: CIBlastRadius = serde_json::from_value(current_body)
        .map_err(|e| format!("parse current CIBlastRadius: {e}"))?;

    let previous_result_path = match &args.previous_result {
        Some(path) => path.clone(),
        None => {
            let accepted_dir = args
                .accepted_dir
                .as_ref()
                .ok_or_else(|| "pass --previous-result or --accepted-dir".to_string())?;
            let path = accepted_result_path(accepted_dir, &current, &current_check.cid)?;
            if !path.exists() {
                return Err(format!(
                    "no accepted result witness for {} at {}",
                    current_check.cid,
                    path.display()
                ));
            }
            path
        }
    };

    let previous_body = read_json_file(&previous_result_path)?;
    let previous_check = check_ci_body(&previous_body).map_err(|e| e.to_string())?;
    if previous_check.kind != "CIJobResultBodyClaim" {
        return Err(format!(
            "previous result body must be CIJobResultBodyClaim, got {}",
            previous_check.kind
        ));
    }
    let previous: CIJobResultBodyClaim = serde_json::from_value(previous_body)
        .map_err(|e| format!("parse previous CIJobResultBodyClaim: {e}"))?;

    let reuse = admit_identical_reuse(&current, &previous).map_err(|e| e.to_string())?;
    let reuse_body = serde_json::to_value(&reuse).map_err(|e| format!("serialize reuse: {e}"))?;
    let reuse_check = check_ci_body(&reuse_body).map_err(|e| e.to_string())?;
    write_json_file(&args.reuse_out, &reuse_body)?;

    Ok(json!({
        "kind": "CIReuseAdmission",
        "ok": true,
        "wouldSkip": true,
        "skipReason": "accepted-identical-input-closure",
        "jobKey": current.job_key,
        "currentBlastRadiusCid": current_check.cid,
        "previousResultWitnessCid": previous_check.cid,
        "acceptedResultPath": previous_result_path.display().to_string(),
        "reuseBodyCid": reuse_check.cid,
        "reuseBodyPath": args.reuse_out.display().to_string(),
        "reuseBody": reuse_body,
    }))
}

fn run_shadow(args: &CiShadowArgs) -> Result<Json, String> {
    let repo = args
        .repo
        .canonicalize()
        .map_err(|e| format!("canonicalize repo {}: {e}", args.repo.display()))?;
    let kits = selected_kits(args)?;
    let base_out = args
        .out_dir
        .clone()
        .unwrap_or_else(|| repo.join(".provekit/ci-shadow"));

    let mut results = Vec::new();
    for kit in kits {
        let out_dir = if args.all_kits {
            base_out.join(&kit)
        } else {
            base_out.clone()
        };
        results.push(build_shadow_for_kit(
            &repo,
            &out_dir,
            &kit,
            &args.runner_identity,
        )?);
    }

    if args.all_kits {
        let summary = json!({
            "kind": "CIShadowSet",
            "ok": true,
            "wouldSkip": false,
            "repo": repo.display().to_string(),
            "results": results,
        });
        fs::create_dir_all(&base_out).map_err(|e| format!("create {}: {e}", base_out.display()))?;
        write_json_file(&base_out.join("summary.json"), &summary)?;
        Ok(summary)
    } else {
        results
            .into_iter()
            .next()
            .ok_or_else(|| "no kit selected".to_string())
    }
}

fn selected_kits(args: &CiShadowArgs) -> Result<Vec<String>, String> {
    if args.all_kits {
        return Ok(KIT_TABLE
            .iter()
            .map(|(kit, _, _, _)| kit.to_string())
            .collect());
    }
    let kit = args
        .kit
        .clone()
        .ok_or_else(|| "pass --kit <kit> or --all-kits".to_string())?;
    if resolve_kit(&kit).is_none() {
        return Err(format!("unknown kit `{kit}`"));
    }
    Ok(vec![kit])
}

const CICP_ACCEPT_KITS: &[&str] = &[
    "rust", "go", "cpp", "ts", "csharp", "java", "python", "ruby", "zig", "c", "swift",
];

fn selected_accept_kits(args: &CiAcceptArgs) -> Result<Vec<String>, String> {
    if args.all_kits {
        return Ok(CICP_ACCEPT_KITS.iter().map(|kit| kit.to_string()).collect());
    }
    let kit = args
        .kit
        .clone()
        .ok_or_else(|| "pass --kit <kit> or --all-kits".to_string())?;
    if !CICP_ACCEPT_KITS.contains(&kit.as_str()) {
        return Err(format!(
            "unknown CICP accept kit `{kit}`; known kits: {}",
            CICP_ACCEPT_KITS.join(", ")
        ));
    }
    if resolve_kit(&kit).is_none() {
        return Err(format!("unknown kit `{kit}`"));
    }
    Ok(vec![kit])
}

fn accept_runner_identity(args: &CiAcceptArgs, kit: &str) -> String {
    args.runner_identity.clone().unwrap_or_else(|| {
        if kit == "swift" {
            "github-actions/macOS/X64".to_string()
        } else {
            "github-actions/Linux/X64".to_string()
        }
    })
}

fn build_shadow_for_kit(
    repo: &Path,
    out_dir: &Path,
    kit: &str,
    runner_identity: &str,
) -> Result<Json, String> {
    let (project_path, _surface, lang) =
        resolve_kit(kit).ok_or_else(|| format!("unknown kit `{kit}`"))?;
    let project_rel = normalize_rel(&project_path);
    let project_abs = repo.join(&project_rel);
    if !project_abs.exists() {
        return Err(format!(
            "kit `{kit}` project path does not exist: {}",
            project_abs.display()
        ));
    }

    let profile = KitCiProfile::for_kit(kit, &lang, project_rel)?;
    let source_closure = path_closure_cid(repo, &profile.source_roots)?;
    let lockfile_cids = file_cids(repo, &profile.lockfile_paths)?;
    let relevant_spec_cids = relevant_protocol_spec_cids(repo)?;
    let fixture_cids = fixture_cids(repo)?;
    let command_cid = value_cid(&json!({
        "kind": "CICommand",
        "schemaVersion": "1",
        "kit": kit,
        "command": profile.command,
    }))?;
    let job_definition_cid = value_cid(&json!({
        "kind": "CIKitJobDefinition",
        "schemaVersion": "1",
        "kit": kit,
        "jobKey": profile.job_key,
        "sourceRoots": profile.source_roots,
        "lockfilePaths": profile.lockfile_paths,
        "command": profile.command,
        "protocolInputs": [
            "protocol/specs",
            "protocol/conformance/cicp"
        ],
    }))?;
    let runner_identity_cid = value_cid(&json!({
        "kind": "CIRunnerIdentity",
        "schemaVersion": "1",
        "identity": runner_identity,
    }))?;
    let toolchain_cids = vec![value_cid(&json!({
        "kind": "CIKitToolchain",
        "schemaVersion": "1",
        "kit": kit,
        "markers": profile.toolchain_markers,
    }))?];
    let policy_cid = value_cid(&json!({
        "kind": "CICPCheckedInReusePolicy",
        "schemaVersion": "1",
        "mode": "checked-in-accepted-witness",
        "acceptedWitnessRoot": ".provekit/ci/accepted",
        "reuseReasons": ["identical-input-closure"],
        "skipBuildOnAcceptedReuse": true,
    }))?;

    let blast = CIBlastRadiusInput {
        job_key: profile.job_key.clone(),
        subject_kind: "kit".to_string(),
        subject: kit.to_string(),
        protocol_catalog_cid: EXPECTED_CATALOG_CID.to_string(),
        job_definition_cid,
        command_cid,
        runner_identity_cid,
        toolchain_cids,
        source_closure_cid: source_closure,
        lockfile_cids,
        generated_input_cids: Vec::new(),
        fixture_cids,
        relevant_spec_cids,
        policy_cid,
        nondeterminism: CINondeterminism {
            network: CINondeterminismMode::Forbidden,
            clock: CINondeterminismMode::Declared,
            secrets: CINondeterminismMode::Forbidden,
            randomness: CINondeterminismMode::Forbidden,
        },
        additional_input_cids: Vec::new(),
    }
    .build()
    .map_err(|e| e.to_string())?;
    let blast_body = serde_json::to_value(&blast).map_err(|e| format!("serialize body: {e}"))?;
    let blast_check = check_ci_body(&blast_body).map_err(|e| e.to_string())?;
    let blast_cid = blast_check.cid;

    fs::create_dir_all(out_dir).map_err(|e| format!("create {}: {e}", out_dir.display()))?;
    let body_path = out_dir.join("blast-radius.json");
    let summary_path = out_dir.join("summary.json");
    write_json_file(&body_path, &blast_body)?;

    let summary = shadow_summary(
        repo,
        kit,
        &profile,
        &blast,
        &blast_cid,
        &body_path,
        &summary_path,
    );
    write_json_file(&summary_path, &summary)?;
    Ok(summary)
}

fn shadow_summary(
    repo: &Path,
    kit: &str,
    profile: &KitCiProfile,
    blast: &CIBlastRadius,
    blast_cid: &str,
    body_path: &Path,
    summary_path: &Path,
) -> Json {
    json!({
        "kind": "CIShadow",
        "ok": true,
        "kit": kit,
        "wouldSkip": false,
        "skipReason": "shadow-mode-never-skips",
        "blastRadiusCid": blast_cid,
        "blastRadiusPath": display_path(repo, body_path),
        "summaryPath": display_path(repo, summary_path),
        "command": profile.command,
        "blastRadius": blast,
    })
}

#[derive(Debug, Clone)]
struct KitCiProfile {
    job_key: String,
    source_roots: Vec<String>,
    lockfile_paths: Vec<String>,
    command: String,
    toolchain_markers: Vec<String>,
}

impl KitCiProfile {
    fn for_kit(kit: &str, lang: &str, project_rel: String) -> Result<Self, String> {
        let command = match kit {
            "rust" => "make prove-rust",
            "go" => "make prove-go",
            "cpp" => "make prove-cpp",
            "ts" => "make prove-ts",
            "csharp" => "make prove-csharp",
            "swift" => "make prove-swift",
            "java" => "make prove-java",
            "python" => "make prove-python",
            "ruby" => "make prove-ruby",
            "zig" => "make prove-zig",
            "c" => "make prove-c",
            "php" => "php tests/cicp_conformance.php",
            other => return Err(format!("unknown kit `{other}`")),
        }
        .to_string();

        Ok(Self {
            job_key: format!("provekit/ci/{lang}"),
            source_roots: vec![project_rel.clone()],
            lockfile_paths: known_lockfiles(kit, &project_rel),
            command,
            toolchain_markers: toolchain_markers(kit),
        })
    }
}

fn known_lockfiles(kit: &str, project_rel: &str) -> Vec<String> {
    let mut paths = match kit {
        "rust" => vec![
            format!("{project_rel}/Cargo.toml"),
            format!("{project_rel}/Cargo.lock"),
        ],
        "go" => vec![
            format!("{project_rel}/go.mod"),
            format!("{project_rel}/go.sum"),
            format!("{project_rel}/provekit-ir-symbolic/go.mod"),
            format!("{project_rel}/provekit-ir-symbolic/go.sum"),
            format!("{project_rel}/provekit-self-contracts/go.mod"),
            format!("{project_rel}/provekit-self-contracts/go.sum"),
            format!("{project_rel}/provekit-lift-go-tests/go.mod"),
            format!("{project_rel}/provekit-lift-go-tests/go.sum"),
        ],
        "cpp" => vec!["MODULE.bazel".to_string(), "MODULE.bazel.lock".to_string()],
        "ts" => vec![
            "package.json".to_string(),
            "pnpm-lock.yaml".to_string(),
            "tsconfig.json".to_string(),
            "vitest.config.ts".to_string(),
        ],
        "csharp" => vec![format!("{project_rel}/Provekit.sln")],
        "swift" => vec![
            format!("{project_rel}/Package.swift"),
            format!("{project_rel}/Package.resolved"),
        ],
        "java" => vec![format!("{project_rel}/pom.xml")],
        "python" => vec![format!(
            "{project_rel}/provekit-lift-py-tests/pyproject.toml"
        )],
        "ruby" => vec![
            format!("{project_rel}/Gemfile"),
            format!("{project_rel}/Gemfile.lock"),
            format!("{project_rel}/provekit.gemspec"),
        ],
        "zig" => vec![project_rel.to_string()],
        "c" => vec![project_rel.to_string()],
        "php" => vec![format!("{project_rel}/composer.json")],
        _ => Vec::new(),
    };
    paths.sort();
    paths.dedup();
    paths
}

fn toolchain_markers(kit: &str) -> Vec<String> {
    match kit {
        "rust" => vec!["rustc", "cargo"],
        "go" => vec!["go"],
        "cpp" => vec!["clang++", "bazel", "cmake"],
        "ts" => vec!["node", "pnpm", "typescript", "vitest"],
        "csharp" => vec!["dotnet"],
        "swift" => vec!["swift"],
        "java" => vec!["java", "maven"],
        "python" => vec!["python", "pytest"],
        "ruby" => vec!["ruby", "bundler"],
        "zig" => vec!["zig"],
        "c" => vec!["cc", "make"],
        "php" => vec!["php", "composer"],
        _ => vec!["unknown"],
    }
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn relevant_protocol_spec_cids(repo: &Path) -> Result<Vec<String>, String> {
    collect_file_cids_under(repo, &["protocol/specs"])
}

fn fixture_cids(repo: &Path) -> Result<Vec<String>, String> {
    collect_file_cids_under(repo, &["protocol/conformance/cicp"])
}

fn path_closure_cid(repo: &Path, roots: &[String]) -> Result<String, String> {
    let mut entries = Vec::new();
    for root in roots {
        let root_path = repo.join(root);
        if root_path.is_file() {
            entries.push(path_entry(repo, &root_path)?);
        } else if root_path.is_dir() {
            for entry in WalkDir::new(&root_path).follow_links(false) {
                let entry = entry.map_err(|e| e.to_string())?;
                let path = entry.path();
                if should_skip_path(path) {
                    continue;
                }
                if path.is_file() {
                    entries.push(path_entry(repo, path)?);
                }
            }
        }
    }
    entries.sort_by(|a, b| {
        a["path"]
            .as_str()
            .unwrap_or("")
            .cmp(b["path"].as_str().unwrap_or(""))
    });
    value_cid(&json!({
        "kind": "CIPathClosure",
        "schemaVersion": "1",
        "entries": entries,
    }))
}

fn collect_file_cids_under(repo: &Path, roots: &[&str]) -> Result<Vec<String>, String> {
    let mut cids = BTreeSet::new();
    for root in roots {
        let root_path = repo.join(root);
        if !root_path.exists() {
            continue;
        }
        if root_path.is_file() {
            cids.insert(raw_file_cid(&root_path)?);
            continue;
        }
        for entry in WalkDir::new(&root_path).follow_links(false) {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if should_skip_path(path) {
                continue;
            }
            if path.is_file() {
                cids.insert(raw_file_cid(path)?);
            }
        }
    }
    Ok(cids.into_iter().collect())
}

fn file_cids(repo: &Path, paths: &[String]) -> Result<Vec<String>, String> {
    let mut cids = BTreeSet::new();
    for rel in paths {
        let path = repo.join(rel);
        if !path.exists() {
            continue;
        }
        if path.is_file() {
            cids.insert(raw_file_cid(&path)?);
        } else if path.is_dir() {
            cids.insert(path_closure_cid(repo, &[rel.clone()])?);
        }
    }
    Ok(cids.into_iter().collect())
}

fn path_entry(repo: &Path, path: &Path) -> Result<Json, String> {
    Ok(json!({
        "path": display_path(repo, path),
        "cid": raw_file_cid(path)?,
    }))
}

fn raw_file_cid(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    Ok(provekit_canonicalizer::blake3_512_of(&bytes))
}

fn value_cid(value: &Json) -> Result<String, String> {
    json_cid(value).map_err(|e| e.to_string())
}

fn should_skip_path(path: &Path) -> bool {
    let ignored_names = [
        ".git",
        ".zig-cache",
        ".build",
        "bazel-bin",
        "bazel-out",
        "bazel-testlogs",
        "bazel-provekit",
        "bin",
        "dist",
        "node_modules",
        "obj",
        "target",
        "zig-out",
    ];
    if path
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .any(|part| ignored_names.contains(&part))
    {
        return true;
    }
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| matches!(ext, "o" | "a" | "so" | "dylib" | "dll" | "exe" | "class"))
}

fn normalize_rel(path: &Path) -> String {
    path.components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect::<Vec<_>>()
        .join("/")
}

fn display_path(repo: &Path, path: &Path) -> String {
    match path.strip_prefix(repo) {
        Ok(rel) => rel
            .components()
            .filter_map(|c| c.as_os_str().to_str())
            .collect::<Vec<_>>()
            .join("/"),
        Err(_) => path.display().to_string(),
    }
}

fn accepted_result_path(
    accepted_dir: &Path,
    current: &CIBlastRadius,
    blast_cid: &str,
) -> Result<PathBuf, String> {
    let subject = safe_path_component(&current.subject)?;
    Ok(accepted_dir
        .join(subject)
        .join(format!("{blast_cid}.job-result.json")))
}

fn safe_path_component(value: &str) -> Result<&str, String> {
    if value.is_empty()
        || value.contains('/')
        || value.contains('\\')
        || value == "."
        || value == ".."
    {
        Err(format!("invalid accepted-witness path component `{value}`"))
    } else {
        Ok(value)
    }
}

fn absolutize_under(repo: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        repo.join(path)
    }
}

fn write_json_file(path: &Path, value: &Json) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    let mut text =
        serde_json::to_string_pretty(value).map_err(|e| format!("serialize JSON: {e}"))?;
    text.push('\n');
    fs::write(path, text).map_err(|e| format!("write {}: {e}", path.display()))
}

fn read_json_file(path: &Path) -> Result<Json, String> {
    let bytes = fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("parse {}: {e}", path.display()))
}

fn ci_result_arg(result: CiResultArg) -> libprovekit::ci::CIJobResult {
    match result {
        CiResultArg::Pass => libprovekit::ci::CIJobResult::Pass,
        CiResultArg::Fail => libprovekit::ci::CIJobResult::Fail,
        CiResultArg::Flaky => libprovekit::ci::CIJobResult::Flaky,
    }
}

fn ci_result_name(result: CiResultArg) -> &'static str {
    match result {
        CiResultArg::Pass => "pass",
        CiResultArg::Fail => "fail",
        CiResultArg::Flaky => "flaky",
    }
}

fn build_result_body(
    blast: &CIBlastRadius,
    blast_cid: &str,
    result: CiResultArg,
    output_cid: Option<String>,
    log_cid: Option<String>,
    started_at: &str,
    finished_at: &str,
    producer_kind: &str,
    producer_name: &str,
    producer_version: &str,
) -> Result<Json, String> {
    let output_cid = match output_cid {
        Some(cid) => cid,
        None => ci_result_artifact_cid(blast, blast_cid, "output")?,
    };
    let log_cid = match log_cid {
        Some(cid) => cid,
        None => ci_result_artifact_cid(blast, blast_cid, "log")?,
    };
    let result_body = CIJobResultInput {
        job_key: blast.job_key.clone(),
        blast_radius_cid: blast_cid.to_string(),
        result: ci_result_arg(result),
        output_cid,
        log_cid,
        started_at: started_at.to_string(),
        finished_at: finished_at.to_string(),
        runner_identity_cid: blast.runner_identity_cid.clone(),
        policy_cid: blast.policy_cid.clone(),
        producer: CIProducer {
            kind: producer_kind.to_string(),
            name: producer_name.to_string(),
            version: producer_version.to_string(),
        },
        additional_input_cids: Vec::new(),
    }
    .build()
    .map_err(|e| e.to_string())?;
    serde_json::to_value(&result_body).map_err(|e| format!("serialize result: {e}"))
}

fn accept_result_input(
    args: &CiAcceptArgs,
    source_repo: &Path,
    kit: &str,
    blast: &CIBlastRadius,
    blast_cid: &str,
) -> Result<(Json, String, &'static str, Option<PathBuf>), String> {
    if args.assume_pass {
        let result_json = build_result_body(
            blast,
            blast_cid,
            CiResultArg::Pass,
            None,
            None,
            "2026-05-07T00:00:00Z",
            "2026-05-07T00:00:00Z",
            "ci-runner",
            "provekit-ci",
            "checked-in",
        )?;
        let result_cid =
            verify_result_body_claim(blast, blast_cid, &result_json, "assumed pass result")?;
        return Ok((result_json, result_cid, "assume-pass", None));
    }

    let candidate_path = match &args.result {
        Some(path) => absolutize_under(source_repo, path),
        None => {
            let results_dir = args
                .results_dir
                .as_deref()
                .map(|path| absolutize_under(source_repo, path))
                .unwrap_or_else(|| source_repo.join(".provekit/ci-shadow"));
            results_dir.join(kit).join("job-result.json")
        }
    };
    let result_json = read_json_file(&candidate_path).map_err(|e| {
        format!(
            "{e}\nwrite mode imports a passed candidate result by default; pass --result, --results-dir, or --assume-pass"
        )
    })?;
    let result_cid = verify_result_body_claim(
        blast,
        blast_cid,
        &result_json,
        &candidate_path.display().to_string(),
    )?;
    Ok((
        result_json,
        result_cid,
        "candidate-result",
        Some(candidate_path),
    ))
}

fn verify_result_body_claim(
    current: &CIBlastRadius,
    current_cid: &str,
    body: &Json,
    label: &str,
) -> Result<String, String> {
    let check = check_ci_body(body).map_err(|e| e.to_string())?;
    if check.kind != "CIJobResultBodyClaim" {
        return Err(format!(
            "accepted result body must be CIJobResultBodyClaim, got {}",
            check.kind
        ));
    }
    let previous: CIJobResultBodyClaim = serde_json::from_value(body.clone())
        .map_err(|e| format!("parse accepted CIJobResultBodyClaim: {e}"))?;
    admit_identical_reuse(current, &previous)
        .map_err(|e| format!("{label} does not admit reuse for {current_cid}: {e}"))?;
    Ok(check.cid)
}

fn verify_accepted_result(
    current: &CIBlastRadius,
    current_cid: &str,
    previous_result_path: &Path,
) -> Result<String, String> {
    let previous_body = read_json_file(previous_result_path)?;
    verify_result_body_claim(
        current,
        current_cid,
        &previous_body,
        &format!("accepted result witness {}", previous_result_path.display()),
    )
}

fn ci_result_artifact_cid(
    blast: &CIBlastRadius,
    blast_cid: &str,
    artifact_kind: &str,
) -> Result<String, String> {
    value_cid(&json!({
        "kind": "CICheckedInResultArtifactReference",
        "schemaVersion": "1",
        "jobKey": blast.job_key,
        "blastRadiusCid": blast_cid,
        "artifactKind": artifact_kind,
    }))
}

struct AcceptScratch {
    path: PathBuf,
}

impl AcceptScratch {
    fn new() -> Result<Self, String> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| format!("system clock before Unix epoch: {e}"))?
            .as_nanos();
        let path = std::env::temp_dir().join(format!("provekit-ci-accept-{stamp}"));
        fs::create_dir_all(&path).map_err(|e| format!("create {}: {e}", path.display()))?;
        Ok(Self { path })
    }
}

impl Drop for AcceptScratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct CleanWorktree {
    owner_repo: PathBuf,
    path: PathBuf,
    temp_root: PathBuf,
}

impl CleanWorktree {
    fn create(repo: &Path) -> Result<Self, String> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| format!("system clock before Unix epoch: {e}"))?
            .as_nanos();
        let temp_root = std::env::temp_dir().join(format!("provekit-ci-accept-worktree-{stamp}"));
        let path = temp_root.join("repo");
        fs::create_dir_all(&temp_root)
            .map_err(|e| format!("create {}: {e}", temp_root.display()))?;
        if let Err(e) = run_git(
            repo,
            &["worktree", "add", "--detach"],
            Some(&path),
            &["HEAD"],
        ) {
            let _ = fs::remove_dir_all(&temp_root);
            return Err(e);
        }
        Ok(Self {
            owner_repo: repo.to_path_buf(),
            path,
            temp_root,
        })
    }
}

impl Drop for CleanWorktree {
    fn drop(&mut self) {
        let path = self.path.display().to_string();
        let _ = Command::new("git")
            .arg("-C")
            .arg(&self.owner_repo)
            .arg("worktree")
            .arg("remove")
            .arg("--force")
            .arg(&path)
            .output();
        let _ = fs::remove_dir_all(&self.temp_root);
    }
}

fn run_git(
    repo: &Path,
    before_path: &[&str],
    path: Option<&Path>,
    after_path: &[&str],
) -> Result<(), String> {
    let mut command = Command::new("git");
    command.arg("-C").arg(repo);
    for arg in before_path {
        command.arg(arg);
    }
    if let Some(path) = path {
        command.arg(path);
    }
    for arg in after_path {
        command.arg(arg);
    }
    let output = command
        .output()
        .map_err(|e| format!("spawn git in {}: {e}", repo.display()))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "git command failed in {}\nstdout={}\nstderr={}",
            repo.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn stale_accept_error(args: &CiAcceptArgs, missing: &[(String, String, PathBuf)]) -> String {
    let mut message =
        String::from("CICP accepted witnesses are stale.\n\nRun:\n  provekit ci accept");
    if args.all_kits {
        message.push_str(" --all-kits");
    } else if let Some(kit) = &args.kit {
        message.push_str(" --kit ");
        message.push_str(kit);
    }
    if args.clean {
        message.push_str(" --clean");
    }
    if args.result.is_none() && args.results_dir.is_none() {
        message.push_str(" --assume-pass");
    }
    message.push_str(" --out ");
    message.push_str(&args.out.display().to_string());
    message.push_str("\n\nMissing:\n");
    for (kit, cid, path) in missing {
        message.push_str(&format!("  {kit:<8} {cid} -> {}\n", path.display()));
    }
    message
}

fn print_result_human(payload: &Json) {
    println!("{}", "ProvekIt CI result".bold());
    println!("  job      : {}", payload["jobKey"].as_str().unwrap_or(""));
    println!(
        "  radius   : {}",
        payload["blastRadiusCid"].as_str().unwrap_or("")
    );
    println!("  body     : {}", payload["bodyCid"].as_str().unwrap_or(""));
    println!("  status   : {}", payload["result"].as_str().unwrap_or(""));
}

fn print_shadow_human(payload: &Json) {
    match payload["kind"].as_str() {
        Some("CIShadowSet") => {
            println!("{}", "ProvekIt CI shadow set".bold());
            for result in payload["results"].as_array().into_iter().flatten() {
                println!(
                    "  {:<8} {} {}",
                    result["kit"].as_str().unwrap_or(""),
                    result["blastRadiusCid"].as_str().unwrap_or(""),
                    "(shadow: no skip)".dimmed()
                );
            }
        }
        _ => {
            println!("{}", "ProvekIt CI shadow".bold());
            println!("  kit      : {}", payload["kit"].as_str().unwrap_or(""));
            println!(
                "  radius   : {}",
                payload["blastRadiusCid"].as_str().unwrap_or("")
            );
            println!("  status   : {}", "shadow only; build still runs".yellow());
        }
    }
}

fn print_reuse_human(payload: &Json) {
    println!("{}", "ProvekIt CI reuse admission".bold());
    println!("  job      : {}", payload["jobKey"].as_str().unwrap_or(""));
    println!(
        "  radius   : {}",
        payload["currentBlastRadiusCid"].as_str().unwrap_or("")
    );
    println!(
        "  reuse    : {}",
        payload["reuseBodyCid"].as_str().unwrap_or("")
    );
    println!("  status   : {}", "skip admitted".green().bold());
}

fn print_accept_human(payload: &Json) {
    println!("{}", "ProvekIt CI accept".bold());
    println!("  mode     : {}", payload["mode"].as_str().unwrap_or(""));
    println!(
        "  clean    : {}",
        payload["clean"].as_bool().unwrap_or(false)
    );
    println!(
        "  added    : {}",
        payload["addedCount"].as_u64().unwrap_or(0)
    );
    println!(
        "  existing : {}",
        payload["existingCount"].as_u64().unwrap_or(0)
    );
    println!(
        "  verified : {}",
        payload["verifiedCount"].as_u64().unwrap_or(0)
    );
    for result in payload["results"].as_array().into_iter().flatten() {
        println!(
            "  {:<8} {:<8} {}",
            result["kit"].as_str().unwrap_or(""),
            result["status"].as_str().unwrap_or(""),
            result["blastRadiusCid"].as_str().unwrap_or("")
        );
    }
}
