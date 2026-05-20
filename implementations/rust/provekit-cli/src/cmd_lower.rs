// SPDX-License-Identifier: Apache-2.0
//
// `provekit lower --target=<lang>` dispatches named substrate terms to the
// per-language lower plugin and emits target-language source.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::{Parser, ValueEnum};
use libprovekit::core::lower_plugin::{
    realize_function_name_with_sugar, realize_spec_from_named_term,
};
use libprovekit::core::{
    execute_path, named_term_document_from_bind_payload, HashMapInputCatalog, Input, KitRegistry,
    LowerKit, Path as CorePath, PathAlgebra, Term,
};
use owo_colors::OwoColorize;
use serde_json::{json, Value as Json};

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value as CValue};
use provekit_claim_envelope::{mint_witness, MintWitnessArgs};
use provekit_proof_envelope::{
    build_proof_envelope, ed25519_pubkey_string, Ed25519Seed, ProofEnvelopeInput,
};

use crate::cmd_bind::NamedTermDocument;
use crate::kit_dispatch::{dispatch_lower_witness, DispatchRealizeTransport};
use crate::{OutputFlags, EXIT_OK, EXIT_USER_ERROR, EXIT_VERIFY_FAIL};

const DEFAULT_WITNESS_PRODUCED_AT: &str = "2026-05-08T00:00:00Z";

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum LowerMode {
    Witness,
}

impl LowerMode {
    fn as_str(self) -> &'static str {
        match self {
            LowerMode::Witness => "witness",
        }
    }
}

#[derive(Parser, Debug, Clone)]
pub struct LowerArgs {
    /// Named term JSON. Reads stdin when omitted or `-`.
    pub input: Option<PathBuf>,
    /// Target source language, for example python, java, c, rust.
    #[arg(long)]
    pub target: Option<String>,
    /// Output file. Writes stdout when omitted or `-`.
    #[arg(short = 'o', long = "output")]
    pub output: Option<PathBuf>,
    /// Project root containing `.provekit/realize/<target>/manifest.toml`.
    #[arg(long)]
    pub project: Option<PathBuf>,
    /// Lowering surface. Defaults to `surface` in the plan, then host kit.
    #[arg(long)]
    pub surface: Option<String>,
    /// Lowering mode. Witness mode emits a .proof witness.
    #[arg(long, value_enum, default_value_t = LowerMode::Witness)]
    pub mode: LowerMode,
    /// JSON RealizerPlan or witness requirement.
    #[arg(long)]
    pub plan: Option<PathBuf>,
    /// Output directory for the produced witness .proof.
    #[arg(long)]
    pub out: Option<PathBuf>,
    #[command(flatten)]
    pub flags: OutputFlags,
}

#[derive(Debug, Clone)]
pub(crate) struct LowerProof {
    pub filename_cid: String,
    pub proof_file: PathBuf,
    pub bytes_written: usize,
    pub output: Json,
}

#[derive(Debug, Clone)]
struct LowerFailure {
    message: String,
    lower_result: Option<Json>,
}

impl LowerFailure {
    fn message(message: String) -> Self {
        Self {
            message,
            lower_result: None,
        }
    }

    fn rejected(message: String, lower_result: Json) -> Self {
        Self {
            message,
            lower_result: Some(lower_result),
        }
    }
}

#[derive(Debug, Clone)]
enum LowerNamedError {
    Message(String),
}

pub fn run(args: LowerArgs) -> u8 {
    if let Some(target) = args.target.as_deref() {
        return lower_named_terms(
            args.input.as_ref(),
            args.output.as_ref(),
            args.project.as_ref(),
            target,
        );
    }

    let project_root = args.project.unwrap_or_else(|| PathBuf::from("."));
    if !project_root.exists() {
        eprintln!(
            "{}: project not found: {}",
            "error".red().bold(),
            project_root.display()
        );
        return EXIT_USER_ERROR;
    }
    let Some(plan_path) = args.plan else {
        eprintln!(
            "{}: pass --target=<language> for named-term lowering",
            "error".red().bold()
        );
        return EXIT_USER_ERROR;
    };
    let plan = match std::fs::read_to_string(&plan_path)
        .map_err(|e| format!("read {}: {e}", plan_path.display()))
        .and_then(|text| serde_json::from_str::<Json>(&text).map_err(|e| e.to_string()))
    {
        Ok(plan) => plan,
        Err(error) => {
            eprintln!("{}: {error}", "error".red().bold());
            return EXIT_USER_ERROR;
        }
    };
    let surface = match args
        .surface
        .or_else(|| optional_str(&plan, "surface").map(str::to_string))
        .or_else(|| {
            plan.pointer("/host/kit")
                .and_then(Json::as_str)
                .map(str::to_string)
        }) {
        Some(surface) => surface,
        None => {
            eprintln!(
                "{}: no lower surface supplied; pass --surface or include host.kit in the plan",
                "error".red().bold()
            );
            return EXIT_USER_ERROR;
        }
    };
    let out_dir = args
        .out
        .unwrap_or_else(|| project_root.join(".provekit").join("witnesses"));

    match lower_witness_requirement_for_surface(
        &project_root,
        &surface,
        &plan,
        &out_dir,
        args.flags.quiet,
    ) {
        Ok(result) => {
            if args.flags.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "ok": true,
                        "project": project_root,
                        "surface": surface,
                        "mode": args.mode.as_str(),
                        "filenameCid": result.filename_cid,
                        "bytesWritten": result.bytes_written,
                        "proofFile": result.proof_file,
                        "output": result.output,
                    }))
                    .expect("serialize lower JSON")
                );
            } else if !args.flags.quiet {
                println!("{}", "lower witness".green().bold());
                println!("  proof CID : {}", result.filename_cid);
                println!("  .proof    : {}", result.proof_file.display());
            } else {
                println!("{}", result.filename_cid);
            }
            EXIT_OK
        }
        Err(error) => {
            if args.flags.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "ok": false,
                        "project": project_root,
                        "surface": surface,
                        "mode": args.mode.as_str(),
                        "error": error.message,
                        "lowerResult": error.lower_result,
                    }))
                    .expect("serialize lower error JSON")
                );
            } else {
                eprintln!("{}: {}", "error".red().bold(), error.message);
            }
            EXIT_VERIFY_FAIL
        }
    }
}

fn lower_named_terms(
    input: Option<&PathBuf>,
    output: Option<&PathBuf>,
    project: Option<&PathBuf>,
    target: &str,
) -> u8 {
    if is_solver_target(target) {
        eprintln!(
            "{}: solver target `{target}` moved to `provekit prove --target={target}`",
            "error".red().bold()
        );
        return EXIT_USER_ERROR;
    }
    let raw = match read_bytes(input) {
        Ok(raw) => raw,
        Err(error) => {
            eprintln!("{}: {error}", "error".red().bold());
            return EXIT_USER_ERROR;
        }
    };
    let named = match parse_named_or_bind_payload(&raw) {
        Ok(named) => named,
        Err(error) => {
            eprintln!("{}: {error}", "error".red().bold());
            return EXIT_USER_ERROR;
        }
    };
    let project_root = project
        .cloned()
        .or_else(|| named.workspace_root.as_ref().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    let source = match lower_named_document(&project_root, target, &named) {
        Ok(source) => source,
        Err(LowerNamedError::Message(error)) => {
            eprintln!("{}: {error}", "error".red().bold());
            return EXIT_USER_ERROR;
        }
    };
    if let Err(error) = write_bytes(output, source.as_bytes()) {
        eprintln!("{}: {error}", "error".red().bold());
        return EXIT_USER_ERROR;
    }
    EXIT_OK
}

fn parse_named_or_bind_payload(raw: &[u8]) -> Result<NamedTermDocument, String> {
    if let Ok(named) = serde_json::from_slice::<NamedTermDocument>(raw) {
        return Ok(named);
    }
    let payload = serde_json::from_slice::<Term>(raw)
        .map_err(|error| format!("parse named-term JSON or bind-result payload: {error}"))?;
    named_term_document_from_bind_payload(&payload).map_err(|error| error.to_string())
}

fn lower_named_document(
    project_root: &Path,
    target: &str,
    named: &NamedTermDocument,
) -> Result<String, LowerNamedError> {
    let mut out = String::new();
    for term in &named.terms {
        let mut spec = realize_spec_from_named_term(term).map_err(LowerNamedError::Message)?;
        // Apply fn_name_sugar override: if the bind payload carried a source function name
        // annotation (non-CID-affecting sugar), use it to restore the function name in the
        // realize request. This is the CLI pipe path equivalent of merge_realize_sidecar.
        let sugar_fn = realize_function_name_with_sugar(term);
        if spec.get("function").and_then(|v| v.as_str()) != Some(sugar_fn) {
            spec["function"] = Json::String(sugar_fn.to_string());
        }
        let source = lower_named_spec_via_path(project_root, target, spec)?;
        out.push_str(&source);
        if !out.ends_with('\n') {
            out.push('\n');
        }
    }
    Ok(out)
}

fn lower_named_spec_via_path(
    project_root: &Path,
    target: &str,
    spec: Json,
) -> Result<String, LowerNamedError> {
    let mut inputs = HashMapInputCatalog::default();
    let input_cid = inputs.insert(Input::Spec(spec));
    let kit_name = format!("lower-{target}");
    let path = Input::Path(Box::new(CorePath {
        algebra: vec![PathAlgebra {
            name: "lower".to_string(),
            kit: kit_name.clone(),
            inputs: vec![input_cid],
            depends_on: vec![],
            verb: Default::default(),
        }],
    }));
    let mut registry = KitRegistry::default();
    registry.register_with_platform_semantics(
        kit_name,
        LowerKit::new(
            project_root.to_path_buf(),
            target.to_string(),
            None,
            DispatchRealizeTransport,
        ),
        target,
        project_root.join(format!("implementations/{target}/conformance/fixtures")),
    );
    let chain = execute_path(&path, &registry, &inputs).map_err(|error| {
        let detail = error
            .composition_refusal()
            .and_then(|refusal| serde_json::to_string(refusal).ok())
            .unwrap_or_else(|| error.to_string());
        LowerNamedError::Message(format!("lower plugin unavailable for `{target}`: {detail}"))
    })?;
    LowerKit::<DispatchRealizeTransport>::realized_source_from_claim(chain.terminal_claim())
        .map(|realized| realized.source)
        .map_err(LowerNamedError::Message)
}

fn is_solver_target(target: &str) -> bool {
    matches!(target, "smt-lib" | "smtlib" | "coq" | "tptp" | "vampire")
}

fn read_bytes(path: Option<&PathBuf>) -> Result<Vec<u8>, String> {
    match path {
        Some(path) if path.as_os_str() != "-" => {
            std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))
        }
        _ => {
            let mut bytes = Vec::new();
            std::io::stdin()
                .read_to_end(&mut bytes)
                .map_err(|e| format!("read stdin: {e}"))?;
            Ok(bytes)
        }
    }
}

fn write_bytes(path: Option<&PathBuf>, bytes: &[u8]) -> Result<(), String> {
    match path {
        Some(path) if path.as_os_str() != "-" => {
            std::fs::write(path, bytes).map_err(|e| format!("write {}: {e}", path.display()))
        }
        _ => {
            let mut stdout = std::io::stdout().lock();
            stdout
                .write_all(bytes)
                .map_err(|e| format!("write stdout: {e}"))
        }
    }
}

pub(crate) fn lower_witness_requirement(
    project_root: &Path,
    requirement: &Json,
    out_dir: &Path,
    quiet: bool,
) -> Result<LowerProof, String> {
    let surface = required_str(requirement, "surface", "witness requirement")?;
    lower_witness_requirement_for_surface(project_root, surface, requirement, out_dir, quiet)
        .map_err(|failure| failure.message)
}

fn lower_witness_requirement_for_surface(
    project_root: &Path,
    surface: &str,
    requirement: &Json,
    out_dir: &Path,
    _quiet: bool,
) -> Result<LowerProof, LowerFailure> {
    let plan = build_realizer_plan(requirement).map_err(LowerFailure::message)?;
    let lower_result =
        dispatch_lower_witness(project_root, surface, &plan).map_err(LowerFailure::message)?;
    mint_witness_proof(project_root, surface, &plan, &lower_result, out_dir)
        .map_err(|message| LowerFailure::rejected(message, lower_result))
}

fn build_realizer_plan(requirement: &Json) -> Result<Json, String> {
    if requirement.get("kind").and_then(Json::as_str) == Some("RealizerPlan") {
        return Ok(requirement.clone());
    }
    let obligation = requirement
        .get("obligation")
        .cloned()
        .ok_or_else(|| "witness requirement missing obligation".to_string())?;
    let host = requirement
        .get("host")
        .cloned()
        .ok_or_else(|| "witness requirement missing host".to_string())?;
    let bindings = requirement
        .get("bindings")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let input_cids = requirement
        .get("inputCids")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let policy_cid = requirement
        .pointer("/policy/policyCid")
        .or_else(|| requirement.get("policyCid"))
        .and_then(Json::as_str)
        .unwrap_or("builtin:provekit-lower-witness-policy");
    Ok(json!({
        "kind": "RealizerPlan",
        "schemaVersion": "1",
        "mode": "attest",
        "obligation": obligation,
        "host": host,
        "bindings": bindings,
        "policyCid": policy_cid,
        "inputCids": input_cids,
    }))
}

fn mint_witness_proof(
    _project_root: &Path,
    surface: &str,
    plan: &Json,
    lower_result: &Json,
    out_dir: &Path,
) -> Result<LowerProof, String> {
    let output = lower_result
        .get("output")
        .ok_or_else(|| "lower result missing output".to_string())?;
    let status = output
        .get("status")
        .and_then(Json::as_str)
        .ok_or_else(|| "lower output missing status".to_string())?;
    if status != "witnessed" {
        let message = output
            .get("message")
            .and_then(Json::as_str)
            .unwrap_or("lower witness rejected");
        return Err(message.to_string());
    }

    let claim_body = lower_result
        .get("claimBody")
        .ok_or_else(|| "witnessed lower result missing claimBody".to_string())?;
    let evidence = lower_result
        .get("evidence")
        .ok_or_else(|| "witnessed lower result missing evidence".to_string())?;
    let claim_body_cid = jcs_cid(claim_body);
    let evidence_root_cid = lower_result
        .get("evidenceCid")
        .and_then(Json::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| jcs_cid(evidence));
    let claim_kind = lower_result
        .get("claimKind")
        .or_else(|| claim_body.get("claimKind"))
        .and_then(Json::as_str)
        .unwrap_or("orp-witness")
        .to_string();
    let verifier_cid = lower_result
        .get("verifierCid")
        .or_else(|| claim_body.get("verifierCid"))
        .and_then(Json::as_str)
        .unwrap_or("builtin:provekit-lower-witness")
        .to_string();
    let policy_cid = lower_result
        .get("policyCid")
        .or_else(|| claim_body.get("policyCid"))
        .or_else(|| plan.get("policyCid"))
        .and_then(Json::as_str)
        .unwrap_or("builtin:provekit-lower-witness-policy")
        .to_string();
    let produced_by = output
        .pointer("/realizer/name")
        .and_then(Json::as_str)
        .unwrap_or("provekit-lower")
        .to_string();
    let produced_at = lower_result
        .get("producedAt")
        .and_then(Json::as_str)
        .unwrap_or(DEFAULT_WITNESS_PRODUCED_AT)
        .to_string();

    let mut input_cids = Vec::new();
    collect_cid_array(lower_result.get("inputCids"), &mut input_cids);
    collect_cid_array(output.get("observedArtifactCids"), &mut input_cids);
    collect_cid_strings(claim_body.get("subjectCids"), &mut input_cids);
    input_cids.sort();
    input_cids.dedup();

    let signer_seed = deterministic_signer_seed(&produced_by);
    let witness = mint_witness(&MintWitnessArgs {
        claim_kind: claim_kind.clone(),
        claim_body_cid,
        verifier_cid,
        policy_cid,
        evidence_root_cid,
        input_cids,
        produced_by: produced_by.clone(),
        produced_at: produced_at.clone(),
        claim_body: json_to_cvalue(claim_body),
        evidence: json_to_cvalue(evidence),
        signer_seed,
    })
    .map_err(|e| format!("mint lower witness memento: {e}"))?;

    let mut members = BTreeMap::new();
    members.insert(witness.cid, witness.canonical_bytes);
    let mut metadata = BTreeMap::new();
    metadata.insert("provekit.lower.mode".into(), "witness".into());
    metadata.insert("provekit.lower.surface".into(), surface.to_string());
    metadata.insert("provekit.lower.claimKind".into(), claim_kind.clone());
    let proof = build_proof_envelope(&ProofEnvelopeInput {
        name: format!("@provekit/lower-witness/{claim_kind}"),
        version: "0.1.0".into(),
        binary_cid: None,
        metadata: Some(metadata),
        members,
        signer_cid: ed25519_pubkey_string(&signer_seed),
        signer_seed,
        declared_at: produced_at,
    });

    std::fs::create_dir_all(out_dir).map_err(|e| format!("mkdir {}: {e}", out_dir.display()))?;
    let proof_file = out_dir.join(format!("{}.proof", proof.cid));
    std::fs::write(&proof_file, &proof.bytes)
        .map_err(|e| format!("write {}: {e}", proof_file.display()))?;

    Ok(LowerProof {
        filename_cid: proof.cid,
        proof_file,
        bytes_written: proof.bytes.len(),
        output: lower_result.clone(),
    })
}

fn optional_str<'a>(value: &'a Json, field: &str) -> Option<&'a str> {
    value.get(field).and_then(Json::as_str)
}

fn required_str<'a>(value: &'a Json, field: &str, context: &str) -> Result<&'a str, String> {
    optional_str(value, field).ok_or_else(|| format!("{context} missing `{field}`"))
}

fn collect_cid_array(value: Option<&Json>, out: &mut Vec<String>) {
    let Some(values) = value.and_then(Json::as_array) else {
        return;
    };
    out.extend(
        values
            .iter()
            .filter_map(Json::as_str)
            .filter(|value| value.starts_with("blake3-512:"))
            .map(str::to_string),
    );
}

fn collect_cid_strings(value: Option<&Json>, out: &mut Vec<String>) {
    match value {
        Some(Json::String(s)) if s.starts_with("blake3-512:") => out.push(s.clone()),
        Some(Json::Array(items)) => {
            for item in items {
                collect_cid_strings(Some(item), out);
            }
        }
        Some(Json::Object(map)) => {
            for item in map.values() {
                collect_cid_strings(Some(item), out);
            }
        }
        _ => {}
    }
}

fn jcs_cid(value: &Json) -> String {
    let canonical = json_to_cvalue(value);
    let jcs = encode_jcs(&canonical);
    blake3_512_of(jcs.as_bytes())
}

fn deterministic_signer_seed(principal: &str) -> Ed25519Seed {
    let digest = blake3_512_of(format!("provekit-lower-signer:{principal}").as_bytes());
    let hex = digest
        .strip_prefix("blake3-512:")
        .expect("blake3_512_of returns tagged digest");
    let mut seed = [0u8; 32];
    for (idx, slot) in seed.iter_mut().enumerate() {
        let hi = hex_nibble(hex.as_bytes()[idx * 2]);
        let lo = hex_nibble(hex.as_bytes()[idx * 2 + 1]);
        *slot = (hi << 4) | lo;
    }
    seed
}

fn hex_nibble(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        b'A'..=b'F' => byte - b'A' + 10,
        _ => 0,
    }
}

fn json_to_cvalue(j: &Json) -> Arc<CValue> {
    match j {
        Json::Null => CValue::null(),
        Json::Bool(b) => CValue::boolean(*b),
        Json::Number(n) => {
            if let Some(i) = n.as_i64() {
                CValue::integer(i)
            } else if let Some(u) = n.as_u64() {
                CValue::integer(u as i64)
            } else if let Some(f) = n.as_f64() {
                CValue::integer(f as i64)
            } else {
                CValue::integer(0)
            }
        }
        Json::String(s) => CValue::string(s.clone()),
        Json::Array(items) => {
            let v: Vec<_> = items.iter().map(json_to_cvalue).collect();
            CValue::array(v)
        }
        Json::Object(map) => {
            let entries: Vec<(String, Arc<CValue>)> = map
                .iter()
                .map(|(k, v)| (k.clone(), json_to_cvalue(v)))
                .collect();
            CValue::object(entries)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_realizer_plan_maps_witness_requirement_to_attest_plan() {
        let requirement = json!({
            "surface": "c",
            "mode": "witness",
            "obligation": {"kind": "predicate", "name": "checked_add_u8.postcondition"},
            "host": {"kit": "c", "artifact": "artifacts/software/checked_add_u8.c"},
            "policy": {"policyCid": "builtin:bridgeworks.checked-add-u8"}
        });
        let plan = build_realizer_plan(&requirement).expect("plan builds");
        assert_eq!(plan["kind"], "RealizerPlan");
        assert_eq!(plan["mode"], "attest");
        assert_eq!(plan["obligation"]["name"], "checked_add_u8.postcondition");
        assert_eq!(plan["policyCid"], "builtin:bridgeworks.checked-add-u8");
    }
}
