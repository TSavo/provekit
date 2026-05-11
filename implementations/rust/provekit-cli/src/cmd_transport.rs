// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::Parser;
use libprovekit::core::{Cid, Term};
use libprovekit::desugar::{load_desugaring_rules_from_dir, DesugaringSet};
use libprovekit::transport::{transport_term, OperationTransport, TermTransport};
use owo_colors::OwoColorize;
use provekit_ir_types::Sort;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

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
enum TransportCliError {
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
    let realized = realize_function(&target_language, &args.function, &params, &target_term)?;
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

#[derive(Debug)]
struct RealizedSource {
    extension: &'static str,
    source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TargetStyle {
    Rust,
    Python,
    Go,
    CSharp,
    TypeScript,
    Zig,
    Ruby,
    Php,
    Java,
}

fn style_for(language: &str) -> Option<TargetStyle> {
    match language {
        "rust" => Some(TargetStyle::Rust),
        "python" => Some(TargetStyle::Python),
        "go" => Some(TargetStyle::Go),
        "csharp" => Some(TargetStyle::CSharp),
        "typescript" => Some(TargetStyle::TypeScript),
        "zig" => Some(TargetStyle::Zig),
        "ruby" => Some(TargetStyle::Ruby),
        "php" => Some(TargetStyle::Php),
        "java" => Some(TargetStyle::Java),
        _ => None,
    }
}

fn realize_function(
    language: &str,
    function: &str,
    params: &[String],
    body: &Term,
) -> Result<RealizedSource, TransportCliError> {
    let style = style_for(language).ok_or_else(|| {
        TransportCliError::Refusal(format!(
            "realize-time:no-realizer no source realizer for target `{language}`"
        ))
    })?;
    let source = match style {
        TargetStyle::Rust => format!(
            "pub fn {function}({}) -> i32 {{\n{}}}\n",
            params
                .iter()
                .map(|param| format!("{param}: i32"))
                .collect::<Vec<_>>()
                .join(", "),
            emit_stmt(body, style, 1)?
        ),
        TargetStyle::Python => format!(
            "def {function}({}):\n{}",
            params.join(", "),
            emit_block(body, style, 1)?
        ),
        TargetStyle::Go => format!(
            "package main\n\nfunc {function}({}) int {{\n{}}}\n",
            params
                .iter()
                .map(|param| format!("{param} int"))
                .collect::<Vec<_>>()
                .join(", "),
            emit_stmt(body, style, 1)?
        ),
        TargetStyle::CSharp => format!(
            "public static class Transported {{\n    public static int {function}({}) {{\n{}    }}\n}}\n",
            params
                .iter()
                .map(|param| format!("int {param}"))
                .collect::<Vec<_>>()
                .join(", "),
            emit_stmt(body, style, 2)?
        ),
        TargetStyle::TypeScript => format!(
            "export function {function}({}): number {{\n{}}}\n",
            params
                .iter()
                .map(|param| format!("{param}: number"))
                .collect::<Vec<_>>()
                .join(", "),
            emit_stmt(body, style, 1)?
        ),
        TargetStyle::Zig => format!(
            "pub fn {function}({}) i32 {{\n{}}}\n",
            params
                .iter()
                .map(|param| format!("{param}: i32"))
                .collect::<Vec<_>>()
                .join(", "),
            emit_stmt(body, style, 1)?
        ),
        TargetStyle::Ruby => format!(
            "def {function}({})\n{}end\n",
            params.join(", "),
            emit_block(body, style, 1)?
        ),
        TargetStyle::Php => format!(
            "<?php\nfunction {function}({}) {{\n{}}}\n",
            params
                .iter()
                .map(|param| format!("${param}"))
                .collect::<Vec<_>>()
                .join(", "),
            emit_stmt(body, style, 1)?
        ),
        TargetStyle::Java => format!(
            "final class Transported {{\n    public static int {function}({}) {{\n{}    }}\n}}\n",
            params
                .iter()
                .map(|param| format!("int {param}"))
                .collect::<Vec<_>>()
                .join(", "),
            emit_stmt(body, style, 2)?
        ),
    };
    let extension = match style {
        TargetStyle::Rust => "rs",
        TargetStyle::Python => "py",
        TargetStyle::Go => "go",
        TargetStyle::CSharp => "cs",
        TargetStyle::TypeScript => "ts",
        TargetStyle::Zig => "zig",
        TargetStyle::Ruby => "rb",
        TargetStyle::Php => "php",
        TargetStyle::Java => "java",
    };
    Ok(RealizedSource { extension, source })
}

fn emit_block(term: &Term, style: TargetStyle, indent: usize) -> Result<String, TransportCliError> {
    let body = emit_stmt(term, style, indent)?;
    if body.trim().is_empty() {
        let pad = indent_str(indent);
        Ok(match style {
            TargetStyle::Python => format!("{pad}pass\n"),
            TargetStyle::Ruby => format!("{pad}nil\n"),
            _ => String::new(),
        })
    } else {
        Ok(body)
    }
}

fn emit_stmt(term: &Term, style: TargetStyle, indent: usize) -> Result<String, TransportCliError> {
    let pad = indent_str(indent);
    match term {
        Term::Op { name, args, .. } if local_op(name) == "seq" => {
            ensure_arity(name, args, 2)?;
            Ok(format!(
                "{}{}",
                emit_stmt(&args[0], style, indent)?,
                emit_stmt(&args[1], style, indent)?
            ))
        }
        Term::Op { name, args, .. }
            if local_op(name) == "if" || local_op(name) == "conditional" =>
        {
            ensure_arity(name, args, 3)?;
            let cond = emit_expr(&args[0], style)?;
            let then_branch = emit_block(&args[1], style, indent + 1)?;
            let else_branch = emit_block(&args[2], style, indent + 1)?;
            match style {
                TargetStyle::Python => {
                    if is_skip(&args[2]) {
                        Ok(format!("{pad}if {cond}:\n{then_branch}"))
                    } else {
                        Ok(format!(
                            "{pad}if {cond}:\n{then_branch}{pad}else:\n{else_branch}"
                        ))
                    }
                }
                TargetStyle::Ruby => {
                    if is_skip(&args[2]) {
                        Ok(format!("{pad}if {cond}\n{then_branch}{pad}end\n"))
                    } else {
                        Ok(format!(
                            "{pad}if {cond}\n{then_branch}{pad}else\n{else_branch}{pad}end\n"
                        ))
                    }
                }
                _ => {
                    let head = if style == TargetStyle::Rust {
                        format!("{pad}if {cond} {{\n")
                    } else {
                        format!("{pad}if ({cond}) {{\n")
                    };
                    if is_skip(&args[2]) {
                        Ok(format!("{head}{then_branch}{pad}}}\n"))
                    } else {
                        Ok(format!(
                            "{head}{then_branch}{pad}}} else {{\n{else_branch}{pad}}}\n"
                        ))
                    }
                }
            }
        }
        Term::Op { name, args, .. } if local_op(name) == "while" => {
            ensure_arity(name, args, 2)?;
            let cond = emit_expr(&args[0], style)?;
            let body = emit_block(&args[1], style, indent + 1)?;
            Ok(match style {
                TargetStyle::Python => format!("{pad}while {cond}:\n{body}"),
                TargetStyle::Ruby => format!("{pad}while {cond}\n{body}{pad}end\n"),
                TargetStyle::Go => format!("{pad}for {cond} {{\n{body}{pad}}}\n"),
                TargetStyle::Rust | TargetStyle::Zig => {
                    format!("{pad}while {cond} {{\n{body}{pad}}}\n")
                }
                _ => format!("{pad}while ({cond}) {{\n{body}{pad}}}\n"),
            })
        }
        Term::Op { name, args, .. } if local_op(name) == "decl" => {
            ensure_arity(name, args, 2)?;
            let target = emit_lvalue(&args[0], style)?;
            let value = emit_expr(&args[1], style)?;
            Ok(match style {
                TargetStyle::Rust => format!("{pad}let mut {target} = {value};\n"),
                TargetStyle::Go => format!("{pad}{target} := {value}\n"),
                TargetStyle::Python | TargetStyle::Ruby => format!("{pad}{target} = {value}\n"),
                TargetStyle::Php => format!("{pad}{target} = {value};\n"),
                TargetStyle::TypeScript => format!("{pad}let {target} = {value};\n"),
                TargetStyle::Zig => format!("{pad}var {target}: i32 = {value};\n"),
                TargetStyle::CSharp | TargetStyle::Java => {
                    format!("{pad}int {target} = {value};\n")
                }
            })
        }
        Term::Op { name, args, .. } if local_op(name) == "assign" => {
            ensure_arity(name, args, 2)?;
            let target = emit_lvalue(&args[0], style)?;
            let value = emit_expr(&args[1], style)?;
            Ok(match style {
                TargetStyle::Python | TargetStyle::Ruby | TargetStyle::Go => {
                    format!("{pad}{target} = {value}\n")
                }
                _ => format!("{pad}{target} = {value};\n"),
            })
        }
        Term::Op { name, args, .. } if local_op(name) == "return" => {
            ensure_arity(name, args, 1)?;
            Ok(format!(
                "{pad}return {}{}\n",
                emit_expr(&args[0], style)?,
                stmt_end(style)
            ))
        }
        Term::Op { name, .. } if local_op(name) == "skip" => Ok(String::new()),
        Term::Op { name, .. } if local_op(name) == "break" => {
            Ok(format!("{pad}break{}\n", stmt_end(style)))
        }
        Term::Op { name, .. } if local_op(name) == "continue" => {
            Ok(format!("{pad}continue{}\n", stmt_end(style)))
        }
        other => Ok(format!(
            "{pad}{}{}\n",
            emit_expr(other, style)?,
            stmt_end(style)
        )),
    }
}

fn emit_lvalue(term: &Term, style: TargetStyle) -> Result<String, TransportCliError> {
    match term {
        Term::Var { name } => Ok(var_name(name, style)),
        _ => emit_expr(term, style),
    }
}

fn emit_expr(term: &Term, style: TargetStyle) -> Result<String, TransportCliError> {
    match term {
        Term::Var { name } => Ok(var_name(name, style)),
        Term::Const { value, .. } => emit_const(value, style),
        Term::Unit => Ok(match style {
            TargetStyle::Python => "None".into(),
            TargetStyle::Ruby => "nil".into(),
            TargetStyle::Php => "null".into(),
            _ => "()".into(),
        }),
        Term::Op { name, args, .. } => {
            let op = local_op(name);
            if let Some(symbol) = binary_symbol(op, style) {
                ensure_arity(name, args, 2)?;
                return Ok(format!(
                    "{} {} {}",
                    emit_expr(&args[0], style)?,
                    symbol,
                    emit_expr(&args[1], style)?
                ));
            }
            match op {
                "neg" => {
                    ensure_arity(name, args, 1)?;
                    Ok(format!("-{}", emit_expr(&args[0], style)?))
                }
                "not" => {
                    ensure_arity(name, args, 1)?;
                    let item = emit_expr(&args[0], style)?;
                    Ok(match style {
                        TargetStyle::Python => format!("not {item}"),
                        _ => format!("!{item}"),
                    })
                }
                "bitnot" => {
                    ensure_arity(name, args, 1)?;
                    Ok(format!("~{}", emit_expr(&args[0], style)?))
                }
                "ite" => {
                    ensure_arity(name, args, 3)?;
                    let c = emit_expr(&args[0], style)?;
                    let t = emit_expr(&args[1], style)?;
                    let e = emit_expr(&args[2], style)?;
                    Ok(match style {
                        TargetStyle::Python => format!("{t} if {c} else {e}"),
                        TargetStyle::Rust | TargetStyle::Zig => format!("if {c} {{ {t} }} else {{ {e} }}"),
                        _ => format!("{c} ? {t} : {e}"),
                    })
                }
                "index" | "array-subscript" => {
                    ensure_arity(name, args, 2)?;
                    Ok(format!("{}[{}]", emit_expr(&args[0], style)?, emit_expr(&args[1], style)?))
                }
                "member" => {
                    ensure_arity(name, args, 2)?;
                    Ok(format!("{}.{}", emit_expr(&args[0], style)?, emit_member_name(&args[1])?))
                }
                "call" => {
                    if args.is_empty() {
                        return Err(TransportCliError::Refusal(
                            "realize-time:bad-call call expects callee plus arguments".into(),
                        ));
                    }
                    let callee = emit_expr(&args[0], style)?;
                    let call_args = args[1..]
                        .iter()
                        .map(|arg| emit_expr(arg, style))
                        .collect::<Result<Vec<_>, _>>()?
                        .join(", ");
                    Ok(format!("{callee}({call_args})"))
                }
                other => Err(TransportCliError::Refusal(format!(
                    "realize-time:unsupported-op {style:?} realizer cannot emit expression operation `{other}`"
                ))),
            }
        }
    }
}

fn emit_const(value: &Value, style: TargetStyle) -> Result<String, TransportCliError> {
    if let Some(n) = value.as_i64() {
        return Ok(n.to_string());
    }
    if let Some(b) = value.as_bool() {
        return Ok(match (style, b) {
            (TargetStyle::Python, true) => "True".into(),
            (TargetStyle::Python, false) => "False".into(),
            (TargetStyle::Ruby, true) => "true".into(),
            (TargetStyle::Ruby, false) => "false".into(),
            (TargetStyle::Php, true) => "true".into(),
            (TargetStyle::Php, false) => "false".into(),
            (_, true) => "true".into(),
            (_, false) => "false".into(),
        });
    }
    if let Some(s) = value.as_str() {
        return Ok(format!("{s:?}"));
    }
    Err(TransportCliError::Refusal(format!(
        "realize-time:unsupported-constant cannot emit constant `{value}`"
    )))
}

fn emit_member_name(term: &Term) -> Result<String, TransportCliError> {
    match term {
        Term::Var { name } => Ok(name.clone()),
        Term::Const { value, .. } => value.as_str().map(str::to_string).ok_or_else(|| {
            TransportCliError::Refusal(
                "realize-time:bad-member member name must be a string or variable".into(),
            )
        }),
        _ => Err(TransportCliError::Refusal(
            "realize-time:bad-member member name must be a string or variable".into(),
        )),
    }
}

fn binary_symbol(op: &str, style: TargetStyle) -> Option<&'static str> {
    match op {
        "add" => Some("+"),
        "sub" => Some("-"),
        "mul" => Some("*"),
        "div" => Some("/"),
        "mod" | "rem" => Some("%"),
        "shl" => Some("<<"),
        "shr" => Some(">>"),
        "ushr" => Some(">>>"),
        "bitand" => Some("&"),
        "bitor" => Some("|"),
        "bitxor" => Some("^"),
        "eq" => Some("=="),
        "ne" => Some("!="),
        "lt" => Some("<"),
        "le" => Some("<="),
        "gt" => Some(">"),
        "ge" => Some(">="),
        "and" => Some(if matches!(style, TargetStyle::Python | TargetStyle::Zig) {
            "and"
        } else {
            "&&"
        }),
        "or" => Some(if matches!(style, TargetStyle::Python | TargetStyle::Zig) {
            "or"
        } else {
            "||"
        }),
        _ => None,
    }
}

fn var_name(name: &str, style: TargetStyle) -> String {
    if style == TargetStyle::Php && !name.starts_with('$') {
        format!("${name}")
    } else {
        name.to_string()
    }
}

fn stmt_end(style: TargetStyle) -> &'static str {
    match style {
        TargetStyle::Python | TargetStyle::Ruby | TargetStyle::Go => "",
        _ => ";",
    }
}

fn indent_str(indent: usize) -> String {
    "    ".repeat(indent)
}

fn local_op(name: &str) -> &str {
    name.split_once(':').map(|(_, local)| local).unwrap_or(name)
}

fn ensure_arity(name: &str, args: &[Term], expected: usize) -> Result<(), TransportCliError> {
    if args.len() == expected {
        Ok(())
    } else {
        Err(TransportCliError::Refusal(format!(
            "operation `{name}` expected {expected} args, got {}",
            args.len()
        )))
    }
}

fn is_skip(term: &Term) -> bool {
    matches!(term, Term::Op { name, .. } if local_op(name) == "skip")
}
