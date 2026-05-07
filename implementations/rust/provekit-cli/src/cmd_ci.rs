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

use clap::{Parser, Subcommand};
use libprovekit::canonical::json_cid;
use libprovekit::ci::{
    admit_identical_reuse, check_ci_body, CIBlastRadius, CIBlastRadiusInput, CIJobResultBodyClaim,
    CINondeterminism, CINondeterminismMode,
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
    /// Compute CICP blast-radius body claims without skipping any CI work yet.
    Shadow(CiShadowArgs),
    /// Admit an identical-input-closure reuse witness for an accepted prior result.
    Reuse(CiReuseArgs),
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
    #[arg(long)]
    pub previous_result: PathBuf,
    /// File path for the emitted CIReuseBodyClaim skip witness.
    #[arg(long)]
    pub reuse_out: PathBuf,
    #[command(flatten)]
    pub out: OutputFlags,
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

    let previous_body = read_json_file(&args.previous_result)?;
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
        "kind": "CICPShadowPolicy",
        "schemaVersion": "1",
        "mode": "shadow-only",
        "skipBuild": false,
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
    let blast_cid = blast.cid().map_err(|e| e.to_string())?;
    let blast_body = serde_json::to_value(&blast).map_err(|e| format!("serialize body: {e}"))?;
    check_ci_body(&blast_body).map_err(|e| e.to_string())?;

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
