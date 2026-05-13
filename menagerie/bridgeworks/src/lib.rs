// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Parser;
use serde::Deserialize;
use serde_json::{json, Value};

const EXIT_OK: u8 = 0;
const EXIT_VERIFY_FAIL: u8 = 1;
const EXIT_USER_ERROR: u8 = 2;
const PROVEKIT_CLI_ENV: &str = "PROVEKIT_CLI";
const PROVEKIT_BRIDGEWORKS_EXTERNAL_CLI_ENV: &str = "PROVEKIT_BRIDGEWORKS_EXTERNAL_CLI";

#[derive(Parser, Debug, Clone, Default)]
pub struct OutputFlags {
    /// Emit structured JSON instead of human-readable text.
    #[arg(long)]
    pub json: bool,
    /// Suppress non-error output.
    #[arg(long)]
    pub quiet: bool,
}

#[derive(Parser, Debug, Clone)]
#[command(
    name = "provekit-bridgeworks",
    version,
    about = "Run Bridgeworks vertical contract-chain exhibits.",
    long_about = "Bridgeworks demonstrates ProvekIt's cross-domain primitive: native artifacts \
project to ProofIR claims, explicit bridge implications become mementos, and one .proof CID \
compresses the resulting contract DAG."
)]
pub struct BridgeworksArgs {
    /// Exhibit directory or specimen.yaml path. Defaults to menagerie/bridgeworks/checked-add-u8.
    pub specimen: Option<PathBuf>,
    /// Check every Bridgeworks exhibit under menagerie/bridgeworks.
    #[arg(long)]
    pub all: bool,
    #[command(flatten)]
    pub out: OutputFlags,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpecimenManifest {
    id: String,
    name: String,
    surface: String,
    positive: PositiveSpec,
    expected: ExpectedSpec,
    implications: Vec<ImplicationSpec>,
    mutations: Vec<MutationSpec>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PositiveSpec {
    project: PathBuf,
    expected_contracts: usize,
    expected_implications: usize,
    expected_authorities: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExpectedSpec {
    proof_cid_file: PathBuf,
    mint_json: PathBuf,
    proof_inspect_json: PathBuf,
}

#[derive(Debug, Deserialize)]
struct ImplicationSpec {
    name: String,
    antecedent: String,
    consequent: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MutationSpec {
    id: String,
    source: PathBuf,
    target: PathBuf,
    expected_refusal: String,
}

#[derive(Debug)]
enum BridgeworksError {
    Setup(String),
    Verify(String),
}

impl fmt::Display for BridgeworksError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BridgeworksError::Setup(message) | BridgeworksError::Verify(message) => {
                f.write_str(message)
            }
        }
    }
}

pub fn run(args: BridgeworksArgs) -> u8 {
    let targets = match resolve_targets(&args) {
        Ok(targets) => targets,
        Err(error) => {
            eprintln!("bridgeworks: {error}");
            return EXIT_USER_ERROR;
        }
    };

    let mut reports = Vec::new();
    let mut setup_errors = Vec::new();
    let mut verify_errors = Vec::new();

    for target in targets {
        match check_specimen(&target) {
            Ok(report) => reports.push(report),
            Err(BridgeworksError::Setup(error)) => {
                setup_errors.push(format!("{}: {error}", target.display()));
            }
            Err(BridgeworksError::Verify(error)) => {
                verify_errors.push(format!("{}: {error}", target.display()));
            }
        }
    }

    let ok = setup_errors.is_empty() && verify_errors.is_empty();
    if args.out.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "ok": ok,
                "reports": reports,
                "setupErrors": setup_errors,
                "verificationErrors": verify_errors,
            }))
            .expect("Bridgeworks report serializes")
        );
    } else if !args.out.quiet {
        for report in &reports {
            println!(
                "bridgeworks: {} PASS",
                report["id"].as_str().unwrap_or("exhibit")
            );
        }
        for error in setup_errors.iter().chain(verify_errors.iter()) {
            eprintln!("bridgeworks: {error}");
        }
    }

    if ok {
        EXIT_OK
    } else if !setup_errors.is_empty() {
        EXIT_USER_ERROR
    } else {
        EXIT_VERIFY_FAIL
    }
}

fn resolve_targets(args: &BridgeworksArgs) -> Result<Vec<PathBuf>, String> {
    if args.all {
        let root = args
            .specimen
            .clone()
            .unwrap_or_else(|| PathBuf::from("menagerie/bridgeworks"));
        let mut out = Vec::new();
        for entry in
            std::fs::read_dir(&root).map_err(|e| format!("read {}: {e}", root.display()))?
        {
            let entry = entry.map_err(|e| format!("read dir entry: {e}"))?;
            let path = entry.path();
            if path.join("specimen.yaml").exists() {
                out.push(path);
            }
        }
        out.sort();
        return Ok(out);
    }

    let path = args
        .specimen
        .clone()
        .unwrap_or_else(|| PathBuf::from("menagerie/bridgeworks/checked-add-u8"));
    if path.file_name().and_then(|name| name.to_str()) == Some("specimen.yaml") {
        Ok(vec![path.parent().unwrap_or(Path::new(".")).to_path_buf()])
    } else {
        Ok(vec![path])
    }
}

fn check_specimen(specimen_dir: &Path) -> Result<Value, BridgeworksError> {
    let manifest_path = specimen_dir.join("specimen.yaml");
    let manifest_text = std::fs::read_to_string(&manifest_path)
        .map_err(|e| BridgeworksError::Setup(format!("read {}: {e}", manifest_path.display())))?;
    let manifest: SpecimenManifest = serde_yaml::from_str(&manifest_text)
        .map_err(|e| BridgeworksError::Setup(format!("parse specimen.yaml: {e}")))?;
    let repo_root = find_repo_root(specimen_dir).map_err(BridgeworksError::Setup)?;
    let project_dir = specimen_dir.join(&manifest.positive.project);

    let positive = mint_and_inspect(&repo_root, &project_dir, &manifest.surface, &manifest.id)
        .map_err(BridgeworksError::Verify)?;
    verify_expected_fixtures(specimen_dir, &manifest, &positive)
        .map_err(BridgeworksError::Verify)?;
    let counts = count_member_kinds(&positive.dump).map_err(BridgeworksError::Verify)?;
    let witness_proof_cids =
        collect_external_witness_proof_roots(&repo_root, &positive.dump, &positive.mint)
            .map_err(BridgeworksError::Verify)?;
    let implication_reports = collect_actual_implications(&positive.dump, &manifest.implications)
        .map_err(BridgeworksError::Verify)?;
    if counts.contracts != manifest.positive.expected_contracts {
        return Err(BridgeworksError::Verify(format!(
            "expected {} contract mementos, observed {}",
            manifest.positive.expected_contracts, counts.contracts
        )));
    }
    if counts.implications != manifest.positive.expected_implications {
        return Err(BridgeworksError::Verify(format!(
            "expected {} implication mementos, observed {}",
            manifest.positive.expected_implications, counts.implications
        )));
    }
    if counts.authorities != manifest.positive.expected_authorities {
        return Err(BridgeworksError::Verify(format!(
            "expected {} authority mementos, observed {}",
            manifest.positive.expected_authorities, counts.authorities
        )));
    }
    if positive.inspect["member_count"].as_u64()
        != Some(
            (manifest.positive.expected_contracts
                + manifest.positive.expected_implications
                + manifest.positive.expected_authorities) as u64,
        )
    {
        return Err(BridgeworksError::Verify(
            "proof inspect member_count did not match contract + implication + authority mementos"
                .into(),
        ));
    }

    let mut mutation_reports = Vec::new();
    for mutation in &manifest.mutations {
        let mutation_parent = repo_root.join("target/provekit-bridgeworks-mutations");
        let temp_project = temp_bridgeworks_dir_in(&mutation_parent, "mutation", &mutation.id)
            .map_err(BridgeworksError::Setup)?;
        copy_dir_recursive(specimen_dir, &temp_project).map_err(BridgeworksError::Setup)?;
        let target = temp_project.join(&mutation.target);
        let source = specimen_dir.join(&mutation.source);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| BridgeworksError::Setup(format!("mkdir {}: {e}", parent.display())))?;
        }
        std::fs::copy(&source, &target).map_err(|e| {
            BridgeworksError::Setup(format!(
                "copy mutation {} to {}: {e}",
                source.display(),
                target.display()
            ))
        })?;

        let result = run_mint(&repo_root, &temp_project, &manifest.surface, &mutation.id);
        let refused = result.is_err();
        let detail = match result {
            Ok(value) => value,
            Err(error) => json!({"error": error}),
        };
        let _ = std::fs::remove_dir_all(&temp_project);
        if !refused {
            return Err(BridgeworksError::Verify(format!(
                "mutation `{}` was accepted; expected refusal `{}`",
                mutation.id, mutation.expected_refusal
            )));
        }
        let refusal = detail.get("error").and_then(Value::as_str).ok_or_else(|| {
            BridgeworksError::Verify(format!(
                "mutation `{}` refused without an error string",
                mutation.id
            ))
        })?;
        if !refusal.contains(&mutation.expected_refusal) {
            return Err(BridgeworksError::Verify(format!(
                "mutation `{}` refused for the wrong contract; expected `{}`, observed:\n{}",
                mutation.id, mutation.expected_refusal, refusal
            )));
        }
        mutation_reports.push(json!({
            "id": mutation.id,
            "refused": true,
            "expectedRefusal": mutation.expected_refusal,
            "expectedRefusalMatched": true,
            "detail": detail,
        }));
    }

    Ok(json!({
        "id": manifest.id,
        "name": manifest.name,
        "surface": manifest.surface,
        "proofCid": positive.mint["filenameCid"],
        "contractSetCid": positive.mint["contractSetCid"],
        "proofFile": positive.mint["proofFile"],
        "memberCounts": {
            "contract": counts.contracts,
            "implication": counts.implications,
            "authority": counts.authorities,
        },
        "witnessProofCids": witness_proof_cids,
        "implications": implication_reports,
        "mutations": mutation_reports,
        "expectedFixtures": {
            "proofCidFile": manifest.expected.proof_cid_file,
            "mintJson": manifest.expected.mint_json,
            "proofInspectJson": manifest.expected.proof_inspect_json,
        },
        "workflow": {
            "runner": "provekit-bridgeworks",
            "provekitCli": provekit_cli_report(&repo_root),
        },
    }))
}

struct MintInspection {
    mint: Value,
    inspect: Value,
    dump: Value,
}

#[derive(Default)]
struct MemberCounts {
    contracts: usize,
    implications: usize,
    authorities: usize,
}

fn mint_and_inspect(
    repo_root: &Path,
    project_dir: &Path,
    surface: &str,
    id: &str,
) -> Result<MintInspection, String> {
    let mint = run_mint(repo_root, project_dir, surface, id)?;
    let proof_file = mint
        .get("proofFile")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("provekit mint JSON for `{id}` missing proofFile"))?;
    let inspect = run_provekit_json(
        repo_root,
        ["proof", "inspect", proof_file, "--json", "--quiet"],
    )?;
    if inspect
        .get("errors")
        .and_then(Value::as_array)
        .map(|errors| !errors.is_empty())
        .unwrap_or(true)
    {
        return Err(format!("proof inspect found errors for `{id}`: {inspect}"));
    }
    let dump = run_provekit_json(repo_root, ["dump", proof_file, "--json", "--quiet"])?;
    Ok(MintInspection {
        mint,
        inspect,
        dump,
    })
}

fn run_mint(
    repo_root: &Path,
    project_dir: &Path,
    surface: &str,
    id: &str,
) -> Result<Value, String> {
    let out_dir = temp_bridgeworks_dir("mint", id)?;
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("mkdir {}: {e}", out_dir.display()))?;
    let mut cmd = provekit_cli_command(repo_root)?;
    cmd.arg("mint")
        .arg("--project")
        .arg(project_dir)
        .arg("--surface")
        .arg(surface)
        .arg("--out")
        .arg(&out_dir)
        .arg("--no-attest")
        .arg("--json")
        .arg("--quiet")
        .current_dir(repo_root);
    if trace_enabled() {
        cmd.env("PROVEKIT_CLI_TRACE", "1");
        cmd.stderr(Stdio::inherit());
    }
    let output = cmd
        .output()
        .map_err(|e| format!("spawn provekit mint for `{id}`: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        let _ = std::fs::remove_dir_all(&out_dir);
        return Err(format!(
            "provekit mint failed for `{id}`\nstdout:\n{stdout}\nstderr:\n{stderr}"
        ));
    }
    let parsed: Value = serde_json::from_str(&stdout)
        .map_err(|e| format!("parse provekit mint JSON for `{id}`: {e}\nstdout:\n{stdout}"))?;
    Ok(parsed)
}

fn run_provekit_json<const N: usize>(repo_root: &Path, args: [&str; N]) -> Result<Value, String> {
    let mut cmd = provekit_cli_command(repo_root)?;
    cmd.args(args).current_dir(repo_root);
    let output = cmd
        .output()
        .map_err(|e| format!("spawn provekit {}: {e}", args.join(" ")))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        return Err(format!(
            "provekit {} failed\nstdout:\n{stdout}\nstderr:\n{stderr}",
            args.join(" ")
        ));
    }
    serde_json::from_str(&stdout).map_err(|e| {
        format!(
            "parse provekit {} JSON: {e}\nstdout:\n{stdout}",
            args.join(" ")
        )
    })
}

fn count_member_kinds(dump: &Value) -> Result<MemberCounts, String> {
    let members = dump
        .get("members")
        .and_then(Value::as_object)
        .ok_or_else(|| "provekit dump JSON missing members object".to_string())?;
    let mut counts = MemberCounts::default();
    for member in members.values() {
        match member.pointer("/header/kind").and_then(Value::as_str) {
            Some("contract") => counts.contracts += 1,
            Some("implication") => counts.implications += 1,
            Some("authority") => counts.authorities += 1,
            Some(other) => return Err(format!("unexpected proof member kind `{other}`")),
            None => return Err("proof member missing /header/kind".into()),
        }
        if member.pointer("/header/kind").and_then(Value::as_str) == Some("implication") {
            let input_count = member
                .pointer("/header/inputCids")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0);
            if input_count < 2 {
                return Err(format!(
                    "implication member has {input_count} inputCids, expected at least 2"
                ));
            }
        }
    }
    Ok(counts)
}

fn collect_external_witness_proof_roots(
    repo_root: &Path,
    root_dump: &Value,
    mint: &Value,
) -> Result<Vec<String>, String> {
    let proof_file = mint
        .get("proofFile")
        .and_then(Value::as_str)
        .ok_or_else(|| "provekit mint JSON missing proofFile".to_string())?;
    let proof_dir = Path::new(proof_file)
        .parent()
        .ok_or_else(|| format!("proofFile `{proof_file}` has no parent directory"))?;
    let members = root_dump
        .get("members")
        .and_then(Value::as_object)
        .ok_or_else(|| "provekit dump JSON missing members object".to_string())?;

    let mut witness_roots = BTreeSet::new();
    for member in members.values() {
        let Some(input_cids) = member
            .pointer("/header/inputCids")
            .and_then(Value::as_array)
        else {
            continue;
        };
        for input_cid in input_cids.iter().filter_map(Value::as_str) {
            let witness_path = proof_dir.join(format!("{input_cid}.proof"));
            if !witness_path.exists() {
                continue;
            }
            let witness_path_arg = witness_path.to_string_lossy().into_owned();
            let inspect = run_provekit_json(
                repo_root,
                [
                    "proof",
                    "inspect",
                    witness_path_arg.as_str(),
                    "--json",
                    "--quiet",
                ],
            )?;
            if inspect
                .get("errors")
                .and_then(Value::as_array)
                .map(|errors| !errors.is_empty())
                .unwrap_or(true)
            {
                return Err(format!(
                    "witness proof `{}` did not inspect cleanly: {inspect}",
                    witness_path.display()
                ));
            }
            let dump = run_provekit_json(
                repo_root,
                ["dump", witness_path_arg.as_str(), "--json", "--quiet"],
            )?;
            if !proof_contains_member_kind(&dump, "witness")? {
                return Err(format!(
                    "external witness proof `{}` has no witness memento",
                    witness_path.display()
                ));
            }
            witness_roots.insert(input_cid.to_string());
        }
    }

    if witness_roots.is_empty() {
        return Err("main proof does not reference any external witness proof root".into());
    }
    Ok(witness_roots.into_iter().collect())
}

fn proof_contains_member_kind(dump: &Value, kind: &str) -> Result<bool, String> {
    let members = dump
        .get("members")
        .and_then(Value::as_object)
        .ok_or_else(|| "provekit dump JSON missing members object".to_string())?;
    Ok(members
        .values()
        .any(|member| member.pointer("/header/kind").and_then(Value::as_str) == Some(kind)))
}

fn collect_actual_implications(
    dump: &Value,
    expected: &[ImplicationSpec],
) -> Result<Vec<Value>, String> {
    let members = dump
        .get("members")
        .and_then(Value::as_object)
        .ok_or_else(|| "provekit dump JSON missing members object".to_string())?;
    let mut contract_cids_by_name = BTreeMap::new();
    let mut implications_by_name = BTreeMap::new();

    for (member_cid, member) in members {
        match member.pointer("/header/kind").and_then(Value::as_str) {
            Some("contract") => {
                if let Some(name) = member.pointer("/header/name").and_then(Value::as_str) {
                    contract_cids_by_name.insert(name.to_string(), member_cid.clone());
                }
            }
            Some("implication") => {
                let name = member
                    .pointer("/metadata/producedBy")
                    .and_then(Value::as_str)
                    .and_then(|produced_by| produced_by.strip_prefix("bridgeworks.edge."))
                    .or_else(|| {
                        member
                            .pointer("/metadata/proofWitness")
                            .and_then(Value::as_str)
                    })
                    .unwrap_or(member_cid);
                let antecedent_cid = member
                    .pointer("/header/antecedentCid")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        format!("implication member `{member_cid}` missing antecedentCid")
                    })?;
                let consequent_cid = member
                    .pointer("/header/consequentCid")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        format!("implication member `{member_cid}` missing consequentCid")
                    })?;
                let input_cids = member
                    .pointer("/header/inputCids")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                implications_by_name.insert(
                    name.to_string(),
                    json!({
                        "implicationCid": member_cid,
                        "antecedentCid": antecedent_cid,
                        "consequentCid": consequent_cid,
                        "inputCids": input_cids,
                    }),
                );
            }
            _ => {}
        }
    }

    expected
        .iter()
        .map(|edge| {
            let antecedent_cid = contract_cids_by_name.get(&edge.antecedent).ok_or_else(|| {
                format!(
                    "manifest implication `{}` antecedent `{}` was not minted as a contract",
                    edge.name, edge.antecedent
                )
            })?;
            let consequent_cid = contract_cids_by_name.get(&edge.consequent).ok_or_else(|| {
                format!(
                    "manifest implication `{}` consequent `{}` was not minted as a contract",
                    edge.name, edge.consequent
                )
            })?;
            let actual = implications_by_name.get(&edge.name).ok_or_else(|| {
                format!(
                    "manifest implication `{}` was not minted as an implication memento",
                    edge.name
                )
            })?;
            if actual.get("antecedentCid").and_then(Value::as_str) != Some(antecedent_cid.as_str())
            {
                return Err(format!(
                    "implication `{}` antecedent CID does not match minted contract `{}`",
                    edge.name, edge.antecedent
                ));
            }
            if actual.get("consequentCid").and_then(Value::as_str) != Some(consequent_cid.as_str())
            {
                return Err(format!(
                    "implication `{}` consequent CID does not match minted contract `{}`",
                    edge.name, edge.consequent
                ));
            }
            Ok(json!({
                "name": edge.name,
                "antecedent": edge.antecedent,
                "consequent": edge.consequent,
                "implicationCid": actual["implicationCid"],
                "antecedentCid": actual["antecedentCid"],
                "consequentCid": actual["consequentCid"],
                "inputCids": actual["inputCids"],
            }))
        })
        .collect()
}

fn verify_expected_fixtures(
    specimen_dir: &Path,
    manifest: &SpecimenManifest,
    positive: &MintInspection,
) -> Result<(), String> {
    let proof_cid = positive
        .mint
        .get("filenameCid")
        .and_then(Value::as_str)
        .ok_or_else(|| "provekit mint JSON missing filenameCid".to_string())?;
    let proof_cid_path = specimen_dir.join(&manifest.expected.proof_cid_file);
    let expected_proof_cid = std::fs::read_to_string(&proof_cid_path)
        .map_err(|e| format!("read expected proof CID {}: {e}", proof_cid_path.display()))?;
    if expected_proof_cid.trim() != proof_cid {
        return Err(format!(
            "expected proof CID fixture {} = `{}`, observed `{}`",
            proof_cid_path.display(),
            expected_proof_cid.trim(),
            proof_cid
        ));
    }

    let mint_fixture = read_json_fixture(specimen_dir.join(&manifest.expected.mint_json))?;
    for pointer in [
        "/ok",
        "/surface",
        "/filenameCid",
        "/contractSetCid",
        "/bytesWritten",
        "/lift",
    ] {
        assert_json_pointer_eq(
            "positive mint fixture",
            &mint_fixture,
            &positive.mint,
            pointer,
        )?;
    }

    let inspect_fixture =
        read_json_fixture(specimen_dir.join(&manifest.expected.proof_inspect_json))?;
    for pointer in [
        "/kind",
        "/schema_version",
        "/file_cid",
        "/filename_cid",
        "/member_count",
        "/metadata_count",
        "/warnings",
        "/errors",
    ] {
        assert_json_pointer_eq(
            "positive proof inspect fixture",
            &inspect_fixture,
            &positive.inspect,
            pointer,
        )?;
    }

    Ok(())
}

fn read_json_fixture(path: PathBuf) -> Result<Value, String> {
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("read fixture {}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("parse fixture {}: {e}", path.display()))
}

fn assert_json_pointer_eq(
    label: &str,
    expected: &Value,
    actual: &Value,
    pointer: &str,
) -> Result<(), String> {
    let expected_value = expected
        .pointer(pointer)
        .ok_or_else(|| format!("{label} missing expected field `{pointer}`"))?;
    let actual_value = actual
        .pointer(pointer)
        .ok_or_else(|| format!("{label} missing actual field `{pointer}`"))?;
    if expected_value != actual_value {
        return Err(format!(
            "{label} mismatch at `{pointer}`: expected {}, observed {}",
            expected_value, actual_value
        ));
    }
    Ok(())
}

fn find_repo_root(start: &Path) -> Result<PathBuf, String> {
    let mut cursor = if start.is_absolute() {
        start.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| format!("current dir: {e}"))?
            .join(start)
    };
    cursor = cursor.canonicalize().unwrap_or(cursor);
    loop {
        if cursor
            .join("implementations/rust/provekit-cli/Cargo.toml")
            .exists()
        {
            return Ok(cursor);
        }
        if !cursor.pop() {
            return Err(format!(
                "could not find repo root above {}",
                start.display()
            ));
        }
    }
}

fn temp_bridgeworks_dir(kind: &str, id: &str) -> Result<PathBuf, String> {
    temp_bridgeworks_dir_in(&std::env::temp_dir(), kind, id)
}

fn temp_bridgeworks_dir_in(base: &Path, kind: &str, id: &str) -> Result<PathBuf, String> {
    let safe_id: String = id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("system clock before UNIX_EPOCH: {e}"))?
        .as_nanos();
    Ok(base.join(format!(
        "provekit-bridgeworks-{kind}-{}-{now}-{safe_id}",
        std::process::id()
    )))
}

fn copy_dir_recursive(source: &Path, target: &Path) -> Result<(), String> {
    std::fs::create_dir_all(target).map_err(|e| format!("mkdir {}: {e}", target.display()))?;
    for entry in std::fs::read_dir(source).map_err(|e| format!("read {}: {e}", source.display()))? {
        let entry = entry.map_err(|e| format!("read dir entry: {e}"))?;
        let path = entry.path();
        let dest = target.join(entry.file_name());
        let file_type = entry
            .file_type()
            .map_err(|e| format!("file type {}: {e}", path.display()))?;
        if file_type.is_dir() {
            if entry.file_name() == "out" || entry.file_name() == "target" {
                continue;
            }
            copy_dir_recursive(&path, &dest)?;
        } else if file_type.is_file() {
            std::fs::copy(&path, &dest)
                .map_err(|e| format!("copy {} to {}: {e}", path.display(), dest.display()))?;
        }
    }
    Ok(())
}

fn provekit_cli_command(repo_root: &Path) -> Result<Command, String> {
    let invocation = provekit_cli_invocation(repo_root);
    let mut args = invocation.command.iter();
    let program = args
        .next()
        .ok_or_else(|| "provekit CLI command is empty".to_string())?;
    let mut cmd = Command::new(program);
    cmd.args(args);
    Ok(cmd)
}

#[derive(Debug, Clone)]
struct ProvekitCliInvocation {
    kind: &'static str,
    command: Vec<String>,
    ignored_external_cli: Option<String>,
}

fn provekit_cli_invocation(repo_root: &Path) -> ProvekitCliInvocation {
    let external_cli = std::env::var(PROVEKIT_CLI_ENV)
        .ok()
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty());
    if let Some(path) = external_cli.clone() {
        if external_cli_enabled() {
            return ProvekitCliInvocation {
                kind: "external-binary",
                command: vec![path],
                ignored_external_cli: None,
            };
        }
    }

    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let manifest = repo_root.join("implementations/rust/provekit-cli/Cargo.toml");
    ProvekitCliInvocation {
        kind: "cargo-run-source",
        command: vec![
            cargo,
            "run".into(),
            "--quiet".into(),
            "--manifest-path".into(),
            manifest.display().to_string(),
            "--".into(),
        ],
        ignored_external_cli: external_cli,
    }
}

fn external_cli_enabled() -> bool {
    std::env::var(PROVEKIT_BRIDGEWORKS_EXTERNAL_CLI_ENV)
        .map(|value| {
            matches!(
                value.as_str(),
                "1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON"
            )
        })
        .unwrap_or(false)
}

fn provekit_cli_report(repo_root: &Path) -> Value {
    let invocation = provekit_cli_invocation(repo_root);
    let mut report = json!({
        "kind": invocation.kind,
        "command": invocation.command,
    });
    if let Some(path) = invocation.ignored_external_cli {
        report["ignoredExternalCli"] = json!({
            "env": PROVEKIT_CLI_ENV,
            "value": path,
            "reason": format!(
                "set {PROVEKIT_BRIDGEWORKS_EXTERNAL_CLI_ENV}=1 to run Bridgeworks against an explicit external provekit binary"
            ),
        });
    }
    report
}

fn trace_enabled() -> bool {
    std::env::var_os("PROVEKIT_BRIDGEWORKS_TRACE").is_some()
}
