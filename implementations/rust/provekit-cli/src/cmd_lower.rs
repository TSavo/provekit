// SPDX-License-Identifier: Apache-2.0
//
// `provekit lower --target=<lang>` dispatches named substrate terms to the
// per-language lower plugin and emits target-language source.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Instant;

use clap::{Parser, ValueEnum};
use libprovekit::core::{
    execute_path, HashMapInputCatalog, Input, KitRegistry, LowerKit, Path as CorePath, PathAlgebra,
};
use owo_colors::OwoColorize;
use serde_json::{json, Value as Json};

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value as CValue};
use provekit_claim_envelope::{mint_witness, MintWitnessArgs};
use provekit_proof_envelope::{
    build_proof_envelope, ed25519_pubkey_string, Ed25519Seed, ProofEnvelopeInput,
};

use crate::cmd_bind::NamedTermDocument;
use crate::kit_dispatch::DispatchRealizeTransport;
use crate::{OutputFlags, EXIT_OK, EXIT_USER_ERROR, EXIT_VERIFY_FAIL};

const LOWER_PROTOCOL_VERSION: &str = "provekit-orp/1";
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct MissingTemplateEntry {
    operation: String,
    args_shape: Vec<String>,
    function: String,
    term_position: String,
}

#[derive(Debug, Clone)]
enum LowerNamedError {
    Message(String),
    MissingTemplates(Vec<MissingTemplateEntry>),
}

#[derive(Debug, Default)]
struct PluginManifest {
    name: String,
    command: Vec<String>,
    working_dir: Option<PathBuf>,
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
    let named: NamedTermDocument = match serde_json::from_slice(&raw) {
        Ok(named) => named,
        Err(error) => {
            eprintln!("{}: parse named-term JSON: {error}", "error".red().bold());
            return EXIT_USER_ERROR;
        }
    };
    let project_root = project
        .cloned()
        .or_else(|| named.workspace_root.as_ref().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    let source = match lower_named_document(&project_root, target, &named) {
        Ok(source) => source,
        Err(LowerNamedError::MissingTemplates(entries)) => {
            eprintln!("{}", missing_template_receipt(target, &entries));
            return EXIT_VERIFY_FAIL;
        }
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

fn lower_named_document(
    project_root: &Path,
    target: &str,
    named: &NamedTermDocument,
) -> Result<String, LowerNamedError> {
    let mut out = String::new();
    let mut missing_templates = Vec::new();
    for term in &named.terms {
        let named_term_tree = term
            .named_term_tree
            .as_ref()
            .map(serde_json::to_value)
            .transpose()
            .map_err(|e| {
                LowerNamedError::Message(format!(
                    "serialize namedTermTree for `{}`: {e}",
                    term.function
                ))
            })?;
        let spec = lower_named_spec(term, named_term_tree);
        let source = match lower_named_spec_via_path(project_root, target, spec) {
            Ok(source) => source,
            Err(LowerNamedError::MissingTemplates(entries)) => {
                missing_templates.extend(entries);
                continue;
            }
            Err(error) => return Err(error),
        };
        out.push_str(&source);
        if !out.ends_with('\n') {
            out.push('\n');
        }
    }
    if !missing_templates.is_empty() {
        return Err(LowerNamedError::MissingTemplates(missing_templates));
    }
    Ok(out)
}

fn lower_named_spec(term: &crate::cmd_bind::NamedTerm, named_term_tree: Option<Json>) -> Json {
    json!({
        "kind": "RealizeRequest",
        "function": term.function,
        "params": term.params,
        "paramTypes": term.param_types,
        "returnType": term.return_type,
        "conceptName": term.concept_name,
        "namedTermTree": named_term_tree,
        "termShapeCid": term.term_shape_cid,
    })
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
        }],
    }));
    let mut registry = KitRegistry::default();
    registry.register(
        kit_name,
        LowerKit::new(
            project_root.to_path_buf(),
            target.to_string(),
            None,
            DispatchRealizeTransport,
        ),
    );
    let claim = execute_path(&path, &registry, &inputs).map_err(|error| {
        let detail = error.to_string();
        if let Some(entries) = missing_templates_from_detail(&detail) {
            LowerNamedError::MissingTemplates(entries)
        } else {
            LowerNamedError::Message(format!("lower plugin unavailable for `{target}`: {detail}"))
        }
    })?;
    LowerKit::<DispatchRealizeTransport>::realized_source_from_claim(&claim)
        .map(|realized| realized.source)
        .map_err(LowerNamedError::Message)
}

fn missing_templates_from_detail(detail: &str) -> Option<Vec<MissingTemplateEntry>> {
    let json_start = detail.find('{')?;
    let error: Json = serde_json::from_str(&detail[json_start..]).ok()?;
    missing_templates_from_error_json(&error)
}

fn missing_templates_from_error_json(error: &Json) -> Option<Vec<MissingTemplateEntry>> {
    let code_matches = error.get("code").and_then(Json::as_i64) == Some(-32100);
    let message_matches = error
        .get("message")
        .and_then(Json::as_str)
        .is_some_and(|message| message == "missing body-template entry");
    if !code_matches && !message_matches {
        return None;
    }
    let data = error.get("data")?;
    let items: Vec<&Json> = match data {
        Json::Array(items) => items.iter().collect(),
        Json::Object(_) => vec![data],
        _ => return None,
    };
    let entries = items
        .into_iter()
        .filter_map(missing_template_entry_from_json)
        .collect::<Vec<_>>();
    if entries.is_empty() {
        None
    } else {
        Some(entries)
    }
}

fn missing_template_entry_from_json(value: &Json) -> Option<MissingTemplateEntry> {
    let operation = value
        .get("operation_kind")
        .or_else(|| value.get("operation"))
        .and_then(Json::as_str)?
        .to_string();
    let args_shape = value
        .get("args_shape")
        .and_then(Json::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Json::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let function = value
        .get("function")
        .or_else(|| value.get("function_context"))
        .and_then(Json::as_str)
        .unwrap_or("<unknown>")
        .to_string();
    let term_position = value
        .get("term_position")
        .and_then(Json::as_str)
        .unwrap_or("<unknown>")
        .to_string();
    Some(MissingTemplateEntry {
        operation,
        args_shape,
        function,
        term_position,
    })
}

fn missing_template_receipt(target: &str, entries: &[MissingTemplateEntry]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "ERROR: provekit lower --target={target} refused.\n"
    ));
    out.push_str("The substrate could not realize the input via body-templates.\n\n");
    out.push_str(&format!(
        "{} body-template {} needed:\n",
        entries.len(),
        if entries.len() == 1 {
            "entry"
        } else {
            "entries"
        }
    ));
    for (index, entry) in entries.iter().enumerate() {
        let args_shape = format_args_shape(&entry.args_shape);
        out.push_str(&format!(
            "\n  {}. operation: {}\n     args_shape: {}\n     function: {}\n     term_position: {}\n     suggest adding to: {}\n",
            index + 1,
            entry.operation,
            args_shape,
            entry.function,
            entry.term_position,
            suggested_body_template_file(&entry.operation),
        ));
    }
    out.push_str("\nAuthor these entries in the appropriate body-template JSON, then re-run.");
    out
}

fn format_args_shape(args_shape: &[String]) -> String {
    if args_shape.is_empty() {
        return "[]".to_string();
    }
    let rendered = args_shape
        .iter()
        .map(|value| serde_json::to_string(value).unwrap_or_else(|_| "\"<invalid>\"".to_string()))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{rendered}]")
}

fn suggested_body_template_file(operation: &str) -> &'static str {
    if operation == "rust-call:hex::encode"
        || operation.starts_with("rust-call:blake3::")
        || operation.starts_with("rust-method:hasher-")
    {
        return "python-canonical-bodies-blake3.json";
    }
    if operation.starts_with("rust-call:")
        || operation.starts_with("rust-method:")
        || matches!(
            operation,
            "concept:add"
                | "concept:array-repeat"
                | "concept:borrow"
                | "concept:method-len"
                | "concept:new"
                | "concept:return"
                | "concept:str-len"
                | "concept:string-push-str"
                | "concept:string-with-capacity"
        )
    {
        return "python-canonical-bodies-rust-runtime.json";
    }
    "python-canonical-bodies.json"
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
    quiet: bool,
) -> Result<LowerProof, LowerFailure> {
    let plan = build_realizer_plan(requirement).map_err(LowerFailure::message)?;
    let lower_result = dispatch_lower(project_root, surface, "witness", &plan, quiet)
        .map_err(LowerFailure::message)?;
    mint_witness_proof(project_root, surface, &plan, &lower_result, out_dir)
        .map_err(|message| LowerFailure::rejected(message, lower_result))
}

fn parse_manifest(path: &Path) -> Result<PluginManifest, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let mut m = PluginManifest::default();
    for line in text.lines() {
        let line = match line.find('#') {
            Some(p) => &line[..p],
            None => line,
        }
        .trim();
        if line.is_empty() || line.starts_with('[') {
            continue;
        }
        let Some(eq) = line.find('=') else { continue };
        let key = line[..eq].trim();
        let val = line[eq + 1..].trim();
        match key {
            "name" => m.name = val.trim_matches('"').to_string(),
            "working_dir" => m.working_dir = Some(PathBuf::from(val.trim_matches('"'))),
            "command" => {
                let inner = val.trim_matches(|c| c == '[' || c == ']');
                m.command = inner
                    .split(',')
                    .map(|s| s.trim().trim_matches('"').to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            _ => {}
        }
    }
    if m.command.is_empty() {
        return Err(format!("manifest {} has no `command`", path.display()));
    }
    Ok(m)
}

fn find_manifest(project_root: &Path, surface: &str) -> Result<PluginManifest, String> {
    let project_local = project_root
        .join(".provekit")
        .join("lower")
        .join(surface)
        .join("manifest.toml");
    if project_local.exists() {
        return parse_manifest(&project_local);
    }
    if let Some(home) = std::env::var_os("HOME") {
        let user_global = PathBuf::from(home)
            .join(".config")
            .join("provekit")
            .join("lower")
            .join(surface)
            .join("manifest.toml");
        if user_global.exists() {
            return parse_manifest(&user_global);
        }
    }
    Err(format!(
        "no lower plugin manifest for surface `{surface}` (looked in .provekit/lower/{surface}/manifest.toml and ~/.config/provekit/lower/{surface}/manifest.toml)"
    ))
}

fn dispatch_lower(
    project_root: &Path,
    surface: &str,
    mode: &str,
    plan: &Json,
    quiet: bool,
) -> Result<Json, String> {
    let started = Instant::now();
    let manifest = find_manifest(project_root, surface)?;
    if !quiet {
        println!(
            "{}: surface=`{}` plugin=`{}` command={:?}",
            "lower".green().bold(),
            surface,
            manifest.name,
            manifest.command
        );
    }

    let mut cmd = Command::new(&manifest.command[0]);
    if manifest.command.len() > 1 {
        cmd.args(&manifest.command[1..]);
    }
    if !manifest.command.iter().any(|arg| arg == "--rpc") {
        cmd.arg("--rpc");
    }
    if let Some(wd) = &manifest.working_dir {
        let resolved = if wd.is_absolute() {
            wd.clone()
        } else {
            project_root.join(wd)
        };
        cmd.current_dir(resolved);
    }
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::inherit());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("spawn lower plugin {:?}: {e}", manifest.command))?;
    let mut stdin = child.stdin.take().ok_or("lower plugin stdin unavailable")?;
    let stdout = child
        .stdout
        .take()
        .ok_or("lower plugin stdout unavailable")?;
    let mut reader = BufReader::new(stdout);

    let workspace_root = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    let init_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "client": {"name": "provekit-cli", "version": env!("CARGO_PKG_VERSION")},
            "protocol_version": LOWER_PROTOCOL_VERSION,
            "workspace_root": workspace_root,
        }
    });
    writeln!(stdin, "{init_req}").map_err(|e| format!("write lower initialize: {e}"))?;
    let _ = read_response(&mut reader, 1)?;

    let realize_req = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "realize",
        "params": {
            "mode": mode,
            "surface": surface,
            "workspace_root": workspace_root,
            "plan": plan,
        }
    });
    writeln!(stdin, "{realize_req}").map_err(|e| format!("write lower realize: {e}"))?;
    let result = read_response(&mut reader, 2)?;

    let shutdown_req = json!({"jsonrpc":"2.0","id":3,"method":"shutdown"});
    let _ = writeln!(stdin, "{shutdown_req}");
    drop(stdin);
    let status = child
        .wait()
        .map_err(|e| format!("wait lower plugin: {e}"))?;
    if !status.success() {
        return Err(format!(
            "lower plugin exited {status} after {:?}",
            started.elapsed()
        ));
    }
    Ok(result)
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

fn read_response(reader: &mut impl BufRead, id: i64) -> Result<Json, String> {
    let mut line = String::new();
    let n = reader
        .read_line(&mut line)
        .map_err(|e| format!("read lower response: {e}"))?;
    if n == 0 {
        return Err("lower plugin closed stdout before responding".into());
    }
    let v: Json = serde_json::from_str(line.trim())
        .map_err(|e| format!("parse lower JSON-RPC response: {e}\n  raw: {line}"))?;
    if v.get("id").and_then(Json::as_i64) != Some(id) {
        return Err(format!(
            "lower response id mismatch: expected {id}, got {v:?}"
        ));
    }
    if let Some(error) = v.get("error") {
        if let Some(message) = error.get("message").and_then(Json::as_str) {
            return Err(message.to_string());
        }
        return Err(format!("lower plugin returned error: {error}"));
    }
    v.get("result")
        .cloned()
        .ok_or_else(|| "lower response missing result".to_string())
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

    #[test]
    fn parses_missing_template_error_detail() {
        let detail = r#"realize kit error: {"code":-32100,"message":"missing body-template entry","data":[{"operation_kind":"call:Widget::build","args_shape":["int"],"function":"unknown_call","term_position":"body.return.call:Widget::build"},{"operation_kind":"missing-concept","args_shape":["str"],"function":"second","term_position":"body"}]}"#;
        let parsed = missing_templates_from_detail(detail).expect("structured detail parses");

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].operation, "call:Widget::build");
        assert_eq!(parsed[0].args_shape, vec!["int"]);
        assert_eq!(parsed[0].function, "unknown_call");
        assert_eq!(parsed[0].term_position, "body.return.call:Widget::build");
        assert_eq!(parsed[1].operation, "missing-concept");
    }

    #[test]
    fn formats_missing_template_receipt_with_all_entries() {
        let entries = vec![
            MissingTemplateEntry {
                operation: "match_arm".to_string(),
                args_shape: vec!["pattern".to_string(), "result".to_string()],
                function: "encode_value".to_string(),
                term_position: "let.rhs.match.arm[0]".to_string(),
            },
            MissingTemplateEntry {
                operation: "rust-call:hex::encode".to_string(),
                args_shape: vec!["bytes".to_string()],
                function: "blake3_512_of".to_string(),
                term_position: "body.let.rhs".to_string(),
            },
        ];

        let receipt = missing_template_receipt("python", &entries);

        assert!(receipt.contains("ERROR: provekit lower --target=python refused."));
        assert!(receipt.contains("2 body-template entries needed:"));
        assert!(receipt.contains("operation: match_arm"));
        assert!(receipt.contains("args_shape: [\"pattern\", \"result\"]"));
        assert!(receipt.contains("function: encode_value"));
        assert!(receipt.contains("term_position: let.rhs.match.arm[0]"));
        assert!(receipt.contains("suggest adding to: python-canonical-bodies.json"));
        assert!(receipt.contains("operation: rust-call:hex::encode"));
        assert!(receipt.contains("suggest adding to: python-canonical-bodies-blake3.json"));
        assert!(receipt.contains("Author these entries in the appropriate body-template JSON"));
    }
}
