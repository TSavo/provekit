// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::{BufRead, BufReader, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use clap::Parser;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{OutputFlags, EXIT_OK, EXIT_USER_ERROR, EXIT_VERIFY_FAIL};

#[derive(Parser, Debug, Clone)]
pub struct ZooArgs {
    /// Specimen directory or specimen.yaml path. Defaults to bug-zoo/species.
    pub specimen: Option<PathBuf>,
    /// Check every species under bug-zoo/species.
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
    kingdom: String,
    surface: String,
    status: String,
    paths: SpecimenPaths,
    commands: SpecimenCommands,
    predicates: Predicates,
    exposures: Vec<Exposure>,
    equivalence: Equivalence,
    expectations: Expectations,
    exposure: ExposureFiles,
    dropper: Dropper,
    #[serde(default)]
    wild_sightings: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpecimenPaths {
    lab_library: PathBuf,
    lab_harness: PathBuf,
    lab_kit_rpc: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpecimenCommands {
    host_check: CommandSpec,
}

#[derive(Debug, Deserialize, Clone)]
struct CommandSpec {
    cwd: PathBuf,
    argv: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Predicates {
    boundary: String,
    sink: String,
    missing_edge: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Exposure {
    id: String,
    surface: String,
    harness: PathBuf,
    kit_rpc: PathBuf,
    lift_rpc: CommandSpec,
    proof_ir_file: PathBuf,
    diagnostic_file: PathBuf,
    lossiness: Lossiness,
}

#[derive(Debug, Deserialize)]
struct Lossiness {
    erased: Vec<String>,
    preserved: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Equivalence {
    #[serde(default)]
    required: Vec<[String; 2]>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Expectations {
    host_compiler: String,
    ordinary_tests: String,
    provekit_verify: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExposureFiles {
    sat_witness_file: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Dropper {
    available: bool,
    #[serde(default)]
    surface: Option<String>,
    #[serde(default)]
    source: Option<PathBuf>,
    #[serde(default)]
    target_symbol: Option<String>,
    #[serde(default)]
    proof_var: Option<String>,
    #[serde(default)]
    realizer_rpc: Option<CommandSpec>,
    #[serde(default)]
    output_source: Option<PathBuf>,
    #[serde(default)]
    closure_proof_ir_file: Option<PathBuf>,
    #[serde(default)]
    verify_output_file: Option<PathBuf>,
}

#[derive(Debug)]
enum ZooError {
    Setup(String),
    Verify(String),
}

impl fmt::Display for ZooError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ZooError::Setup(message) | ZooError::Verify(message) => f.write_str(message),
        }
    }
}

impl ZooError {
    fn setup(message: impl Into<String>) -> Self {
        ZooError::Setup(message.into())
    }

    fn verify(message: impl Into<String>) -> Self {
        ZooError::Verify(message.into())
    }
}

pub fn run(args: ZooArgs) -> u8 {
    let targets = match resolve_targets(&args) {
        Ok(targets) => targets,
        Err(error) => {
            eprintln!("zoo: {error}");
            return EXIT_USER_ERROR;
        }
    };

    let mut reports = Vec::new();
    let mut setup_failures = Vec::new();
    let mut verification_failures = Vec::new();
    for specimen_dir in targets {
        match check_specimen(&specimen_dir, args.out.quiet || args.out.json) {
            Ok(report) => reports.push(report),
            Err(ZooError::Setup(error)) => {
                setup_failures.push(format!("{}: {error}", specimen_dir.display()))
            }
            Err(ZooError::Verify(error)) => {
                verification_failures.push(format!("{}: {error}", specimen_dir.display()))
            }
        }
    }
    let ok = setup_failures.is_empty() && verification_failures.is_empty();

    if args.out.json {
        let errors = setup_failures
            .iter()
            .chain(verification_failures.iter())
            .cloned()
            .collect::<Vec<_>>();
        let out = json!({
            "ok": ok,
            "reports": reports,
            "errors": errors,
            "setupErrors": setup_failures.clone(),
            "verificationErrors": verification_failures.clone(),
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&out).expect("zoo report serializes")
        );
    } else if !args.out.quiet {
        for report in &reports {
            println!("zoo: {} PASS", report["id"].as_str().unwrap_or("specimen"));
        }
        for failure in setup_failures.iter().chain(verification_failures.iter()) {
            eprintln!("zoo: {failure}");
        }
    }

    if ok {
        EXIT_OK
    } else if !setup_failures.is_empty() {
        EXIT_USER_ERROR
    } else {
        EXIT_VERIFY_FAIL
    }
}

fn resolve_targets(args: &ZooArgs) -> Result<Vec<PathBuf>, String> {
    if args.all {
        let root = args
            .specimen
            .clone()
            .unwrap_or_else(|| PathBuf::from("bug-zoo/species"));
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

    let path = args.specimen.clone().unwrap_or_else(|| {
        PathBuf::from("bug-zoo/species/BZ-SHAPE-005-java-null-boundary-equivalence")
    });
    if path.file_name().and_then(|name| name.to_str()) == Some("specimen.yaml") {
        Ok(vec![path.parent().unwrap_or(Path::new(".")).to_path_buf()])
    } else {
        Ok(vec![path])
    }
}

fn check_specimen(specimen_dir: &Path, quiet: bool) -> Result<Value, ZooError> {
    let manifest_path = specimen_dir.join("specimen.yaml");
    let text = std::fs::read_to_string(&manifest_path)
        .map_err(|e| ZooError::setup(format!("read {}: {e}", manifest_path.display())))?;
    let manifest: SpecimenManifest = serde_yaml::from_str(&text)
        .map_err(|e| ZooError::setup(format!("parse {}: {e}", manifest_path.display())))?;

    let mut errors = validate_manifest_shape(&manifest);
    errors.extend(validate_paths(specimen_dir, &manifest));
    if !errors.is_empty() {
        return Err(ZooError::setup(errors.join("; ")));
    }

    run_host_check(specimen_dir, &manifest.commands.host_check).map_err(ZooError::verify)?;

    let mut cids = BTreeMap::new();
    for exposure in &manifest.exposures {
        let lifted = invoke_lift_rpc(specimen_dir, exposure).map_err(ZooError::verify)?;
        let expected =
            read_json(specimen_dir.join(&exposure.proof_ir_file)).map_err(ZooError::setup)?;
        let lifted_ir = lifted
            .get("ir")
            .cloned()
            .ok_or_else(|| ZooError::verify(format!("exposure `{}` missing ir", exposure.id)))?;
        let lifted_cid = proof_ir_cid(&lifted_ir).map_err(ZooError::verify)?;
        let expected_cid = proof_ir_cid(&expected).map_err(ZooError::setup)?;
        if lifted_cid != expected_cid {
            return Err(ZooError::verify(format!(
                "exposure `{}` ProofIR CID mismatch: lifted {lifted_cid}, expected {expected_cid}",
                exposure.id
            )));
        }

        let diagnostic_path = specimen_dir.join(&exposure.diagnostic_file);
        let diag = std::fs::read_to_string(&diagnostic_path).map_err(|e| {
            ZooError::verify(format!(
                "read diagnostic {} for `{}`: {e}",
                diagnostic_path.display(),
                exposure.id
            ))
        })?;
        if !diag.contains(&manifest.predicates.missing_edge) {
            return Err(ZooError::verify(format!(
                "diagnostic for `{}` does not mention missing edge `{}`",
                exposure.id, manifest.predicates.missing_edge
            )));
        }

        cids.insert(exposure.id.clone(), lifted_cid);
    }

    for [left, right] in &manifest.equivalence.required {
        if cids.get(left) != cids.get(right) {
            return Err(ZooError::verify(format!(
                "equivalence failed: `{left}` CID {:?} != `{right}` CID {:?}",
                cids.get(left),
                cids.get(right)
            )));
        }
    }

    let sat_witness = read_json(specimen_dir.join(&manifest.exposure.sat_witness_file))
        .map_err(ZooError::setup)?;
    let dropper_report = verify_dropper(specimen_dir, &manifest).map_err(ZooError::verify)?;
    if !quiet {
        println!("zoo: {} hostCheck PASS", manifest.id);
        for (id, cid) in &cids {
            println!("zoo: exposure {id} {cid}");
        }
        for [left, right] in &manifest.equivalence.required {
            println!("zoo: equivalence {left} == {right} PASS");
        }
        println!(
            "zoo: expected verify failure {} PASS",
            manifest.predicates.missing_edge
        );
        if dropper_report.is_some() {
            println!("zoo: dropper closed {} PASS", manifest.predicates.missing_edge);
        }
    }

    Ok(json!({
        "id": manifest.id,
        "name": manifest.name,
        "kingdom": manifest.kingdom,
        "surface": manifest.surface,
        "status": manifest.status,
        "proofIrCids": cids,
        "missingEdge": manifest.predicates.missing_edge,
        "expectations": {
            "hostCompiler": manifest.expectations.host_compiler,
            "ordinaryTests": manifest.expectations.ordinary_tests,
            "provekitVerify": manifest.expectations.provekit_verify,
        },
        "dropperAvailable": manifest.dropper.available,
        "dropper": dropper_report,
        "wildSightings": manifest.wild_sightings,
        "satWitness": sat_witness,
    }))
}

fn validate_manifest_shape(manifest: &SpecimenManifest) -> Vec<String> {
    let mut errors = Vec::new();

    if manifest.id.trim().is_empty() {
        errors.push("id is required".into());
    }
    if manifest.exposures.is_empty() {
        errors.push("at least one exposure is required".into());
    }
    if manifest.predicates.boundary.trim().is_empty() {
        errors.push("predicates.boundary is required".into());
    }
    if manifest.predicates.sink.trim().is_empty() {
        errors.push("predicates.sink is required".into());
    }
    if manifest.predicates.missing_edge.trim().is_empty() {
        errors.push("predicates.missingEdge is required".into());
    }
    if manifest.kingdom.trim().is_empty() {
        errors.push("kingdom is required".into());
    }
    if manifest.surface.trim().is_empty() {
        errors.push("surface is required".into());
    }
    if manifest.status.trim().is_empty() {
        errors.push("status is required".into());
    }
    if manifest.commands.host_check.argv.is_empty() {
        errors.push("commands.hostCheck.argv is required".into());
    }
    if manifest.expectations.host_compiler.trim().is_empty() {
        errors.push("expectations.hostCompiler is required".into());
    }
    if manifest.expectations.ordinary_tests.trim().is_empty() {
        errors.push("expectations.ordinaryTests is required".into());
    }
    if manifest.expectations.provekit_verify.trim().is_empty() {
        errors.push("expectations.provekitVerify is required".into());
    }

    let mut exposure_ids = BTreeSet::new();
    for exposure in &manifest.exposures {
        if !exposure_ids.insert(exposure.id.clone()) {
            errors.push(format!("duplicate exposure id `{}`", exposure.id));
        }
        if exposure.lift_rpc.argv.is_empty() {
            errors.push(format!(
                "exposure `{}` liftRpc.argv is required",
                exposure.id
            ));
        }
        if exposure.lossiness.erased.is_empty() || exposure.lossiness.preserved.is_empty() {
            errors.push(format!(
                "exposure `{}` must describe lossiness erased and preserved boundaries",
                exposure.id
            ));
        }
    }

    for [left, right] in &manifest.equivalence.required {
        if !exposure_ids.contains(left) {
            errors.push(format!("equivalence references unknown exposure `{left}`"));
        }
        if !exposure_ids.contains(right) {
            errors.push(format!("equivalence references unknown exposure `{right}`"));
        }
    }

    if manifest.dropper.available {
        if manifest.dropper.surface.as_deref().unwrap_or("").trim().is_empty() {
            errors.push("dropper.surface is required when dropper.available is true".into());
        }
        if manifest.dropper.source.is_none() {
            errors.push("dropper.source is required when dropper.available is true".into());
        }
        if manifest
            .dropper
            .target_symbol
            .as_deref()
            .unwrap_or("")
            .trim()
            .is_empty()
        {
            errors.push("dropper.targetSymbol is required when dropper.available is true".into());
        }
        if manifest
            .dropper
            .proof_var
            .as_deref()
            .unwrap_or("")
            .trim()
            .is_empty()
        {
            errors.push("dropper.proofVar is required when dropper.available is true".into());
        }
        match &manifest.dropper.realizer_rpc {
            Some(command) if command.argv.is_empty() => {
                errors.push("dropper.realizerRpc.argv is required when dropper.available is true".into())
            }
            None => errors.push("dropper.realizerRpc is required when dropper.available is true".into()),
            Some(_) => {}
        }
        if manifest.dropper.output_source.is_none() {
            errors.push("dropper.outputSource is required when dropper.available is true".into());
        }
        if manifest.dropper.closure_proof_ir_file.is_none() {
            errors
                .push("dropper.closureProofIrFile is required when dropper.available is true".into());
        }
        if manifest.dropper.verify_output_file.is_none() {
            errors.push("dropper.verifyOutputFile is required when dropper.available is true".into());
        }
    }

    errors
}

fn validate_paths(specimen_dir: &Path, manifest: &SpecimenManifest) -> Vec<String> {
    let mut errors = Vec::new();

    for path in [
        &manifest.paths.lab_library,
        &manifest.paths.lab_harness,
        &manifest.paths.lab_kit_rpc,
        &manifest.exposure.sat_witness_file,
    ] {
        if manifest_path_escapes_specimen_root(path) {
            errors.push(format!(
                "invalid path `{}` escapes specimen root",
                path.display()
            ));
            continue;
        }
        let full_path = specimen_dir.join(path);
        if !full_path.exists() {
            errors.push(format!("missing {}", full_path.display()));
        }
    }

    for path in [&manifest.commands.host_check.cwd] {
        if manifest_path_escapes_specimen_root(path) {
            errors.push(format!(
                "invalid path `{}` escapes specimen root",
                path.display()
            ));
            continue;
        }
        let full_path = specimen_dir.join(path);
        if !full_path.exists() {
            errors.push(format!("missing {}", full_path.display()));
        }
    }

    for exposure in &manifest.exposures {
        for path in [
            &exposure.harness,
            &exposure.kit_rpc,
            &exposure.proof_ir_file,
            &exposure.diagnostic_file,
            &exposure.lift_rpc.cwd,
        ] {
            if manifest_path_escapes_specimen_root(path) {
                errors.push(format!(
                    "invalid path `{}` escapes specimen root",
                    path.display()
                ));
                continue;
            }
            let full_path = specimen_dir.join(path);
            if !full_path.exists() {
                errors.push(format!("missing {}", full_path.display()));
            }
        }
    }

    if manifest.dropper.available {
        for path in [
            manifest.dropper.source.as_ref(),
            manifest.dropper.output_source.as_ref(),
            manifest.dropper.closure_proof_ir_file.as_ref(),
            manifest.dropper.verify_output_file.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            if manifest_path_escapes_specimen_root(path) {
                errors.push(format!(
                    "invalid path `{}` escapes specimen root",
                    path.display()
                ));
                continue;
            }
            let full_path = specimen_dir.join(path);
            if !full_path.exists() {
                errors.push(format!("missing {}", full_path.display()));
            }
        }
        if let Some(command) = &manifest.dropper.realizer_rpc {
            if manifest_path_escapes_specimen_root(&command.cwd) {
                errors.push(format!(
                    "invalid path `{}` escapes specimen root",
                    command.cwd.display()
                ));
            } else {
                let full_path = specimen_dir.join(&command.cwd);
                if !full_path.exists() {
                    errors.push(format!("missing {}", full_path.display()));
                }
            }
        }
    }

    errors
}

fn run_host_check(specimen_dir: &Path, command: &CommandSpec) -> Result<(), String> {
    run_command(specimen_dir, command).map(|_| ())
}

fn run_command(specimen_dir: &Path, command: &CommandSpec) -> Result<String, String> {
    if command.argv.is_empty() {
        return Err("empty argv".into());
    }
    if manifest_path_escapes_specimen_root(&command.cwd) {
        return Err(format!(
            "invalid path `{}` escapes specimen root",
            command.cwd.display()
        ));
    }

    let cwd = specimen_dir.join(&command.cwd);
    let output = Command::new(&command.argv[0])
        .args(&command.argv[1..])
        .current_dir(&cwd)
        .output()
        .map_err(|e| format!("spawn {:?} in {}: {e}", command.argv, cwd.display()))?;
    if !output.status.success() {
        return Err(format!(
            "command {:?} failed in {}\nstdout:\n{}\nstderr:\n{}",
            command.argv,
            cwd.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn read_json(path: PathBuf) -> Result<Value, String> {
    let text =
        std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("parse {}: {e}", path.display()))
}

fn invoke_lift_rpc(specimen_dir: &Path, exposure: &Exposure) -> Result<Value, String> {
    if exposure.lift_rpc.argv.is_empty() {
        return Err(format!(
            "exposure `{}` liftRpc.argv is required",
            exposure.id
        ));
    }
    if manifest_path_escapes_specimen_root(&exposure.lift_rpc.cwd)
        || manifest_path_escapes_specimen_root(&exposure.harness)
    {
        return Err(format!(
            "exposure `{}` contains a path that escapes specimen root",
            exposure.id
        ));
    }

    let cwd = specimen_dir.join(&exposure.lift_rpc.cwd);
    let harness = specimen_dir.join(&exposure.harness);
    let harness_root = harness.canonicalize().unwrap_or(harness);
    let harness_root = harness_root.to_string_lossy().to_string();
    let mut cmd = Command::new(&exposure.lift_rpc.argv[0]);
    cmd.args(&exposure.lift_rpc.argv[1..])
        .current_dir(&cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());

    let mut child = cmd.spawn().map_err(|e| {
        format!(
            "spawn lift RPC for `{}` in {}: {e}",
            exposure.id,
            cwd.display()
        )
    })?;
    let mut stdin = match child.stdin.take() {
        Some(stdin) => stdin,
        None => {
            cleanup_rpc_child(&mut child, None, false);
            return Err("lifter has no stdin".into());
        }
    };
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            cleanup_rpc_child(&mut child, Some(stdin), false);
            return Err("lifter has no stdout".into());
        }
    };
    let mut reader = BufReader::new(stdout);

    let exchange_result =
        invoke_lift_rpc_exchange(&mut stdin, &mut reader, &harness_root, exposure);
    drop(reader);
    cleanup_rpc_child(&mut child, Some(stdin), exchange_result.is_ok());
    exchange_result
}

fn invoke_lift_rpc_exchange(
    stdin: &mut ChildStdin,
    reader: &mut impl BufRead,
    harness_root: &str,
    exposure: &Exposure,
) -> Result<Value, String> {
    let init_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "client": {"name": "provekit-zoo", "version": env!("CARGO_PKG_VERSION")},
            "protocol_version": "provekit-lift/1",
            "workspace_root": harness_root,
            "config_path": ".provekit/config.toml",
        },
    });
    writeln!(stdin, "{init_req}").map_err(|e| format!("write initialize: {e}"))?;
    let _ = read_response(reader, 1)?;

    let lift_req = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "lift",
        "params": {
            "surface": exposure.surface,
            "workspace_root": harness_root,
            "source_paths": [harness_root],
            "options": {"layer": "all"},
        },
    });
    writeln!(stdin, "{lift_req}").map_err(|e| format!("write lift: {e}"))?;
    let lift_resp = read_response(reader, 2)?;

    match lift_resp.get("kind").and_then(|value| value.as_str()) {
        Some("ir-document") => Ok(lift_resp),
        other => Err(format!(
            "exposure `{}` returned unsupported lift kind {:?}",
            exposure.id, other
        )),
    }
}

fn verify_dropper(specimen_dir: &Path, manifest: &SpecimenManifest) -> Result<Option<Value>, String> {
    if !manifest.dropper.available {
        return Ok(None);
    }

    let dropper = &manifest.dropper;
    let source_path = required_dropper_path(&dropper.source, "dropper.source")?;
    let output_source_path =
        required_dropper_path(&dropper.output_source, "dropper.outputSource")?;
    let closure_ir_path =
        required_dropper_path(&dropper.closure_proof_ir_file, "dropper.closureProofIrFile")?;
    let verify_output_path =
        required_dropper_path(&dropper.verify_output_file, "dropper.verifyOutputFile")?;
    let surface = required_dropper_str(&dropper.surface, "dropper.surface")?;
    let target_symbol = required_dropper_str(&dropper.target_symbol, "dropper.targetSymbol")?;
    let proof_var = required_dropper_str(&dropper.proof_var, "dropper.proofVar")?;
    let realizer_rpc = dropper
        .realizer_rpc
        .as_ref()
        .ok_or_else(|| "dropper.realizerRpc is required".to_string())?;

    let source_abs = specimen_dir.join(source_path);
    let source = std::fs::read_to_string(&source_abs)
        .map_err(|e| format!("read {}: {e}", source_abs.display()))?;

    let output = invoke_dropper_rpc(
        specimen_dir,
        realizer_rpc,
        surface,
        target_symbol,
        proof_var,
        &source,
        &manifest.predicates,
    )?;

    if output.get("status").and_then(Value::as_str) != Some("closed") {
        return Err(format!("dropper output was not closed: {output}"));
    }

    let modified_source = output
        .get("modifiedSource")
        .and_then(Value::as_str)
        .ok_or_else(|| "dropper output missing modifiedSource".to_string())?;
    let expected_source_abs = specimen_dir.join(output_source_path);
    let expected_source = std::fs::read_to_string(&expected_source_abs)
        .map_err(|e| format!("read {}: {e}", expected_source_abs.display()))?;
    if modified_source != expected_source {
        return Err(format!(
            "dropper modified source mismatch against {}",
            expected_source_abs.display()
        ));
    }

    let post_lift_ir = output
        .get("postLift")
        .and_then(|post_lift| post_lift.get("ir"))
        .ok_or_else(|| "dropper output missing postLift.ir".to_string())?;
    let expected_ir = read_json(specimen_dir.join(closure_ir_path))?;
    let post_lift_cid = proof_ir_cid(post_lift_ir)?;
    let expected_ir_cid = proof_ir_cid(&expected_ir)?;
    if post_lift_cid != expected_ir_cid {
        return Err(format!(
            "dropper closure ProofIR CID mismatch: lifted {post_lift_cid}, expected {expected_ir_cid}"
        ));
    }

    let verify_output = read_json(specimen_dir.join(verify_output_path))?;
    if verify_output.get("status").and_then(Value::as_str) != Some("closed") {
        return Err("dropper verifyOutputFile must record status: closed".into());
    }
    if verify_output.get("missingEdge").and_then(Value::as_str)
        != Some(manifest.predicates.missing_edge.as_str())
    {
        return Err("dropper verifyOutputFile missingEdge does not match manifest".into());
    }

    Ok(Some(json!({
        "status": "closed",
        "surface": surface,
        "source": source_path,
        "targetSymbol": target_symbol,
        "proofVar": proof_var,
        "transformedArtifactCid": output.get("transformedArtifactCid").cloned().unwrap_or(Value::Null),
        "postLiftCid": output.get("postLiftCid").cloned().unwrap_or(Value::Null),
        "closureWitnessCid": output.get("closureWitnessCid").cloned().unwrap_or(Value::Null),
        "closureProofIrCid": post_lift_cid,
        "verifyOutput": verify_output,
    })))
}

fn invoke_dropper_rpc(
    specimen_dir: &Path,
    command: &CommandSpec,
    surface: &str,
    target_symbol: &str,
    proof_var: &str,
    source: &str,
    predicates: &Predicates,
) -> Result<Value, String> {
    if command.argv.is_empty() {
        return Err("dropper realizer argv is empty".into());
    }
    if manifest_path_escapes_specimen_root(&command.cwd) {
        return Err(format!(
            "invalid path `{}` escapes specimen root",
            command.cwd.display()
        ));
    }

    let cwd = specimen_dir.join(&command.cwd);
    let workspace_root = specimen_dir
        .canonicalize()
        .unwrap_or_else(|_| specimen_dir.to_path_buf())
        .to_string_lossy()
        .to_string();
    let mut cmd = Command::new(&command.argv[0]);
    cmd.args(&command.argv[1..])
        .current_dir(&cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("spawn dropper RPC in {}: {e}", cwd.display()))?;
    let mut stdin = match child.stdin.take() {
        Some(stdin) => stdin,
        None => {
            cleanup_rpc_child(&mut child, None, false);
            return Err("dropper realizer has no stdin".into());
        }
    };
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            cleanup_rpc_child(&mut child, Some(stdin), false);
            return Err("dropper realizer has no stdout".into());
        }
    };
    let mut reader = BufReader::new(stdout);

    let exchange_result = invoke_dropper_rpc_exchange(
        &mut stdin,
        &mut reader,
        &workspace_root,
        surface,
        target_symbol,
        proof_var,
        source,
        predicates,
    );
    drop(reader);
    cleanup_rpc_child(&mut child, Some(stdin), exchange_result.is_ok());
    exchange_result
}

fn invoke_dropper_rpc_exchange(
    stdin: &mut ChildStdin,
    reader: &mut impl BufRead,
    workspace_root: &str,
    surface: &str,
    target_symbol: &str,
    proof_var: &str,
    source: &str,
    predicates: &Predicates,
) -> Result<Value, String> {
    let init_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "client": {"name": "provekit-zoo", "version": env!("CARGO_PKG_VERSION")},
            "protocol_version": "provekit-orp/1",
            "workspace_root": workspace_root,
        },
    });
    writeln!(stdin, "{init_req}").map_err(|e| format!("write dropper initialize: {e}"))?;
    let _ = read_response(reader, 1)?;

    let gap_cid = provekit_canonicalizer::blake3_512_of(predicates.missing_edge.as_bytes());
    let policy_cid = provekit_canonicalizer::blake3_512_of(b"bug-zoo-dropper-policy-v0");
    let realize_req = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "realize",
        "params": {
            "plan": {
                "kind": "RealizerPlan",
                "schemaVersion": "1",
                "mode": "transform",
                "gapCid": gap_cid,
                "sourcePredicate": predicates.boundary,
                "targetPredicate": predicates.sink,
                "policyCid": policy_cid,
                "surface": surface,
                "targetSymbol": target_symbol,
                "proofVar": proof_var,
                "source": source,
            }
        },
    });
    writeln!(stdin, "{realize_req}").map_err(|e| format!("write dropper realize: {e}"))?;
    let result = read_response(reader, 2)?;
    result
        .get("output")
        .cloned()
        .ok_or_else(|| "dropper response missing output".into())
}

fn required_dropper_path<'a>(path: &'a Option<PathBuf>, field: &str) -> Result<&'a Path, String> {
    path.as_deref()
        .ok_or_else(|| format!("{field} is required when dropper.available is true"))
}

fn required_dropper_str<'a>(value: &'a Option<String>, field: &str) -> Result<&'a str, String> {
    value
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{field} is required when dropper.available is true"))
}

fn cleanup_rpc_child(child: &mut Child, stdin: Option<ChildStdin>, send_shutdown: bool) {
    if let Some(mut stdin) = stdin {
        if send_shutdown {
            let shutdown_req = json!({"jsonrpc": "2.0", "id": 3, "method": "shutdown"});
            let _ = writeln!(stdin, "{shutdown_req}");
        }
        drop(stdin);
    }

    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(25));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return;
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return;
            }
        }
    }
}

fn read_response(reader: &mut impl BufRead, id: i64) -> Result<Value, String> {
    let mut line = String::new();
    let n = reader
        .read_line(&mut line)
        .map_err(|e| format!("read RPC response: {e}"))?;
    if n == 0 {
        return Err("plugin closed stdout before responding".into());
    }
    let v: Value = serde_json::from_str(line.trim())
        .map_err(|e| format!("parse JSON-RPC response: {e}\nraw: {line}"))?;
    if v.get("id").and_then(|value| value.as_i64()) != Some(id) {
        return Err(format!("response id mismatch: expected {id}, got {v:?}"));
    }
    if let Some(error) = v.get("error") {
        return Err(format!("plugin returned error: {error}"));
    }
    v.get("result")
        .cloned()
        .ok_or_else(|| "response missing result".into())
}

fn proof_ir_cid(value: &Value) -> Result<String, String> {
    let canonical = json_to_cvalue(value)?;
    let bytes = provekit_canonicalizer::encode_jcs(&canonical).into_bytes();
    Ok(provekit_canonicalizer::blake3_512_of(&bytes))
}

fn json_to_cvalue(j: &Value) -> Result<std::sync::Arc<provekit_canonicalizer::Value>, String> {
    use provekit_canonicalizer::Value as CValue;

    match j {
        Value::Null => Ok(CValue::null()),
        Value::Bool(value) => Ok(CValue::boolean(*value)),
        Value::Number(number) => number
            .as_i64()
            .map(CValue::integer)
            .ok_or_else(|| format!("ProofIR JSON number `{number}` is not a supported integer")),
        Value::String(value) => Ok(CValue::string(value.clone())),
        Value::Array(items) => items
            .iter()
            .map(json_to_cvalue)
            .collect::<Result<Vec<_>, _>>()
            .map(CValue::array),
        Value::Object(map) => map
            .iter()
            .map(|(key, value)| Ok((key.clone(), json_to_cvalue(value)?)))
            .collect::<Result<Vec<_>, String>>()
            .map(CValue::object),
    }
}

fn manifest_path_escapes_specimen_root(path: &Path) -> bool {
    path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use tempfile::tempdir;

    fn valid_manifest() -> SpecimenManifest {
        SpecimenManifest {
            id: "BZ-SHAPE-005".into(),
            name: "x".into(),
            kingdom: "shape".into(),
            surface: "java".into(),
            status: "lab".into(),
            paths: SpecimenPaths {
                lab_library: "lab/library".into(),
                lab_harness: "lab/harness".into(),
                lab_kit_rpc: "lab/kit-rpc".into(),
            },
            commands: SpecimenCommands {
                host_check: CommandSpec {
                    cwd: "lab/harness".into(),
                    argv: vec!["./run.sh".into()],
                },
            },
            predicates: Predicates {
                boundary: "maybe_null(name)".into(),
                sink: "non_null(name)".into(),
                missing_edge: "maybe_null(name) => non_null(name)".into(),
            },
            exposures: vec![Exposure {
                id: "spring-web".into(),
                surface: "java-spring-web".into(),
                harness: "exposed/spring-web/harness".into(),
                kit_rpc: "exposed/spring-web/kit-rpc".into(),
                lift_rpc: CommandSpec {
                    cwd: "exposed/spring-web/kit-rpc".into(),
                    argv: vec!["./run-java-lifter.sh".into()],
                },
                proof_ir_file: "exposed/spring-web/expected.proofir.json".into(),
                diagnostic_file: "exposed/spring-web/expected-diagnostic.txt".into(),
                lossiness: Lossiness {
                    erased: vec!["Spring binding".into()],
                    preserved: vec!["precondition neq(name, null)".into()],
                },
            }],
            equivalence: Equivalence { required: vec![] },
            expectations: Expectations {
                host_compiler: "pass".into(),
                ordinary_tests: "pass".into(),
                provekit_verify: "fail".into(),
            },
            exposure: ExposureFiles {
                sat_witness_file: "exposed/sat-witness.json".into(),
            },
            dropper: unavailable_dropper(),
            wild_sightings: vec![],
        }
    }

    fn unavailable_dropper() -> Dropper {
        Dropper {
            available: false,
            surface: None,
            source: None,
            target_symbol: None,
            proof_var: None,
            realizer_rpc: None,
            output_source: None,
            closure_proof_ir_file: None,
            verify_output_file: None,
        }
    }

    fn create_valid_paths(specimen_dir: &Path, manifest: &SpecimenManifest) {
        for path in [
            &manifest.paths.lab_library,
            &manifest.paths.lab_harness,
            &manifest.paths.lab_kit_rpc,
            &manifest.exposures[0].harness,
            &manifest.exposures[0].kit_rpc,
        ] {
            fs::create_dir_all(specimen_dir.join(path)).expect("create directory");
        }

        for path in [
            &manifest.exposure.sat_witness_file,
            &manifest.exposures[0].proof_ir_file,
            &manifest.exposures[0].diagnostic_file,
        ] {
            let path = specimen_dir.join(path);
            fs::create_dir_all(path.parent().expect("fixture path has parent"))
                .expect("create file parent");
            fs::write(path, "").expect("create file");
        }
    }

    #[test]
    fn parses_manifest_with_exposures_and_equivalence() {
        let raw = r#"
id: BZ-SHAPE-005
name: Java Null Boundary Equivalence
kingdom: shape
surface: java
status: lab
paths:
  labLibrary: lab/library
  labHarness: lab/harness
  labKitRpc: lab/kit-rpc
commands:
  hostCheck:
    cwd: lab/harness
    argv: ["./run.sh"]
predicates:
  boundary: maybe_null(name)
  sink: non_null(name)
  missingEdge: maybe_null(name) => non_null(name)
exposures:
  - id: provekit-native
    surface: java-provekit-native
    harness: exposed/provekit-native/harness
    kitRpc: exposed/provekit-native/kit-rpc
    liftRpc:
      cwd: exposed/provekit-native/kit-rpc
      argv: ["./run-java-lifter.sh"]
    proofIrFile: exposed/provekit-native/expected.proofir.json
    diagnosticFile: exposed/provekit-native/expected-diagnostic.txt
    lossiness:
      erased: ["Java body"]
      preserved: ["precondition neq(name, null)"]
  - id: spring-web
    surface: java-spring-web
    harness: exposed/spring-web/harness
    kitRpc: exposed/spring-web/kit-rpc
    liftRpc:
      cwd: exposed/spring-web/kit-rpc
      argv: ["./run-java-lifter.sh"]
    proofIrFile: exposed/spring-web/expected.proofir.json
    diagnosticFile: exposed/spring-web/expected-diagnostic.txt
    lossiness:
      erased: ["Spring binding"]
      preserved: ["precondition neq(name, null)"]
equivalence:
  required:
    - [provekit-native, spring-web]
expectations:
  hostCompiler: pass
  ordinaryTests: pass
  provekitVerify: fail
exposure:
  satWitnessFile: exposed/sat-witness.json
dropper:
  available: false
wildSightings: []
"#;
        let manifest: SpecimenManifest = serde_yaml::from_str(raw).expect("parse manifest");
        assert_eq!(manifest.id, "BZ-SHAPE-005");
        assert_eq!(manifest.exposures.len(), 2);
        assert_eq!(
            manifest.equivalence.required[0],
            ["provekit-native", "spring-web"]
        );
    }

    #[test]
    fn parses_manifest_with_java_dropper_realizer() {
        let raw = r#"
id: BZ-SHAPE-005
name: Java Null Boundary Equivalence
kingdom: shape
surface: java
status: lab
paths:
  labLibrary: lab/library
  labHarness: lab/harness
  labKitRpc: lab/kit-rpc
commands:
  hostCheck:
    cwd: lab/harness
    argv: ["./run.sh"]
predicates:
  boundary: maybe_null(name)
  sink: non_null(name)
  missingEdge: maybe_null(name) => non_null(name)
exposures:
  - id: provekit-native
    surface: java-provekit-native
    harness: exposed/provekit-native/harness
    kitRpc: exposed/provekit-native/kit-rpc
    liftRpc:
      cwd: exposed/provekit-native/kit-rpc
      argv: ["./run-java-lifter.sh"]
    proofIrFile: exposed/provekit-native/expected.proofir.json
    diagnosticFile: exposed/provekit-native/expected-diagnostic.txt
    lossiness:
      erased: ["Java body"]
      preserved: ["precondition neq(name, null)"]
equivalence:
  required: []
expectations:
  hostCompiler: pass
  ordinaryTests: pass
  provekitVerify: fail
exposure:
  satWitnessFile: exposed/sat-witness.json
dropper:
  available: true
  surface: java-provekit-native
  source: lab/library/src/main/java/zoo/UserDirectory.java
  targetSymbol: lookup
  proofVar: name
  realizerRpc:
    cwd: dropped/provekit-native/kit-rpc
    argv: ["./run-java-realizer.sh"]
  outputSource: dropped/provekit-native/library/src/main/java/zoo/UserDirectory.java
  closureProofIrFile: dropped/provekit-native/closure.proofir.json
  verifyOutputFile: dropped/provekit-native/verify-output.json
wildSightings: []
"#;
        let manifest: SpecimenManifest = serde_yaml::from_str(raw).expect("parse manifest");

        assert!(manifest.dropper.available);
        assert_eq!(manifest.dropper.surface.as_deref(), Some("java-provekit-native"));
        assert_eq!(manifest.dropper.target_symbol.as_deref(), Some("lookup"));
        assert_eq!(manifest.dropper.proof_var.as_deref(), Some("name"));
        let realizer = manifest
            .dropper
            .realizer_rpc
            .as_ref()
            .expect("dropper realizer RPC config");
        assert_eq!(realizer.cwd, PathBuf::from("dropped/provekit-native/kit-rpc"));
        assert_eq!(realizer.argv, vec!["./run-java-realizer.sh"]);
    }

    #[test]
    fn validation_rejects_missing_lossiness() {
        let mut manifest = valid_manifest();
        manifest.exposures[0].lossiness.erased.clear();

        let errors = validate_manifest_shape(&manifest);
        assert!(errors.iter().any(|e| e.contains("lossiness")));
    }

    #[test]
    fn validation_rejects_duplicate_exposure_ids() {
        let mut manifest = valid_manifest();
        manifest.exposures.push(Exposure {
            id: "spring-web".into(),
            surface: "java-provekit-native".into(),
            harness: "exposed/provekit-native/harness".into(),
            kit_rpc: "exposed/provekit-native/kit-rpc".into(),
            lift_rpc: CommandSpec {
                cwd: "exposed/provekit-native/kit-rpc".into(),
                argv: vec!["./run-java-lifter.sh".into()],
            },
            proof_ir_file: "exposed/provekit-native/expected.proofir.json".into(),
            diagnostic_file: "exposed/provekit-native/expected-diagnostic.txt".into(),
            lossiness: Lossiness {
                erased: vec!["Java body".into()],
                preserved: vec!["precondition neq(name, null)".into()],
            },
        });

        let errors = validate_manifest_shape(&manifest);
        assert!(errors
            .iter()
            .any(|error| error.contains("duplicate exposure id `spring-web`")));
    }

    #[test]
    fn validation_rejects_unknown_equivalence_references() {
        let mut manifest = valid_manifest();
        manifest.equivalence.required = vec![["spring-web".into(), "missing".into()]];

        let errors = validate_manifest_shape(&manifest);
        assert!(errors
            .iter()
            .any(|error| error.contains("equivalence references unknown exposure `missing`")));
    }

    #[test]
    fn validation_rejects_empty_command_argv() {
        let mut manifest = valid_manifest();
        manifest.commands.host_check.argv.clear();
        manifest.exposures[0].lift_rpc.argv.clear();

        let errors = validate_manifest_shape(&manifest);
        assert!(errors
            .iter()
            .any(|error| error.contains("commands.hostCheck.argv is required")));
        assert!(errors
            .iter()
            .any(|error| error.contains("exposure `spring-web` liftRpc.argv is required")));
    }

    #[test]
    fn path_validation_rejects_escape_paths() {
        let specimen = tempdir().expect("create specimen root");
        let mut manifest = valid_manifest();
        manifest.paths.lab_library = "../outside".into();
        manifest.exposures[0].proof_ir_file = "/tmp/expected.proofir.json".into();

        let errors = validate_paths(specimen.path(), &manifest);
        assert!(errors
            .iter()
            .any(|error| error.contains("invalid path `../outside` escapes specimen root")));
        assert!(errors.iter().any(|error| {
            error.contains("invalid path `/tmp/expected.proofir.json` escapes specimen root")
        }));
    }

    #[test]
    fn path_validation_reports_missing_files() {
        let specimen = tempdir().expect("create specimen root");
        let manifest = valid_manifest();
        create_valid_paths(specimen.path(), &manifest);
        fs::remove_file(specimen.path().join(&manifest.exposure.sat_witness_file))
            .expect("remove sat witness");
        fs::remove_file(specimen.path().join(&manifest.exposures[0].proof_ir_file))
            .expect("remove proof ir");

        let errors = validate_paths(specimen.path(), &manifest);
        assert!(errors
            .iter()
            .any(|error| error.contains("sat-witness.json")));
        assert!(errors
            .iter()
            .any(|error| error.contains("expected.proofir.json")));
    }

    #[test]
    fn missing_specimen_manifest_returns_user_error() {
        let specimen = tempdir().expect("create specimen root");
        let code = run(ZooArgs {
            specimen: Some(specimen.path().to_path_buf()),
            all: false,
            out: OutputFlags {
                json: true,
                quiet: true,
            },
        });

        assert_eq!(code, EXIT_USER_ERROR);
    }
}
