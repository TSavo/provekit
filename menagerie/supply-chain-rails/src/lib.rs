// SPDX-License-Identifier: Apache-2.0

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Parser;
use serde_json::{json, Value};

const EXIT_OK: u8 = 0;
const EXIT_VERIFY_FAIL: u8 = 1;
const EXIT_USER_ERROR: u8 = 2;
const PROVEKIT_CLI_ENV: &str = "PROVEKIT_CLI";
const PROVEKIT_SUPPLY_CHAIN_EXTERNAL_CLI_ENV: &str = "PROVEKIT_SUPPLY_CHAIN_EXTERNAL_CLI";

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
    name = "provekit-supply-chain-rails",
    version,
    about = "Run Supply Chain Rails package-admission exhibits.",
    long_about = "Supply Chain Rails demonstrates that conventional package receipts can stay green while ProvekIt rejects a package-shaped release on contract, witness, binary, or CI rails."
)]
pub struct SupplyChainRailsArgs {
    /// Exhibit directory or specimen.yaml path. Defaults to menagerie/supply-chain-rails/authenticated-betrayal.
    pub specimen: Option<PathBuf>,
    /// Check every Supply Chain Rails exhibit under menagerie/supply-chain-rails.
    #[arg(long)]
    pub all: bool,
    #[command(flatten)]
    pub out: OutputFlags,
}

#[derive(Debug)]
enum SupplyChainRailsError {
    Setup(String),
    Verify(String),
}

impl fmt::Display for SupplyChainRailsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SupplyChainRailsError::Setup(message) | SupplyChainRailsError::Verify(message) => {
                f.write_str(message)
            }
        }
    }
}

pub fn run(args: SupplyChainRailsArgs) -> u8 {
    let targets = match resolve_targets(&args) {
        Ok(targets) => targets,
        Err(error) => {
            eprintln!("supply-chain-rails: {error}");
            return EXIT_USER_ERROR;
        }
    };

    let mut reports = Vec::new();
    let mut setup_errors = Vec::new();
    let mut verify_errors = Vec::new();
    for target in targets {
        match check_specimen(&target) {
            Ok(report) => reports.push(report),
            Err(SupplyChainRailsError::Setup(error)) => {
                setup_errors.push(format!("{}: {error}", target.display()));
            }
            Err(SupplyChainRailsError::Verify(error)) => {
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
            .expect("Supply Chain Rails report serializes")
        );
    } else {
        if !args.out.quiet {
            for report in &reports {
                println!(
                    "supply-chain-rails: {} PASS",
                    report["id"].as_str().unwrap_or("exhibit")
                );
            }
        }
        for error in setup_errors.iter().chain(verify_errors.iter()) {
            eprintln!("supply-chain-rails: {error}");
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

fn resolve_targets(args: &SupplyChainRailsArgs) -> Result<Vec<PathBuf>, String> {
    if args.all {
        let root = args
            .specimen
            .clone()
            .unwrap_or_else(|| PathBuf::from("menagerie/supply-chain-rails"));
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
        if out.is_empty() {
            return Err(format!(
                "no Supply Chain Rails specimens found under {}",
                root.display()
            ));
        }
        return Ok(out);
    }

    let path = args
        .specimen
        .clone()
        .unwrap_or_else(|| PathBuf::from("menagerie/supply-chain-rails/authenticated-betrayal"));
    if path.file_name().and_then(|name| name.to_str()) == Some("specimen.yaml") {
        Ok(vec![path.parent().unwrap_or(Path::new(".")).to_path_buf()])
    } else {
        Ok(vec![path])
    }
}

fn check_specimen(specimen_dir: &Path) -> Result<Value, SupplyChainRailsError> {
    let manifest_path = specimen_dir.join("specimen.yaml");
    if !manifest_path.exists() {
        return Err(SupplyChainRailsError::Setup(format!(
            "missing {}",
            manifest_path.display()
        )));
    }
    let repo_root = find_repo_root(specimen_dir).map_err(SupplyChainRailsError::Setup)?;
    let baseline = specimen_dir.join("packages/safe-json-1.4.1");
    let lie = specimen_dir.join("packages/safe-json-1.4.2-lie");
    let substituted = specimen_dir.join("packages/safe-json-1.4.2-substituted/package.tgz");
    let expected = specimen_dir.join("expected");

    let baseline_inspect = provekit_json_ok(
        &repo_root,
        vec![
            "package".into(),
            "inspect".into(),
            baseline.display().to_string(),
        ],
    )?;
    assert_str(
        &baseline_inspect,
        "/package/name",
        "safe-json",
        "baseline package name",
    )?;
    let baseline_out = temp_dir("supply-chain-rails-baseline")?;
    let baseline_mint = provekit_json_ok(
        &repo_root,
        vec![
            "mint".into(),
            "--project".into(),
            baseline.display().to_string(),
            "--out".into(),
            baseline_out.display().to_string(),
            "--no-attest".into(),
        ],
    )?;
    assert_bool(&baseline_mint, "/ok", true, "baseline mint")?;

    let conventional = provekit_json_ok(
        &repo_root,
        vec![
            "package".into(),
            "inspect".into(),
            lie.display().to_string(),
        ],
    )?;
    assert_str(
        &conventional,
        "/conventionalReceipts/slsaVerificationSummary/verdict",
        "green",
        "SLSA verifier VSA receipt",
    )?;
    assert_str(
        &conventional,
        "/conventionalReceipts/inTotoPipeline/verdict",
        "green",
        "in-toto pipeline receipt",
    )?;
    let baseline_closure = baseline_inspect
        .pointer("/ci/inputClosureCid")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            SupplyChainRailsError::Verify(
                "baseline package inspect missing /ci/inputClosureCid".into(),
            )
        })?;
    let candidate_closure = conventional
        .pointer("/ci/inputClosureCid")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            SupplyChainRailsError::Verify(
                "candidate package inspect missing /ci/inputClosureCid".into(),
            )
        })?;
    if baseline_closure == candidate_closure {
        return Err(SupplyChainRailsError::Verify(
            "stale CI rail expected candidate inputClosureCid to differ from baseline".into(),
        ));
    }

    let lift_lie = provekit_json_ok(&repo_root, vec!["lift".into(), lie.display().to_string()])?;
    assert_str(
        &lift_lie,
        "/release/contractSetCid",
        "blake3-512:274b2fc435f45fd5390bb05547f787a601afee025f403d398cfb4a4e2e8855257c9ac5e2148346c9dabb0b88391da7c149b8beeb54cf4c106a99f6724b61b4d7",
        "preserved contract set",
    )?;
    let witness_plan = witness_plan_for(&lift_lie, "runtime.no-env-secret-read")
        .map_err(SupplyChainRailsError::Verify)?;
    let plan_path = temp_dir("supply-chain-rails-plan")?.join("runtime-no-env-plan.json");
    write_json(&plan_path, &witness_plan).map_err(SupplyChainRailsError::Setup)?;
    let lower_lie = provekit_json_fail(
        &repo_root,
        vec![
            "lower".into(),
            "--project".into(),
            lie.display().to_string(),
            "--surface".into(),
            "javascript".into(),
            "--mode".into(),
            "witness".into(),
            "--plan".into(),
            plan_path.display().to_string(),
        ],
    )?;
    assert_str(
        &lower_lie,
        "/lowerResult/output/status",
        "rejected",
        "preserved-contract witness",
    )?;
    assert_str(
        &lower_lie,
        "/lowerResult/output/reasonCode",
        "env-secret-read",
        "witness refusal reason",
    )?;

    let version_red = provekit_json_fail(
        &repo_root,
        vec![
            "version".into(),
            "check-extension".into(),
            "--previous".into(),
            expected.join("release-1.4.1.json").display().to_string(),
            "--candidate".into(),
            expected
                .join("release-1.4.2-weakened.json")
                .display()
                .to_string(),
        ],
    )?;
    assert_contains_string(
        &version_red,
        "/missingContracts",
        "runtime.no-env-secret-read",
        "weakened release missing preserved contract",
    )?;

    let binary_red = provekit_json_fail(
        &repo_root,
        vec![
            "verify".into(),
            "--artifact".into(),
            substituted.display().to_string(),
            "--proof".into(),
            expected.join("release-1.4.2.json").display().to_string(),
        ],
    )?;
    assert_str(
        &binary_red,
        "/reason",
        "binaryCid mismatch",
        "substituted bytes rail",
    )?;

    let policy_green = provekit_json_ok(
        &repo_root,
        vec![
            "verify".into(),
            "--proof".into(),
            expected.join("release-1.4.2.json").display().to_string(),
            "--policy".into(),
            expected.join("policy.json").display().to_string(),
        ],
    )?;

    Ok(json!({
        "id": "SCR-SHAPE-001",
        "name": "Authenticated Betrayal",
        "claim": "conventional SLSA and in-toto receipts stay green while ProvekIt rejects admission for an undischargeable preserved contract",
        "baseline": {
            "package": baseline_inspect["package"],
            "minted": baseline_mint["ok"],
            "filenameCid": baseline_mint["filenameCid"],
            "contractSetCid": baseline_mint["contractSetCid"],
            "proofFile": baseline_mint["proofFile"],
        },
        "ordinarySupplyChainReceipts": {
            "package": conventional["package"],
            "conventionalReceipts": conventional["conventionalReceipts"],
            "admission": conventional["admission"],
        },
        "redRails": {
            "witness": {
                "verdict": "rejected",
                "status": lower_lie.pointer("/lowerResult/output/status"),
                "reasonCode": lower_lie.pointer("/lowerResult/output/reasonCode"),
                "message": lower_lie.pointer("/lowerResult/output/message"),
                "evidenceCid": lower_lie.pointer("/lowerResult/output/evidenceCid"),
                "findings": lower_lie.pointer("/lowerResult/output/findings"),
                "unsupportedSemantics": lower_lie.pointer("/lowerResult/output/unsupportedSemantics"),
                "sourceSpans": lower_lie.pointer("/lowerResult/output/sourceSpans"),
            },
            "contractSet": {
                "verdict": version_red["verdict"],
                "missingContracts": version_red["missingContracts"],
                "rule": version_red["rule"],
            },
            "binary": {
                "verdict": binary_red["verdict"],
                "reason": binary_red["reason"],
                "attestedBinaryCid": binary_red["attestedBinaryCid"],
                "observedBinaryCid": binary_red["observedBinaryCid"],
            },
            "ciInputClosure": {
                "verdict": "rejected",
                "reason": "inputClosureCid mismatch",
                "acceptedInputClosureCid": baseline_closure,
                "candidateInputClosureCid": candidate_closure,
            }
        },
        "greenRails": {
            "policy": {
                "verdict": policy_green["verdict"],
                "reason": policy_green["reason"],
            }
        },
        "workflow": {
            "runner": "provekit-supply-chain-rails",
            "provekitCli": provekit_cli_report(&repo_root),
        }
    }))
}

fn provekit_json_ok(repo_root: &Path, args: Vec<String>) -> Result<Value, SupplyChainRailsError> {
    let output = run_provekit_raw(repo_root, &args).map_err(SupplyChainRailsError::Setup)?;
    parse_json_output(output, true).map_err(SupplyChainRailsError::Verify)
}

fn provekit_json_fail(repo_root: &Path, args: Vec<String>) -> Result<Value, SupplyChainRailsError> {
    let output = run_provekit_raw(repo_root, &args).map_err(SupplyChainRailsError::Setup)?;
    parse_json_output(output, false).map_err(SupplyChainRailsError::Verify)
}

struct ProvekitOutput {
    status_success: bool,
    stdout: String,
    stderr: String,
    command: String,
}

fn run_provekit_raw(repo_root: &Path, args: &[String]) -> Result<ProvekitOutput, String> {
    let invocation = provekit_cli_invocation(repo_root);
    let mut cmd_args = invocation.command.clone();
    cmd_args.extend(args.iter().cloned());
    cmd_args.push("--json".into());
    cmd_args.push("--quiet".into());
    let program = cmd_args
        .first()
        .ok_or_else(|| "provekit command is empty".to_string())?
        .clone();
    let mut cmd = Command::new(program);
    cmd.args(&cmd_args[1..]).current_dir(repo_root);
    let output = cmd
        .output()
        .map_err(|e| format!("spawn {}: {e}", cmd_args.join(" ")))?;
    Ok(ProvekitOutput {
        status_success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        command: cmd_args.join(" "),
    })
}

fn parse_json_output(output: ProvekitOutput, expect_success: bool) -> Result<Value, String> {
    if output.status_success != expect_success {
        return Err(format!(
            "`{}` exit status mismatch; expected success={expect_success}\nstdout:\n{}\nstderr:\n{}",
            output.command, output.stdout, output.stderr
        ));
    }
    serde_json::from_str(&output.stdout).map_err(|e| {
        format!(
            "parse JSON from `{}`: {e}\nstdout:\n{}\nstderr:\n{}",
            output.command, output.stdout, output.stderr
        )
    })
}

fn witness_plan_for(lift: &Value, attach_to: &str) -> Result<Value, String> {
    let witnesses = lift
        .get("witnesses")
        .and_then(Value::as_array)
        .ok_or_else(|| "lift JSON missing witnesses array".to_string())?;
    witnesses
        .iter()
        .find(|witness| witness.get("attachTo").and_then(Value::as_str) == Some(attach_to))
        .cloned()
        .ok_or_else(|| format!("lift JSON missing witness attached to `{attach_to}`"))
}

fn assert_str(
    value: &Value,
    pointer: &str,
    expected: &str,
    label: &str,
) -> Result<(), SupplyChainRailsError> {
    let observed = value
        .pointer(pointer)
        .and_then(Value::as_str)
        .ok_or_else(|| {
            SupplyChainRailsError::Verify(format!("{label} missing string field `{pointer}`"))
        })?;
    if observed != expected {
        return Err(SupplyChainRailsError::Verify(format!(
            "{label} expected `{expected}`, observed `{observed}`"
        )));
    }
    Ok(())
}

fn assert_bool(
    value: &Value,
    pointer: &str,
    expected: bool,
    label: &str,
) -> Result<(), SupplyChainRailsError> {
    let observed = value
        .pointer(pointer)
        .and_then(Value::as_bool)
        .ok_or_else(|| {
            SupplyChainRailsError::Verify(format!("{label} missing bool field `{pointer}`"))
        })?;
    if observed != expected {
        return Err(SupplyChainRailsError::Verify(format!(
            "{label} expected `{expected}`, observed `{observed}`"
        )));
    }
    Ok(())
}

fn assert_contains_string(
    value: &Value,
    pointer: &str,
    expected: &str,
    label: &str,
) -> Result<(), SupplyChainRailsError> {
    let array = value
        .pointer(pointer)
        .and_then(Value::as_array)
        .ok_or_else(|| {
            SupplyChainRailsError::Verify(format!("{label} missing array field `{pointer}`"))
        })?;
    if !array.iter().any(|item| item.as_str() == Some(expected)) {
        return Err(SupplyChainRailsError::Verify(format!(
            "{label} expected `{expected}` in `{pointer}`, observed {array:?}"
        )));
    }
    Ok(())
}

fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }
    let mut text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    text.push('\n');
    std::fs::write(path, text).map_err(|e| format!("write {}: {e}", path.display()))
}

fn temp_dir(kind: &str) -> Result<PathBuf, SupplyChainRailsError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| SupplyChainRailsError::Setup(format!("system clock before UNIX_EPOCH: {e}")))?
        .as_nanos();
    let path = std::env::temp_dir().join(format!("provekit-{kind}-{}-{now}", std::process::id()));
    std::fs::create_dir_all(&path)
        .map_err(|e| SupplyChainRailsError::Setup(format!("mkdir {}: {e}", path.display())))?;
    Ok(path)
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
    std::env::var(PROVEKIT_SUPPLY_CHAIN_EXTERNAL_CLI_ENV)
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
                "set {PROVEKIT_SUPPLY_CHAIN_EXTERNAL_CLI_ENV}=1 to run Supply Chain Rails against an explicit external provekit binary"
            ),
        });
    }
    report
}
