// SPDX-License-Identifier: Apache-2.0
//
// Lift-plugin resolver and legacy CLI adapter.
//
// The transport and primitive claim construction are `libprovekit::core::Kit`.
// This module only resolves the surface manifest, builds the lift request
// input, and keeps the legacy CLI response escape hatch while old command edges
// are migrated.

use std::path::{Path, PathBuf};
use std::time::Instant;

use libprovekit::core::{
    address, execute_path, ConformanceDeclaration, Dialect, DomainClaim, HashMapInputCatalog,
    Input, KitRegistry, LiftKit, LiftPluginKit, LiftPluginKitError, Path as CorePath, PathAlgebra,
    PathExecutionError, Term, Verb,
};
use owo_colors::OwoColorize;
use provekit_ir_types::CompositionRefusalMemento;
use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub(crate) struct LiftPluginManifest {
    pub name: String,
    pub command: Vec<String>,
    pub working_dir: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub(crate) struct LiftPluginSession {
    pub claim: DomainClaim,
    legacy_response: Value,
}

impl LiftPluginSession {
    pub(crate) fn response(&self) -> &Value {
        &self.legacy_response
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct LiftPluginOptions {
    pub identify_only: bool,
    pub library_bindings: bool,
}

#[derive(Debug, Clone)]
pub(crate) enum LiftPluginError {
    MissingBinary { binary: String },
    Refused(Box<CompositionRefusalMemento>),
    Failed(String),
}

impl From<LiftPluginKitError> for LiftPluginError {
    fn from(value: LiftPluginKitError) -> Self {
        match value {
            LiftPluginKitError::MissingBinary { binary } => Self::MissingBinary { binary },
            LiftPluginKitError::Failed(message) => Self::Failed(message),
            LiftPluginKitError::LegacyResponseUnavailable => {
                Self::Failed("lift plugin term no longer carries a legacy response".to_string())
            }
        }
    }
}

impl std::fmt::Display for LiftPluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingBinary { binary } => write!(f, "lifter binary `{binary}` not found"),
            Self::Refused(refusal) => write!(
                f,
                "composition refused: {}: {}",
                refusal.header.failure_kind, refusal.header.failure_detail
            ),
            Self::Failed(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for LiftPluginError {}

pub(crate) fn dispatch_lift(
    project_root: &Path,
    surface: &str,
    options: LiftPluginOptions,
    quiet: bool,
) -> Result<LiftPluginSession, LiftPluginError> {
    let started = Instant::now();
    let manifest = find_manifest(project_root, surface).map_err(LiftPluginError::Failed)?;
    trace_log(format!(
        "lift rpc start surface={surface} project={} plugin={} command={:?}",
        project_root.display(),
        manifest.name,
        manifest.command
    ));
    if !quiet {
        println!(
            "{}: surface=`{}` plugin=`{}` command={:?}",
            "dispatch".green().bold(),
            surface,
            manifest.name,
            manifest.command
        );
    }

    let lift_params = build_lift_params(project_root, surface, options);
    let kit = LiftPluginKit::new(
        surface,
        manifest.command.clone(),
        resolved_working_dir(project_root, &manifest),
    );
    trace_log(format!("lift kit parse surface={surface}"));
    let core_session = kit.parse_session(&Input::Spec(lift_params.clone()))?;
    trace_log(format!(
        "lift kit parsed surface={surface} elapsed={:?}",
        started.elapsed()
    ));
    if !quiet {
        if let Some(name) = core_session
            .initialize_response
            .get("name")
            .and_then(|value| value.as_str())
        {
            println!("{}: plugin `{}` ready", "ok".green().bold(), name);
        }
    }

    Ok(LiftPluginSession {
        legacy_response: core_session.legacy_response,
        claim: core_session.claim,
    })
}

pub(crate) fn dispatch_lift_path(
    project_root: &Path,
    surface: &str,
    options: LiftPluginOptions,
    quiet: bool,
) -> Result<LiftPluginSession, LiftPluginError> {
    let started = Instant::now();
    let manifest = find_manifest(project_root, surface);
    if !quiet {
        match &manifest {
            Ok(manifest) => println!(
                "{}: surface=`{}` plugin=`{}` command={:?}",
                "dispatch".green().bold(),
                surface,
                manifest.name,
                manifest.command
            ),
            Err(error) => println!(
                "{}: surface=`{}` registry miss: {}",
                "dispatch".yellow().bold(),
                surface,
                error
            ),
        }
    }

    let lift_params = build_lift_params(project_root, surface, options);
    let dialect = dialect_for_surface(surface);
    let kit_name = lift_kit_name(surface);
    let source = Input::Source {
        dialect: dialect.clone(),
        bytes: serde_json::to_vec(&lift_params)
            .map_err(|error| LiftPluginError::Failed(format!("encode lift request: {error}")))?,
    };
    let source_cid = address(&source);
    let mut inputs = HashMapInputCatalog::default();
    inputs.put(source_cid.clone(), source);
    let path_input = Input::Path(Box::new(CorePath {
        algebra: vec![PathAlgebra {
            name: "lift".to_string(),
            kit: kit_name.clone(),
            inputs: vec![source_cid],
            depends_on: vec![],
            verb: Verb::Transform,
        }],
    }));
    let mut registry = KitRegistry::default();
    if let Ok(manifest) = &manifest {
        registry.register(
            kit_name,
            LiftKit::new(
                dialect,
                surface,
                manifest.command.clone(),
                resolved_working_dir(project_root, manifest),
            ),
            ConformanceDeclaration::NonCarrier {
                reason: "lifts source bytes to DomainClaim; no target source produced",
            },
        );
    }

    trace_log(format!("lift path execute surface={surface}"));
    let chain = execute_path(&path_input, &registry, &inputs).map_err(lift_error_from_path)?;
    let claim = chain.terminal_claim().clone();
    trace_log(format!(
        "lift path executed surface={surface} elapsed={:?}",
        started.elapsed()
    ));
    let legacy_response = claim
        .payload
        .as_ref()
        .ok_or_else(|| LiftPluginError::Failed("lift claim missing term payload".to_string()))
        .and_then(response_from_payload_term)?;

    Ok(LiftPluginSession {
        legacy_response,
        claim,
    })
}

fn response_from_payload_term(term: &Term) -> Result<Value, LiftPluginError> {
    match term {
        Term::Const { value, .. } => Ok(value.clone()),
        _ => Err(LiftPluginError::Failed(
            "lift claim payload was not a lift response term".to_string(),
        )),
    }
}

fn lift_error_from_path(error: PathExecutionError) -> LiftPluginError {
    match error {
        PathExecutionError::Refused(refusal) => LiftPluginError::Refused(refusal),
        PathExecutionError::Kit(error) => match error {
            libprovekit::core::KitError::Transformation(message)
                if message.starts_with("lift plugin transport: lifter binary `") =>
            {
                LiftPluginError::Failed(message)
            }
            other => LiftPluginError::Failed(other.to_string()),
        },
        other => LiftPluginError::Failed(other.to_string()),
    }
}

fn dialect_for_surface(surface: &str) -> Dialect {
    match surface {
        "rust" => Dialect::Rust,
        "c" => Dialect::C,
        "x86-64" | "x86_64" => Dialect::X86_64,
        "aarch64" => Dialect::AArch64,
        "wasm" => Dialect::Wasm,
        "jvm-bytecode" => Dialect::JvmBytecode,
        "coq" => Dialect::Coq,
        "smt-lib" => Dialect::SmtLib,
        other => Dialect::Other(other.to_string()),
    }
}

fn lift_kit_name(surface: &str) -> String {
    format!("lift-{surface}")
}

fn parse_manifest(path: &Path) -> Result<LiftPluginManifest, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let mut manifest = LiftPluginManifest {
        name: String::new(),
        command: Vec::new(),
        working_dir: None,
    };
    for line in text.lines() {
        let line = match line.find('#') {
            Some(pos) => &line[..pos],
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
            "name" => manifest.name = val.trim_matches('"').to_string(),
            "working_dir" => manifest.working_dir = Some(PathBuf::from(val.trim_matches('"'))),
            "command" => {
                let inner = val.trim_matches(|c| c == '[' || c == ']');
                manifest.command = inner
                    .split(',')
                    .map(|s| s.trim().trim_matches('"').to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            _ => {}
        }
    }
    if manifest.command.is_empty() {
        return Err(format!("manifest {} has no `command`", path.display()));
    }
    Ok(manifest)
}

fn find_manifest(project_root: &Path, surface: &str) -> Result<LiftPluginManifest, String> {
    let project_local = project_root
        .join(".provekit")
        .join("lift")
        .join(surface)
        .join("manifest.toml");
    if project_local.exists() {
        return parse_manifest(&project_local);
    }
    if let Some(home) = std::env::var_os("HOME") {
        let user_global = PathBuf::from(home)
            .join(".config")
            .join("provekit")
            .join("lift")
            .join(surface)
            .join("manifest.toml");
        if user_global.exists() {
            return parse_manifest(&user_global);
        }
    }
    Err(format!(
        "no plugin manifest for surface `{surface}` (looked in .provekit/lift/{surface}/manifest.toml and ~/.config/provekit/lift/{surface}/manifest.toml)"
    ))
}

fn resolved_working_dir(project_root: &Path, manifest: &LiftPluginManifest) -> Option<PathBuf> {
    manifest.working_dir.as_ref().map(|working_dir| {
        if working_dir.is_absolute() {
            working_dir.clone()
        } else {
            project_root.join(working_dir)
        }
    })
}

pub fn build_lift_params(project_root: &Path, surface: &str, options: LiftPluginOptions) -> Value {
    let workspace_root = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    let layer = if options.identify_only {
        "identify-only"
    } else if options.library_bindings {
        "library-bindings"
    } else {
        "all"
    };
    json!({
        "surface": surface,
        "workspace_root": workspace_root,
        "config_path": ".provekit/config.toml",
        "source_paths": ["."],
        "options": {
            "layer": layer,
            "identifyOnly": options.identify_only,
        }
    })
}

fn trace_enabled() -> bool {
    std::env::var_os("PROVEKIT_CLI_TRACE").is_some()
}

fn trace_log(message: impl std::fmt::Display) {
    if trace_enabled() {
        eprintln!("provekit trace: {message}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libprovekit::core::{DomainKind, Term};
    use provekit_ir_types::Sort;

    #[test]
    fn lift_plugin_options_select_library_bindings_layer() {
        let request = build_lift_params(
            Path::new("."),
            "python",
            LiftPluginOptions {
                identify_only: false,
                library_bindings: true,
            },
        );

        assert_eq!(
            request["options"]["layer"].as_str(),
            Some("library-bindings")
        );
        assert_eq!(request["options"]["identifyOnly"].as_bool(), Some(false));
    }

    #[test]
    fn lift_session_is_domain_claim_first_and_legacy_response_round_trips() {
        let response = json!({
            "kind": "ir-document",
            "ir": [],
            "diagnostics": []
        });
        let request = build_lift_params(
            Path::new("."),
            "rust",
            LiftPluginOptions {
                identify_only: false,
                library_bindings: false,
            },
        );

        let term = Term::Const {
            value: response.clone(),
            sort: Sort::Primitive {
                name: "LiftPluginResponse".to_string(),
            },
        };
        let kit = LiftPluginKit::new("rust", Vec::new(), None);
        let input = Input::Spec(request);
        let claim = kit
            .claim_from_response_term(&input, term)
            .expect("lift response becomes a primitive claim");
        let session = LiftPluginSession {
            claim,
            legacy_response: response.clone(),
        };

        assert_eq!(
            session.claim.domain,
            DomainKind::Other("lift-plugin".to_string())
        );
        assert_eq!(session.claim.from.len(), 1);
        assert!(session.claim.premises.is_empty());
        assert_eq!(session.claim.artifacts.len(), 1);
        assert_eq!(session.response(), &response);
    }
}
