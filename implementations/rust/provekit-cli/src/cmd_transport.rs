// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::Parser;
use libprovekit::core::{Cid, Term};
use libprovekit::transport::{transport_term, OperationTransport, TermTransport};
use owo_colors::OwoColorize;
use provekit_ir_types::Sort;
use serde::Serialize;
use serde_json::{json, Value};

use crate::{OutputFlags, EXIT_OK, EXIT_USER_ERROR, EXIT_VERIFY_FAIL};

#[derive(Parser, Debug, Clone)]
pub struct TransportArgs {
    /// Source file to port through the concept hub.
    pub src_file: PathBuf,
    /// Target language. v1 supports `rust` for C11 inputs.
    #[arg(long)]
    pub to: String,
    /// Function to project from the source file.
    #[arg(long, default_value = "foo")]
    pub function: String,
    /// Output directory for term artifacts and realized target source.
    #[arg(long = "out")]
    pub output_dir: Option<PathBuf>,
    #[command(flatten)]
    pub flags: OutputFlags,
}

#[derive(Debug, Serialize)]
struct TransportReport {
    status: &'static str,
    source_file: String,
    target_language: String,
    function: String,
    artifacts: BTreeMap<String, String>,
    normalizations: Vec<String>,
    morphism_receipts: Vec<String>,
    deferred: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
enum TransportCliError {
    #[error("Refusal: {0}")]
    Refusal(String),
    #[error("{0}")]
    Failed(String),
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

fn print_report(report: TransportReport) {
    if report.artifacts.is_empty() {
        return;
    }
    println!(
        "{}: {} -> {}",
        "transport".green().bold(),
        report.source_file,
        report.target_language
    );
    for (name, path) in &report.artifacts {
        println!("  {name}: {path}");
    }
}

fn run_inner(args: TransportArgs) -> Result<TransportReport, TransportCliError> {
    if args.to != "rust" {
        return Err(TransportCliError::Refusal(format!(
            "target `{}` is not supported by the v1 transport demo; supported target: rust",
            args.to
        )));
    }
    if !args.src_file.exists() {
        return Err(TransportCliError::Refusal(format!(
            "source file not found: {}",
            args.src_file.display()
        )));
    }
    let src_file = fs::canonicalize(&args.src_file).map_err(|error| {
        TransportCliError::Failed(format!("canonicalize {}: {error}", args.src_file.display()))
    })?;
    let root = repo_root()?;
    let out_dir = args
        .output_dir
        .clone()
        .unwrap_or_else(|| root.join("menagerie/cross-language-port/artifacts"));
    fs::create_dir_all(&out_dir).map_err(|error| {
        TransportCliError::Failed(format!("create {}: {error}", out_dir.display()))
    })?;

    let catalog = CatalogCids::load(&root)?;
    let projected = project_c11_term(&root, &src_file, &args.function)?;
    let raw_term = projected
        .get("term")
        .ok_or_else(|| TransportCliError::Failed("C projector response missing `term`".into()))?;
    let mut normalizations = Vec::new();
    let c11_term = parse_c11_projected_term(raw_term, &catalog, &mut normalizations)?;

    let c11_to_concept = c11_to_concept_transport(&catalog);
    let concept_to_rust = concept_to_rust_transport(&catalog);
    let rust_to_concept = rust_to_concept_transport(&catalog);

    let concept_term = transport_term(&c11_to_concept, &c11_term)
        .map_err(|error| TransportCliError::Refusal(error.to_string()))?;
    let rust_term = transport_term(&concept_to_rust, &concept_term)
        .map_err(|error| TransportCliError::Refusal(error.to_string()))?;
    let roundtrip_concept = transport_term(&rust_to_concept, &rust_term)
        .map_err(|error| TransportCliError::Refusal(error.to_string()))?;
    if roundtrip_concept != concept_term {
        return Err(TransportCliError::Failed(
            "concept -> rust -> concept transport did not round trip".into(),
        ));
    }

    let source_text = fs::read_to_string(&src_file).map_err(|error| {
        TransportCliError::Failed(format!("read {}: {error}", src_file.display()))
    })?;
    let params = parse_int_params(&source_text, &args.function).unwrap_or_else(|| {
        let mut vars = BTreeSet::new();
        collect_vars(&rust_term, &mut vars);
        vars.into_iter().collect()
    });
    let rust_source = realize_rust_function(&args.function, &params, &rust_term)?;

    let mut artifacts = BTreeMap::new();
    let c11_path = out_dir.join("c11.term.json");
    let concept_path = out_dir.join("concept.term.json");
    let rust_path = out_dir.join("rust.term.json");
    let roundtrip_path = out_dir.join("roundtrip.concept.term.json");
    let source_path = out_dir.join(format!("{}.rs", args.function));
    write_json(&c11_path, &c11_term)?;
    write_json(&concept_path, &concept_term)?;
    write_json(&rust_path, &rust_term)?;
    write_json(&roundtrip_path, &roundtrip_concept)?;
    fs::write(&source_path, rust_source).map_err(|error| {
        TransportCliError::Failed(format!("write {}: {error}", source_path.display()))
    })?;
    artifacts.insert("c11_term".into(), c11_path.display().to_string());
    artifacts.insert("concept_term".into(), concept_path.display().to_string());
    artifacts.insert("rust_term".into(), rust_path.display().to_string());
    artifacts.insert(
        "roundtrip_concept_term".into(),
        roundtrip_path.display().to_string(),
    );
    artifacts.insert("rust_source".into(), source_path.display().to_string());

    let report = TransportReport {
        status: "transported",
        source_file: src_file.display().to_string(),
        target_language: args.to,
        function: args.function,
        artifacts,
        normalizations,
        morphism_receipts: catalog.receipts.clone(),
        deferred: vec![
            "general C11 desugaring beyond integer bop_eq and literal unary minus".into(),
            "calling provekit-walk as a stable Rust re-lift subcommand from provekit transport".into(),
            "full `provekit migrate <src> --to <lang>` protocol; existing migrate spec is catalog-version migration".into(),
        ],
    };

    if args.flags.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).expect("serialize transport report")
        );
    } else if !args.flags.quiet {
        print_report(report.clone_for_print());
    }
    Ok(report)
}

impl TransportReport {
    fn clone_for_print(&self) -> Self {
        Self {
            status: self.status,
            source_file: self.source_file.clone(),
            target_language: self.target_language.clone(),
            function: self.function.clone(),
            artifacts: self.artifacts.clone(),
            normalizations: self.normalizations.clone(),
            morphism_receipts: self.morphism_receipts.clone(),
            deferred: self.deferred.clone(),
        }
    }
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

fn project_c11_term(
    root: &Path,
    source: &Path,
    function: &str,
) -> Result<Value, TransportCliError> {
    let projector = root.join("implementations/c/provekit-walk-c/provekit-c11-term-project");
    if !projector.exists() {
        return Err(TransportCliError::Refusal(format!(
            "C term projector is not built: {}; run `make -C implementations/c/provekit-walk-c`",
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
            "C lifter refused the source (status {}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| TransportCliError::Failed(format!("parse C term projector JSON: {error}")))
}

#[derive(Debug)]
struct CatalogCids {
    c11: BTreeMap<String, Cid>,
    rust: BTreeMap<String, Cid>,
    concept: BTreeMap<String, Cid>,
    receipts: Vec<String>,
}

impl CatalogCids {
    fn load(root: &Path) -> Result<Self, TransportCliError> {
        Ok(Self {
            c11: load_component_cids(
                &root.join("menagerie/c11-language-signature/component-cids.json"),
            )?,
            rust: load_component_cids(
                &root.join("menagerie/rust-language-signature/component-cids.json"),
            )?,
            concept: load_concept_cids(&root.join("menagerie/concept-shapes/cids.tsv"))?,
            receipts: load_primitive_receipts(&root.join("menagerie/concept-shapes/cids.tsv"))?,
        })
    }

    fn c11_op(&self, name: &str) -> Cid {
        self.c11
            .get(name)
            .unwrap_or_else(|| panic!("missing C11 cid for {name}"))
            .clone()
    }

    fn rust_op(&self, name: &str) -> Cid {
        self.rust
            .get(name)
            .unwrap_or_else(|| panic!("missing Rust cid for {name}"))
            .clone()
    }

    fn concept_op(&self, name: &str) -> Cid {
        self.concept
            .get(name)
            .unwrap_or_else(|| panic!("missing concept cid for {name}"))
            .clone()
    }
}

fn load_component_cids(path: &Path) -> Result<BTreeMap<String, Cid>, TransportCliError> {
    let text = fs::read_to_string(path)
        .map_err(|error| TransportCliError::Failed(format!("read {}: {error}", path.display())))?;
    let rows: Vec<Value> = serde_json::from_str(&text)
        .map_err(|error| TransportCliError::Failed(format!("parse {}: {error}", path.display())))?;
    let mut out = BTreeMap::new();
    for row in rows {
        if row.get("kind").and_then(Value::as_str) != Some("algorithm") {
            continue;
        }
        let Some(spec) = row.get("spec").and_then(Value::as_str) else {
            continue;
        };
        let Some(cid) = row.get("cid").and_then(Value::as_str) else {
            continue;
        };
        let Some(name) = spec
            .strip_prefix("op_")
            .and_then(|s| s.strip_suffix(".spec.json"))
        else {
            continue;
        };
        out.insert(
            name.to_string(),
            Cid::parse(cid).map_err(|error| {
                TransportCliError::Failed(format!("invalid cid in {}: {error}", path.display()))
            })?,
        );
    }
    Ok(out)
}

fn load_concept_cids(path: &Path) -> Result<BTreeMap<String, Cid>, TransportCliError> {
    let text = fs::read_to_string(path)
        .map_err(|error| TransportCliError::Failed(format!("read {}: {error}", path.display())))?;
    let mut out = BTreeMap::new();
    for line in text.lines().skip(1) {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 3 || parts[0] != "shape" || !parts[1].starts_with("concept:") {
            continue;
        }
        let key = parts[1].strip_prefix("concept:").unwrap_or(parts[1]);
        out.insert(
            key.to_string(),
            Cid::parse(parts[2]).map_err(|error| {
                TransportCliError::Failed(format!(
                    "invalid concept cid in {}: {error}",
                    path.display()
                ))
            })?,
        );
    }
    Ok(out)
}

fn load_primitive_receipts(path: &Path) -> Result<Vec<String>, TransportCliError> {
    let text = fs::read_to_string(path)
        .map_err(|error| TransportCliError::Failed(format!("read {}: {error}", path.display())))?;
    let primitive_names = BTreeSet::from([
        "morphism_c11_if_to_conditional",
        "morphism_rust_if_to_conditional",
        "morphism_c11_seq_to_seq",
        "morphism_rust_seq_to_seq",
        "morphism_c11_return_to_return",
        "morphism_rust_return_to_return",
        "morphism_c11_eq_to_eq",
        "morphism_rust_eq_to_eq",
        "morphism_c11_skip_to_skip",
        "morphism_rust_skip_to_skip",
    ]);
    Ok(text
        .lines()
        .skip(1)
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 3 && parts[0] == "receipt" && primitive_names.contains(parts[1]) {
                Some(format!("{}={}", parts[1], parts[2]))
            } else {
                None
            }
        })
        .collect())
}

fn c11_to_concept_transport(catalog: &CatalogCids) -> TermTransport {
    TermTransport::new(vec![
        OperationTransport::new(
            "c11:if",
            catalog.c11_op("if"),
            "concept:conditional",
            catalog.concept_op("conditional"),
        ),
        OperationTransport::new(
            "c11:seq",
            catalog.c11_op("seq"),
            "concept:seq",
            catalog.concept_op("seq"),
        ),
        OperationTransport::new(
            "c11:return",
            catalog.c11_op("return"),
            "concept:return",
            catalog.concept_op("return"),
        ),
        OperationTransport::new(
            "c11:eq",
            catalog.c11_op("eq"),
            "concept:eq",
            catalog.concept_op("eq"),
        ),
        OperationTransport::new(
            "c11:skip",
            catalog.c11_op("skip"),
            "concept:skip",
            catalog.concept_op("skip"),
        ),
    ])
}

fn concept_to_rust_transport(catalog: &CatalogCids) -> TermTransport {
    TermTransport::new(vec![
        OperationTransport::new(
            "concept:conditional",
            catalog.concept_op("conditional"),
            "rust:if",
            catalog.rust_op("if"),
        ),
        OperationTransport::new(
            "concept:seq",
            catalog.concept_op("seq"),
            "rust:seq",
            catalog.rust_op("seq"),
        ),
        OperationTransport::new(
            "concept:return",
            catalog.concept_op("return"),
            "rust:return",
            catalog.rust_op("return"),
        ),
        OperationTransport::new(
            "concept:eq",
            catalog.concept_op("eq"),
            "rust:eq",
            catalog.rust_op("eq"),
        ),
        OperationTransport::new(
            "concept:skip",
            catalog.concept_op("skip"),
            "rust:skip",
            catalog.rust_op("skip"),
        ),
    ])
}

fn rust_to_concept_transport(catalog: &CatalogCids) -> TermTransport {
    TermTransport::new(vec![
        OperationTransport::new(
            "rust:if",
            catalog.rust_op("if"),
            "concept:conditional",
            catalog.concept_op("conditional"),
        ),
        OperationTransport::new(
            "rust:seq",
            catalog.rust_op("seq"),
            "concept:seq",
            catalog.concept_op("seq"),
        ),
        OperationTransport::new(
            "rust:return",
            catalog.rust_op("return"),
            "concept:return",
            catalog.concept_op("return"),
        ),
        OperationTransport::new(
            "rust:eq",
            catalog.rust_op("eq"),
            "concept:eq",
            catalog.concept_op("eq"),
        ),
        OperationTransport::new(
            "rust:skip",
            catalog.rust_op("skip"),
            "concept:skip",
            catalog.concept_op("skip"),
        ),
    ])
}

fn parse_c11_projected_term(
    value: &Value,
    catalog: &CatalogCids,
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
            if name == "uop_neg" {
                if raw_args.len() == 1 {
                    if let Some(term) = parse_negative_integer_literal(&raw_args[0])? {
                        normalizations.push(
                            "folded c11:uop_neg(integer-literal) into a signed integer constant"
                                .into(),
                        );
                        return Ok(term);
                    }
                }
                return Err(TransportCliError::Refusal(
                    "c11:uop_neg has no discharged morphism in this demo except literal constant folding"
                        .into(),
                ));
            }

            let parsed_args = raw_args
                .iter()
                .map(|arg| parse_c11_projected_term(arg, catalog, normalizations))
                .collect::<Result<Vec<_>, _>>()?;
            let (mapped_name, cid) = match name {
                "if" => ("c11:if", catalog.c11_op("if")),
                "seq" => ("c11:seq", catalog.c11_op("seq")),
                "return" => ("c11:return", catalog.c11_op("return")),
                "skip" => ("c11:skip", catalog.c11_op("skip")),
                "bop_eq" => {
                    normalizations.push(
                        "normalized side-effect-free integer c11:bop_eq to primitive c11:eq".into(),
                    );
                    ("c11:eq", catalog.c11_op("eq"))
                }
                "eq" => ("c11:eq", catalog.c11_op("eq")),
                other => {
                    return Err(TransportCliError::Refusal(format!(
                        "operation `c11:{other}` lacks a discharged morphism into the concept hub"
                    )))
                }
            };
            Ok(Term::Op {
                op_cid: cid,
                name: mapped_name.to_string(),
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
            "unsupported C11 term node kind `{other}`"
        ))),
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
        if pieces.len() == 2 && pieces[0] == "int" {
            out.push(pieces[1].trim_matches('*').to_string());
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn realize_rust_function(
    function: &str,
    params: &[String],
    body: &Term,
) -> Result<String, TransportCliError> {
    let params = params
        .iter()
        .map(|param| format!("{param}: i32"))
        .collect::<Vec<_>>()
        .join(", ");
    let mut out = format!("pub fn {function}({params}) -> i32 {{\n");
    out.push_str(&emit_stmt(body, 1)?);
    out.push_str("}\n");
    Ok(out)
}

fn emit_stmt(term: &Term, indent: usize) -> Result<String, TransportCliError> {
    let pad = "    ".repeat(indent);
    match term {
        Term::Op { name, args, .. } if name == "rust:seq" => {
            ensure_arity(name, args, 2)?;
            Ok(format!(
                "{}{}",
                emit_stmt(&args[0], indent)?,
                emit_stmt(&args[1], indent)?
            ))
        }
        Term::Op { name, args, .. } if name == "rust:if" => {
            ensure_arity(name, args, 3)?;
            let cond = emit_expr(&args[0])?;
            let then_branch = emit_stmt(&args[1], indent + 1)?;
            let else_branch = emit_stmt(&args[2], indent + 1)?;
            if is_skip(&args[2]) {
                Ok(format!("{pad}if {cond} {{\n{then_branch}{pad}}}\n"))
            } else {
                Ok(format!(
                    "{pad}if {cond} {{\n{then_branch}{pad}}} else {{\n{else_branch}{pad}}}\n"
                ))
            }
        }
        Term::Op { name, args, .. } if name == "rust:return" => {
            ensure_arity(name, args, 1)?;
            Ok(format!("{pad}return {};\n", emit_expr(&args[0])?))
        }
        Term::Op { name, .. } if name == "rust:skip" => Ok(String::new()),
        other => Err(TransportCliError::Refusal(format!(
            "Rust realizer cannot emit statement term `{}`",
            term_name(other)
        ))),
    }
}

fn emit_expr(term: &Term) -> Result<String, TransportCliError> {
    match term {
        Term::Var { name } => Ok(name.clone()),
        Term::Const { value, .. } => {
            if let Some(n) = value.as_i64() {
                Ok(n.to_string())
            } else if let Some(s) = value.as_str() {
                Ok(format!("{:?}", s))
            } else {
                Err(TransportCliError::Refusal(format!(
                    "Rust realizer cannot emit constant `{value}`"
                )))
            }
        }
        Term::Op { name, args, .. } if name == "rust:eq" => {
            ensure_arity(name, args, 2)?;
            Ok(format!(
                "{} == {}",
                emit_expr(&args[0])?,
                emit_expr(&args[1])?
            ))
        }
        other => Err(TransportCliError::Refusal(format!(
            "Rust realizer cannot emit expression term `{}`",
            term_name(other)
        ))),
    }
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
    matches!(term, Term::Op { name, .. } if name == "rust:skip")
}

fn term_name(term: &Term) -> &str {
    match term {
        Term::Op { name, .. } => name,
        Term::Var { .. } => "var",
        Term::Const { .. } => "const",
        Term::Unit => "unit",
    }
}
