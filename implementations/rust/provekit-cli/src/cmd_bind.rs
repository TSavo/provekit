// SPDX-License-Identifier: Apache-2.0
//
// `provekit bind`: substrate-only algebra pass.
//
// Input is ProofIR term JSON, normally the `ir-document` emitted by
// `provekit lift`. Output is the JCS-canonical named-term document for
// authoring and lower-plugin dispatch. The core BindKit still materializes the
// canonical bind-result Term::Op payload for path execution.
// Migration mode remains on the legacy rewrite path.

use std::io::{Read, Write};
use std::path::PathBuf;

use clap::Parser;
use libprovekit::core::{
    address, execute_path, BindKit, BindOptions, ConformanceDeclaration, HashMapInputCatalog,
    Input, KitRegistry, Path as CorePath, PathAlgebra, PathExecutionError, Term, Verb,
};
use owo_colors::OwoColorize;
use provekit_ir_types::{CompositionRefusalMemento, Sort};
use serde_json::Value as Json;

use crate::kit_dispatch::dispatch_exam_manifest;
use crate::{EXIT_OK, EXIT_USER_ERROR};

pub use libprovekit::core::{NamedTerm, NamedTermDocument};

#[derive(Parser, Debug, Clone)]
pub struct BindArgs {
    /// ProofIR term JSON. Reads stdin when omitted or `-`.
    pub input: Option<PathBuf>,

    /// Output file. Writes stdout when omitted or `-`.
    #[arg(short = 'o', long = "output")]
    pub output: Option<PathBuf>,

    /// Legacy migration root. Kept for cmd_bind_migrate compatibility.
    #[arg(long, alias = "project", default_value = ".")]
    pub root: PathBuf,

    /// Source language hint for diagnostics and named-term metadata.
    #[arg(long, default_value = "auto")]
    pub lang: String,

    /// Exam manifest path or CID used to cite bind refusal gap records.
    #[arg(long)]
    pub exam_manifest: Option<String>,

    /// Exam-manifest plugin name. Falls back to the built-in loader when absent.
    #[arg(long, default_value = "default")]
    pub exam_manifest_plugin: String,

    /// Legacy threshold hint. No effect in the four-verb model.
    #[arg(long, default_value = "1")]
    pub threshold: usize,

    /// Legacy rewrite flag. No effect in the four-verb model.
    #[arg(long, default_value = "invisible", value_parser = parse_rewrite)]
    pub rewrite: RewriteShape,

    /// Legacy observation mode flag. No effect in the four-verb model.
    #[arg(long, value_delimiter = ',', default_value = "monitor", value_parser = parse_mode)]
    pub mode: Vec<RuntimeMode>,

    /// Legacy target-language flag. Use `provekit lower --target=<lang>`.
    #[arg(long)]
    pub target_language: Option<String>,

    /// Source library surface for migration rewrite.
    #[arg(long)]
    pub library_from: Option<String>,

    /// Target library surface for migration rewrite.
    #[arg(long)]
    pub library_to: Option<String>,

    /// Scope migration effect propagation to one triggering callsite CID.
    #[arg(long)]
    pub focus: Option<String>,

    /// Source directory for migration rewrite.
    #[arg(long)]
    pub source_dir: Option<PathBuf>,

    /// Output directory for migration rewrite.
    #[arg(long)]
    pub out_dir: Option<PathBuf>,

    /// Receipt path for migration rewrite.
    #[arg(long)]
    pub receipt: Option<PathBuf>,

    /// Fixture sqlite database for row-shape witnesses during migration.
    #[arg(long)]
    pub witness_fixture: Option<PathBuf>,

    /// Write migrated source to out-dir. Without this flag the migration path is a dry run.
    #[arg(long)]
    pub write: bool,

    /// Suppress non-error diagnostics.
    #[arg(long)]
    pub quiet: bool,

    /// PEP 1.7.0 plugin flags retained only for migration compatibility.
    #[command(flatten)]
    pub plugins: crate::PluginFlags,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RewriteShape {
    Annotate,
    Canonical,
    Invisible,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeMode {
    Monitor,
    Emitter,
    Witness,
    Gate,
}

fn parse_rewrite(s: &str) -> Result<RewriteShape, String> {
    match s {
        "annotate" => Ok(RewriteShape::Annotate),
        "canonical" => Ok(RewriteShape::Canonical),
        "invisible" => Ok(RewriteShape::Invisible),
        other => Err(format!(
            "unknown rewrite shape '{other}'; expected annotate, canonical, or invisible"
        )),
    }
}

fn parse_mode(s: &str) -> Result<RuntimeMode, String> {
    match s {
        "monitor" => Ok(RuntimeMode::Monitor),
        "emitter" => Ok(RuntimeMode::Emitter),
        "witness" => Ok(RuntimeMode::Witness),
        "gate" => Ok(RuntimeMode::Gate),
        other => Err(format!(
            "unknown runtime mode '{other}'; expected monitor, emitter, witness, or gate"
        )),
    }
}

pub fn run(args: BindArgs) -> u8 {
    if is_migration_request(&args) {
        return crate::cmd_bind_migrate::run(args);
    }

    let raw = match read_input(args.input.as_ref()) {
        Ok(raw) => raw,
        Err(error) => {
            eprintln!("{}: {error}", "error".red().bold());
            return EXIT_USER_ERROR;
        }
    };
    let term_json: Json = match serde_json::from_slice(&raw) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("{}: parse ProofIR term JSON: {error}", "error".red().bold());
            return EXIT_USER_ERROR;
        }
    };
    let payload = match run_bind_path(term_json, &args) {
        Ok(payload) => payload,
        Err(error) => {
            eprintln!("{}: {error}", "error".red().bold());
            return EXIT_USER_ERROR;
        }
    };
    let jcs = match libprovekit::canonical::json_jcs(&payload) {
        Ok(jcs) => jcs,
        Err(error) => {
            eprintln!(
                "{}: canonicalize named-term document: {error}",
                "error".red().bold()
            );
            return EXIT_USER_ERROR;
        }
    };
    if let Err(error) = write_output(args.output.as_ref(), jcs.as_bytes()) {
        eprintln!("{}: {error}", "error".red().bold());
        return EXIT_USER_ERROR;
    }
    if !args.quiet
        && args
            .output
            .as_ref()
            .is_some_and(|path| path.as_os_str() != "-")
    {
        eprintln!("bind: wrote named-term document");
    }
    EXIT_OK
}

fn run_bind_path(term_json: Json, args: &BindArgs) -> Result<Json, BindCliError> {
    let exam_manifest = args
        .exam_manifest
        .as_deref()
        .map(|target| {
            dispatch_exam_manifest(&args.root, &args.exam_manifest_plugin, target)
                .map_err(|error| BindCliError::Failed(error.to_string()))
        })
        .transpose()?;
    let term = Term::Const {
        value: term_json,
        sort: Sort::Primitive {
            name: "LiftPluginResponse".to_string(),
        },
    };
    let term_cid = address(&term);
    let mut inputs = HashMapInputCatalog::default();
    inputs.put(term_cid.clone(), Input::Term(term));
    let path_input = Input::Path(Box::new(CorePath {
        algebra: vec![PathAlgebra {
            name: "bind".to_string(),
            kit: "bind-default".to_string(),
            inputs: vec![term_cid],
            depends_on: vec![],
            verb: Verb::Transform,
        }],
    }));
    let mut registry = KitRegistry::default();
    registry.register(
        "bind-default",
        BindKit::new(BindOptions {
            lang: args.lang.clone(),
            exam_manifest,
        }),
        ConformanceDeclaration::NonCarrier {
            reason: "transforms Input::Term to NamedTerm DomainClaim; emits no target source",
        },
    );
    let chain = execute_path(&path_input, &registry, &inputs).map_err(BindCliError::from_path)?;
    let claim = chain.terminal_claim();
    let payload = claim
        .payload
        .as_ref()
        .ok_or_else(|| BindCliError::Failed("bind claim missing term payload".to_string()))?;
    serde_json::to_value(payload)
        .map_err(|error| BindCliError::Failed(format!("serialize bind payload: {error}")))
}

#[derive(Debug)]
enum BindCliError {
    Refused(Box<CompositionRefusalMemento>),
    Failed(String),
}

impl BindCliError {
    fn from_path(error: PathExecutionError) -> Self {
        match error {
            PathExecutionError::Refused(refusal) => Self::Refused(refusal),
            other => Self::Failed(other.to_string()),
        }
    }
}

impl std::fmt::Display for BindCliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refused(refusal) => {
                f.write_str(&serde_json::to_string(refusal).unwrap_or_else(|_| {
                    format!(
                        "{}: {}",
                        refusal.header.failure_kind, refusal.header.failure_detail
                    )
                }))
            }
            Self::Failed(message) => f.write_str(message),
        }
    }
}

fn is_migration_request(args: &BindArgs) -> bool {
    args.library_from.is_some()
        || args.library_to.is_some()
        || args.focus.is_some()
        || args.source_dir.is_some()
        || args.out_dir.is_some()
        || args.receipt.is_some()
        || args.witness_fixture.is_some()
        || args.write
}

fn read_input(path: Option<&PathBuf>) -> Result<Vec<u8>, String> {
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

fn write_output(path: Option<&PathBuf>, bytes: &[u8]) -> Result<(), String> {
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
