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

const EXIT_OK: u8 = 0;
const EXIT_VERIFY_FAIL: u8 = 1;
const EXIT_USER_ERROR: u8 = 2;

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
    name = "provekit-bug-zoo",
    version,
    about = "Run self-contained Bug Zoo specimens and verify their witnessed ProofIR shape.",
    long_about = "Bug Zoo is ProvekIt's executable laboratory. It runs each specimen with the \
specimen's own host toolchain, invokes the language lifter RPC, and verifies the \
canonical ProofIR bytes and CIDs for each Green/Red/Green bug story."
)]
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
    status: String,
    predicates: Predicates,
    languages: Vec<LanguageSpecimen>,
    #[serde(default)]
    wild_sightings: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LanguageSpecimen {
    id: String,
    surface: String,
    paths: SpecimenPaths,
    commands: SpecimenCommands,
    #[serde(rename = "exhibits")]
    exhibits: Vec<Exposure>,
    equivalence: Equivalence,
    exposure: ExposureFiles,
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
    fixed: FixedExposure,
    lossiness: Lossiness,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixedExposure {
    harness: PathBuf,
    kit_rpc: PathBuf,
    lift_rpc: CommandSpec,
    proof_ir_file: PathBuf,
    diagnostic_file: PathBuf,
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
struct ExposureFiles {
    sat_witness_file: PathBuf,
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

    let path = args
        .specimen
        .clone()
        .unwrap_or_else(|| PathBuf::from("bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence"));
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

    let mut language_reports = Vec::new();
    let mut all_cids = BTreeMap::new();

    for language in &manifest.languages {
        run_host_check(specimen_dir, &language.commands.host_check).map_err(|error| {
            ZooError::verify(format!(
                "language `{}` hostCheck failed: {error}",
                language.id
            ))
        })?;

        let mut cids = BTreeMap::new();
        let mut fixed_cids = BTreeMap::new();
        for exhibit in &language.exhibits {
            let lifted = invoke_lift_rpc(
                specimen_dir,
                &exhibit.id,
                &exhibit.surface,
                &exhibit.harness,
                &exhibit.lift_rpc,
            )
            .map_err(|error| {
                ZooError::verify(format!(
                    "language `{}` exhibit `{}` lift failed: {error}",
                    language.id, exhibit.id
                ))
            })?;
            let expected =
                read_json(specimen_dir.join(&exhibit.proof_ir_file)).map_err(|error| {
                    ZooError::setup(format!(
                        "language `{}` exhibit `{}` expected ProofIR read failed: {error}",
                        language.id, exhibit.id
                    ))
                })?;
            let lifted_ir = lifted.get("ir").cloned().ok_or_else(|| {
                ZooError::verify(format!(
                    "language `{}` exhibit `{}` missing ir",
                    language.id, exhibit.id
                ))
            })?;
            let lifted_cid = proof_ir_cid(&lifted_ir).map_err(|error| {
                ZooError::verify(format!(
                    "language `{}` exhibit `{}` lifted ProofIR CID failed: {error}",
                    language.id, exhibit.id
                ))
            })?;
            let expected_cid = proof_ir_cid(&expected).map_err(|error| {
                ZooError::setup(format!(
                    "language `{}` exhibit `{}` expected ProofIR CID failed: {error}",
                    language.id, exhibit.id
                ))
            })?;
            if lifted_cid != expected_cid {
                return Err(ZooError::verify(format!(
                    "language `{}` exhibit `{}` ProofIR CID mismatch: lifted {lifted_cid}, expected {expected_cid}",
                    language.id, exhibit.id
                )));
            }

            let diagnostic_path = specimen_dir.join(&exhibit.diagnostic_file);
            let diag = std::fs::read_to_string(&diagnostic_path).map_err(|e| {
                ZooError::verify(format!(
                    "language `{}` exhibit `{}` diagnostic read failed at {}: {e}",
                    language.id,
                    exhibit.id,
                    diagnostic_path.display(),
                ))
            })?;
            expect_red_diagnostic(&diag, &manifest.predicates, &language.id, &exhibit.id)
                .map_err(ZooError::verify)?;

            cids.insert(exhibit.id.clone(), lifted_cid.clone());
            all_cids.insert(
                format!("{}:exhibit:{}", language.id, exhibit.id),
                lifted_cid,
            );

            let fixed = &exhibit.fixed;
            let fixed_lifted = invoke_lift_rpc(
                specimen_dir,
                &exhibit.id,
                &exhibit.surface,
                &fixed.harness,
                &fixed.lift_rpc,
            )
            .map_err(|error| {
                ZooError::verify(format!(
                    "language `{}` fixed `{}` lift failed: {error}",
                    language.id, exhibit.id
                ))
            })?;
            let fixed_expected =
                read_json(specimen_dir.join(&fixed.proof_ir_file)).map_err(|error| {
                    ZooError::setup(format!(
                        "language `{}` fixed `{}` expected ProofIR read failed: {error}",
                        language.id, exhibit.id
                    ))
                })?;
            let fixed_lifted_ir = fixed_lifted.get("ir").cloned().ok_or_else(|| {
                ZooError::verify(format!(
                    "language `{}` fixed `{}` missing ir",
                    language.id, exhibit.id
                ))
            })?;
            let fixed_lifted_cid = proof_ir_cid(&fixed_lifted_ir).map_err(|error| {
                ZooError::verify(format!(
                    "language `{}` fixed `{}` lifted ProofIR CID failed: {error}",
                    language.id, exhibit.id
                ))
            })?;
            let fixed_expected_cid = proof_ir_cid(&fixed_expected).map_err(|error| {
                ZooError::setup(format!(
                    "language `{}` fixed `{}` expected ProofIR CID failed: {error}",
                    language.id, exhibit.id
                ))
            })?;
            if fixed_lifted_cid != fixed_expected_cid {
                return Err(ZooError::verify(format!(
                    "language `{}` fixed `{}` ProofIR CID mismatch: lifted {fixed_lifted_cid}, expected {fixed_expected_cid}",
                    language.id, exhibit.id
                )));
            }

            let fixed_diagnostic_path = specimen_dir.join(&fixed.diagnostic_file);
            let fixed_diag = std::fs::read_to_string(&fixed_diagnostic_path).map_err(|e| {
                ZooError::verify(format!(
                    "language `{}` fixed `{}` diagnostic read failed at {}: {e}",
                    language.id,
                    exhibit.id,
                    fixed_diagnostic_path.display(),
                ))
            })?;
            expect_green_diagnostic(&fixed_diag, &manifest.predicates, &language.id, &exhibit.id)
                .map_err(ZooError::verify)?;

            fixed_cids.insert(exhibit.id.clone(), fixed_lifted_cid.clone());
            all_cids.insert(
                format!("{}:fixed:{}", language.id, exhibit.id),
                fixed_lifted_cid,
            );
        }

        for [left, right] in &language.equivalence.required {
            if cids.get(left) != cids.get(right) {
                return Err(ZooError::verify(format!(
                    "language `{}` equivalence failed: `{left}` CID {:?} != `{right}` CID {:?}",
                    language.id,
                    cids.get(left),
                    cids.get(right)
                )));
            }
        }

        let sat_witness = read_json(specimen_dir.join(&language.exposure.sat_witness_file))
            .map_err(|error| {
                ZooError::setup(format!(
                    "language `{}` sat witness read failed: {error}",
                    language.id
                ))
            })?;
        if !quiet {
            println!("zoo: {} {} hostCheck PASS", manifest.id, language.id);
            for (id, cid) in &cids {
                println!("zoo: {} exhibit {id} {cid}", language.id);
            }
            for (id, cid) in &fixed_cids {
                println!("zoo: {} fixed {id} {cid}", language.id);
            }
            for [left, right] in &language.equivalence.required {
                println!("zoo: {} equivalence {left} == {right} PASS", language.id);
            }
            println!(
                "zoo: {} red diagnostic {} PASS",
                language.id, manifest.predicates.missing_edge
            );
            println!("zoo: {} fixed diagnostics clean PASS", language.id);
        }

        language_reports.push(json!({
            "id": language.id,
            "surface": language.surface,
            "proofIrCids": cids,
            "fixedProofIrCids": fixed_cids,
            "wildSightings": language.wild_sightings,
            "satWitness": sat_witness,
        }));
    }

    Ok(json!({
        "id": manifest.id,
        "name": manifest.name,
        "kingdom": manifest.kingdom,
        "status": manifest.status,
        "missingEdge": manifest.predicates.missing_edge,
        "proofIrCids": all_cids,
        "languages": language_reports,
        "wildSightings": manifest.wild_sightings,
    }))
}

fn validate_manifest_shape(manifest: &SpecimenManifest) -> Vec<String> {
    let mut errors = Vec::new();

    if manifest.id.trim().is_empty() {
        errors.push("id is required".into());
    }
    if manifest.name.trim().is_empty() {
        errors.push("name is required".into());
    }
    if manifest.kingdom.trim().is_empty() {
        errors.push("kingdom is required".into());
    }
    if manifest.status.trim().is_empty() {
        errors.push("status is required".into());
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
    if manifest.languages.is_empty() {
        errors.push("at least one language is required".into());
    }

    let mut language_ids = BTreeSet::new();
    for language in &manifest.languages {
        if !language_ids.insert(language.id.clone()) {
            errors.push(format!("duplicate language id `{}`", language.id));
        }
        errors.extend(validate_language_shape(language));
    }

    errors
}

fn validate_language_shape(language: &LanguageSpecimen) -> Vec<String> {
    let mut errors = Vec::new();

    if language.id.trim().is_empty() {
        errors.push("language.id is required".into());
    }
    if language.surface.trim().is_empty() {
        errors.push(format!("language `{}` surface is required", language.id));
    }
    if language.exhibits.is_empty() {
        errors.push(format!(
            "language `{}` at least one exhibit is required",
            language.id
        ));
    }
    if language.commands.host_check.argv.is_empty() {
        errors.push(format!(
            "language `{}` commands.hostCheck.argv is required",
            language.id
        ));
    }

    let mut exhibit_ids = BTreeSet::new();
    for exhibit in &language.exhibits {
        if !exhibit_ids.insert(exhibit.id.clone()) {
            errors.push(format!(
                "language `{}` duplicate exhibit id `{}`",
                language.id, exhibit.id
            ));
        }
        if exhibit.lift_rpc.argv.is_empty() {
            errors.push(format!(
                "language `{}` exhibit `{}` liftRpc.argv is required",
                language.id, exhibit.id
            ));
        }
        if exhibit.fixed.lift_rpc.argv.is_empty() {
            errors.push(format!(
                "language `{}` fixed `{}` liftRpc.argv is required",
                language.id, exhibit.id
            ));
        }
        if exhibit.lossiness.erased.is_empty() || exhibit.lossiness.preserved.is_empty() {
            errors.push(format!(
                "language `{}` exhibit `{}` must describe lossiness erased and preserved boundaries",
                language.id, exhibit.id
            ));
        }
    }

    for [left, right] in &language.equivalence.required {
        if !exhibit_ids.contains(left) {
            errors.push(format!(
                "language `{}` equivalence references unknown exhibit `{left}`",
                language.id
            ));
        }
        if !exhibit_ids.contains(right) {
            errors.push(format!(
                "language `{}` equivalence references unknown exhibit `{right}`",
                language.id
            ));
        }
    }

    errors
}

fn validate_paths(specimen_dir: &Path, manifest: &SpecimenManifest) -> Vec<String> {
    let mut errors = Vec::new();
    for language in &manifest.languages {
        errors.extend(validate_language_paths(specimen_dir, language));
    }
    errors
}

fn validate_language_paths(specimen_dir: &Path, language: &LanguageSpecimen) -> Vec<String> {
    let mut errors = Vec::new();
    for path in [
        &language.paths.lab_library,
        &language.paths.lab_harness,
        &language.paths.lab_kit_rpc,
        &language.exposure.sat_witness_file,
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

    for path in [&language.commands.host_check.cwd] {
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

    for exhibit in &language.exhibits {
        for path in [
            &exhibit.harness,
            &exhibit.kit_rpc,
            &exhibit.proof_ir_file,
            &exhibit.diagnostic_file,
            &exhibit.lift_rpc.cwd,
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
        for path in [
            &exhibit.fixed.harness,
            &exhibit.fixed.kit_rpc,
            &exhibit.fixed.proof_ir_file,
            &exhibit.fixed.diagnostic_file,
            &exhibit.fixed.lift_rpc.cwd,
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

fn expect_red_diagnostic(
    diagnostic: &str,
    predicates: &Predicates,
    language_id: &str,
    exhibit_id: &str,
) -> Result<(), String> {
    if diagnostic.contains(&predicates.missing_edge) {
        return Ok(());
    }
    Err(format!(
        "language `{language_id}` exhibit `{exhibit_id}` diagnostic does not mention missing edge `{}`",
        predicates.missing_edge
    ))
}

fn expect_green_diagnostic(
    diagnostic: &str,
    predicates: &Predicates,
    language_id: &str,
    exhibit_id: &str,
) -> Result<(), String> {
    if !diagnostic.contains(&predicates.missing_edge) {
        return Ok(());
    }
    Err(format!(
        "language `{language_id}` fixed `{exhibit_id}` diagnostic still mentions missing edge `{}`",
        predicates.missing_edge
    ))
}

fn invoke_lift_rpc(
    specimen_dir: &Path,
    id: &str,
    surface: &str,
    harness: &Path,
    lift_rpc: &CommandSpec,
) -> Result<Value, String> {
    if lift_rpc.argv.is_empty() {
        return Err(format!("`{id}` liftRpc.argv is required"));
    }
    if manifest_path_escapes_specimen_root(&lift_rpc.cwd)
        || manifest_path_escapes_specimen_root(harness)
    {
        return Err(format!("`{id}` contains a path that escapes specimen root"));
    }

    let cwd = specimen_dir.join(&lift_rpc.cwd);
    let harness_path = specimen_dir.join(harness);
    let harness_root = harness_path.canonicalize().unwrap_or(harness_path);
    let harness_root = harness_root.to_string_lossy().to_string();
    let mut cmd = Command::new(&lift_rpc.argv[0]);
    cmd.args(&lift_rpc.argv[1..])
        .current_dir(&cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("spawn lift RPC for `{id}` in {}: {e}", cwd.display(),))?;
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
        invoke_lift_rpc_exchange(&mut stdin, &mut reader, &harness_root, id, surface);
    drop(reader);
    cleanup_rpc_child(&mut child, Some(stdin), exchange_result.is_ok());
    exchange_result
}

fn invoke_lift_rpc_exchange(
    stdin: &mut ChildStdin,
    reader: &mut impl BufRead,
    harness_root: &str,
    id: &str,
    surface: &str,
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
            "surface": surface,
            "workspace_root": harness_root,
            "source_paths": [harness_root],
            "options": {"layer": "all"},
        },
    });
    writeln!(stdin, "{lift_req}").map_err(|e| format!("write lift: {e}"))?;
    let lift_resp = read_response(reader, 2)?;

    match lift_resp.get("kind").and_then(|value| value.as_str()) {
        Some("ir-document") => Ok(lift_resp),
        other => Err(format!("`{id}` returned unsupported lift kind {:?}", other)),
    }
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
    canonical_json_cid(value)
}

fn canonical_json_cid(value: &Value) -> Result<String, String> {
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
            name: "Null Boundary Equivalence".into(),
            kingdom: "shape".into(),
            status: "lab".into(),
            predicates: Predicates {
                boundary: "maybe_null(name)".into(),
                sink: "non_null(name)".into(),
                missing_edge: "maybe_null(name) => non_null(name)".into(),
            },
            languages: vec![LanguageSpecimen {
                id: "java".into(),
                surface: "java".into(),
                paths: SpecimenPaths {
                    lab_library: "java/lab/library".into(),
                    lab_harness: "java/lab/harness".into(),
                    lab_kit_rpc: "java/lab/kit-rpc".into(),
                },
                commands: SpecimenCommands {
                    host_check: CommandSpec {
                        cwd: "java/lab/harness".into(),
                        argv: vec!["./run.sh".into()],
                    },
                },
                exhibits: vec![Exposure {
                    id: "spring-web".into(),
                    surface: "java-spring-web".into(),
                    harness: "java/exhibit/spring-web/harness".into(),
                    kit_rpc: "java/exhibit/spring-web/kit-rpc".into(),
                    lift_rpc: CommandSpec {
                        cwd: "java/exhibit/spring-web/kit-rpc".into(),
                        argv: vec!["./run-java-lifter.sh".into()],
                    },
                    proof_ir_file: "java/exhibit/spring-web/expected.proofir.json".into(),
                    diagnostic_file: "java/exhibit/spring-web/expected-diagnostic.txt".into(),
                    fixed: valid_fixed_exposure(),
                    lossiness: Lossiness {
                        erased: vec!["Spring binding".into()],
                        preserved: vec!["precondition neq(name, null)".into()],
                    },
                }],
                equivalence: Equivalence { required: vec![] },
                exposure: ExposureFiles {
                    sat_witness_file: "java/exhibit/sat-witness.json".into(),
                },
                wild_sightings: vec![],
            }],
            wild_sightings: vec![],
        }
    }

    fn valid_fixed_exposure() -> FixedExposure {
        FixedExposure {
            harness: "java/fixed/spring-web/harness".into(),
            kit_rpc: "java/fixed/spring-web/kit-rpc".into(),
            lift_rpc: CommandSpec {
                cwd: "java/fixed/spring-web/kit-rpc".into(),
                argv: vec!["./run-java-lifter.sh".into()],
            },
            proof_ir_file: "java/fixed/spring-web/expected.proofir.json".into(),
            diagnostic_file: "java/fixed/spring-web/expected-diagnostic.txt".into(),
        }
    }

    fn create_valid_paths(specimen_dir: &Path, manifest: &SpecimenManifest) {
        let language = &manifest.languages[0];
        let exhibit = &language.exhibits[0];
        for path in [
            &language.paths.lab_library,
            &language.paths.lab_harness,
            &language.paths.lab_kit_rpc,
            &exhibit.harness,
            &exhibit.kit_rpc,
            &exhibit.fixed.harness,
            &exhibit.fixed.kit_rpc,
        ] {
            fs::create_dir_all(specimen_dir.join(path)).expect("create directory");
        }

        for path in [
            &language.exposure.sat_witness_file,
            &exhibit.proof_ir_file,
            &exhibit.diagnostic_file,
            &exhibit.fixed.proof_ir_file,
            &exhibit.fixed.diagnostic_file,
        ] {
            let path = specimen_dir.join(path);
            fs::create_dir_all(path.parent().expect("fixture path has parent"))
                .expect("create file parent");
            fs::write(path, "").expect("create file");
        }
    }

    #[test]
    fn parses_manifest_with_exhibits_and_equivalence() {
        let raw = r#"
id: BZ-SHAPE-005
name: Null Boundary Equivalence
kingdom: shape
status: lab
predicates:
  boundary: maybe_null(name)
  sink: non_null(name)
  missingEdge: maybe_null(name) => non_null(name)
languages:
  - id: java
    surface: java-provekit-native-and-spring-web
    paths:
      labLibrary: java/lab/library
      labHarness: java/lab/harness
      labKitRpc: java/lab/kit-rpc
    commands:
      hostCheck:
        cwd: java/lab/harness
        argv: ["./run.sh"]
    exhibits:
      - id: provekit-native
        surface: java-provekit-native
        harness: java/exhibit/provekit-native/harness
        kitRpc: java/exhibit/provekit-native/kit-rpc
        liftRpc:
          cwd: java/exhibit/provekit-native/kit-rpc
          argv: ["./run-java-lifter.sh"]
        proofIrFile: java/exhibit/provekit-native/expected.proofir.json
        diagnosticFile: java/exhibit/provekit-native/expected-diagnostic.txt
        fixed:
          harness: java/fixed/provekit-native/harness
          kitRpc: java/fixed/provekit-native/kit-rpc
          liftRpc:
            cwd: java/fixed/provekit-native/kit-rpc
            argv: ["./run-java-lifter.sh"]
          proofIrFile: java/fixed/provekit-native/expected.proofir.json
          diagnosticFile: java/fixed/provekit-native/expected-diagnostic.txt
        lossiness:
          erased: ["Java body"]
          preserved: ["precondition neq(name, null)"]
      - id: spring-web
        surface: java-spring-web
        harness: java/exhibit/spring-web/harness
        kitRpc: java/exhibit/spring-web/kit-rpc
        liftRpc:
          cwd: java/exhibit/spring-web/kit-rpc
          argv: ["./run-java-lifter.sh"]
        proofIrFile: java/exhibit/spring-web/expected.proofir.json
        diagnosticFile: java/exhibit/spring-web/expected-diagnostic.txt
        fixed:
          harness: java/fixed/spring-web/harness
          kitRpc: java/fixed/spring-web/kit-rpc
          liftRpc:
            cwd: java/fixed/spring-web/kit-rpc
            argv: ["./run-java-lifter.sh"]
          proofIrFile: java/fixed/spring-web/expected.proofir.json
          diagnosticFile: java/fixed/spring-web/expected-diagnostic.txt
        lossiness:
          erased: ["Spring binding"]
          preserved: ["precondition neq(name, null)"]
    equivalence:
      required:
        - [provekit-native, spring-web]
    exposure:
      satWitnessFile: java/exhibit/sat-witness.json
wildSightings: []
"#;
        let manifest: SpecimenManifest = serde_yaml::from_str(raw).expect("parse manifest");
        assert_eq!(manifest.id, "BZ-SHAPE-005");
        assert_eq!(manifest.languages[0].exhibits.len(), 2);
        assert_eq!(
            manifest.languages[0].equivalence.required[0],
            ["provekit-native", "spring-web"]
        );
    }

    #[test]
    fn parses_exhibit_with_fixed_green_pair() {
        let raw = r#"
id: BZ-SHAPE-005
name: Null Boundary Equivalence
kingdom: shape
status: lab
predicates:
  boundary: maybe_null(name)
  sink: non_null(name)
  missingEdge: maybe_null(name) => non_null(name)
languages:
  - id: java
    surface: java-provekit-native
    paths:
      labLibrary: java/lab/library
      labHarness: java/lab/harness
      labKitRpc: java/lab/kit-rpc
    commands:
      hostCheck:
        cwd: java/lab/harness
        argv: ["./run.sh"]
    exhibits:
      - id: provekit-native
        surface: java-provekit-native
        harness: java/exhibit/provekit-native/harness
        kitRpc: java/exhibit/provekit-native/kit-rpc
        liftRpc:
          cwd: java/exhibit/provekit-native/kit-rpc
          argv: ["./run-java-lifter.sh"]
        proofIrFile: java/exhibit/provekit-native/expected.proofir.json
        diagnosticFile: java/exhibit/provekit-native/expected-diagnostic.txt
        fixed:
          harness: java/fixed/provekit-native/harness
          kitRpc: java/fixed/provekit-native/kit-rpc
          liftRpc:
            cwd: java/fixed/provekit-native/kit-rpc
            argv: ["./run-java-lifter.sh"]
          proofIrFile: java/fixed/provekit-native/expected.proofir.json
          diagnosticFile: java/fixed/provekit-native/expected-diagnostic.txt
        lossiness:
          erased: ["Java body"]
          preserved: ["precondition neq(name, null)"]
    equivalence:
      required: []
    exposure:
      satWitnessFile: java/exhibit/sat-witness.json
wildSightings: []
"#;
        let manifest: SpecimenManifest = serde_yaml::from_str(raw).expect("parse manifest");
        let fixed = &manifest.languages[0].exhibits[0].fixed;

        assert_eq!(
            fixed.harness,
            PathBuf::from("java/fixed/provekit-native/harness")
        );
        assert_eq!(
            fixed.proof_ir_file,
            PathBuf::from("java/fixed/provekit-native/expected.proofir.json")
        );
    }

    #[test]
    fn diagnostics_encode_red_exhibit_and_green_fixed_pair() {
        let predicates = Predicates {
            boundary: "maybe_null(name)".into(),
            sink: "non_null(name)".into(),
            missing_edge: "maybe_null(name) => non_null(name)".into(),
        };

        expect_red_diagnostic(
            "missing edge: maybe_null(name) => non_null(name)",
            &predicates,
            "java",
            "provekit-native",
        )
        .expect("red exhibit diagnostic should mention the missing edge");

        expect_green_diagnostic(
            "clean: null boundary closed",
            &predicates,
            "java",
            "provekit-native",
        )
        .expect("green fixed diagnostic should not mention the missing edge");

        let err = expect_green_diagnostic(
            "still missing maybe_null(name) => non_null(name)",
            &predicates,
            "java",
            "provekit-native",
        )
        .expect_err("green diagnostic must reject the missing edge");
        assert!(err.contains("fixed `provekit-native` diagnostic still mentions missing edge"));
    }

    #[test]
    fn parses_species_manifest_with_language_exhibits() {
        let raw = r#"
id: BZ-SHAPE-005
name: Null Boundary Equivalence
kingdom: shape
status: lab
predicates:
  boundary: maybe_null(name)
  sink: non_null(name)
  missingEdge: maybe_null(name) => non_null(name)
languages:
  - id: java
    surface: java-provekit-native-and-spring-web
    paths:
      labLibrary: java/lab/library
      labHarness: java/lab/harness
      labKitRpc: java/lab/kit-rpc
    commands:
      hostCheck:
        cwd: java/lab/harness
        argv: ["./run.sh"]
    exhibits:
      - id: provekit-native
        surface: java-provekit-native
        harness: java/exhibit/provekit-native/harness
        kitRpc: java/exhibit/provekit-native/kit-rpc
        liftRpc:
          cwd: java/exhibit/provekit-native/kit-rpc
          argv: ["./run-java-lifter.sh"]
        proofIrFile: java/exhibit/provekit-native/expected.proofir.json
        diagnosticFile: java/exhibit/provekit-native/expected-diagnostic.txt
        fixed:
          harness: java/fixed/provekit-native/harness
          kitRpc: java/fixed/provekit-native/kit-rpc
          liftRpc:
            cwd: java/fixed/provekit-native/kit-rpc
            argv: ["./run-java-lifter.sh"]
          proofIrFile: java/fixed/provekit-native/expected.proofir.json
          diagnosticFile: java/fixed/provekit-native/expected-diagnostic.txt
        lossiness:
          erased: ["Java body"]
          preserved: ["precondition neq(name, null)"]
    equivalence:
      required: []
    exposure:
      satWitnessFile: java/exhibit/sat-witness.json
  - id: typescript
    surface: typescript-zod-and-class-validator
    paths:
      labLibrary: typescript/lab/library
      labHarness: typescript/lab/harness
      labKitRpc: typescript/lab/kit-rpc
    commands:
      hostCheck:
        cwd: typescript/lab/harness
        argv: ["./run.sh"]
    exhibits:
      - id: zod
        surface: typescript-zod
        harness: typescript/exhibit/zod/harness
        kitRpc: typescript/exhibit/zod/kit-rpc
        liftRpc:
          cwd: typescript/exhibit/zod/kit-rpc
          argv: ["./run-ts-lifter.sh"]
        proofIrFile: typescript/exhibit/zod/expected.proofir.json
        diagnosticFile: typescript/exhibit/zod/expected-diagnostic.txt
        fixed:
          harness: typescript/fixed/zod/harness
          kitRpc: typescript/fixed/zod/kit-rpc
          liftRpc:
            cwd: typescript/fixed/zod/kit-rpc
            argv: ["./run-ts-lifter.sh"]
          proofIrFile: typescript/fixed/zod/expected.proofir.json
          diagnosticFile: typescript/fixed/zod/expected-diagnostic.txt
        lossiness:
          erased: ["TypeScript body"]
          preserved: ["precondition neq(name, null)"]
    equivalence:
      required: []
    exposure:
      satWitnessFile: typescript/exhibit/sat-witness.json
wildSightings: []
"#;
        let manifest: SpecimenManifest = serde_yaml::from_str(raw).expect("parse manifest");

        assert_eq!(manifest.id, "BZ-SHAPE-005");
        assert_eq!(manifest.languages.len(), 2);
        assert_eq!(manifest.languages[0].id, "java");
        assert_eq!(manifest.languages[0].exhibits[0].id, "provekit-native");
        assert_eq!(manifest.languages[1].id, "typescript");
        assert_eq!(
            manifest.languages[1].exhibits[0].harness,
            PathBuf::from("typescript/exhibit/zod/harness")
        );
    }

    #[test]
    fn validation_rejects_missing_lossiness() {
        let mut manifest = valid_manifest();
        manifest.languages[0].exhibits[0].lossiness.erased.clear();

        let errors = validate_manifest_shape(&manifest);
        assert!(errors.iter().any(|e| e.contains("lossiness")));
    }

    #[test]
    fn validation_rejects_duplicate_exhibit_ids() {
        let mut manifest = valid_manifest();
        manifest.languages[0].exhibits.push(Exposure {
            id: "spring-web".into(),
            surface: "java-provekit-native".into(),
            harness: "java/exhibit/provekit-native/harness".into(),
            kit_rpc: "java/exhibit/provekit-native/kit-rpc".into(),
            lift_rpc: CommandSpec {
                cwd: "java/exhibit/provekit-native/kit-rpc".into(),
                argv: vec!["./run-java-lifter.sh".into()],
            },
            proof_ir_file: "java/exhibit/provekit-native/expected.proofir.json".into(),
            diagnostic_file: "java/exhibit/provekit-native/expected-diagnostic.txt".into(),
            fixed: FixedExposure {
                harness: "java/fixed/provekit-native/harness".into(),
                kit_rpc: "java/fixed/provekit-native/kit-rpc".into(),
                lift_rpc: CommandSpec {
                    cwd: "java/fixed/provekit-native/kit-rpc".into(),
                    argv: vec!["./run-java-lifter.sh".into()],
                },
                proof_ir_file: "java/fixed/provekit-native/expected.proofir.json".into(),
                diagnostic_file: "java/fixed/provekit-native/expected-diagnostic.txt".into(),
            },
            lossiness: Lossiness {
                erased: vec!["Java body".into()],
                preserved: vec!["precondition neq(name, null)".into()],
            },
        });

        let errors = validate_manifest_shape(&manifest);
        assert!(errors
            .iter()
            .any(|error| error.contains("duplicate exhibit id `spring-web`")));
    }

    #[test]
    fn validation_rejects_unknown_equivalence_references() {
        let mut manifest = valid_manifest();
        manifest.languages[0].equivalence.required = vec![["spring-web".into(), "missing".into()]];

        let errors = validate_manifest_shape(&manifest);
        assert!(errors
            .iter()
            .any(|error| error.contains("equivalence references unknown exhibit `missing`")));
    }

    #[test]
    fn validation_rejects_empty_command_argv() {
        let mut manifest = valid_manifest();
        manifest.languages[0].commands.host_check.argv.clear();
        manifest.languages[0].exhibits[0].lift_rpc.argv.clear();

        let errors = validate_manifest_shape(&manifest);
        assert!(errors
            .iter()
            .any(|error| error.contains("commands.hostCheck.argv is required")));
        assert!(errors
            .iter()
            .any(|error| error.contains("exhibit `spring-web` liftRpc.argv is required")));
    }

    #[test]
    fn path_validation_rejects_escape_paths() {
        let specimen = tempdir().expect("create specimen root");
        let mut manifest = valid_manifest();
        manifest.languages[0].paths.lab_library = "../outside".into();
        manifest.languages[0].exhibits[0].proof_ir_file = "/tmp/expected.proofir.json".into();

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
        fs::remove_file(
            specimen
                .path()
                .join(&manifest.languages[0].exposure.sat_witness_file),
        )
        .expect("remove sat witness");
        fs::remove_file(
            specimen
                .path()
                .join(&manifest.languages[0].exhibits[0].proof_ir_file),
        )
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
