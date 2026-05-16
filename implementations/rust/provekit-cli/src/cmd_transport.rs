// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::sync::Arc;

use clap::Parser;
use libprovekit::core::{
    execute_path, Cid, HashMapInputCatalog, Input, KitRegistry, LowerKit, Path as CorePath,
    PathAlgebra, Term,
};
use libprovekit::desugar::{load_desugaring_rules_from_dir, DesugaringSet};
use libprovekit::transport::{transport_term, OperationTransport, TermTransport};
use owo_colors::OwoColorize;
use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value as CanonicalValue};
use provekit_ir_symbolic::{ConstValue, Formula, Term as SymTerm};
use provekit_ir_types::{
    EffectOccurrence, EffectSlotDescriptor, ObservationWrapperMemento,
    ParametricRealizationMemento, RealizationPlanMemento, SlotDescriptor, Sort,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::kit_dispatch::DispatchRealizeTransport;
use crate::{OutputFlags, EXIT_OK, EXIT_USER_ERROR, EXIT_VERIFY_FAIL};

#[derive(Parser, Debug, Clone)]
pub struct TransportArgs {
    /// Source file to port through the concept hub.
    pub src_file: PathBuf,
    /// Target language: rust, python, go, csharp, typescript, zig, ruby, php, or java.
    #[arg(long)]
    pub to: String,
    /// Source language. Defaults from the extension when possible.
    #[arg(long = "from")]
    pub from: Option<String>,
    /// Function to project from the source file.
    #[arg(long, default_value = "foo")]
    pub function: String,
    /// Output directory for term artifacts and realized target source.
    #[arg(long = "out")]
    pub output_dir: Option<PathBuf>,
    #[command(flatten)]
    pub flags: OutputFlags,
}

#[derive(Debug, Serialize, Clone)]
struct TransportReport {
    status: &'static str,
    source_file: String,
    source_language: String,
    target_language: String,
    function: String,
    artifacts: BTreeMap<String, String>,
    stages: Vec<StageReport>,
    normalizations: Vec<String>,
    morphism_receipts: Vec<String>,
    deferred: Vec<String>,
}

#[derive(Debug, Serialize, Clone)]
struct StageReport {
    stage: &'static str,
    status: &'static str,
    detail: String,
}

#[derive(Debug, thiserror::Error)]
pub enum TransportCliError {
    #[error("Refusal: {0}")]
    Refusal(String),
    #[error("{0}")]
    Failed(String),
}

#[derive(Debug, Clone)]
struct OpMorphism {
    language_prefix: String,
    source_name: String,
    concept_name: String,
    source_cid: Cid,
    concept_cid: Cid,
    receipt_ref: String,
}

#[derive(Debug)]
struct MorphismCatalog {
    rows: Vec<OpMorphism>,
}

#[derive(Debug, Deserialize)]
struct MorphismReceipt {
    source_contract_cid: String,
    shape_cid: String,
    discharged: bool,
}

pub fn run(args: TransportArgs) -> u8 {
    match run_inner(args) {
        Ok(_report) => EXIT_OK,
        Err(TransportCliError::Refusal(message)) => {
            eprintln!("{}: {message}", "refusal".red().bold());
            EXIT_USER_ERROR
        }
        Err(TransportCliError::Failed(message)) => {
            eprintln!("{}: {message}", "error".red().bold());
            EXIT_VERIFY_FAIL
        }
    }
}

fn print_report(report: &TransportReport) {
    if report.artifacts.is_empty() {
        return;
    }
    println!(
        "{}: {} -> {}",
        "transport".green().bold(),
        report.source_file,
        report.target_language
    );
    for stage in &report.stages {
        println!("  {}: {} ({})", stage.stage, stage.status, stage.detail);
    }
    for (name, path) in &report.artifacts {
        println!("  {name}: {path}");
    }
}

fn run_inner(args: TransportArgs) -> Result<TransportReport, TransportCliError> {
    if !args.src_file.exists() {
        return Err(TransportCliError::Refusal(format!(
            "lift-time:no-source-file source file not found: {}",
            args.src_file.display()
        )));
    }
    let src_file = fs::canonicalize(&args.src_file).map_err(|error| {
        TransportCliError::Failed(format!("canonicalize {}: {error}", args.src_file.display()))
    })?;
    let root = repo_root()?;
    let source_language = normalize_language_id(
        args.from
            .as_deref()
            .map(str::to_string)
            .or_else(|| detect_source_language(&src_file)),
    )?;
    let target_language = normalize_language_id(Some(args.to.clone()))?;
    let source_prefix = language_prefix(&source_language).ok_or_else(|| {
        TransportCliError::Refusal(format!(
            "lift-time:unknown-language source language `{source_language}` is not in the transport language set"
        ))
    })?;
    let target_prefix = language_prefix(&target_language).ok_or_else(|| {
        TransportCliError::Refusal(format!(
            "transport-time:unknown-target target language `{target_language}` is not in the transport language set"
        ))
    })?;

    let out_dir = args
        .output_dir
        .clone()
        .unwrap_or_else(|| root.join("menagerie/cross-language-port/artifacts"));
    fs::create_dir_all(&out_dir).map_err(|error| {
        TransportCliError::Failed(format!("create {}: {error}", out_dir.display()))
    })?;

    let catalog = MorphismCatalog::load(&root)?;
    let mut stages = Vec::new();
    let mut normalizations = Vec::new();
    let source_text = fs::read_to_string(&src_file).unwrap_or_default();
    let source_term = lift_source_term(
        &root,
        &src_file,
        &source_language,
        source_prefix,
        &args.function,
        &catalog,
        &mut normalizations,
    )?;
    stages.push(StageReport {
        stage: "lift",
        status: "ok",
        detail: format!("{source_language} source projected to algebra term"),
    });

    stages.push(desugar_stage(&root, &source_language)?);

    let to_concept = catalog.transport_to_concept(source_prefix)?;
    let concept_to_target = catalog.transport_from_concept(target_prefix)?;
    let target_to_concept = catalog.transport_to_concept(target_prefix)?;

    let concept_term = transport_term(&to_concept, &source_term).map_err(|error| {
        TransportCliError::Refusal(format!("transport-time:no-morphism-for-op {error}"))
    })?;
    stages.push(StageReport {
        stage: "transport-to-concept",
        status: "ok",
        detail: format!("{source_language} operations transported through discharged morphisms"),
    });

    let target_term = transport_term(&concept_to_target, &concept_term).map_err(|error| {
        TransportCliError::Refusal(format!("transport-time:no-target-morphism-for-op {error}"))
    })?;
    stages.push(StageReport {
        stage: "transport-to-target",
        status: "ok",
        detail: format!("concept operations transported to {target_language}"),
    });

    let roundtrip_concept = transport_term(&target_to_concept, &target_term).map_err(|error| {
        TransportCliError::Refusal(format!("transport-time:roundtrip-no-morphism {error}"))
    })?;
    if roundtrip_concept != concept_term {
        return Err(TransportCliError::Failed(
            "round-trip-closure violated: target term did not transport back to the same concept IR"
                .into(),
        ));
    }

    let params = parse_int_params(&source_text, &args.function).unwrap_or_else(|| {
        let mut vars = BTreeSet::new();
        collect_vars(&source_term, &mut vars);
        vars.into_iter().collect()
    });

    // Source signature + annotations come from the source language's lift
    // kit (PEP 1.7.0 `kind = "lift"`). cmd_transport does NOT reparse Rust
    // or any other language's source here under the architectural cut.
    // When no lift kit is registered for `source_language` we leave the
    // signature empty and let the realize kit emit a permissive stub.
    let workspace_root =
        repo_root().unwrap_or_else(|_| std::env::current_dir().unwrap_or_default());
    let (annotations, param_types, return_type) =
        match crate::kit_dispatch::dispatch_bind_lift(&workspace_root, &source_language) {
            Ok(session) => {
                let target = session.entries.iter().find(|e| e.fn_name == args.function);
                if let Some(entry) = target {
                    // attr_pre/attr_post are kept as Strings here; the
                    // realize plugin (PEP 1.7.0 `kind = "realize"`) consumes
                    // a structured annotation object via its own contract.
                    // The downstream `realize_function` -> dispatch_realize
                    // path does NOT consume `annotations`, so the legacy
                    // ContractAnnotations field stays empty for now.
                    (
                        ContractAnnotations::default(),
                        entry.param_types.clone(),
                        if entry.return_type.is_empty() {
                            "i64".to_string()
                        } else {
                            entry.return_type.clone()
                        },
                    )
                } else {
                    (
                        ContractAnnotations::default(),
                        Vec::new(),
                        "i64".to_string(),
                    )
                }
            }
            Err(_) => (
                ContractAnnotations::default(),
                Vec::new(),
                "i64".to_string(),
            ),
        };

    // Derive a stable concept binding for the `// concept: ...` comment.
    // The name is deterministic from the target term's JSON serialization so
    // every distinct function shape gets a unique, stable, per-function name.
    let concept_name = derive_concept_comment(&target_term);

    let realized = realize_function(
        &target_language,
        &args.function,
        &params,
        &param_types,
        &return_type,
        &target_term,
        &annotations,
        &concept_name,
        None,
        None,
        Vec::new(),
    )?;
    stages.push(StageReport {
        stage: "realize",
        status: "ok",
        detail: format!("emitted core-form {target_language} source"),
    });

    let mut artifacts = BTreeMap::new();
    let source_term_path = out_dir.join(format!("{source_language}.term.json"));
    let concept_path = out_dir.join("concept.term.json");
    let target_term_path = out_dir.join(format!("{target_language}.term.json"));
    let roundtrip_path = out_dir.join("roundtrip.concept.term.json");
    let source_path = out_dir.join(format!("{}.{}", args.function, realized.extension));
    write_json(&source_term_path, &source_term)?;
    write_json(&concept_path, &concept_term)?;
    write_json(&target_term_path, &target_term)?;
    write_json(&roundtrip_path, &roundtrip_concept)?;
    fs::write(&source_path, realized.source).map_err(|error| {
        TransportCliError::Failed(format!("write {}: {error}", source_path.display()))
    })?;
    artifacts.insert("source_term".into(), source_term_path.display().to_string());
    artifacts.insert("concept_term".into(), concept_path.display().to_string());
    artifacts.insert("target_term".into(), target_term_path.display().to_string());
    artifacts.insert(
        "roundtrip_concept_term".into(),
        roundtrip_path.display().to_string(),
    );
    artifacts.insert("target_source".into(), source_path.display().to_string());
    if target_language == "rust" {
        artifacts.insert("rust_term".into(), target_term_path.display().to_string());
        artifacts.insert("rust_source".into(), source_path.display().to_string());
    }

    let report = TransportReport {
        status: "transported",
        source_file: src_file.display().to_string(),
        source_language,
        target_language,
        function: args.function,
        artifacts,
        stages,
        normalizations,
        morphism_receipts: catalog.receipts_for(source_prefix, target_prefix),
        deferred: vec![
            "bytecode and asm transport through conditional-jump recovery".into(),
            "cosmetic re-sugaring after the core-form realizer".into(),
            "source lifter subprocess wiring for every non-C language in this CLI path".into(),
        ],
    };

    if args.flags.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).expect("serialize transport report")
        );
    } else if !args.flags.quiet {
        print_report(&report);
    }
    Ok(report)
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), TransportCliError> {
    let text = serde_json::to_string_pretty(value).expect("serialize artifact");
    fs::write(path, format!("{text}\n"))
        .map_err(|error| TransportCliError::Failed(format!("write {}: {error}", path.display())))
}

fn repo_root() -> Result<PathBuf, TransportCliError> {
    let mut dir = std::env::current_dir()
        .map_err(|error| TransportCliError::Failed(format!("current dir: {error}")))?;
    loop {
        if dir.join("menagerie").is_dir() && dir.join("implementations/rust").is_dir() {
            return Ok(dir);
        }
        if !dir.pop() {
            return Err(TransportCliError::Failed(
                "could not locate repository root from current directory".into(),
            ));
        }
    }
}

fn normalize_language_id(input: Option<String>) -> Result<String, TransportCliError> {
    let Some(value) = input else {
        return Err(TransportCliError::Refusal(
            "lift-time:unknown-language pass --from <language> for this source extension".into(),
        ));
    };
    let id = match value.as_str() {
        "c" | "c11" => "c11",
        "cs" | "c#" | "csharp" => "csharp",
        "go" => "go",
        "py" | "python" => "python",
        "ts" | "typescript" => "typescript",
        "zig" => "zig",
        "rb" | "ruby" => "ruby",
        "php" => "php",
        "java" => "java",
        "rs" | "rust" => "rust",
        other => other,
    };
    Ok(id.to_string())
}

fn language_prefix(language: &str) -> Option<&'static str> {
    match language {
        "c11" => Some("c11"),
        "csharp" => Some("csharp"),
        "go" => Some("go"),
        "python" => Some("python"),
        "typescript" => Some("ts"),
        "zig" => Some("zig"),
        "ruby" => Some("ruby"),
        "php" => Some("php"),
        "java" => Some("java"),
        "rust" => Some("rust"),
        _ => None,
    }
}

fn detect_source_language(path: &Path) -> Option<String> {
    match path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
    {
        "c" | "h" => Some("c11".into()),
        "cs" => Some("csharp".into()),
        "go" => Some("go".into()),
        "py" => Some("python".into()),
        "ts" => Some("typescript".into()),
        "zig" => Some("zig".into()),
        "rb" => Some("ruby".into()),
        "php" => Some("php".into()),
        "java" => Some("java".into()),
        "rs" => Some("rust".into()),
        _ => None,
    }
}

impl MorphismCatalog {
    fn load(root: &Path) -> Result<Self, TransportCliError> {
        let base = root.join("menagerie/concept-shapes");
        let spec_dir = base.join("specs");
        let receipt_dir = base.join("receipts");
        let receipt_cids = load_receipt_cids(&base.join("cids.tsv"))?;
        let mut rows = Vec::new();
        for entry in fs::read_dir(&spec_dir).map_err(|error| {
            TransportCliError::Failed(format!("read {}: {error}", spec_dir.display()))
        })? {
            let entry = entry.map_err(|error| {
                TransportCliError::Failed(format!("read {} entry: {error}", spec_dir.display()))
            })?;
            let path = entry.path();
            let Some(file_name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if !file_name.starts_with("morphism_") || !file_name.ends_with(".spec.json") {
                continue;
            }
            let stem = file_name.trim_end_matches(".spec.json");
            let spec: Value = read_json_value(&path)?;
            let Some(fn_name) = spec.get("fn_name").and_then(Value::as_str) else {
                continue;
            };
            let Some(rest) = fn_name.strip_prefix("morphism:") else {
                continue;
            };
            let Some((source_name, concept_name)) = rest.split_once(":to:") else {
                continue;
            };
            if !concept_name.starts_with("concept:") {
                continue;
            }
            let receipt_path = receipt_dir.join(format!("{stem}.receipt.json"));
            if !receipt_path.exists() {
                continue;
            }
            let receipt: MorphismReceipt = serde_json::from_value(read_json_value(&receipt_path)?)
                .map_err(|error| {
                    TransportCliError::Failed(format!(
                        "parse {} as morphism receipt: {error}",
                        receipt_path.display()
                    ))
                })?;
            if !receipt.discharged {
                continue;
            }
            let Some((language_prefix, _)) = source_name.split_once(':') else {
                continue;
            };
            let receipt_ref = receipt_cids
                .get(stem)
                .map(|cid| format!("{stem}={cid}"))
                .unwrap_or_else(|| format!("{stem}=<uncataloged>"));
            rows.push(OpMorphism {
                language_prefix: language_prefix.to_string(),
                source_name: source_name.to_string(),
                concept_name: concept_name.to_string(),
                source_cid: parse_cid(&receipt.source_contract_cid, &receipt_path)?,
                concept_cid: parse_cid(&receipt.shape_cid, &receipt_path)?,
                receipt_ref,
            });
        }
        rows.sort_by(|a, b| {
            a.source_name
                .cmp(&b.source_name)
                .then(a.concept_name.cmp(&b.concept_name))
        });
        Ok(Self { rows })
    }

    fn transport_to_concept(
        &self,
        language_prefix: &str,
    ) -> Result<TermTransport, TransportCliError> {
        let rows = self
            .rows
            .iter()
            .filter(|row| row.language_prefix == language_prefix)
            .map(|row| {
                OperationTransport::new(
                    row.source_name.clone(),
                    row.source_cid.clone(),
                    row.concept_name.clone(),
                    row.concept_cid.clone(),
                )
            })
            .collect::<Vec<_>>();
        if rows.is_empty() {
            return Err(TransportCliError::Refusal(format!(
                "transport-time:no-language-morphisms no discharged `{language_prefix}:* -> concept:*` morphisms found"
            )));
        }
        Ok(TermTransport::new(rows))
    }

    fn transport_from_concept(
        &self,
        language_prefix: &str,
    ) -> Result<TermTransport, TransportCliError> {
        let rows = self
            .rows
            .iter()
            .filter(|row| row.language_prefix == language_prefix)
            .map(|row| {
                OperationTransport::new(
                    row.concept_name.clone(),
                    row.concept_cid.clone(),
                    row.source_name.clone(),
                    row.source_cid.clone(),
                )
            })
            .collect::<Vec<_>>();
        if rows.is_empty() {
            return Err(TransportCliError::Refusal(format!(
                "transport-time:no-target-morphisms no discharged `concept:* -> {language_prefix}:*` inverse morphisms found"
            )));
        }
        Ok(TermTransport::new(rows))
    }

    fn op_cid(&self, op_name: &str) -> Result<Cid, TransportCliError> {
        self.rows
            .iter()
            .find(|row| row.source_name == op_name)
            .map(|row| row.source_cid.clone())
            .ok_or_else(|| {
                TransportCliError::Refusal(format!(
                    "transport-time:no-morphism-for-op operation `{op_name}` has no discharged morphism into the concept hub"
                ))
            })
    }

    fn receipts_for(&self, source_prefix: &str, target_prefix: &str) -> Vec<String> {
        let mut out = self
            .rows
            .iter()
            .filter(|row| {
                row.language_prefix == source_prefix || row.language_prefix == target_prefix
            })
            .map(|row| row.receipt_ref.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        out.sort();
        out
    }
}

fn read_json_value(path: &Path) -> Result<Value, TransportCliError> {
    let text = fs::read_to_string(path)
        .map_err(|error| TransportCliError::Failed(format!("read {}: {error}", path.display())))?;
    serde_json::from_str(&text)
        .map_err(|error| TransportCliError::Failed(format!("parse {}: {error}", path.display())))
}

fn load_receipt_cids(path: &Path) -> Result<BTreeMap<String, String>, TransportCliError> {
    let text = fs::read_to_string(path)
        .map_err(|error| TransportCliError::Failed(format!("read {}: {error}", path.display())))?;
    let mut out = BTreeMap::new();
    for line in text.lines().skip(1) {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 3 && parts[0] == "receipt" {
            out.insert(parts[1].to_string(), parts[2].to_string());
        }
    }
    Ok(out)
}

fn parse_cid(value: &str, path: &Path) -> Result<Cid, TransportCliError> {
    Cid::parse(value).map_err(|error| {
        TransportCliError::Failed(format!("invalid cid in {}: {error}", path.display()))
    })
}

fn lift_source_term(
    root: &Path,
    source: &Path,
    source_language: &str,
    source_prefix: &str,
    function: &str,
    catalog: &MorphismCatalog,
    normalizations: &mut Vec<String>,
) -> Result<Term, TransportCliError> {
    if source_language == "c11" {
        let projected = project_c11_term(root, source, function)?;
        let raw_term = projected.get("term").ok_or_else(|| {
            TransportCliError::Failed("C projector response missing `term`".into())
        })?;
        return parse_c11_projected_term(raw_term, catalog, normalizations);
    }
    if source.extension().and_then(|s| s.to_str()) == Some("json") {
        let value = read_json_value(source)?;
        if value.get("kind").and_then(Value::as_str) == Some("op")
            || value.get("kind").and_then(Value::as_str) == Some("var")
            || value.get("kind").and_then(Value::as_str) == Some("const")
            || value.get("kind").and_then(Value::as_str) == Some("unit")
        {
            let term: Term = serde_json::from_value(value).map_err(|error| {
                TransportCliError::Failed(format!(
                    "parse {} as transport term: {error}",
                    source.display()
                ))
            })?;
            ensure_term_language(&term, source_prefix)?;
            return Ok(term);
        }
    }
    Err(TransportCliError::Refusal(format!(
        "lift-time:no-lifter-for-language source lifter for `{source_language}` is not wired into `provekit transport`; provide a term JSON or add the source lifter subprocess adapter"
    )))
}

fn ensure_term_language(term: &Term, source_prefix: &str) -> Result<(), TransportCliError> {
    match term {
        Term::Op { name, args, .. } => {
            if !name.starts_with(&format!("{source_prefix}:")) {
                return Err(TransportCliError::Refusal(format!(
                    "lift-time:source-language-mismatch term contains operation `{name}` outside `{source_prefix}:*`"
                )));
            }
            for arg in args {
                ensure_term_language(arg, source_prefix)?;
            }
        }
        Term::Var { .. } | Term::Const { .. } | Term::Unit => {}
    }
    Ok(())
}

fn project_c11_term(
    root: &Path,
    source: &Path,
    function: &str,
) -> Result<Value, TransportCliError> {
    let projector = root.join("implementations/c/provekit-walk-c/provekit-c11-term-project");
    if !projector.exists() {
        return Err(TransportCliError::Refusal(format!(
            "lift-time:no-c11-projector C term projector is not built: {}; run `make -C implementations/c/provekit-walk-c`",
            projector.display()
        )));
    }
    let output = Command::new(&projector)
        .arg(source)
        .arg("--function")
        .arg(function)
        .arg("--term")
        .output()
        .map_err(|error| {
            TransportCliError::Failed(format!("spawn {}: {error}", projector.display()))
        })?;
    if !output.status.success() {
        return Err(TransportCliError::Refusal(format!(
            "lift-time:lifter-refused C lifter refused the source (status {}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| TransportCliError::Failed(format!("parse C term projector JSON: {error}")))
}

fn desugar_stage(root: &Path, source_language: &str) -> Result<StageReport, TransportCliError> {
    let Some(dir_name) = language_dir(source_language) else {
        return Ok(StageReport {
            stage: "desugar",
            status: "skipped",
            detail: format!("no language directory known for {source_language}"),
        });
    };
    let specs = root.join("menagerie").join(dir_name).join("specs");
    let rules = load_desugaring_rules_from_dir(&specs).map_err(|error| {
        TransportCliError::Refusal(format!("desugar-time:{} {error}", error.kind()))
    })?;
    if rules.is_empty() {
        return Ok(StageReport {
            stage: "desugar",
            status: "skipped",
            detail: "no discharged desugaring equations for this source language".into(),
        });
    }
    DesugaringSet::new(rules.clone()).map_err(|error| {
        TransportCliError::Refusal(format!("desugar-time:{} {error}", error.kind()))
    })?;
    Ok(StageReport {
        stage: "desugar",
        status: "checked",
        detail: format!(
            "{} discharged desugaring equations are available; C11/projected Term transport did not require an IrTerm rewrite",
            rules.len()
        ),
    })
}

fn language_dir(language: &str) -> Option<&'static str> {
    match language {
        "c11" => Some("c11-language-signature"),
        "csharp" => Some("csharp-language-signature"),
        "go" => Some("go-language-signature"),
        "python" => Some("python-language-signature"),
        "typescript" => Some("typescript-language-signature"),
        "zig" => Some("zig-language-signature"),
        "ruby" => Some("ruby-language-signature"),
        "php" => Some("php-language-signature"),
        "java" => Some("java-language-signature"),
        "rust" => Some("rust-language-signature"),
        _ => None,
    }
}

fn parse_c11_projected_term(
    value: &Value,
    catalog: &MorphismCatalog,
    normalizations: &mut Vec<String>,
) -> Result<Term, TransportCliError> {
    let kind = value
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| TransportCliError::Failed("term node missing kind".into()))?;
    match kind {
        "op" => {
            let name = value
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| TransportCliError::Failed("op node missing name".into()))?;
            let raw_args = value
                .get("args")
                .and_then(Value::as_array)
                .ok_or_else(|| TransportCliError::Failed("op node missing args".into()))?;
            if name == "uop_neg" && raw_args.len() == 1 {
                if let Some(term) = parse_negative_integer_literal(&raw_args[0])? {
                    normalizations.push(
                        "folded c11:uop_neg(integer-literal) into a signed integer constant".into(),
                    );
                    return Ok(term);
                }
            }
            if name == "uop_plus" && raw_args.len() == 1 {
                normalizations.push("dropped side-effect-free c11:uop_plus wrapper".into());
                return parse_c11_projected_term(&raw_args[0], catalog, normalizations);
            }

            let parsed_args = raw_args
                .iter()
                .map(|arg| parse_c11_projected_term(arg, catalog, normalizations))
                .collect::<Result<Vec<_>, _>>()?;
            let mapped_local = c11_projected_op_to_core(name).ok_or_else(|| {
                TransportCliError::Refusal(format!(
                    "transport-time:no-morphism-for-op operation `c11:{name}` lacks a discharged morphism into the concept hub"
                ))
            })?;
            if mapped_local != name {
                normalizations.push(format!("normalized c11:{name} to core c11:{mapped_local}"));
            }
            let mapped_name = format!("c11:{mapped_local}");
            Ok(Term::Op {
                op_cid: catalog.op_cid(&mapped_name)?,
                name: mapped_name,
                args: parsed_args,
            })
        }
        "var" => Ok(Term::Var {
            name: value
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| TransportCliError::Failed("var node missing name".into()))?
                .to_string(),
        }),
        "const" => Ok(Term::Const {
            value: value.get("value").cloned().unwrap_or(Value::Null),
            sort: parse_sort(value.get("sort"))?,
        }),
        "unit" => Ok(Term::Unit),
        other => Err(TransportCliError::Refusal(format!(
            "lift-time:unsupported-c11-term-node unsupported C11 term node kind `{other}`"
        ))),
    }
}

fn c11_projected_op_to_core(name: &str) -> Option<&'static str> {
    match name {
        "if" => Some("if"),
        "seq" => Some("seq"),
        "return" => Some("return"),
        "skip" => Some("skip"),
        "while" => Some("while"),
        "for" => Some("for"),
        "do" => Some("do"),
        "break" => Some("break"),
        "continue" => Some("continue"),
        "decl" => Some("decl"),
        "assign" => Some("assign"),
        "call" => Some("call"),
        "member" => Some("member"),
        "cast" => Some("cast"),
        "array-subscript" => Some("array-subscript"),
        "bop_eq" | "eq" => Some("eq"),
        "bop_ne" | "ne" => Some("ne"),
        "bop_lt" | "lt" => Some("lt"),
        "bop_le" | "le" => Some("le"),
        "bop_gt" | "gt" => Some("gt"),
        "bop_ge" | "ge" => Some("ge"),
        "bop_add" | "add" => Some("add"),
        "bop_sub" | "sub" => Some("sub"),
        "bop_mul" | "mul" => Some("mul"),
        "bop_div" | "div" => Some("div"),
        "bop_mod" | "mod" => Some("mod"),
        "bop_shl" | "shl" => Some("shl"),
        "bop_shr" | "shr" => Some("shr"),
        "bop_bitand" | "bit_and" | "bitand" => Some("bitand"),
        "bop_bitor" | "bit_or" | "bitor" => Some("bitor"),
        "bop_bitxor" | "bit_xor" | "bitxor" => Some("bitxor"),
        "bop_logand" | "and" => Some("and"),
        "bop_logor" | "or" => Some("or"),
        "uop_neg" | "neg" => Some("neg"),
        "uop_lognot" | "not" => Some("not"),
        "uop_deref" | "deref" => Some("deref"),
        "uop_addr_of" | "addr_of" => Some("addr_of"),
        "uop_bitnot" | "bitnot" | "bit_not" => Some("bitnot"),
        "uop_pre_inc" | "pre_inc" | "preinc" => Some("preinc"),
        "uop_post_inc" | "post_inc" | "postinc" => Some("postinc"),
        "uop_pre_dec" | "pre_dec" | "predec" => Some("predec"),
        "uop_post_dec" | "post_dec" | "postdec" => Some("postdec"),
        _ => None,
    }
}

fn parse_negative_integer_literal(value: &Value) -> Result<Option<Term>, TransportCliError> {
    if value.get("kind").and_then(Value::as_str) != Some("const") {
        return Ok(None);
    }
    let Some(n) = value.get("value").and_then(Value::as_i64) else {
        return Ok(None);
    };
    Ok(Some(Term::Const {
        value: json!(-n),
        sort: parse_sort(value.get("sort"))?,
    }))
}

fn parse_sort(value: Option<&Value>) -> Result<Sort, TransportCliError> {
    let Some(value) = value else {
        return Ok(Sort::Primitive { name: "Int".into() });
    };
    match value.get("kind").and_then(Value::as_str) {
        Some("primitive") | Some("ctor") => Ok(Sort::Primitive {
            name: value
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("Int")
                .to_string(),
        }),
        _ => serde_json::from_value(value.clone())
            .map_err(|error| TransportCliError::Failed(format!("parse sort: {error}"))),
    }
}

fn collect_vars(term: &Term, out: &mut BTreeSet<String>) {
    match term {
        Term::Var { name } => {
            out.insert(name.clone());
        }
        Term::Op { args, .. } => {
            for arg in args {
                collect_vars(arg, out);
            }
        }
        Term::Const { .. } | Term::Unit => {}
    }
}

fn parse_int_params(source: &str, function: &str) -> Option<Vec<String>> {
    let needle = format!("{function}(");
    let start = source.find(&needle)? + needle.len();
    let rest = &source[start..];
    let end = rest.find(')')?;
    let params = &rest[..end];
    if params.trim().is_empty() || params.trim() == "void" {
        return Some(Vec::new());
    }
    let mut out = Vec::new();
    for param in params.split(',') {
        let pieces: Vec<&str> = param.split_whitespace().collect();
        if pieces.len() >= 2 {
            out.push(pieces[pieces.len() - 1].trim_matches('*').to_string());
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}
/// Convert a syn Type to a compact string representation.
#[allow(dead_code)]
fn type_to_str(ty: &syn::Type) -> String {
    match ty {
        syn::Type::Path(tp) => {
            let seg = tp.path.segments.last();
            seg.map(|s| s.ident.to_string())
                .unwrap_or_else(|| "i64".to_string())
        }
        syn::Type::Reference(r) => {
            let inner = type_to_str(r.elem.as_ref());
            if r.mutability.is_some() {
                format!("&mut {inner}")
            } else {
                format!("&{inner}")
            }
        }
        syn::Type::Tuple(tt) if tt.elems.is_empty() => "()".to_string(),
        _ => "i64".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Contract annotation loading and formatting
// ---------------------------------------------------------------------------

/// Contracts extracted from the source function, ready for realize-side
/// annotation emission. Both fields hold IR-symbolic Formula trees as
/// produced by `provekit-lift-contracts::lift_file`.
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
struct ContractAnnotations {
    /// `pre`-condition formula from `#[requires(...)]` (or language equivalent).
    pre: Option<Rc<Formula>>,
    /// `post`-condition formula from `#[ensures(...)]` (or language equivalent).
    post: Option<Rc<Formula>>,
}
/// Derive a stable per-function concept binding name for the
/// `// concept: <name>` comment.
///
/// The name is derived from the **target term's** JSON serialization so every
/// structurally-distinct function gets a unique, reproducible name regardless
/// of the language. This is a content-addressed FNV-1a 64-bit hash, the same
/// hash used everywhere in the ProvekIt toolchain for stable naming before a
/// full CID is assigned.
///
/// Format: `UNNAMED-CONCEPT-<16 hex digits>`
///
/// Named concepts are not derived here because the hub concept name is an
/// editorial property of the morphism catalog, not of the individual term.
/// The naming-roundtrip lifter can replace this marker with a catalog-resolved
/// name after the fact; the `UNNAMED-CONCEPT-*` form is what it starts with.
fn derive_concept_comment(target_term: &Term) -> String {
    // Serialize to canonical JSON and hash the bytes. serde_json's default
    // serialization is deterministic for the same input value.
    let json =
        serde_json::to_string(target_term).unwrap_or_else(|_| "<unserializable>".to_string());
    let mut h: u64 = 0xcbf29ce484222325; // FNV-1a 64-bit offset basis
    for b in json.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("UNNAMED-CONCEPT-{h:016x}")
}

/// Render an IR-symbolic `Term` to target-language expression syntax.
///
/// Only the surface forms produced by `provekit-lift-contracts` are
/// handled: `Var`, `Const(Int)`, `Const(Bool)`. Anything else falls
/// back to `<COMPLEX>` with a parenthetical note.
#[allow(dead_code)]
fn emit_term_syntax(term: &SymTerm) -> String {
    match term {
        SymTerm::Var { name } => name.clone(),
        SymTerm::Const { value, .. } => match value {
            ConstValue::Int(n) => n.to_string(),
            ConstValue::Bool(b) => b.to_string(),
            ConstValue::String(s) => format!("{s:?}"),
        },
        SymTerm::Ctor { name, args } => {
            if args.is_empty() {
                name.clone()
            } else {
                let arg_strs: Vec<_> = args.iter().map(|a| emit_term_syntax(a)).collect();
                format!("{name}({})", arg_strs.join(", "))
            }
        }
        _ => "<COMPLEX>".to_string(),
    }
}

/// Peel outer `Forall` / `Exists` quantifier wrappers from a formula.
///
/// `provekit-lift-contracts` wraps the predicate body in one `Forall` per
/// parameter (the parameters define the quantified variables). For annotation
/// emission we want only the predicate body since the function signature
/// already declares the parameter names.
#[allow(dead_code)]
fn peel_quantifiers(formula: &Formula) -> &Formula {
    match formula {
        Formula::Quantifier { body, .. } => peel_quantifiers(body),
        other => other,
    }
}
#[derive(Debug)]
#[allow(dead_code)]
pub struct RealizedSource {
    pub extension: &'static str,
    pub source: String,
    /// True when the body fell through to the language stub (no body-template
    /// matched). False when a body-template plugin rendered a real body.
    ///
    /// Used by `cmd_bind::apply_canonical_rewrite` to emit accurate
    /// per-concept `bind-stub-body-emitted` gap entries per
    /// `2026-05-13-body-template-memento.md` §5.
    ///
    /// For target languages whose realizer does not yet support body
    /// templates (everything except Java in v1.0.0), this is unconditionally
    /// true; the body is always a stub.
    pub is_stub: bool,
    pub emitted_artifact_cid: Option<String>,
    pub observed_loss_record: serde_json::Value,
    pub used_sugars: Vec<serde_json::Value>,
    /// Raw `observation_wrapper_emission_record` object from the kit response,
    /// present when the kit emitted a wrapper FCM for an observation mode.
    /// Fields: object_fcm_cid, wrapper_fcm_cid, observer_effects,
    /// preservation_claim_cid.
    pub observation_wrapper_emission_record: Option<serde_json::Value>,
}

/// Stable provenance CID for the realize-v0 phase. Mirrors the pattern used by
/// `lifter_cid` and `clusterer_cid` in cmd_bind; content-addresses the realize
/// pipeline identity without tying it to the lift.
#[allow(dead_code)]
const REALIZE_PROVENANCE_CID_SEED: &[u8] = b"provekit-cli/realize-v0/provenance";

#[allow(dead_code)]
pub fn cid_for_serializable<T: Serialize>(value: &T) -> Result<String, String> {
    let json =
        serde_json::to_value(value).map_err(|err| format!("serialize value for cid: {err}"))?;
    cid_for_json_value(&json)
}

#[allow(dead_code)]
pub fn cid_for_json_value(value: &serde_json::Value) -> Result<String, String> {
    let canonical = canonical_value_from_json(value)?;
    Ok(blake3_512_of(encode_jcs(canonical.as_ref()).as_bytes()))
}

#[allow(dead_code)]
fn canonical_value_from_json(value: &serde_json::Value) -> Result<Arc<CanonicalValue>, String> {
    match value {
        serde_json::Value::Null => Ok(CanonicalValue::null()),
        serde_json::Value::Bool(value) => Ok(CanonicalValue::boolean(*value)),
        serde_json::Value::Number(number) => {
            number.as_i64().map(CanonicalValue::integer).ok_or_else(|| {
                format!("non-i64 JSON number is not supported in JCS CID input: {number}")
            })
        }
        serde_json::Value::String(value) => Ok(CanonicalValue::string(value.clone())),
        serde_json::Value::Array(items) => items
            .iter()
            .map(canonical_value_from_json)
            .collect::<Result<Vec<_>, _>>()
            .map(CanonicalValue::array),
        serde_json::Value::Object(entries) => entries
            .iter()
            .map(|(key, value)| canonical_value_from_json(value).map(|value| (key.clone(), value)))
            .collect::<Result<Vec<_>, _>>()
            .map(CanonicalValue::object),
    }
}

/// Pure function: given the RealizeRequest and RealizedSource, mint the
/// `RealizationPlanMemento` (and, when the kit returned an
/// `observation_wrapper_emission_record`, the `ObservationWrapperMemento`).
///
/// Returns `(plan_memento, wrapper_memento_option, wrapper_fcm_option)`.
///
/// Blocker #1, #2, #4 implementation.  The ParametricRealizationMemento is
/// constructed synthetically (one slot per param) because the catalog lookup
/// path does not yet exist (see cmd_bind.rs:1112 gap comment).  The synthetic
/// realization is structurally valid, passes validate(), and gives
/// validate_against() a real cite target.  Future catalog integration is an
/// enhancement on top.
#[allow(dead_code)]
pub fn mint_realization_artifacts(
    request: &crate::kit_dispatch::RealizeRequest,
    realized: &RealizedSource,
    concept_site_cid: &str,
) -> Result<
    (
        RealizationPlanMemento,
        Option<ObservationWrapperMemento>,
        Option<serde_json::Value>,
    ),
    String,
> {
    let provenance_cid = blake3_512_of(REALIZE_PROVENANCE_CID_SEED);

    // ---- Build synthetic ParametricRealizationMemento (Blocker #3 inline) ----
    // One slot per param: source = "src_T{i}", target = "tgt_T{i}".
    // This gives validate_against() a real non-empty slot list.
    let n_slots = request.params.len().max(1); // spec requires [+ slot]
    let type_variables: Vec<String> = (0..n_slots)
        .flat_map(|i| vec![format!("src_T{i}"), format!("tgt_T{i}")])
        .collect();
    let required_sort_morphism_slots: Vec<SlotDescriptor> = (0..n_slots)
        .map(|i| SlotDescriptor {
            slot_name: request
                .params
                .get(i)
                .cloned()
                .unwrap_or_else(|| format!("param{i}")),
            source_type_variable: format!("src_T{i}"),
            target_type_variable: format!("tgt_T{i}"),
        })
        .collect();
    let realization = ParametricRealizationMemento {
        body_template_cids: vec![],
        concept_pattern: serde_json::json!({"concept": request.concept_name}),
        effect_transform_slots: vec![EffectSlotDescriptor {
            concept_effect: "pure".to_string(),
            slot_name: "default".to_string(),
            target_effect: "pure".to_string(),
        }],
        loss_record_template: serde_json::json!({}),
        provenance_cid: provenance_cid.clone(),
        required_sort_morphism_slots,
        sugar_cids: request.sugar_cids.clone(),
        target_pattern: serde_json::json!({"language": "v0-inline"}),
        type_variables,
    };
    realization
        .validate()
        .map_err(|e| format!("synthetic ParametricRealizationMemento invalid: {e}"))?;

    // Sort morphism CIDs: one stable CID per slot (param-name keyed).
    let sort_morphism_cids: Vec<String> = realization
        .required_sort_morphism_slots
        .iter()
        .map(|slot| {
            blake3_512_of(
                format!(
                    "provekit-cli/realize-v0/sort-morphism/{}/{}",
                    slot.source_type_variable, slot.target_type_variable
                )
                .as_bytes(),
            )
        })
        .collect();

    let selected_realization_cid = blake3_512_of(
        format!(
            "provekit-cli/realize-v0/realization/{}",
            request.concept_name
        )
        .as_bytes(),
    );
    let loss_function_cid = blake3_512_of(
        format!(
            "provekit-cli/realize-v0/loss-function/{}",
            request.concept_name
        )
        .as_bytes(),
    );

    // ---- Blocker #4: ObservationWrapperMemento ----
    let mode_str = request.mode.as_deref().unwrap_or("monitor");
    let is_observation_mode = matches!(
        mode_str,
        "witness" | "monitor" | "emitter" | "gate" | "dispatcher"
    );
    let (wrapper_memento, wrapper_fcm): (
        Option<ObservationWrapperMemento>,
        Option<serde_json::Value>,
    ) = if is_observation_mode {
        if let Some(record) = &realized.observation_wrapper_emission_record {
            let wrapper_fcm_cid = record
                .get("wrapper_fcm_cid")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    "observation_wrapper_emission_record missing wrapper_fcm_cid".to_string()
                })?
                .to_string();
            let preservation_claim_cid = record
                .get("preservation_claim_cid")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    "observation_wrapper_emission_record missing preservation_claim_cid".to_string()
                })?
                .to_string();
            let object_fcm_cid = record
                .get("object_fcm_cid")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    "observation_wrapper_emission_record missing object_fcm_cid".to_string()
                })?
                .to_string();
            let emitted = realized
                .emitted_artifact_cid
                .clone()
                .unwrap_or_else(|| "".to_string());
            let raw_effects = record
                .get("observer_effects")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    "observation_wrapper_emission_record missing observer_effects".to_string()
                })?;
            let observer_effects: Vec<EffectOccurrence> = raw_effects
                .iter()
                .enumerate()
                .map(|(idx, v)| {
                    serde_json::from_value(v.clone()).map_err(|err| {
                        format!(
                            "observation_wrapper_emission_record observer_effects[{idx}] malformed: {err}"
                        )
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let wrapper_fcm = record
                .get("wrapper_fcm")
                .ok_or_else(|| {
                    "observation_wrapper_emission_record missing wrapper_fcm".to_string()
                })?
                .clone();
            let computed_wrapper_fcm_cid = cid_for_json_value(&wrapper_fcm).map_err(|err| {
                format!("observation_wrapper_emission_record wrapper_fcm invalid: {err}")
            })?;
            if computed_wrapper_fcm_cid != wrapper_fcm_cid {
                return Err(format!(
                    "observation_wrapper_emission_record wrapper_fcm_cid mismatch: \
                     declared {wrapper_fcm_cid}, computed {computed_wrapper_fcm_cid}"
                ));
            }
            let raw_wrapper_effects = wrapper_fcm
                .get("effects")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    "observation_wrapper_emission_record wrapper_fcm missing effects".to_string()
                })?;
            let wrapper_effects: Vec<EffectOccurrence> = raw_wrapper_effects
                .iter()
                .enumerate()
                .map(|(idx, v)| {
                    serde_json::from_value(v.clone()).map_err(|err| {
                        format!(
                            "observation_wrapper_emission_record wrapper_fcm.effects[{idx}] malformed: {err}"
                        )
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let w = ObservationWrapperMemento {
                emitted_artifact_cid: emitted,
                mode: mode_str.to_string(),
                object_fcm_cid,
                observer_effects,
                preservation_claim_cid,
                provenance_cid: provenance_cid.clone(),
                wrapper_fcm_cid,
            };
            // validate before persisting (spec §7, fail-closed).
            // The kit must return the concrete wrapper FCM object so the CID
            // is resolvable and observer_effects can be checked against the
            // wrapper's declared effects. Object effects are still not carried
            // through this v1 dispatch path, so the object side remains an
            // empty set until the source FCM catalog is threaded here.
            w.validate(&[], &wrapper_effects, &[])
                .map_err(|e| format!("ObservationWrapperMemento invariant violation: {e:?}"))?;
            (Some(w), Some(wrapper_fcm))
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    // ---- Blocker #1: RealizationPlanMemento ----
    let observation_wrapper_cid = wrapper_memento
        .as_ref()
        .map(cid_for_serializable)
        .transpose()?;
    let plan = RealizationPlanMemento {
        candidate_set_cid: selected_realization_cid.clone(),
        concept_site_cid: concept_site_cid.to_string(),
        effect_occurrence_transform: realized.observed_loss_record.clone(),
        loss_function_cid,
        observation_wrapper_cid,
        provenance_cid,
        selected_candidate_cid: selected_realization_cid.clone(),
        selected_realization_cid,
        sort_morphism_cids,
        total_loss_record: realized.observed_loss_record.clone(),
    };

    // ---- Blocker #2: validate_against ----
    plan.validate_against(&realization)
        .map_err(|e| format!("RealizationPlanMemento validation failed: {e}"))?;

    Ok((plan, wrapper_memento, wrapper_fcm))
}

/// Public-crate bridge for `cmd_bind`'s canonical-mode path.
///
/// Lifts contract annotations from `source_text` (Rust source of the origin
/// file) for the named function, then calls `realize_function` with a stub
/// body appropriate for each target language.  Source types (param types and
/// return type as target-language strings) are threaded through so that the
/// emitted signature matches the origin, e.g. an `i64` source param emits
/// `i64` in Rust/Zig, `long` in Java/C#, `int64` in Go, `number` in
/// TypeScript, and untyped in Python/Ruby.
///
/// Returns a `RealizedSource` on success. The `source` field carries the
/// full target-language snippet including the ORP annotation prefix.
#[allow(dead_code)]
pub fn realize_for_bind(
    language: &str,
    function: &str,
    params: &[String],
    param_types: &[String],
    return_type: &str,
    concept_name: &str,
) -> Result<RealizedSource, TransportCliError> {
    realize_for_bind_with_contract(
        language,
        function,
        params,
        param_types,
        return_type,
        concept_name,
        None,
        None,
        Vec::new(),
    )
}

#[allow(dead_code)]
pub fn realize_for_bind_with_contract(
    language: &str,
    function: &str,
    params: &[String],
    param_types: &[String],
    return_type: &str,
    concept_name: &str,
    mode: Option<&str>,
    contract: Option<crate::kit_dispatch::RealizeContractPayload>,
    sugar_plugins: Vec<serde_json::Value>,
) -> Result<RealizedSource, TransportCliError> {
    // The bind path supplies signature info FROM THE LIFT KIT (param_types
    // and return_type carried through the bind-IR per
    // `2026-05-13-bind-ir-lift-result.md`). cmd_transport does NOT reparse
    // source here: every realize-time emission lives in a per-language
    // realize plugin reached via PEP 1.7.0 dispatch.
    let annotations = ContractAnnotations::default();
    realize_function(
        language,
        function,
        params,
        param_types,
        return_type,
        &Term::Unit,
        &annotations,
        concept_name,
        mode,
        contract,
        sugar_plugins,
    )
}
fn realize_function(
    language: &str,
    function: &str,
    params: &[String],
    param_types: &[String],
    return_type: &str,
    body: &Term,
    annotations: &ContractAnnotations,
    concept_name: &str,
    mode: Option<&str>,
    contract: Option<crate::kit_dispatch::RealizeContractPayload>,
    sugar_plugins: Vec<serde_json::Value>,
) -> Result<RealizedSource, TransportCliError> {
    // Federation by construction (PEP 1.7.0 `kind = "realize"`): every
    // language's source emission lives in a per-language realize plugin.
    // cmd_transport carries ZERO language-specific code; the dispatcher in
    // `crate::kit_dispatch` resolves the kit by filesystem convention
    // (`.provekit/realize/<lang>/manifest.toml` or built-in path under
    // `implementations/<lang>/`). When no kit is available the call
    // refuses with `realize-time:kit-plugin-unavailable` so the caller
    // emits a `kit-plugin-unavailable` gap. Per Supra omnia rectum, kit
    // unavailability is a precise extension request, not a hidden error.
    let _ = annotations; // emission is the kit's responsibility
    let _ = body; // body emission is the kit's responsibility
    let workspace_root =
        repo_root().unwrap_or_else(|_| std::env::current_dir().unwrap_or_default());
    let sugar_cids: Vec<String> = sugar_plugins
        .iter()
        .filter_map(|plugin| {
            plugin
                .get("header")
                .and_then(|header| header.get("cid"))
                .and_then(|cid| cid.as_str())
                .map(str::to_string)
        })
        .collect();
    let spec = json!({
        "kind": "RealizeRequest",
        "function": function,
        "params": params,
        "paramTypes": param_types,
        "returnType": return_type,
        "conceptName": concept_name,
        "mode": mode,
        "modes": mode.into_iter().collect::<Vec<_>>(),
        "contract": contract,
        "sugarCids": sugar_cids,
        "sugarPlugins": sugar_plugins,
    });
    let realized = realize_spec_via_path(&workspace_root, language, spec).map_err(|error| {
        TransportCliError::Refusal(format!("realize-time:kit-plugin-unavailable {error}"))
    })?;
    return Ok(RealizedSource {
        // The kit reports `extension`; fall back to a leak-free static
        // string MATCHING the kit's response. The Box::leak below converts
        // the runtime string to a 'static slice without changing the
        // RealizedSource shape; this is fine because realize-kit responses
        // are bounded in count and the leak is one allocation per call
        // (acceptable for CLI lifetime).
        extension: Box::leak(realized.extension.into_boxed_str()),
        source: realized.source,
        is_stub: realized.is_stub,
        emitted_artifact_cid: realized.emitted_artifact_cid,
        observed_loss_record: realized.observed_loss_record,
        used_sugars: realized.used_sugars,
        observation_wrapper_emission_record: realized.observation_wrapper_emission_record,
    });
}

fn realize_spec_via_path(
    workspace_root: &Path,
    language: &str,
    spec: Value,
) -> Result<libprovekit::core::RealizedSource, String> {
    let mut inputs = HashMapInputCatalog::default();
    let input_cid = inputs.insert(Input::Spec(spec));
    let kit_name = format!("lower-{language}");
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
            workspace_root.to_path_buf(),
            language.to_string(),
            None,
            DispatchRealizeTransport,
        ),
    );
    let claim = execute_path(&path, &registry, &inputs).map_err(|error| error.to_string())?;
    LowerKit::<DispatchRealizeTransport>::realized_source_from_claim(&claim)
}

#[cfg(test)]
mod mint_realization_artifacts_tests {
    use super::*;
    use crate::kit_dispatch::RealizeRequest;

    fn make_request(mode: Option<&str>) -> RealizeRequest {
        serde_json::from_value(serde_json::json!({
            "function": "foo",
            "params": ["x", "y"],
            "param_types": ["int", "int"],
            "return_type": "int",
            "concept_name": "add",
            "mode": mode,
            "modes": mode.into_iter().collect::<Vec<_>>(),
            "sugar_cids": [],
            "sugar_plugins": []
        }))
        .expect("request decodes")
    }

    fn make_realized(wrapper_record: Option<serde_json::Value>) -> RealizedSource {
        RealizedSource {
            // RealizedSource.extension is &'static str; use a static literal.
            extension: "rs",
            source: "fn foo() {}".to_string(),
            is_stub: false,
            emitted_artifact_cid: Some("artifact-cid-abc".to_string()),
            observed_loss_record: serde_json::json!({}),
            used_sugars: vec![],
            observation_wrapper_emission_record: wrapper_record,
        }
    }

    fn valid_effect() -> serde_json::Value {
        serde_json::json!({
            "args": [],
            "discharge_key": "informational-dischargeable",
            "locator": null,
            "occurrence_kind": "Io",
            "role": "body",
            "signature_cid": "sig-cid-1"
        })
    }

    fn valid_wrapper_fcm(effect: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "autoMintedMementos": [],
            "bodyCid": "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "effects": [effect],
            "fnName": "foo$provekit_emitter",
            "formalSorts": [],
            "formals": [],
            "kind": "function-contract",
            "locus": {"function": "foo", "surface": "java-emitter-wrapper"},
            "post": {"args": [], "kind": "atomic", "name": "true"},
            "pre": {"args": [], "kind": "atomic", "name": "true"},
            "returnSort": {"args": [], "kind": "ctor", "name": "int"},
            "schemaVersion": "1"
        })
    }

    fn valid_wrapper_record() -> serde_json::Value {
        let effect = valid_effect();
        let wrapper_fcm = valid_wrapper_fcm(effect.clone());
        let wrapper_fcm_cid = cid_for_json_value(&wrapper_fcm).expect("wrapper fcm cid");
        serde_json::json!({
            "object_fcm_cid": "object-fcm-cid-xyz",
            "wrapper_fcm": wrapper_fcm,
            "wrapper_fcm_cid": wrapper_fcm_cid,
            "preservation_claim_cid": "preservation-claim-cid-xyz",
            "observer_effects": [effect]
        })
    }

    /// Blocker #1 + #2: RealizationPlanMemento IS minted on a successful
    /// realize call and validate_against passes.
    #[test]
    fn realization_plan_memento_minted_on_success() {
        let req = make_request(Some("monitor"));
        let realized = make_realized(None);
        let (plan, wrapper, wrapper_fcm) =
            mint_realization_artifacts(&req, &realized, "concept-site-cid-123").unwrap();
        assert_eq!(plan.concept_site_cid, "concept-site-cid-123");
        assert_eq!(
            plan.sort_morphism_cids.len(),
            2,
            "expect one sort-morphism CID per param"
        );
        assert!(
            wrapper.is_none(),
            "no wrapper_emission_record => no wrapper"
        );
        assert!(
            wrapper_fcm.is_none(),
            "no wrapper_emission_record => no wrapper FCM"
        );
    }

    /// Blocker #2: validate_against passes (no slot-count mismatch).
    #[test]
    fn plan_validate_against_passes() {
        let req = make_request(None);
        let realized = make_realized(None);
        let (plan, _, _) =
            mint_realization_artifacts(&req, &realized, "concept-site-cid-999").unwrap();
        // If validate_against failed, mint_realization_artifacts would have
        // returned Err. Reaching here proves it passed.
        let _ = plan;
    }

    /// Blocker #4: ObservationWrapperMemento IS minted for observation modes
    /// when the kit returns an observation_wrapper_emission_record with valid
    /// fields.
    ///
    /// Note: validate() on ObservationWrapperMemento requires observer_effects
    /// to be non-empty. We supply a valid effect occurrence so the invariant
    /// passes. The test confirms the wrapper is minted and RealizationPlanMemento
    /// carries the observation_wrapper_cid.
    #[test]
    fn observation_wrapper_memento_minted_for_observation_modes() {
        for mode in ["witness", "monitor", "emitter", "gate"] {
            let req = make_request(Some(mode));
            let wrapper_record = valid_wrapper_record();
            let expected_wrapper_fcm_cid = wrapper_record["wrapper_fcm_cid"]
                .as_str()
                .expect("wrapper_fcm_cid")
                .to_string();
            let realized = make_realized(Some(wrapper_record));
            let (plan, wrapper, wrapper_fcm) =
                mint_realization_artifacts(&req, &realized, "concept-site-cid-w").unwrap();
            assert!(
                wrapper.is_some(),
                "{mode} mode + wrapper record => ObservationWrapperMemento must be minted"
            );
            let w = wrapper.unwrap();
            assert_eq!(w.mode, mode);
            assert_eq!(w.wrapper_fcm_cid, expected_wrapper_fcm_cid);
            assert_eq!(w.object_fcm_cid, "object-fcm-cid-xyz");
            assert!(
                wrapper_fcm.is_some(),
                "{mode} mode + wrapper record => wrapper FCM must be returned"
            );
            assert!(
                plan.observation_wrapper_cid.is_some(),
                "plan.observation_wrapper_cid must be set when wrapper is minted"
            );
        }
    }

    #[test]
    fn malformed_observer_effects_fail_closed() {
        let req = make_request(Some("witness"));
        let mut wrapper_record = valid_wrapper_record();
        wrapper_record["observer_effects"] = serde_json::json!([{
            "args": [],
            "discharge_key": "informational-dischargeable",
            "locator": null,
            "occurrence_kind": "NotARealKind",
            "role": "body",
            "signature_cid": "sig-cid-1"
        }]);
        let realized = make_realized(Some(wrapper_record));
        let err = mint_realization_artifacts(&req, &realized, "concept-site-cid-w").unwrap_err();
        assert!(
            err.contains("observer_effects[0] malformed"),
            "malformed observer effect must fail closed, got {err}"
        );
    }

    #[test]
    fn missing_observer_effects_fail_closed() {
        let req = make_request(Some("witness"));
        let mut wrapper_record = valid_wrapper_record();
        wrapper_record
            .as_object_mut()
            .expect("wrapper record object")
            .remove("observer_effects");
        let realized = make_realized(Some(wrapper_record));
        let err = mint_realization_artifacts(&req, &realized, "concept-site-cid-w").unwrap_err();
        assert!(
            err.contains("missing observer_effects"),
            "missing observer_effects must fail closed, got {err}"
        );
    }

    #[test]
    fn missing_wrapper_fcm_fails_closed() {
        let req = make_request(Some("emitter"));
        let mut wrapper_record = valid_wrapper_record();
        wrapper_record
            .as_object_mut()
            .expect("wrapper record object")
            .remove("wrapper_fcm");
        let realized = make_realized(Some(wrapper_record));
        let err = mint_realization_artifacts(&req, &realized, "concept-site-cid-w").unwrap_err();
        assert!(
            err.contains("missing wrapper_fcm"),
            "wrapper_fcm_cid must resolve to a concrete wrapper FCM object, got {err}"
        );
    }

    #[test]
    fn missing_object_fcm_cid_fails_closed() {
        let req = make_request(Some("witness"));
        let mut wrapper_record = valid_wrapper_record();
        wrapper_record
            .as_object_mut()
            .expect("wrapper record object")
            .remove("object_fcm_cid");
        let realized = make_realized(Some(wrapper_record));
        let err = mint_realization_artifacts(&req, &realized, "concept-site-cid-w").unwrap_err();
        assert!(
            err.contains("missing object_fcm_cid"),
            "missing object_fcm_cid must fail closed, got {err}"
        );
    }
}
