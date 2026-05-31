// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use clap::Parser;
use serde::Serialize;
use serde_json::Value;

#[derive(Parser, Debug, Clone)]
pub struct SelfCheckArgs {
    /// Target crate directory. Defaults to implementations/rust/provekit-cli.
    #[arg(long)]
    pub target: Option<PathBuf>,
    /// Emit stable machine-readable JSON.
    #[arg(long)]
    pub json: bool,
    /// Opt in to the rust-analyzer oracle for target minting.
    #[arg(long)]
    pub oracle: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LiftScoreboard {
    fn_contracts: usize,
    body_discharge_eligible: usize,
    body_discharge_ineligible: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BridgeScoreboard {
    emitted: usize,
    lift_gaps: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct OracleScoreboard {
    requested: bool,
    engaged: bool,
    attempted: u64,
    resolved: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DischargeSplit {
    panic_safe: usize,
    reflexive: usize,
    vacuous: usize,
    undecidable: usize,
    false_pass: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Site {
    file: String,
    line: usize,
    callee: String,
    reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PanicCensusEntry {
    file: String,
    line: usize,
    callee: String,
    status: String,
    reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SelfCheckScoreboard {
    target: String,
    #[serde(rename = "catalogCid")]
    catalog_cid: String,
    lift: LiftScoreboard,
    bridges: BridgeScoreboard,
    oracle: OracleScoreboard,
    silently_dropped: usize,
    dropped_sites: Vec<Site>,
    discharge_split: DischargeSplit,
    panic_census: Vec<PanicCensusEntry>,
}

struct MintOutput {
    proof_file: PathBuf,
    json: Value,
}

pub fn run(args: SelfCheckArgs) -> u8 {
    match run_inner(&args) {
        Ok(scoreboard) => {
            emit_scoreboard(&scoreboard, args.json);
            let mut failed = false;
            if scoreboard.silently_dropped > 0 {
                failed = true;
                eprintln!(
                    "SELF-CHECK INVARIANT VIOLATION: silentlyDropped={} but it must be 0",
                    scoreboard.silently_dropped
                );
                for site in &scoreboard.dropped_sites {
                    eprintln!(
                        "  dropped: {}:{} {}, reason: {}",
                        site.file, site.line, site.callee, site.reason
                    );
                }
            }
            if scoreboard.discharge_split.false_pass > 0 {
                failed = true;
                eprintln!(
                    "SELF-CHECK INVARIANT VIOLATION: falsePass={} but it must be 0",
                    scoreboard.discharge_split.false_pass
                );
                for site in scoreboard
                    .panic_census
                    .iter()
                    .filter(|site| site.reason.contains("false pass"))
                {
                    eprintln!(
                        "  false pass: {}:{} {}, reason: {}",
                        site.file, site.line, site.callee, site.reason
                    );
                }
            }
            if scoreboard.oracle.requested && !scoreboard.oracle.engaged {
                failed = true;
                eprintln!(
                    "self-check --oracle requested but the oracle resolved 0/{} receivers; the census is SYNTACTIC-ONLY (provekit-linkerd unreachable or not warm). Set PROVEKIT_LINKERD_BIN and pre-warm, or run doctor.",
                    scoreboard.oracle.attempted
                );
            }
            if failed {
                crate::EXIT_VERIFY_FAIL
            } else {
                crate::EXIT_OK
            }
        }
        Err(error) => {
            eprintln!("self-check failed: {error}");
            crate::EXIT_VERIFY_FAIL
        }
    }
}

fn run_inner(args: &SelfCheckArgs) -> Result<SelfCheckScoreboard, String> {
    let repo_root = discover_repo_root()?;
    let target_abs = resolve_target(&repo_root, args.target.as_ref())?;
    let target_rel = repo_relative(&repo_root, &target_abs);
    let bin = std::env::current_exe().map_err(|e| format!("resolve current executable: {e}"))?;
    let scratch = std::env::temp_dir()
        .join("provekit-self-check")
        .join(sanitize_path_component(&target_rel));
    recreate_dir(&scratch)?;

    let imports = target_abs.join(".provekit").join("imports");
    recreate_imports(&imports)?;

    let dependency_specs = [
        (
            "libprovekit",
            repo_root.join("implementations/rust/libprovekit"),
        ),
        (
            "shim-std",
            repo_root.join("examples/provekit-shim-rust-std"),
        ),
    ];
    for (name, dep) in dependency_specs {
        if same_path(&dep, &target_abs) {
            continue;
        }
        let out_dir = scratch.join(format!("dep-{name}"));
        let minted = mint_project(&bin, &repo_root, &dep, &out_dir, false)?;
        copy_proof_to_imports(&minted.proof_file, &imports)?;
    }

    let target_out = scratch.join("target");
    let target_mint = mint_project(&bin, &repo_root, &target_abs, &target_out, args.oracle)?;
    let prove_json = prove_project(&bin, &repo_root, &target_abs, &target_out)?;

    build_scoreboard(&target_rel, &target_mint.json, &prove_json)
}

fn discover_repo_root() -> Result<PathBuf, String> {
    let cwd = std::env::current_dir().map_err(|e| format!("read current directory: {e}"))?;
    for candidate in cwd.ancestors() {
        if candidate.join("implementations/rust/Cargo.toml").is_file()
            && candidate
                .join("docs/self-application/GOAL-provekit-proves-provekit.md")
                .is_file()
        {
            return candidate
                .canonicalize()
                .map_err(|e| format!("canonicalize repo root {}: {e}", candidate.display()));
        }
    }
    Err("could not locate provekit repo root from current directory".to_string())
}

fn resolve_target(repo_root: &Path, target: Option<&PathBuf>) -> Result<PathBuf, String> {
    let target = target
        .cloned()
        .unwrap_or_else(|| PathBuf::from("implementations/rust/provekit-cli"));
    let abs = if target.is_absolute() {
        target
    } else {
        repo_root.join(target)
    };
    if !abs.exists() {
        return Err(format!("target does not exist: {}", abs.display()));
    }
    if !abs.join(".provekit/config.toml").is_file() {
        return Err(format!(
            "target is not a provekit kit, missing {}",
            abs.join(".provekit/config.toml").display()
        ));
    }
    abs.canonicalize()
        .map_err(|e| format!("canonicalize target {}: {e}", abs.display()))
}

fn recreate_dir(path: &Path) -> Result<(), String> {
    if path.exists() {
        fs::remove_dir_all(path).map_err(|e| format!("remove {}: {e}", path.display()))?;
    }
    fs::create_dir_all(path).map_err(|e| format!("mkdir {}: {e}", path.display()))
}

fn recreate_imports(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|e| format!("mkdir {}: {e}", path.display()))?;
    for entry in fs::read_dir(path).map_err(|e| format!("read {}: {e}", path.display()))? {
        let entry = entry.map_err(|e| format!("read {} entry: {e}", path.display()))?;
        if entry.path().extension() == Some(OsStr::new("proof")) {
            fs::remove_file(entry.path())
                .map_err(|e| format!("remove {}: {e}", entry.path().display()))?;
        }
    }
    Ok(())
}

fn mint_project(
    bin: &Path,
    repo_root: &Path,
    project: &Path,
    out_dir: &Path,
    oracle: bool,
) -> Result<MintOutput, String> {
    fs::create_dir_all(out_dir).map_err(|e| format!("mkdir {}: {e}", out_dir.display()))?;
    let mut cmd = Command::new(bin);
    cmd.current_dir(repo_root)
        .arg("mint")
        .arg("--project")
        .arg(project)
        .arg("--out")
        .arg(out_dir)
        .arg("--json")
        .arg("--quiet")
        .env("RUST_LOG", "info,provekit_walk_rpc=info");
    if oracle {
        cmd.env("PROVEKIT_RESOLVE_ORACLE", "rust-analyzer");
    } else {
        cmd.env_remove("PROVEKIT_RESOLVE_ORACLE");
    }
    let output = cmd
        .output()
        .map_err(|e| format!("run mint for {}: {e}", project.display()))?;
    if !output.status.success() {
        return Err(command_failure("mint", project, &output));
    }
    let json = parse_json_stdout("mint", project, &output)?;
    let proof_file = json
        .get("proofFile")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| format!("mint for {} did not report proofFile", project.display()))?;
    if !proof_file.is_file() {
        return Err(format!(
            "mint for {} reported missing proofFile {}",
            project.display(),
            proof_file.display()
        ));
    }
    Ok(MintOutput { proof_file, json })
}

fn prove_project(
    bin: &Path,
    repo_root: &Path,
    target: &Path,
    with_dir: &Path,
) -> Result<Value, String> {
    let output = Command::new(bin)
        .current_dir(repo_root)
        .arg("prove")
        .arg(target)
        .arg("--with")
        .arg(with_dir)
        .arg("--json")
        .output()
        .map_err(|e| format!("run prove for {}: {e}", target.display()))?;
    let json = parse_json_stdout("prove", target, &output)?;
    let total = json
        .get("totalCallsites")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if total == 0 {
        return Err(format!(
            "prove for {} reported zero callsites, refusing a vacuous self-check",
            target.display()
        ));
    }
    Ok(json)
}

fn copy_proof_to_imports(proof_file: &Path, imports: &Path) -> Result<(), String> {
    let file_name = proof_file
        .file_name()
        .ok_or_else(|| format!("proof path has no file name: {}", proof_file.display()))?;
    let dest = imports.join(file_name);
    fs::copy(proof_file, &dest).map_err(|e| {
        format!(
            "copy dependency proof {} to {}: {e}",
            proof_file.display(),
            dest.display()
        )
    })?;
    Ok(())
}

fn build_scoreboard(
    target_rel: &str,
    mint_json: &Value,
    prove_json: &Value,
) -> Result<SelfCheckScoreboard, String> {
    let catalog_cid = mint_json
        .get("filenameCid")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or("target mint JSON missing filenameCid")?
        .to_string();
    let lift_json = mint_json
        .get("lift")
        .ok_or("target mint JSON missing lift result")?;
    let (lift, bridges, dropped_sites, unbridged_panic_sites) = lift_scoreboards(lift_json);
    let oracle = oracle_scoreboard(mint_json);
    let discharge_split = discharge_split(prove_json);
    let panic_census = panic_census(prove_json, unbridged_panic_sites);
    let silently_dropped = dropped_sites.len();

    Ok(SelfCheckScoreboard {
        target: target_rel.to_string(),
        catalog_cid,
        lift,
        bridges,
        oracle,
        silently_dropped,
        dropped_sites,
        discharge_split,
        panic_census,
    })
}

fn lift_scoreboards(lift_json: &Value) -> (LiftScoreboard, BridgeScoreboard, Vec<Site>, Vec<Site>) {
    let mut fn_contracts = 0usize;
    let mut body_discharge_eligible = 0usize;
    let mut body_discharge_ineligible = BTreeMap::new();
    let mut bridges_emitted = 0usize;

    if let Some(ir) = lift_json.get("ir").and_then(|v| v.as_array()) {
        for entry in ir {
            match entry.get("kind").and_then(|v| v.as_str()) {
                Some("function-contract") => {
                    fn_contracts += 1;
                    if entry
                        .get("bodyDischargeEligible")
                        .or_else(|| entry.get("body_discharge_eligible"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                    {
                        body_discharge_eligible += 1;
                    } else {
                        let reason = entry
                            .get("bodyDischargeRefusalReason")
                            .or_else(|| entry.get("body_discharge_refusal_reason"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("unspecified")
                            .to_string();
                        *body_discharge_ineligible.entry(reason).or_insert(0) += 1;
                    }
                }
                Some("bridge") => bridges_emitted += 1,
                _ => {}
            }
        }
    }

    let mut lift_gaps = BTreeMap::new();
    let mut dropped_sites = Vec::new();
    let mut unbridged_panic_sites = Vec::new();
    if let Some(diagnostics) = lift_json.get("diagnostics").and_then(|v| v.as_array()) {
        for diagnostic in diagnostics {
            let kind = diagnostic.get("kind").and_then(|v| v.as_str());
            let reason = diagnostic
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("unspecified");
            if kind == Some("lift-gap") {
                *lift_gaps.entry(reason.to_string()).or_insert(0) += 1;
            }
            if kind == Some("lift-gap") && reason == "body-discharge-ineligible" {
                dropped_sites.push(site_from_diagnostic(diagnostic, reason));
            }
            if diagnostic
                .get("panicSite")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
                || reason == "panic-site-unproven"
            {
                unbridged_panic_sites.push(site_from_diagnostic(diagnostic, reason));
            }
        }
    }
    dropped_sites.sort_by(site_cmp);
    unbridged_panic_sites.sort_by(site_cmp);

    (
        LiftScoreboard {
            fn_contracts,
            body_discharge_eligible,
            body_discharge_ineligible,
        },
        BridgeScoreboard {
            emitted: bridges_emitted,
            lift_gaps,
        },
        dropped_sites,
        unbridged_panic_sites,
    )
}

fn discharge_split(prove_json: &Value) -> DischargeSplit {
    let mut split = DischargeSplit {
        panic_safe: 0,
        reflexive: 0,
        vacuous: 0,
        undecidable: 0,
        false_pass: 0,
    };
    let Some(rows) = prove_json.get("rows").and_then(|v| v.as_array()) else {
        if let Some(ds) = prove_json.get("dischargeSplit") {
            split.panic_safe = usize_field(ds, "panicSafe");
            split.reflexive = usize_field(ds, "reflexive");
            split.vacuous = usize_field(ds, "vacuous");
            split.undecidable = usize_field(ds, "undecidable");
            split.false_pass = usize_field(ds, "falsePass");
        }
        return split;
    };

    for row in rows {
        let status = row.get("status").and_then(|v| v.as_str()).unwrap_or("");
        if status != "discharged" {
            split.undecidable += 1;
            continue;
        }
        let method = row.get("dischargeMethod").and_then(|v| v.as_str());
        let panic_site = row
            .get("panicSite")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if panic_site && method != Some("panic-safe") {
            split.false_pass += 1;
            continue;
        }
        match method {
            Some("panic-safe") if panic_site => split.panic_safe += 1,
            Some("reflexive") => split.reflexive += 1,
            Some("vacuous") => split.vacuous += 1,
            _ => {}
        }
    }
    split
}

fn panic_census(prove_json: &Value, unbridged: Vec<Site>) -> Vec<PanicCensusEntry> {
    let mut by_site: BTreeMap<(String, usize, String), PanicCensusEntry> = BTreeMap::new();
    if let Some(rows) = prove_json.get("rows").and_then(|v| v.as_array()) {
        for row in rows {
            if !row
                .get("panicSite")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                continue;
            }
            let file = row
                .get("file")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let line = row.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let callee = row
                .get("callee")
                .and_then(|v| v.as_str())
                .or_else(|| row.get("bridge").and_then(|v| v.as_str()))
                .unwrap_or("unknown")
                .to_string();
            let method = row.get("dischargeMethod").and_then(|v| v.as_str());
            let row_status = row.get("status").and_then(|v| v.as_str()).unwrap_or("");
            let (status, reason) = if row_status == "discharged" && method == Some("panic-safe") {
                ("proven".to_string(), row_reason(row))
            } else if row_status == "discharged" {
                (
                    "unproven".to_string(),
                    format!(
                        "false pass: panic site discharged with method {}",
                        method.unwrap_or("unspecified")
                    ),
                )
            } else {
                ("unproven".to_string(), row_reason(row))
            };
            by_site.insert(
                (file.clone(), line, callee.clone()),
                PanicCensusEntry {
                    file,
                    line,
                    callee,
                    status,
                    reason,
                },
            );
        }
    }

    for site in unbridged {
        by_site
            .entry((site.file.clone(), site.line, site.callee.clone()))
            .or_insert(PanicCensusEntry {
                file: site.file,
                line: site.line,
                callee: site.callee,
                status: "unproven".to_string(),
                reason: site.reason,
            });
    }

    by_site.into_values().collect()
}

fn row_reason(row: &Value) -> String {
    row.get("reason")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("no reason reported")
        .to_string()
}

fn site_from_diagnostic(diagnostic: &Value, fallback_reason: &str) -> Site {
    Site {
        file: diagnostic
            .get("file")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string(),
        line: diagnostic
            .get("line")
            .or_else(|| diagnostic.get("start_line"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize,
        callee: diagnostic
            .get("callee")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string(),
        reason: diagnostic
            .get("detail")
            .and_then(|v| v.as_str())
            .or_else(|| diagnostic.get("reason").and_then(|v| v.as_str()))
            .unwrap_or(fallback_reason)
            .to_string(),
    }
}

fn emit_scoreboard(scoreboard: &SelfCheckScoreboard, json: bool) {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(scoreboard).expect("serialize self-check scoreboard")
        );
        return;
    }

    let ineligible_total: usize = scoreboard
        .lift
        .body_discharge_ineligible
        .values()
        .copied()
        .sum();
    let lift_gap_total: usize = scoreboard.bridges.lift_gaps.values().copied().sum();
    println!("ProvekIt self-check");
    println!("target: {}", scoreboard.target);
    println!("catalogCid: {}", scoreboard.catalog_cid);
    println!(
        "lift: {} fn-contracts, {} body-discharge-eligible, {} ineligible",
        scoreboard.lift.fn_contracts, scoreboard.lift.body_discharge_eligible, ineligible_total
    );
    if !scoreboard.lift.body_discharge_ineligible.is_empty() {
        println!(
            "lift refusals: {}",
            format_breakdown(&scoreboard.lift.body_discharge_ineligible)
        );
    }
    println!(
        "bridges: {} emitted, {} lift-gaps",
        scoreboard.bridges.emitted, lift_gap_total
    );
    if !scoreboard.bridges.lift_gaps.is_empty() {
        println!(
            "lift gaps: {}",
            format_breakdown(&scoreboard.bridges.lift_gaps)
        );
    }
    println!(
        "oracle: requested={}, engaged={}, attempted={}, resolved={}",
        scoreboard.oracle.requested,
        scoreboard.oracle.engaged,
        scoreboard.oracle.attempted,
        scoreboard.oracle.resolved
    );
    println!("silentlyDropped: {}", scoreboard.silently_dropped);
    println!(
        "dischargeSplit: panicSafe={}, reflexive={}, vacuous={}, undecidable={}, falsePass={}",
        scoreboard.discharge_split.panic_safe,
        scoreboard.discharge_split.reflexive,
        scoreboard.discharge_split.vacuous,
        scoreboard.discharge_split.undecidable,
        scoreboard.discharge_split.false_pass
    );
    println!("panicCensus: {} site(s)", scoreboard.panic_census.len());
    for site in &scoreboard.panic_census {
        println!(
            "  {}:{} {} {}, reason: {}",
            site.file, site.line, site.callee, site.status, site.reason
        );
    }
}

fn format_breakdown(map: &BTreeMap<String, usize>) -> String {
    map.iter()
        .map(|(reason, count)| format!("{reason}={count}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn parse_json_stdout(command: &str, project: &Path, output: &Output) -> Result<Value, String> {
    serde_json::from_slice(&output.stdout).map_err(|e| {
        format!(
            "{command} for {} did not emit JSON stdout: {e}\nstdout:\n{}\nstderr:\n{}",
            project.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn command_failure(command: &str, project: &Path, output: &Output) -> String {
    format!(
        "{command} for {} exited with {}\nstdout:\n{}\nstderr:\n{}",
        project.display(),
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn repo_relative(repo_root: &Path, path: &Path) -> String {
    path.strip_prefix(repo_root)
        .unwrap_or(path)
        .display()
        .to_string()
        .replace('\\', "/")
}

fn sanitize_path_component(path: &str) -> String {
    path.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

fn site_cmp(left: &Site, right: &Site) -> std::cmp::Ordering {
    (&left.file, left.line, &left.callee, &left.reason).cmp(&(
        &right.file,
        right.line,
        &right.callee,
        &right.reason,
    ))
}

fn usize_field(value: &Value, key: &str) -> usize {
    value.get(key).and_then(|v| v.as_u64()).unwrap_or(0) as usize
}

fn oracle_scoreboard(mint_json: &Value) -> OracleScoreboard {
    let mint_oracle = mint_json.get("oracle");
    let lift = mint_json.get("lift");
    let requested = mint_oracle
        .and_then(|v| v.get("requested"))
        .and_then(Value::as_bool)
        .or_else(|| {
            lift.and_then(|v| v.get("oracle_requested"))
                .and_then(Value::as_bool)
        })
        .unwrap_or(false);
    let attempted = mint_oracle
        .and_then(|v| v.get("attempted"))
        .and_then(Value::as_u64)
        .or_else(|| {
            lift.and_then(|v| v.get("receivers_attempted"))
                .and_then(Value::as_u64)
        })
        .unwrap_or(0);
    let resolved = mint_oracle
        .and_then(|v| v.get("resolved"))
        .and_then(Value::as_u64)
        .or_else(|| {
            lift.and_then(|v| v.get("receivers_resolved"))
                .and_then(Value::as_u64)
        })
        .unwrap_or(0);
    OracleScoreboard {
        requested,
        engaged: requested && resolved > 0,
        attempted,
        resolved,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn mint_json(requested: bool, attempted: u64, resolved: u64) -> Value {
        json!({
            "filenameCid": "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "oracle": {
                "requested": requested,
                "reachable": resolved > 0,
                "attempted": attempted,
                "resolved": resolved
            },
            "lift": {
                "kind": "ir-document",
                "ir": [],
                "diagnostics": []
            }
        })
    }

    fn prove_json() -> Value {
        json!({
            "rows": []
        })
    }

    #[test]
    fn build_scoreboard_sets_oracle_engaged_from_requested_and_resolved() {
        let cases = [
            (false, 0, false),
            (false, 1, false),
            (true, 0, false),
            (true, 1, true),
        ];

        for (requested, resolved, expected_engaged) in cases {
            let scoreboard =
                build_scoreboard("target", &mint_json(requested, 7, resolved), &prove_json())
                    .expect("scoreboard");
            assert_eq!(scoreboard.oracle.requested, requested);
            assert_eq!(scoreboard.oracle.attempted, 7);
            assert_eq!(scoreboard.oracle.resolved, resolved);
            assert_eq!(
                scoreboard.oracle.engaged, expected_engaged,
                "requested={requested}, resolved={resolved}"
            );
        }
    }
}
