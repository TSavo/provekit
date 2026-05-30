// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use clap::Parser;
use provekit_verifier::{smt_emitter, ObligationVerdict, SolverPlan};
use serde::Deserialize;
use serde_json::{json, Value};

const EXIT_OK: u8 = 0;
const EXIT_VERIFY_FAIL: u8 = 1;
const EXIT_USER_ERROR: u8 = 2;
const PROVEKIT_CLI_ENV: &str = "PROVEKIT_CLI";
const PROVEKIT_BUG_ZOO_EXTERNAL_CLI_ENV: &str = "PROVEKIT_BUG_ZOO_EXTERNAL_CLI";
const FORMULA_PROVER_LABEL: &str = "provekit-verifier formula gate";

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
    about = "Run self-contained Bug Zoo specimens and verify their witnessed receipt shape.",
    long_about = "Bug Zoo is ProvekIt's executable laboratory. It runs each specimen with the \
specimen's own host toolchain, invokes the ProvekIt CLI lift/link path, and verifies the \
canonical ProofIR or LinkBundle bytes and CIDs for each Green/Red/Green bug story."
)]
pub struct ZooArgs {
    /// Specimen directory or specimen.yaml path. Defaults to menagerie/bug-zoo/species.
    pub specimen: Option<PathBuf>,
    /// Check every species under menagerie/bug-zoo/species.
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
    #[serde(default)]
    exhibits: Vec<Exposure>,
    #[serde(default)]
    link_exhibits: Vec<LinkExposure>,
    equivalence: Equivalence,
    exposure: ExposureFiles,
    #[serde(default)]
    composition: Option<Composition>,
    #[serde(default)]
    wild_sightings: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
struct SpecimenPaths {
    lab_library: PathBuf,
    lab_harness: PathBuf,
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
    proof_ir_file: PathBuf,
    diagnostic_file: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LinkExposure {
    id: String,
    surface: String,
    project: PathBuf,
    #[serde(default)]
    go_lsp_bin: Option<PathBuf>,
    link_bundle_file: PathBuf,
    diagnostic_file: PathBuf,
    fixed: FixedLinkExposure,
    lossiness: Lossiness,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixedLinkExposure {
    project: PathBuf,
    #[serde(default)]
    go_lsp_bin: Option<PathBuf>,
    link_bundle_file: PathBuf,
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

#[derive(Debug, Deserialize)]
struct Composition {
    checks: Vec<CompositionCheck>,
}

#[derive(Debug, Clone, Copy)]
struct ZooProofIrLiftContract<'a> {
    accepted_exhibit_lift: bool,
    accepted_fixed_lift: bool,
    exhibit_proof_ir_cid: &'a str,
    exhibit_proof_ir_json_cid: &'a str,
    fixed_proof_ir_cid: &'a str,
    fixed_proof_ir_json_cid: &'a str,
}

#[derive(Debug, Clone, Copy)]
struct ZooProofIrDocument<'a> {
    json_cid: &'a str,
}

#[allow(non_snake_case)]
impl<'a> ZooProofIrLiftContract<'a> {
    fn accepted_exhibit(proof_ir_cid: &'a str, proof_ir_json_cid: &'a str) -> Self {
        Self {
            accepted_exhibit_lift: true,
            accepted_fixed_lift: false,
            exhibit_proof_ir_cid: proof_ir_cid,
            exhibit_proof_ir_json_cid: proof_ir_json_cid,
            fixed_proof_ir_cid: "",
            fixed_proof_ir_json_cid: "",
        }
    }

    fn accepted_fixed(proof_ir_cid: &'a str, proof_ir_json_cid: &'a str) -> Self {
        Self {
            accepted_exhibit_lift: false,
            accepted_fixed_lift: true,
            exhibit_proof_ir_cid: "",
            exhibit_proof_ir_json_cid: "",
            fixed_proof_ir_cid: proof_ir_cid,
            fixed_proof_ir_json_cid: proof_ir_json_cid,
        }
    }

    fn acceptedExhibitLift(&self) -> bool {
        self.accepted_exhibit_lift
    }

    fn acceptedFixedLift(&self) -> bool {
        self.accepted_fixed_lift
    }

    fn exhibitProofIrCid(&self) -> &'a str {
        self.exhibit_proof_ir_cid
    }

    fn exhibitProofIr(&self) -> ZooProofIrDocument<'a> {
        ZooProofIrDocument {
            json_cid: self.exhibit_proof_ir_json_cid,
        }
    }

    fn fixedProofIrCid(&self) -> &'a str {
        self.fixed_proof_ir_cid
    }

    fn fixedProofIr(&self) -> ZooProofIrDocument<'a> {
        ZooProofIrDocument {
            json_cid: self.fixed_proof_ir_json_cid,
        }
    }
}

impl<'a> ZooProofIrDocument<'a> {
    fn json_document_cid(&self) -> &'a str {
        self.json_cid
    }
}

#[derive(Debug, Clone, Copy)]
struct ZooDiagnosticContract {
    accepted_fixed_diagnostic: bool,
    missing_edge_absent: bool,
}

#[allow(non_snake_case)]
impl ZooDiagnosticContract {
    fn accepted_fixed(diagnostic: &str, missing_edge: &str) -> Self {
        Self {
            accepted_fixed_diagnostic: true,
            missing_edge_absent: !diagnostic.contains(missing_edge),
        }
    }

    fn acceptedFixedDiagnostic(&self) -> bool {
        self.accepted_fixed_diagnostic
    }

    fn missingEdgeAbsent(&self) -> bool {
        self.missing_edge_absent
    }
}

#[allow(non_snake_case)]
fn zoo_exhibit_proofir_cid_is_derived(lift: &ZooProofIrLiftContract<'_>) {
    assert!(lift.exhibitProofIrCid() == lift.exhibitProofIr().json_document_cid());
}

#[allow(non_snake_case)]
fn zoo_fixed_proofir_cid_is_derived(lift: &ZooProofIrLiftContract<'_>) {
    assert!(lift.fixedProofIrCid() == lift.fixedProofIr().json_document_cid());
}

#[allow(non_snake_case)]
fn zoo_fixed_diagnostic_has_no_missing_edge(diagnostic: &ZooDiagnosticContract) {
    assert!(diagnostic.missingEdgeAbsent() == true);
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompositionCheck {
    id: String,
    phase: CompositionPhase,
    expected: CompositionExpected,
    #[serde(default)]
    witness_source: CompositionWitnessSource,
    witness_exhibit: String,
    #[serde(default)]
    requirement_exhibit: Option<String>,
    witness_formula: Value,
    requirement_formula: Value,
    #[serde(default)]
    bindings: BTreeMap<String, String>,
    assignment: BTreeMap<String, i64>,
    diagnostic_file: PathBuf,
}

#[derive(Debug, Deserialize, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
enum CompositionPhase {
    Exhibit,
    Fixed,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum CompositionExpected {
    Missing,
    Satisfied,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum CompositionWitnessSource {
    ProofIr,
    Lab,
}

impl Default for CompositionWitnessSource {
    fn default() -> Self {
        Self::ProofIr
    }
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
            .unwrap_or_else(|| PathBuf::from("menagerie/bug-zoo/species"));
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
        PathBuf::from("menagerie/bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence")
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
    let repo_root = find_repo_root(specimen_dir).map_err(ZooError::setup)?;

    let mut errors = validate_manifest_shape(&manifest);
    errors.extend(validate_paths(specimen_dir, &manifest));
    if !errors.is_empty() {
        return Err(ZooError::setup(errors.join("; ")));
    }

    let mut language_reports = Vec::new();
    let mut proof_ir_cids = BTreeMap::new();
    let mut receipt_cids = BTreeMap::new();

    for language in &manifest.languages {
        run_host_check(specimen_dir, &language.commands.host_check).map_err(|error| {
            ZooError::verify(format!(
                "language `{}` hostCheck failed: {error}",
                language.id
            ))
        })?;

        let mut cids = BTreeMap::new();
        let mut fixed_cids = BTreeMap::new();
        let mut link_bundle_cids = BTreeMap::new();
        let mut fixed_link_bundle_cids = BTreeMap::new();
        let mut exhibit_irs = BTreeMap::new();
        let mut fixed_irs = BTreeMap::new();
        for exhibit in &language.exhibits {
            let lifted = invoke_provekit_cli_lift(
                specimen_dir,
                &exhibit.id,
                &exhibit.surface,
                &exhibit.harness,
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
            let exhibit_lift_contract =
                ZooProofIrLiftContract::accepted_exhibit(&lifted_cid, &lifted_cid);
            if exhibit_lift_contract.acceptedExhibitLift() == true {
                zoo_exhibit_proofir_cid_is_derived(&exhibit_lift_contract);
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
            proof_ir_cids.insert(
                format!("{}:exhibit:{}", language.id, exhibit.id),
                lifted_cid.clone(),
            );
            receipt_cids.insert(
                format!("{}:exhibit:{}", language.id, exhibit.id),
                lifted_cid,
            );
            exhibit_irs.insert(exhibit.id.clone(), lifted_ir);

            let fixed = &exhibit.fixed;
            let fixed_lifted = invoke_provekit_cli_lift(
                specimen_dir,
                &exhibit.id,
                &exhibit.surface,
                &fixed.harness,
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
            let fixed_lift_contract =
                ZooProofIrLiftContract::accepted_fixed(&fixed_lifted_cid, &fixed_lifted_cid);
            if fixed_lift_contract.acceptedFixedLift() == true {
                zoo_fixed_proofir_cid_is_derived(&fixed_lift_contract);
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
            proof_ir_cids.insert(
                format!("{}:fixed:{}", language.id, exhibit.id),
                fixed_lifted_cid.clone(),
            );
            receipt_cids.insert(
                format!("{}:fixed:{}", language.id, exhibit.id),
                fixed_lifted_cid,
            );
            fixed_irs.insert(exhibit.id.clone(), fixed_lifted_ir);
        }

        for exhibit in &language.link_exhibits {
            let expected =
                read_json(specimen_dir.join(&exhibit.link_bundle_file)).map_err(|error| {
                    ZooError::setup(format!(
                        "language `{}` link exhibit `{}` expected LinkBundle read failed: {error}",
                        language.id, exhibit.id
                    ))
                })?;
            let linked = read_checked_in_link_bundle(
                specimen_dir,
                &exhibit.id,
                &exhibit.project,
                exhibit.go_lsp_bin.as_deref(),
            )
            .map_err(|error| {
                ZooError::verify(format!(
                    "language `{}` link exhibit `{}` failed: {error}",
                    language.id, exhibit.id
                ))
            })?;
            if linked.bundle != expected {
                return Err(ZooError::verify(format!(
                    "language `{}` link exhibit `{}` LinkBundle mismatch: linked {}, expected {:?}",
                    language.id,
                    exhibit.id,
                    linked.cid,
                    expected.get("linkBundleCid")
                )));
            }
            if linked.exit_code != EXIT_VERIFY_FAIL
                || !linked.has_linker_error_kind("unprovable-obligation")
            {
                return Err(ZooError::verify(format!(
                    "language `{}` link exhibit `{}` expected checked-in LinkBundle to record an unprovable obligation, but exit was {} and error kinds were {:?}",
                    language.id, exhibit.id, linked.exit_code, linked.error_kinds
                )));
            }
            let diagnostic_path = specimen_dir.join(&exhibit.diagnostic_file);
            let diag = std::fs::read_to_string(&diagnostic_path).map_err(|e| {
                ZooError::verify(format!(
                    "language `{}` link exhibit `{}` diagnostic read failed at {}: {e}",
                    language.id,
                    exhibit.id,
                    diagnostic_path.display(),
                ))
            })?;
            expect_red_diagnostic(&diag, &manifest.predicates, &language.id, &exhibit.id)
                .map_err(ZooError::verify)?;

            link_bundle_cids.insert(exhibit.id.clone(), linked.cid.clone());
            receipt_cids.insert(
                format!("{}:link-exhibit:{}", language.id, exhibit.id),
                linked.cid,
            );

            let fixed = &exhibit.fixed;
            let fixed_expected =
                read_json(specimen_dir.join(&fixed.link_bundle_file)).map_err(|error| {
                    ZooError::setup(format!(
                        "language `{}` fixed link `{}` expected LinkBundle read failed: {error}",
                        language.id, exhibit.id
                    ))
                })?;
            let fixed_go_lsp_bin = fixed
                .go_lsp_bin
                .as_deref()
                .or(exhibit.go_lsp_bin.as_deref());
            let fixed_linked = read_checked_in_link_bundle(
                specimen_dir,
                &format!("{}-fixed", exhibit.id),
                &fixed.project,
                fixed_go_lsp_bin,
            )
            .map_err(|error| {
                ZooError::verify(format!(
                    "language `{}` fixed link `{}` failed: {error}",
                    language.id, exhibit.id
                ))
            })?;
            if fixed_linked.bundle != fixed_expected {
                return Err(ZooError::verify(format!(
                    "language `{}` fixed link `{}` LinkBundle mismatch: linked {}, expected {:?}",
                    language.id,
                    exhibit.id,
                    fixed_linked.cid,
                    fixed_expected.get("linkBundleCid")
                )));
            }
            if fixed_linked.exit_code != EXIT_OK || fixed_linked.linker_error_count != 0 {
                return Err(ZooError::verify(format!(
                    "language `{}` fixed link `{}` expected clean checked-in LinkBundle, but exit was {} with {} linker error(s)",
                    language.id, exhibit.id, fixed_linked.exit_code, fixed_linked.linker_error_count
                )));
            }
            let fixed_diagnostic_path = specimen_dir.join(&fixed.diagnostic_file);
            let fixed_diag = std::fs::read_to_string(&fixed_diagnostic_path).map_err(|e| {
                ZooError::verify(format!(
                    "language `{}` fixed link `{}` diagnostic read failed at {}: {e}",
                    language.id,
                    exhibit.id,
                    fixed_diagnostic_path.display(),
                ))
            })?;
            expect_green_diagnostic(&fixed_diag, &manifest.predicates, &language.id, &exhibit.id)
                .map_err(ZooError::verify)?;

            fixed_link_bundle_cids.insert(exhibit.id.clone(), fixed_linked.cid.clone());
            receipt_cids.insert(
                format!("{}:fixed-link:{}", language.id, exhibit.id),
                fixed_linked.cid,
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

        let composition_reports = verify_composition_checks(
            specimen_dir,
            language,
            &manifest.predicates,
            &exhibit_irs,
            &fixed_irs,
        )
        .map_err(|error| {
            ZooError::verify(format!(
                "language `{}` composition failed: {error}",
                language.id
            ))
        })?;

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
            for (id, cid) in &link_bundle_cids {
                println!("zoo: {} link exhibit {id} {cid}", language.id);
            }
            for (id, cid) in &fixed_link_bundle_cids {
                println!("zoo: {} fixed link {id} {cid}", language.id);
            }
            for [left, right] in &language.equivalence.required {
                println!("zoo: {} equivalence {left} == {right} PASS", language.id);
            }
            println!(
                "zoo: {} red diagnostic {} PASS",
                language.id, manifest.predicates.missing_edge
            );
            println!("zoo: {} fixed diagnostics clean PASS", language.id);
            for report in &composition_reports {
                println!(
                    "zoo: {} composition {} {} PASS",
                    language.id,
                    report["id"].as_str().unwrap_or("check"),
                    report["expected"].as_str().unwrap_or("unknown")
                );
            }
        }

        language_reports.push(json!({
            "id": language.id,
            "surface": language.surface,
            "lab": {
                "hostCheck": "passed",
                "provekitWorkflow": "none",
            },
            "proofIrCids": cids,
            "fixedProofIrCids": fixed_cids,
            "linkBundleCids": link_bundle_cids,
            "fixedLinkBundleCids": fixed_link_bundle_cids,
            "composition": composition_reports,
            "wildSightings": language.wild_sightings,
            "satWitness": sat_witness,
        }));
    }

    Ok(json!({
        "id": manifest.id,
        "name": manifest.name,
        "kingdom": manifest.kingdom,
        "status": manifest.status,
        "workflow": {
            "runner": "provekit-bug-zoo",
            "provekitCli": provekit_cli_report(&repo_root),
        },
        "missingEdge": manifest.predicates.missing_edge,
        "proofIrCids": proof_ir_cids,
        "receiptCids": receipt_cids,
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
    if language.exhibits.is_empty() && language.link_exhibits.is_empty() {
        errors.push(format!(
            "language `{}` at least one exhibit or linkExhibit is required",
            language.id
        ));
    }
    if language.commands.host_check.argv.is_empty() {
        errors.push(format!(
            "language `{}` commands.hostCheck.argv is required",
            language.id
        ));
    }

    let mut proof_exhibit_ids = BTreeSet::new();
    let mut all_exhibit_ids = BTreeSet::new();
    for exhibit in &language.exhibits {
        proof_exhibit_ids.insert(exhibit.id.clone());
        if !all_exhibit_ids.insert(exhibit.id.clone()) {
            errors.push(format!(
                "language `{}` duplicate exhibit id `{}`",
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
    for exhibit in &language.link_exhibits {
        if !all_exhibit_ids.insert(exhibit.id.clone()) {
            errors.push(format!(
                "language `{}` duplicate exhibit id `{}`",
                language.id, exhibit.id
            ));
        }
        if exhibit.id.trim().is_empty() {
            errors.push(format!(
                "language `{}` link exhibit id is required",
                language.id
            ));
        }
        if exhibit.surface.trim().is_empty() {
            errors.push(format!(
                "language `{}` link exhibit `{}` surface is required",
                language.id, exhibit.id
            ));
        }
        if exhibit.lossiness.erased.is_empty() || exhibit.lossiness.preserved.is_empty() {
            errors.push(format!(
                "language `{}` link exhibit `{}` must describe lossiness erased and preserved boundaries",
                language.id, exhibit.id
            ));
        }
    }

    for [left, right] in &language.equivalence.required {
        if !proof_exhibit_ids.contains(left) {
            errors.push(format!(
                "language `{}` equivalence references unknown ProofIR exhibit `{left}`",
                language.id
            ));
        }
        if !proof_exhibit_ids.contains(right) {
            errors.push(format!(
                "language `{}` equivalence references unknown ProofIR exhibit `{right}`",
                language.id
            ));
        }
    }

    if let Some(composition) = &language.composition {
        if composition.checks.is_empty() {
            errors.push(format!(
                "language `{}` composition.checks must not be empty",
                language.id
            ));
        }
        let mut check_ids = BTreeSet::new();
        for check in &composition.checks {
            if check.id.trim().is_empty() {
                errors.push(format!(
                    "language `{}` composition check id is required",
                    language.id
                ));
            } else if !check_ids.insert(check.id.clone()) {
                errors.push(format!(
                    "language `{}` duplicate composition check id `{}`",
                    language.id, check.id
                ));
            }
            if !proof_exhibit_ids.contains(&check.witness_exhibit) {
                errors.push(format!(
                    "language `{}` composition check `{}` references unknown ProofIR witness exhibit `{}`",
                    language.id, check.id, check.witness_exhibit
                ));
            }
            if let Some(requirement_exhibit) = &check.requirement_exhibit {
                if !proof_exhibit_ids.contains(requirement_exhibit) {
                    errors.push(format!(
                        "language `{}` composition check `{}` references unknown ProofIR requirement exhibit `{}`",
                        language.id, check.id, requirement_exhibit
                    ));
                }
            }
            if check.witness_formula.is_null() {
                errors.push(format!(
                    "language `{}` composition check `{}` witnessFormula is required",
                    language.id, check.id
                ));
            }
            if check.requirement_formula.is_null() {
                errors.push(format!(
                    "language `{}` composition check `{}` requirementFormula is required",
                    language.id, check.id
                ));
            }
            if check.assignment.is_empty() {
                errors.push(format!(
                    "language `{}` composition check `{}` assignment is required",
                    language.id, check.id
                ));
            }
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

        for harness in [&exhibit.harness, &exhibit.fixed.harness] {
            for path in [
                harness.join(".provekit/config.toml"),
                harness
                    .join(".provekit/lift")
                    .join(&exhibit.surface)
                    .join("manifest.toml"),
            ] {
                if manifest_path_escapes_specimen_root(&path) {
                    errors.push(format!(
                        "invalid path `{}` escapes specimen root",
                        path.display()
                    ));
                    continue;
                }
                let full_path = specimen_dir.join(&path);
                if !full_path.exists() {
                    errors.push(format!("missing {}", full_path.display()));
                }
            }
        }
    }

    for exhibit in &language.link_exhibits {
        for path in [
            Some(&exhibit.project),
            exhibit.go_lsp_bin.as_ref(),
            Some(&exhibit.link_bundle_file),
            Some(&exhibit.diagnostic_file),
            Some(&exhibit.fixed.project),
            exhibit.fixed.go_lsp_bin.as_ref(),
            Some(&exhibit.fixed.link_bundle_file),
            Some(&exhibit.fixed.diagnostic_file),
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
    }

    if let Some(composition) = &language.composition {
        for check in &composition.checks {
            if manifest_path_escapes_specimen_root(&check.diagnostic_file) {
                errors.push(format!(
                    "invalid path `{}` escapes specimen root",
                    check.diagnostic_file.display()
                ));
                continue;
            }
            let full_path = specimen_dir.join(&check.diagnostic_file);
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
    let started = Instant::now();
    trace_log(format!(
        "host-check start cwd={} argv={:?}",
        cwd.display(),
        command.argv
    ));
    let mut cmd = host_command(&command.argv[0]);
    let output = cmd
        .args(&command.argv[1..])
        .current_dir(&cwd)
        .output()
        .map_err(|e| format!("spawn {:?} in {}: {e}", command.argv, cwd.display()))?;
    trace_log(format!(
        "host-check exit cwd={} status={} elapsed={:?}",
        cwd.display(),
        output.status,
        started.elapsed()
    ));
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

fn verify_composition_checks(
    specimen_dir: &Path,
    language: &LanguageSpecimen,
    predicates: &Predicates,
    exhibit_irs: &BTreeMap<String, Value>,
    fixed_irs: &BTreeMap<String, Value>,
) -> Result<Vec<Value>, String> {
    let Some(composition) = &language.composition else {
        return Ok(Vec::new());
    };

    let mut reports = Vec::new();
    for check in &composition.checks {
        let phase_irs = match check.phase {
            CompositionPhase::Exhibit => exhibit_irs,
            CompositionPhase::Fixed => fixed_irs,
        };
        let witness_ir = phase_irs.get(&check.witness_exhibit).ok_or_else(|| {
            format!(
                "check `{}` missing witness exhibit `{}` for phase {:?}",
                check.id, check.witness_exhibit, check.phase
            )
        })?;
        let requirement_ir = match &check.requirement_exhibit {
            Some(requirement_exhibit) => {
                Some(phase_irs.get(requirement_exhibit).ok_or_else(|| {
                    format!(
                        "check `{}` missing requirement exhibit `{}` for phase {:?}",
                        check.id, requirement_exhibit, check.phase
                    )
                })?)
            }
            None => None,
        };

        if check.witness_source == CompositionWitnessSource::ProofIr
            && !json_contains(witness_ir, &check.witness_formula)
        {
            return Err(format!(
                "check `{}` witness formula is not present in lifted `{}` {:?} ProofIR",
                check.id, check.witness_exhibit, check.phase
            ));
        }
        if let Some(requirement_ir) = requirement_ir {
            if !json_contains(requirement_ir, &check.requirement_formula) {
                return Err(format!(
                    "check `{}` requirement formula is not present in lifted `{}` {:?} ProofIR",
                    check.id,
                    check
                        .requirement_exhibit
                        .as_deref()
                        .unwrap_or("requirement"),
                    check.phase
                ));
            }
        }

        let formula = scoped_implication_formula(check);
        let proof = invoke_provekit_formula_gate(specimen_dir, &check.id, &formula)?;
        let diagnostic_path = specimen_dir.join(&check.diagnostic_file);
        let diagnostic = std::fs::read_to_string(&diagnostic_path)
            .map_err(|e| format!("check `{}` diagnostic read failed: {e}", check.id))?;

        match check.expected {
            CompositionExpected::Missing => {
                if proof.exit_code != EXIT_VERIFY_FAIL || proof.status != "unsatisfied" {
                    return Err(format!(
                        "check `{}` expected provekit to catch a missing implication, but status was `{}` (exit {})",
                        check.id, proof.status, proof.exit_code
                    ));
                }
                expect_red_diagnostic(&diagnostic, predicates, &language.id, &check.id)?;
            }
            CompositionExpected::Satisfied => {
                if proof.exit_code != EXIT_OK || proof.status != "discharged" {
                    return Err(format!(
                        "check `{}` expected provekit to discharge the implication, but status was `{}` (exit {})",
                        check.id, proof.status, proof.exit_code
                    ));
                }
                expect_green_diagnostic(&diagnostic, predicates, &language.id, &check.id)?;
            }
        }

        reports.push(json!({
            "id": check.id,
            "phase": match check.phase {
                CompositionPhase::Exhibit => "exhibit",
                CompositionPhase::Fixed => "fixed",
            },
            "expected": match check.expected {
                CompositionExpected::Missing => "missing",
                CompositionExpected::Satisfied => "satisfied",
            },
            "witnessExhibit": check.witness_exhibit,
            "witnessSource": match check.witness_source {
                CompositionWitnessSource::ProofIr => "proof-ir",
                CompositionWitnessSource::Lab => "lab",
            },
            "requirementExhibit": check.requirement_exhibit,
            "holds": proof.status == "discharged",
            "provekitStatus": proof.status,
            "provekitReason": proof.reason,
            "provekitSignal": match check.expected {
                CompositionExpected::Missing => "red",
                CompositionExpected::Satisfied => "green",
            },
            "provedBy": FORMULA_PROVER_LABEL,
            "formula": formula,
        }));
    }

    Ok(reports)
}

fn scoped_implication_formula(check: &CompositionCheck) -> Value {
    json!({
        "kind": "implies",
        "operands": [
            check.witness_formula.clone(),
            substitute_bindings(&check.requirement_formula, &check.bindings),
        ],
    })
}

fn substitute_bindings(value: &Value, bindings: &BTreeMap<String, String>) -> Value {
    match value {
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| substitute_bindings(item, bindings))
                .collect(),
        ),
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (key, child) in map {
                out.insert(key.clone(), substitute_bindings(child, bindings));
            }
            if map.get("kind").and_then(Value::as_str) == Some("var") {
                if let Some(name) = map.get("name").and_then(Value::as_str) {
                    if let Some(bound) = bindings.get(name) {
                        out.insert("name".into(), Value::String(bound.clone()));
                    }
                }
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

fn json_contains(haystack: &Value, needle: &Value) -> bool {
    if haystack == needle {
        return true;
    }
    match haystack {
        Value::Array(items) => items.iter().any(|item| json_contains(item, needle)),
        Value::Object(map) => map.values().any(|value| json_contains(value, needle)),
        _ => false,
    }
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
        let diagnostic_contract =
            ZooDiagnosticContract::accepted_fixed(diagnostic, &predicates.missing_edge);
        if diagnostic_contract.acceptedFixedDiagnostic() == true {
            zoo_fixed_diagnostic_has_no_missing_edge(&diagnostic_contract);
        }
        return Ok(());
    }
    Err(format!(
        "language `{language_id}` fixed `{exhibit_id}` diagnostic still mentions missing edge `{}`",
        predicates.missing_edge
    ))
}

#[derive(Debug)]
struct FormulaProofReport {
    exit_code: u8,
    status: String,
    reason: String,
}

#[derive(Debug)]
struct LinkRunReport {
    exit_code: u8,
    bundle: Value,
    cid: String,
    linker_error_count: usize,
    error_kinds: Vec<String>,
}

impl LinkRunReport {
    fn has_linker_error_kind(&self, kind: &str) -> bool {
        self.error_kinds.iter().any(|entry| entry == kind)
    }
}

fn invoke_provekit_cli_lift(
    specimen_dir: &Path,
    id: &str,
    surface: &str,
    harness: &Path,
) -> Result<Value, String> {
    if manifest_path_escapes_specimen_root(harness) {
        return Err(format!("`{id}` contains a path that escapes specimen root"));
    }

    let harness_path = specimen_dir.join(harness);
    let harness_root = harness_path.canonicalize().unwrap_or(harness_path);
    let repo_root = find_repo_root(specimen_dir)?;
    let out_dir = temp_zoo_dir("mint", id)?;
    let mut cmd = provekit_cli_command(&repo_root)?;
    if trace_enabled() {
        cmd.env("PROVEKIT_CLI_TRACE", "1");
        cmd.stderr(Stdio::inherit());
    }
    cmd.arg("mint")
        .arg("--project")
        .arg(&harness_root)
        .arg("--out")
        .arg(&out_dir)
        .arg("--no-attest")
        .arg("--json")
        .arg("--quiet")
        .current_dir(&repo_root);

    let started = Instant::now();
    trace_log(format!(
        "provekit mint start id={id} surface={surface} project={} out={}",
        harness_root.display(),
        out_dir.display()
    ));
    let output = cmd.output().map_err(|e| {
        format!(
            "spawn provekit mint for `{id}` in project {}: {e}",
            harness_root.display()
        )
    })?;
    trace_log(format!(
        "provekit mint exit id={id} status={} elapsed={:?}",
        output.status,
        started.elapsed()
    ));
    let _ = std::fs::remove_dir_all(&out_dir);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        return Err(format!(
            "provekit mint failed for `{id}` in project {}\nstdout:\n{stdout}\nstderr:\n{stderr}",
            harness_root.display()
        ));
    }

    let report: Value = serde_json::from_str(&stdout).map_err(|e| {
        format!("parse provekit mint JSON for `{id}`: {e}\nstdout:\n{stdout}\nstderr:\n{stderr}")
    })?;
    if report.get("surface").and_then(Value::as_str) != Some(surface) {
        return Err(format!(
            "`{id}` configured surface mismatch: expected `{surface}`, provekit mint used {:?}",
            report.get("surface")
        ));
    }
    let lift_resp = report
        .get("lift")
        .cloned()
        .ok_or_else(|| format!("`{id}` provekit mint JSON missing `lift`"))?;
    match lift_resp.get("kind").and_then(Value::as_str) {
        Some("ir-document") => Ok(lift_resp),
        other => Err(format!(
            "`{id}` returned unsupported lift kind {:?} through provekit mint",
            other
        )),
    }
}

fn read_checked_in_link_bundle(
    specimen_dir: &Path,
    id: &str,
    project: &Path,
    go_lsp_bin: Option<&Path>,
) -> Result<LinkRunReport, String> {
    if manifest_path_escapes_specimen_root(project) {
        return Err(format!(
            "`{id}` contains a project path that escapes specimen root"
        ));
    }
    if let Some(go_lsp_bin) = go_lsp_bin {
        if manifest_path_escapes_specimen_root(go_lsp_bin) {
            return Err(format!(
                "`{id}` contains a goLspBin path that escapes specimen root"
            ));
        }
    }

    let project_path = specimen_dir.join(project);
    let bundle_path = project_path.join("link-bundle.json");

    let started = Instant::now();
    trace_log(format!(
        "bug-zoo link-bundle receipt read start id={id} project={}",
        project_path.display()
    ));
    let bundle = read_json(bundle_path.clone()).map_err(|error| {
        format!(
            "read checked-in LinkBundle receipt for `{id}` at {}: {error}",
            bundle_path.display()
        )
    })?;
    trace_log(format!(
        "bug-zoo link-bundle receipt read exit id={id} elapsed={:?}",
        started.elapsed()
    ));

    let cid = bundle
        .get("linkBundleCid")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("LinkBundle receipt for `{id}` missing `linkBundleCid`"))?
        .to_string();
    let linker_errors = bundle
        .get("linkerErrors")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("LinkBundle receipt for `{id}` missing `linkerErrors` array"))?;
    let linker_error_count = linker_errors.len();
    let error_kinds = linker_errors
        .iter()
        .filter_map(|error| error.get("errorKind").and_then(Value::as_str))
        .map(str::to_string)
        .collect::<Vec<_>>();

    Ok(LinkRunReport {
        exit_code: if linker_error_count == 0 {
            EXIT_OK
        } else {
            EXIT_VERIFY_FAIL
        },
        bundle,
        cid,
        linker_error_count,
        error_kinds,
    })
}

fn invoke_provekit_formula_gate(
    specimen_dir: &Path,
    id: &str,
    formula: &Value,
) -> Result<FormulaProofReport, String> {
    let started = Instant::now();
    trace_log(format!(
        "provekit-verifier formula gate start id={id} specimen={}",
        specimen_dir.display()
    ));

    let smt =
        smt_emitter::emit(formula).map_err(|e| format!("emit SMT-LIB formula for `{id}`: {e}"))?;
    let plan = SolverPlan::Single("z3".to_string());
    let registry = provekit_verifier::solvers::registry::build_default_z3("z3");
    let (verdict, reason, _) = provekit_verifier::run_plan(&plan, &registry, &smt, Some(formula));
    trace_log(format!(
        "provekit-verifier formula gate exit id={id} status={} elapsed={:?}",
        verdict.as_str(),
        started.elapsed()
    ));

    Ok(FormulaProofReport {
        exit_code: if verdict == ObligationVerdict::Discharged {
            EXIT_OK
        } else {
            EXIT_VERIFY_FAIL
        },
        status: verdict.as_str().to_string(),
        reason,
    })
}

fn trace_enabled() -> bool {
    std::env::var_os("PROVEKIT_BUG_ZOO_TRACE").is_some()
}

fn trace_log(message: impl fmt::Display) {
    if trace_enabled() {
        eprintln!("zoo trace: {message}");
    }
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

fn temp_zoo_dir(kind: &str, id: &str) -> Result<PathBuf, String> {
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
    Ok(std::env::temp_dir().join(format!(
        "provekit-bug-zoo-{kind}-{}-{now}-{safe_id}",
        std::process::id()
    )))
}

fn provekit_cli_command(repo_root: &Path) -> Result<Command, String> {
    let invocation = provekit_cli_invocation(repo_root);
    let mut args = invocation.command.iter();
    let program = args
        .next()
        .ok_or_else(|| "provekit CLI command is empty".to_string())?;
    let mut cmd = host_command(program);
    cmd.args(args);
    Ok(cmd)
}

fn host_command(program: &str) -> Command {
    let mut cmd = Command::new(program);
    if let Some(path) = homebrew_openjdk_path() {
        cmd.env("PATH", path);
    }
    cmd
}

fn homebrew_openjdk_path() -> Option<std::ffi::OsString> {
    let original_path = std::env::var_os("PATH").unwrap_or_default();
    for candidate in [
        "/usr/local/opt/openjdk/bin",
        "/opt/homebrew/opt/openjdk/bin",
    ] {
        let candidate = Path::new(candidate);
        if candidate.join("java").is_file() && candidate.join("javac").is_file() {
            let mut paths = vec![candidate.to_path_buf()];
            paths.extend(std::env::split_paths(&original_path));
            return std::env::join_paths(paths).ok();
        }
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
    std::env::var(PROVEKIT_BUG_ZOO_EXTERNAL_CLI_ENV)
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
                "set {PROVEKIT_BUG_ZOO_EXTERNAL_CLI_ENV}=1 to run Bug Zoo against an explicit external provekit binary"
            ),
        });
    }
    report
}

/// Strip `locus` keys recursively from a JSON value so that machine-specific
/// absolute file paths recorded in `locus.file` do not affect the CID.
/// This makes golden ProofIR files portable across machines (CI vs authoring host).
fn strip_locus(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(strip_locus).collect()),
        Value::Object(map) => {
            let filtered: serde_json::Map<String, Value> = map
                .iter()
                .filter(|(k, _)| k.as_str() != "locus")
                .map(|(k, v)| (k.clone(), strip_locus(v)))
                .collect();
            Value::Object(filtered)
        }
        other => other.clone(),
    }
}

fn proof_ir_cid(value: &Value) -> Result<String, String> {
    canonical_json_cid(&strip_locus(value))
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
    use std::ffi::OsString;
    use std::fs;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    use tempfile::tempdir;

    fn env_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .expect("env lock poisoned")
    }

    struct EnvGuard {
        provekit_cli: Option<OsString>,
        external_cli: Option<OsString>,
    }

    impl EnvGuard {
        fn capture() -> Self {
            Self {
                provekit_cli: std::env::var_os(PROVEKIT_CLI_ENV),
                external_cli: std::env::var_os(PROVEKIT_BUG_ZOO_EXTERNAL_CLI_ENV),
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(value) = self.provekit_cli.take() {
                std::env::set_var(PROVEKIT_CLI_ENV, value);
            } else {
                std::env::remove_var(PROVEKIT_CLI_ENV);
            }
            if let Some(value) = self.external_cli.take() {
                std::env::set_var(PROVEKIT_BUG_ZOO_EXTERNAL_CLI_ENV, value);
            } else {
                std::env::remove_var(PROVEKIT_BUG_ZOO_EXTERNAL_CLI_ENV);
            }
        }
    }

    #[test]
    fn provekit_cli_defaults_to_source_routing_even_when_external_env_is_present() {
        let _lock = env_lock();
        let _guard = EnvGuard::capture();
        std::env::set_var(PROVEKIT_CLI_ENV, "/tmp/stale-provekit");
        std::env::remove_var(PROVEKIT_BUG_ZOO_EXTERNAL_CLI_ENV);

        let invocation = provekit_cli_invocation(Path::new("/repo"));

        assert_eq!(invocation.kind, "cargo-run-source");
        assert_eq!(
            invocation.ignored_external_cli.as_deref(),
            Some("/tmp/stale-provekit")
        );
        assert!(invocation.command.iter().any(|arg| arg == "run"));
        assert!(invocation
            .command
            .iter()
            .any(|arg| arg.ends_with("implementations/rust/provekit-cli/Cargo.toml")));
    }

    #[test]
    fn provekit_cli_uses_external_binary_only_when_explicitly_enabled() {
        let _lock = env_lock();
        let _guard = EnvGuard::capture();
        std::env::set_var(PROVEKIT_CLI_ENV, "/tmp/current-provekit");
        std::env::set_var(PROVEKIT_BUG_ZOO_EXTERNAL_CLI_ENV, "1");

        let invocation = provekit_cli_invocation(Path::new("/repo"));

        assert_eq!(invocation.kind, "external-binary");
        assert_eq!(invocation.command, vec!["/tmp/current-provekit"]);
        assert_eq!(invocation.ignored_external_cli, None);
    }

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
                    proof_ir_file: "java/exhibit/spring-web/expected.proofir.json".into(),
                    diagnostic_file: "java/exhibit/spring-web/expected-diagnostic.txt".into(),
                    fixed: valid_fixed_exposure(),
                    lossiness: Lossiness {
                        erased: vec!["Spring binding".into()],
                        preserved: vec!["precondition neq(name, null)".into()],
                    },
                }],
                link_exhibits: vec![],
                equivalence: Equivalence { required: vec![] },
                exposure: ExposureFiles {
                    sat_witness_file: "java/exhibit/sat-witness.json".into(),
                },
                composition: None,
                wild_sightings: vec![],
            }],
            wild_sightings: vec![],
        }
    }

    fn valid_fixed_exposure() -> FixedExposure {
        FixedExposure {
            harness: "java/fixed/spring-web/harness".into(),
            kit_rpc: "java/fixed/spring-web/kit-rpc".into(),
            proof_ir_file: "java/fixed/spring-web/expected.proofir.json".into(),
            diagnostic_file: "java/fixed/spring-web/expected-diagnostic.txt".into(),
        }
    }

    fn valid_link_exposure() -> LinkExposure {
        LinkExposure {
            id: "cgo-rust-callee".into(),
            surface: "rust-go-cgo-link".into(),
            project: "rust-go/exhibit/cgo-rust-callee/harness".into(),
            go_lsp_bin: Some("rust-go/kit-rpc/run-go-lsp.sh".into()),
            link_bundle_file: "rust-go/exhibit/cgo-rust-callee/harness/link-bundle.json".into(),
            diagnostic_file: "rust-go/exhibit/cgo-rust-callee/expected-diagnostic.txt".into(),
            fixed: FixedLinkExposure {
                project: "rust-go/fixed/cgo-rust-callee/harness".into(),
                go_lsp_bin: None,
                link_bundle_file: "rust-go/fixed/cgo-rust-callee/harness/link-bundle.json".into(),
                diagnostic_file: "rust-go/fixed/cgo-rust-callee/expected-diagnostic.txt".into(),
            },
            lossiness: Lossiness {
                erased: vec!["Go cgo mechanics".into()],
                preserved: vec!["cross-kit obligation".into()],
            },
        }
    }

    fn create_valid_paths(specimen_dir: &Path, manifest: &SpecimenManifest) {
        let language = &manifest.languages[0];
        let exhibit = &language.exhibits[0];
        for path in [
            &language.paths.lab_library,
            &language.paths.lab_harness,
            &exhibit.harness,
            &exhibit.kit_rpc,
            &exhibit.fixed.harness,
            &exhibit.fixed.kit_rpc,
        ] {
            fs::create_dir_all(specimen_dir.join(path)).expect("create directory");
        }
        for harness in [&exhibit.harness, &exhibit.fixed.harness] {
            let config = harness.join(".provekit/config.toml");
            let manifest = harness
                .join(".provekit/lift")
                .join(&exhibit.surface)
                .join("manifest.toml");
            if let Some(parent) = config.parent() {
                fs::create_dir_all(specimen_dir.join(parent)).expect("create .provekit");
            }
            if let Some(parent) = manifest.parent() {
                fs::create_dir_all(specimen_dir.join(parent)).expect("create lift manifest dir");
            }
            fs::write(
                specimen_dir.join(config),
                format!("[authoring]\nsurface = \"{}\"\n", exhibit.surface),
            )
            .expect("write config");
            fs::write(
                specimen_dir.join(manifest),
                format!("name = \"{}\"\n", exhibit.surface),
            )
            .expect("write manifest");
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
    commands:
      hostCheck:
        cwd: java/lab/harness
        argv: ["./run.sh"]
    exhibits:
      - id: provekit-native
        surface: java-provekit-native
        harness: java/exhibit/provekit-native/harness
        kitRpc: java/exhibit/provekit-native/kit-rpc
        proofIrFile: java/exhibit/provekit-native/expected.proofir.json
        diagnosticFile: java/exhibit/provekit-native/expected-diagnostic.txt
        fixed:
          harness: java/fixed/provekit-native/harness
          kitRpc: java/fixed/provekit-native/kit-rpc
          proofIrFile: java/fixed/provekit-native/expected.proofir.json
          diagnosticFile: java/fixed/provekit-native/expected-diagnostic.txt
        lossiness:
          erased: ["Java body"]
          preserved: ["precondition neq(name, null)"]
      - id: spring-web
        surface: java-spring-web
        harness: java/exhibit/spring-web/harness
        kitRpc: java/exhibit/spring-web/kit-rpc
        proofIrFile: java/exhibit/spring-web/expected.proofir.json
        diagnosticFile: java/exhibit/spring-web/expected-diagnostic.txt
        fixed:
          harness: java/fixed/spring-web/harness
          kitRpc: java/fixed/spring-web/kit-rpc
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
    commands:
      hostCheck:
        cwd: java/lab/harness
        argv: ["./run.sh"]
    exhibits:
      - id: provekit-native
        surface: java-provekit-native
        harness: java/exhibit/provekit-native/harness
        kitRpc: java/exhibit/provekit-native/kit-rpc
        proofIrFile: java/exhibit/provekit-native/expected.proofir.json
        diagnosticFile: java/exhibit/provekit-native/expected-diagnostic.txt
        fixed:
          harness: java/fixed/provekit-native/harness
          kitRpc: java/fixed/provekit-native/kit-rpc
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
    fn parses_manifest_without_lab_provekit_workflow() {
        let raw = r#"
id: BZ-SHAPE-006
name: Value Scope Escape
kingdom: shape
status: lab
predicates:
  boundary: eq(value, 42)
  sink: gte(value, 43)
  missingEdge: eq(value, 42) => gte(value, 43)
languages:
  - id: java
    surface: java-junit
    paths:
      labLibrary: java/lab/library
      labHarness: java/lab/harness
    commands:
      hostCheck:
        cwd: java/lab/harness
        argv: ["./run.sh"]
    exhibits:
      - id: junit
        surface: java-junit
        harness: java/exhibit/junit/harness
        kitRpc: java/exhibit/junit/kit-rpc
        proofIrFile: java/exhibit/junit/expected.proofir.json
        diagnosticFile: java/exhibit/junit/expected-diagnostic.txt
        fixed:
          harness: java/fixed/junit/harness
          kitRpc: java/fixed/junit/kit-rpc
          proofIrFile: java/fixed/junit/expected.proofir.json
          diagnosticFile: java/fixed/junit/expected-diagnostic.txt
        lossiness:
          erased: ["JUnit runner lifecycle"]
          preserved: ["point witness eq(value$0, 42)"]
    equivalence:
      required: []
    exposure:
      satWitnessFile: java/exhibit/sat-witness.json
wildSightings: []
"#;
        let manifest: SpecimenManifest = serde_yaml::from_str(raw)
            .expect("lab state should not require a ProvekIt RPC workflow");

        assert_eq!(
            manifest.languages[0].paths.lab_harness,
            PathBuf::from("java/lab/harness")
        );
    }

    #[test]
    fn manifest_rejects_lab_provekit_workflow_path() {
        let raw = r#"
id: BZ-SHAPE-006
name: Value Scope Escape
kingdom: shape
status: lab
predicates:
  boundary: eq(value, 42)
  sink: gte(value, 43)
  missingEdge: eq(value, 42) => gte(value, 43)
languages:
  - id: java
    surface: java-junit
    paths:
      labLibrary: java/lab/library
      labHarness: java/lab/harness
      labKitRpc: java/lab/kit-rpc
    commands:
      hostCheck:
        cwd: java/lab/harness
        argv: ["./run.sh"]
    exhibits:
      - id: junit
        surface: java-junit
        harness: java/exhibit/junit/harness
        kitRpc: java/exhibit/junit/kit-rpc
        proofIrFile: java/exhibit/junit/expected.proofir.json
        diagnosticFile: java/exhibit/junit/expected-diagnostic.txt
        fixed:
          harness: java/fixed/junit/harness
          kitRpc: java/fixed/junit/kit-rpc
          proofIrFile: java/fixed/junit/expected.proofir.json
          diagnosticFile: java/fixed/junit/expected-diagnostic.txt
        lossiness:
          erased: ["JUnit runner lifecycle"]
          preserved: ["point witness eq(value$0, 42)"]
    equivalence:
      required: []
    exposure:
      satWitnessFile: java/exhibit/sat-witness.json
wildSightings: []
"#;

        let error = serde_yaml::from_str::<SpecimenManifest>(raw)
            .expect_err("lab state must not declare a ProvekIt RPC workflow");

        assert!(
            error.to_string().contains("labKitRpc"),
            "error should name the rejected lab workflow field: {error}"
        );
    }

    #[test]
    fn parses_lab_witness_composition_checks() {
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
    commands:
      hostCheck:
        cwd: java/lab/harness
        argv: ["./run.sh"]
    exhibits:
      - id: provekit-native
        surface: java-provekit-native
        harness: java/exhibit/provekit-native/harness
        kitRpc: java/exhibit/provekit-native/kit-rpc
        proofIrFile: java/exhibit/provekit-native/expected.proofir.json
        diagnosticFile: java/exhibit/provekit-native/expected-diagnostic.txt
        fixed:
          harness: java/fixed/provekit-native/harness
          kitRpc: java/fixed/provekit-native/kit-rpc
          proofIrFile: java/fixed/provekit-native/expected.proofir.json
          diagnosticFile: java/fixed/provekit-native/expected-diagnostic.txt
        lossiness:
          erased: ["Java method body"]
          preserved: ["precondition neq(name, null)"]
    equivalence:
      required: []
    exposure:
      satWitnessFile: java/exhibit/sat-witness.json
    composition:
      checks:
        - id: lab-null-does-not-satisfy-non-null
          phase: exhibit
          expected: missing
          witnessSource: lab
          witnessExhibit: provekit-native
          requirementExhibit: provekit-native
          witnessFormula:
            kind: atomic
            name: eq
            args:
              - { kind: var, name: name }
              - { kind: const, value: null, sort: { kind: primitive, name: Ref } }
          requirementFormula:
            kind: atomic
            name: neq
            args:
              - { kind: var, name: name }
              - { kind: const, value: null, sort: { kind: primitive, name: Ref } }
          assignment:
            name: 0
          diagnosticFile: java/exhibit/provekit-native/expected-diagnostic.txt
wildSightings: []
"#;
        let manifest: SpecimenManifest = serde_yaml::from_str(raw).expect("parse manifest");
        let check = &manifest.languages[0]
            .composition
            .as_ref()
            .expect("composition checks")
            .checks[0];

        assert_eq!(check.witness_source, CompositionWitnessSource::Lab);
    }

    #[test]
    fn parses_manifest_with_link_exhibits_without_proof_ir_exhibits() {
        let raw = r#"
id: BZ-SHAPE-007
name: Polyglot Link Obligation
kingdom: shape
status: lab
predicates:
  boundary: post_caller
  sink: pre_callee
  missingEdge: post_caller => pre_callee
languages:
  - id: rust-go
    surface: rust-go-cgo-link
    paths:
      labLibrary: rust-go/lab/library
      labHarness: rust-go/lab/harness
    commands:
      hostCheck:
        cwd: rust-go/lab/harness
        argv: ["./run.sh"]
    exhibits: []
    linkExhibits:
      - id: cgo-rust-callee
        surface: rust-go-cgo-link
        project: rust-go/exhibit/cgo-rust-callee/harness
        goLspBin: rust-go/kit-rpc/run-go-lsp.sh
        linkBundleFile: rust-go/exhibit/cgo-rust-callee/harness/link-bundle.json
        diagnosticFile: rust-go/exhibit/cgo-rust-callee/expected-diagnostic.txt
        fixed:
          project: rust-go/fixed/cgo-rust-callee/harness
          linkBundleFile: rust-go/fixed/cgo-rust-callee/harness/link-bundle.json
          diagnosticFile: rust-go/fixed/cgo-rust-callee/expected-diagnostic.txt
        lossiness:
          erased: ["Go cgo implementation mechanics", "Rust function body"]
          preserved: ["cross-kit obligation post_caller => pre_callee"]
    equivalence:
      required: []
    exposure:
      satWitnessFile: rust-go/exhibit/sat-witness.json
wildSightings: []
"#;
        let manifest: SpecimenManifest = serde_yaml::from_str(raw).expect("parse manifest");

        assert!(
            validate_manifest_shape(&manifest).is_empty(),
            "link-only exhibits should satisfy the language shape"
        );
    }

    #[test]
    fn parses_value_scope_composition_checks() {
        let raw = r#"
id: BZ-SHAPE-006
name: Value Scope Escape
kingdom: shape
status: lab
predicates:
  boundary: eq(value, 42)
  sink: gte(value, 43)
  missingEdge: eq(value, 42) => gte(value, 43)
languages:
  - id: java
    surface: java-junit-and-spring-value-witnesses
    paths:
      labLibrary: java/lab/library
      labHarness: java/lab/harness
    commands:
      hostCheck:
        cwd: java/lab/harness
        argv: ["./run.sh"]
    exhibits:
      - id: junit
        surface: java-junit
        harness: java/exhibit/junit/harness
        kitRpc: java/exhibit/junit/kit-rpc
        proofIrFile: java/exhibit/junit/expected.proofir.json
        diagnosticFile: java/exhibit/junit/expected-diagnostic.txt
        fixed:
          harness: java/fixed/junit/harness
          kitRpc: java/fixed/junit/kit-rpc
          proofIrFile: java/fixed/junit/expected.proofir.json
          diagnosticFile: java/fixed/junit/expected-diagnostic.txt
        lossiness:
          erased: ["JUnit runner lifecycle"]
          preserved: ["point witness eq(value$0, 42)"]
      - id: spring
        surface: java-spring-bean-validation
        harness: java/exhibit/spring/harness
        kitRpc: java/exhibit/spring/kit-rpc
        proofIrFile: java/exhibit/spring/expected.proofir.json
        diagnosticFile: java/exhibit/spring/expected-diagnostic.txt
        fixed:
          harness: java/fixed/spring/harness
          kitRpc: java/fixed/spring/kit-rpc
          proofIrFile: java/fixed/spring/expected.proofir.json
          diagnosticFile: java/fixed/spring/expected-diagnostic.txt
        lossiness:
          erased: ["Spring request binding machinery"]
          preserved: ["precondition gte(value, 43)"]
    equivalence:
      required: []
    exposure:
      satWitnessFile: java/exhibit/sat-witness.json
    composition:
      checks:
        - id: junit-42-does-not-satisfy-min-43
          phase: exhibit
          expected: missing
          witnessExhibit: junit
          witnessFormula:
            kind: atomic
            name: eq
            args:
              - { kind: var, name: "value$0" }
              - { kind: const, value: 42, sort: { kind: primitive, name: Int } }
          requirementFormula:
            kind: atomic
            name: gte
            args:
              - { kind: var, name: value }
              - { kind: const, value: 43, sort: { kind: primitive, name: Int } }
          bindings:
            value: "value$0"
          assignment:
            value$0: 42
          diagnosticFile: java/exhibit/value-scope-escape/expected-diagnostic.txt
        - id: junit-43-satisfies-min-43
          phase: fixed
          expected: satisfied
          witnessExhibit: junit
          witnessFormula:
            kind: atomic
            name: eq
            args:
              - { kind: var, name: "value$0" }
              - { kind: const, value: 43, sort: { kind: primitive, name: Int } }
          requirementFormula:
            kind: atomic
            name: gte
            args:
              - { kind: var, name: value }
              - { kind: const, value: 43, sort: { kind: primitive, name: Int } }
          bindings:
            value: "value$0"
          assignment:
            value$0: 43
          diagnosticFile: java/fixed/value-scope-escape/expected-diagnostic.txt
wildSightings: []
"#;
        let manifest: SpecimenManifest = serde_yaml::from_str(raw).expect("parse manifest");
        let composition = manifest.languages[0]
            .composition
            .as_ref()
            .expect("java language has composition checks");

        assert_eq!(composition.checks.len(), 2);
        assert_eq!(composition.checks[0].phase, CompositionPhase::Exhibit);
        assert_eq!(composition.checks[0].expected, CompositionExpected::Missing);
        assert!(composition.checks[0].requirement_exhibit.is_none());
        assert_eq!(composition.checks[0].bindings["value"], "value$0");
        assert_eq!(composition.checks[1].phase, CompositionPhase::Fixed);
        assert_eq!(
            composition.checks[1].diagnostic_file,
            PathBuf::from("java/fixed/value-scope-escape/expected-diagnostic.txt")
        );
    }

    #[test]
    fn value_scope_implication_uses_callsite_bindings() {
        let witness_42 = serde_json::json!({
            "kind": "atomic",
            "name": "eq",
            "args": [
                {"kind": "var", "name": "value$0"},
                {"kind": "const", "value": 42, "sort": {"kind": "primitive", "name": "Int"}}
            ]
        });
        let requirement = serde_json::json!({
            "kind": "atomic",
            "name": "gte",
            "args": [
                {"kind": "var", "name": "value"},
                {"kind": "const", "value": 43, "sort": {"kind": "primitive", "name": "Int"}}
            ]
        });
        let bindings = BTreeMap::from([("value".to_string(), "value$0".to_string())]);
        let check = CompositionCheck {
            id: "junit-42-does-not-satisfy-min-43".into(),
            phase: CompositionPhase::Exhibit,
            expected: CompositionExpected::Missing,
            witness_source: CompositionWitnessSource::ProofIr,
            witness_exhibit: "junit".into(),
            requirement_exhibit: None,
            witness_formula: witness_42.clone(),
            requirement_formula: requirement,
            bindings,
            assignment: BTreeMap::from([("value$0".to_string(), 42)]),
            diagnostic_file: "java/exhibit/value-scope-escape/expected-diagnostic.txt".into(),
        };

        assert_eq!(
            scoped_implication_formula(&check),
            serde_json::json!({
                "kind": "implies",
                "operands": [
                    witness_42,
                    {
                        "kind": "atomic",
                        "name": "gte",
                        "args": [
                            {"kind": "var", "name": "value$0"},
                            {"kind": "const", "value": 43, "sort": {"kind": "primitive", "name": "Int"}}
                        ]
                    }
                ]
            }),
            "the zoo should hand provekit an implication in the witness value scope"
        );
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
    commands:
      hostCheck:
        cwd: java/lab/harness
        argv: ["./run.sh"]
    exhibits:
      - id: provekit-native
        surface: java-provekit-native
        harness: java/exhibit/provekit-native/harness
        kitRpc: java/exhibit/provekit-native/kit-rpc
        proofIrFile: java/exhibit/provekit-native/expected.proofir.json
        diagnosticFile: java/exhibit/provekit-native/expected-diagnostic.txt
        fixed:
          harness: java/fixed/provekit-native/harness
          kitRpc: java/fixed/provekit-native/kit-rpc
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
    commands:
      hostCheck:
        cwd: typescript/lab/harness
        argv: ["./run.sh"]
    exhibits:
      - id: zod
        surface: typescript-zod
        harness: typescript/exhibit/zod/harness
        kitRpc: typescript/exhibit/zod/kit-rpc
        proofIrFile: typescript/exhibit/zod/expected.proofir.json
        diagnosticFile: typescript/exhibit/zod/expected-diagnostic.txt
        fixed:
          harness: typescript/fixed/zod/harness
          kitRpc: typescript/fixed/zod/kit-rpc
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
            proof_ir_file: "java/exhibit/provekit-native/expected.proofir.json".into(),
            diagnostic_file: "java/exhibit/provekit-native/expected-diagnostic.txt".into(),
            fixed: FixedExposure {
                harness: "java/fixed/provekit-native/harness".into(),
                kit_rpc: "java/fixed/provekit-native/kit-rpc".into(),
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
        assert!(errors.iter().any(
            |error| error.contains("equivalence references unknown ProofIR exhibit `missing`")
        ));
    }

    #[test]
    fn validation_rejects_link_exhibits_in_proof_ir_equivalence() {
        let mut manifest = valid_manifest();
        let language = &mut manifest.languages[0];
        language.exhibits.clear();
        language.link_exhibits = vec![valid_link_exposure()];
        language.equivalence.required = vec![["cgo-rust-callee".into(), "cgo-rust-callee".into()]];

        let errors = validate_manifest_shape(&manifest);
        assert!(errors.iter().any(|error| {
            error.contains("equivalence references unknown ProofIR exhibit `cgo-rust-callee`")
        }));
    }

    #[test]
    fn validation_rejects_link_exhibits_in_proof_ir_composition() {
        let mut manifest = valid_manifest();
        let language = &mut manifest.languages[0];
        language.exhibits.clear();
        language.link_exhibits = vec![valid_link_exposure()];
        language.composition = Some(Composition {
            checks: vec![CompositionCheck {
                id: "link-edge-composition".into(),
                phase: CompositionPhase::Exhibit,
                expected: CompositionExpected::Missing,
                witness_source: CompositionWitnessSource::ProofIr,
                witness_exhibit: "cgo-rust-callee".into(),
                requirement_exhibit: Some("cgo-rust-callee".into()),
                witness_formula: json!({"kind": "atomic", "name": "post_caller", "args": []}),
                requirement_formula: json!({"kind": "atomic", "name": "pre_callee", "args": []}),
                bindings: BTreeMap::new(),
                assignment: BTreeMap::from([("n".into(), 0)]),
                diagnostic_file: "rust-go/exhibit/cgo-rust-callee/expected-diagnostic.txt".into(),
            }],
        });

        let errors = validate_manifest_shape(&manifest);
        assert!(errors.iter().any(|error| {
            error.contains(
                "composition check `link-edge-composition` references unknown ProofIR witness exhibit `cgo-rust-callee`",
            )
        }));
        assert!(errors.iter().any(|error| {
            error.contains(
                "composition check `link-edge-composition` references unknown ProofIR requirement exhibit `cgo-rust-callee`",
            )
        }));
    }

    #[test]
    fn validation_rejects_empty_command_argv() {
        let mut manifest = valid_manifest();
        manifest.languages[0].commands.host_check.argv.clear();

        let errors = validate_manifest_shape(&manifest);
        assert!(errors
            .iter()
            .any(|error| error.contains("commands.hostCheck.argv is required")));
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
