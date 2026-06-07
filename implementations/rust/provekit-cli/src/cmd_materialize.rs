// SPDX-License-Identifier: Apache-2.0
//
// `provekit materialize` turns concept-citation carriers in source files into
// library-bound source by composing the existing LowerKit/realize path.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use clap::Parser;
use libprovekit::core::emit_obligation::{build_bridge_body, member_envelope_canonical};
use libprovekit::core::{
    execute_path, ConformanceDeclaration, HashMapInputCatalog, Input, KitRegistry, LowerKit,
    Path as CorePath, PathAlgebra, RealizedSource, Verb,
};
// Source-transform primitives live in libprovekit (#1335 Phase A). The glob
// re-export keeps the carrier-parsing surface available to `materialize_source_text`
// after the extract-and-move; Phase C reroutes the per-site loop through
// `transform_source_text` + the `SiteTransformKit` trait (#1337).
pub(crate) use libprovekit::core::source_transform::*;
use owo_colors::OwoColorize;
use provekit_proof_envelope::{
    build_proof_envelope, ed25519_pubkey_string, Ed25519Seed, ProofEnvelopeInput,
};
use serde_json::Value as Json;
use walkdir::WalkDir;

use crate::kit_dispatch::{
    dispatch_materialize_check, dispatch_materialize_source, provides_concepts_for_realize,
    DispatchRealizeTransport, MaterializeSourceError,
};
use crate::{OutputFlags, EXIT_OK, EXIT_USER_ERROR, EXIT_VERIFY_FAIL};

const MATERIALIZE_BRIDGE_DECLARED_AT: &str = "2026-05-27T00:00:00.000Z";
const MATERIALIZE_BRIDGE_SIGNER_SEED: Ed25519Seed = [0x6d; 32];

#[derive(Parser, Debug, Clone)]
pub struct MaterializeArgs {
    /// Library surface to materialize, e.g. `typescript-better-sqlite3` or `better-sqlite3`.
    #[arg(long)]
    pub library: String,
    /// Source directory to scan for `provekit-concept:` carriers.
    #[arg(long = "source-dir")]
    pub source_dir: PathBuf,
    /// Project root containing `.provekit/realize/*/manifest.toml`. Defaults to source-dir parent/current.
    #[arg(long)]
    pub project: Option<PathBuf>,
    /// Target language. Inferred from a language-prefixed --library or project markers when omitted.
    #[arg(long, alias = "language")]
    pub target: Option<String>,
    /// Write files in place. Omitted means dry-run to stdout.
    #[arg(long)]
    pub write: bool,
    /// Write materialized files under this directory, preserving paths relative to --source-dir.
    #[arg(long = "out-dir", conflicts_with = "write")]
    pub out_dir: Option<PathBuf>,
    /// After emitting files (requires --out-dir), ask the selected realize kit
    /// to run its native check over the output directory. Non-zero kit verdict
    /// causes `materialize` to return exit code 1. Off by default.
    #[arg(long = "compile-check", requires = "out_dir")]
    pub compile_check: bool,
    #[command(flatten)]
    pub out: OutputFlags,
}

#[derive(Debug, Clone)]
struct MaterializedFile {
    source_path: PathBuf,
    relative_path: PathBuf,
    content: String,
    /// Per-file receipt. Carries the trichotomy (exact / lossy /
    /// refused) plus per-site witnesses + loss/refusal mementos. Phase E
    /// (`#1339`) wires this so materialize no longer aborts on first
    /// refusal: every site's outcome lives in the receipt instead.
    receipt: SourceTransformReceipt,
    /// Per-site realization fragments for the LANGUAGE KIT's assemble RPC.
    /// Empty for kits/languages whose realize plugin produced no body (e.g.
    /// refuse-only files). The `--out-dir` path routes these through
    /// `dispatch_assemble`; the substrate writes back what the kit returns.
    fragments: Vec<Json>,
    /// Source-declared package/namespace (e.g. Java `package x.y;`). Passed
    /// to the assemble RPC as `package_hint` so the kit reproduces it. None
    /// when the source declares none.
    package_hint: Option<String>,
    /// Kit-owned source materialization can declare native compile metadata
    /// alongside the returned files. The CLI aggregates this and passes it
    /// back to the kit check RPC without interpreting it.
    compile_classpath: Vec<String>,
}

pub fn run(args: MaterializeArgs) -> u8 {
    let project_root = args.project.clone().unwrap_or_else(|| {
        args.source_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    });
    if !project_root.exists() {
        eprintln!(
            "{}: project not found: {}",
            "error".red().bold(),
            project_root.display()
        );
        return EXIT_USER_ERROR;
    }
    if !args.source_dir.is_dir() {
        eprintln!(
            "{}: source-dir not found or not a directory: {}",
            "error".red().bold(),
            args.source_dir.display()
        );
        return EXIT_USER_ERROR;
    }

    let (target_lang, library_tag) =
        match resolve_library_surface(&project_root, args.target.as_deref(), args.library.as_str())
        {
            Ok(surface) => surface,
            Err(error) => {
                eprintln!("{}: {error}", "error".red().bold());
                return EXIT_USER_ERROR;
            }
        };

    let files = match materialize_source_dir_via_kit(
        &project_root,
        &args.source_dir,
        &target_lang,
        library_tag.as_deref(),
    ) {
        Ok(Some(files)) => files,
        Ok(None) => match materialize_source_dir(
            &project_root,
            &args.source_dir,
            &target_lang,
            library_tag.as_deref(),
        ) {
            Ok(files) => files,
            Err(error) => {
                eprintln!("{}: {error}", "error".red().bold());
                return EXIT_VERIFY_FAIL;
            }
        },
        Err(error) => {
            eprintln!("{}: {error}", "error".red().bold());
            return EXIT_VERIFY_FAIL;
        }
    };

    if files.is_empty() {
        if !args.out.quiet {
            eprintln!(
                "{} found 0 concept citation(s)",
                "materialize".green().bold()
            );
        }
        return EXIT_OK;
    }

    if let Some(out_dir) = args.out_dir.as_ref() {
        // The compilation unit is assembled and checked BY THE LANGUAGE KIT over RPC
        // (imports, helper hoisting, package, class wrapping are the kit's
        // job; the substrate holds no language syntax). When the kit supports
        // the assemble RPC, route every file's fragments through it and write
        // back what the kit returns, then ask the kit to run any native
        // compile/test check with the kit-declared metadata. If the selected
        // kit does not implement assemble, fail closed: target-language source
        // assembly is kit-owned, not a CLI fallback surface.
        match emit_out_dir_via_kit_assemble(
            &project_root,
            &target_lang,
            library_tag.as_deref(),
            out_dir,
            &files,
            args.compile_check,
        ) {
            EmitOutcome::Assembled(code) => {
                if code != EXIT_OK {
                    return code;
                }
            }
        }
    } else if args.write {
        if let Err(error) = write_in_place(&files) {
            eprintln!("{}: {error}", "error".red().bold());
            return EXIT_USER_ERROR;
        }
    } else if let Err(error) = print_dry_run(&files) {
        eprintln!("{}: {error}", "error".red().bold());
        return EXIT_USER_ERROR;
    }

    if args.write || args.out_dir.is_some() {
        if let Err(error) = sync_materialize_bridge_proof(&project_root, &files) {
            eprintln!("{}: {error}", "error".red().bold());
            return EXIT_USER_ERROR;
        }
    }

    // Phase E (`#1339`): emit a per-file SourceTransformReceipt summary so
    // refusals are first-class output rather than a string-Err abort.
    // In dry-run mode (no --write, no --out-dir), receipts are printed
    // as JSON alongside the realized source. In write modes, an aggregate
    // line per file goes to stdout (or stderr when --quiet).
    let mut total_exact = 0usize;
    let mut total_lossy = 0usize;
    let mut total_refused = 0usize;
    for file in &files {
        total_exact += file.receipt.aggregate_summary.exact;
        total_lossy += file.receipt.aggregate_summary.lossy;
        total_refused += file.receipt.aggregate_summary.refused;
    }
    let dry_run = !args.write && args.out_dir.is_none();
    if dry_run {
        // JSON receipt on stdout so consumers can parse it. Mirrors the
        // shape `cmd_bind_migrate` writes via `MigrateReceiptEnvelope`
        // for the receipt path; here the receipt is per-file because
        // materialize walks a directory and emits per-file output.
        for file in &files {
            let receipt_json = serde_json::to_string_pretty(&file.receipt)
                .unwrap_or_else(|err| format!("{{\"error\": \"{err}\"}}"));
            println!("// receipt: {}", file.relative_path.display());
            println!("{receipt_json}");
        }
    }

    if !args.out.quiet {
        if args.write || args.out_dir.is_some() {
            println!(
                "{} materialized {total_exact} exact + {total_lossy} lossy + {total_refused} refused across {} file(s)",
                "materialize".green().bold(),
                files.len()
            );
        } else {
            eprintln!(
                "{} dry-run: {total_exact} exact + {total_lossy} lossy + {total_refused} refused across {} file(s)",
                "materialize".green().bold(),
                files.len()
            );
        }
    }
    EXIT_OK
}

fn run_materialize_check(
    project_root: &Path,
    target_lang: &str,
    library_tag: Option<&str>,
    out_dir: &Path,
    compile_classpath: &[String],
) -> u8 {
    eprintln!(
        "{} compile-check: asking {} kit over RPC for {}",
        "materialize".cyan().bold(),
        target_lang,
        out_dir.display()
    );
    match dispatch_materialize_check(
        project_root,
        target_lang,
        library_tag,
        out_dir,
        compile_classpath,
    ) {
        Ok(report) => {
            let ok = report.get("ok").and_then(Json::as_bool).unwrap_or(false);
            let command = report
                .get("command")
                .and_then(Json::as_str)
                .unwrap_or("kit materialize check");
            if ok {
                eprintln!(
                    "{} compile-check: {command} passed",
                    "materialize".green().bold()
                );
                EXIT_OK
            } else {
                eprintln!("{}: compile-check: {command} failed", "error".red().bold());
                if let Some(stderr) = report.get("stderr").and_then(Json::as_str) {
                    if !stderr.is_empty() {
                        eprintln!("{stderr}");
                    }
                }
                if let Some(stdout) = report.get("stdout").and_then(Json::as_str) {
                    if !stdout.is_empty() {
                        eprintln!("{stdout}");
                    }
                }
                EXIT_VERIFY_FAIL
            }
        }
        Err(error) => {
            eprintln!("{}: compile-check: {error}", "error".red().bold());
            EXIT_VERIFY_FAIL
        }
    }
}

fn resolve_library_surface(
    project_root: &Path,
    target: Option<&str>,
    library: &str,
) -> Result<(String, Option<String>), String> {
    let library = library.trim();
    if library.is_empty() {
        return Err("--library must not be empty".to_string());
    }
    if let Some(target) = target {
        // The `{target}-` prefix on the library surface is OPTIONAL sugar
        // (e.g. `java-jackson` is `(java, jackson)`). But some real library
        // tags literally begin with `{target}-` — `java-io` is the JDK
        // `java.io` package, tag `java-io`, NOT `(java, io)`. Stripping
        // unconditionally truncated it to `io` and broke dispatch. Resolve
        // manifest-aware: prefer whichever of {full, stripped} actually has a
        // realize manifest for this target; only fall back to the bare strip
        // when neither does (back-compat for unregistered tags).
        let stripped = library.strip_prefix(&format!("{target}-"));
        if realize_tag_exists(project_root, target, library) {
            return Ok((target.to_string(), Some(library.to_string())));
        }
        let tag = stripped.unwrap_or(library);
        if tag.is_empty() {
            return Err(format!(
                "library surface `{library}` has empty library tag after stripping `{target}-` prefix"
            ));
        }
        return Ok((target.to_string(), Some(tag.to_string())));
    }
    if let Some((language, tag)) = resolve_registered_library_surface(project_root, library)? {
        return Ok((language, Some(tag)));
    }
    Err(format!(
        "could not infer target language for library `{library}` from registered realize manifests; pass --target=<language> or register .provekit/realize/<surface>/manifest.toml"
    ))
}

fn resolve_registered_library_surface(
    project_root: &Path,
    library: &str,
) -> Result<Option<(String, String)>, String> {
    use crate::kit_dispatch::registry_realize_language_candidates;

    let candidates = registry_realize_language_candidates(project_root)?;
    let mut matches = candidates
        .into_iter()
        .filter_map(|candidate| {
            let prefixed = format!("{}-{}", candidate.language, candidate.tag);
            if candidate.tag == library || prefixed == library {
                Some((candidate.language, candidate.tag, candidate.source))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    matches.sort();
    matches.dedup();

    match matches.as_slice() {
        [] => Ok(None),
        [(language, tag, _source)] => Ok(Some((language.clone(), tag.clone()))),
        _ => {
            let registered = matches
                .iter()
                .map(|(language, tag, source)| format!("{language}/{tag} from {source}"))
                .collect::<Vec<_>>()
                .join(", ");
            Err(format!(
                "ambiguous target language for library `{library}`; pass --target=<language>. registered matches: {registered}"
            ))
        }
    }
}

/// True when a realize manifest registered for `target_lang` declares exactly
/// `library_tag` as its `library_tag`. Used by `resolve_library_surface` to
/// avoid truncating tags that literally begin with `{target_lang}-` (e.g. the
/// JDK `java-io` tag) when stripping the optional language prefix.
fn realize_tag_exists(project_root: &Path, target_lang: &str, library_tag: &str) -> bool {
    use crate::kit_dispatch::registry_realize_candidates;
    registry_realize_candidates(project_root, target_lang)
        .map(|cands| cands.iter().any(|c| c.tag == library_tag))
        .unwrap_or(false)
}

fn materialize_source_dir(
    project_root: &Path,
    source_dir: &Path,
    target_lang: &str,
    library_tag: Option<&str>,
) -> Result<Vec<MaterializedFile>, String> {
    let mut files = Vec::new();
    for entry in WalkDir::new(source_dir)
        .into_iter()
        .filter_entry(|entry| should_scan_entry(entry.path()))
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if !path.is_file() || !is_supported_source_file(path) {
            continue;
        }
        let raw = std::fs::read_to_string(path)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        let (content, receipt, fragments) =
            materialize_source_text(project_root, target_lang, library_tag, &raw)
                .map_err(|error| format!("{}: {error}", path.display()))?;
        let replacements = receipt.aggregate_summary.exact + receipt.aggregate_summary.lossy;
        if replacements == 0 && receipt.aggregate_summary.refused == 0 {
            continue;
        }
        let relative_path = path.strip_prefix(source_dir).unwrap_or(path).to_path_buf();
        let package_hint = extract_package_hint(&raw);
        files.push(MaterializedFile {
            source_path: path.to_path_buf(),
            relative_path,
            content,
            receipt,
            fragments,
            package_hint,
            compile_classpath: Vec::new(),
        });
    }
    Ok(files)
}

fn materialize_source_dir_via_kit(
    project_root: &Path,
    source_dir: &Path,
    target_lang: &str,
    library_tag: Option<&str>,
) -> Result<Option<Vec<MaterializedFile>>, String> {
    let result =
        match dispatch_materialize_source(project_root, target_lang, library_tag, source_dir) {
            Ok(result) => result,
            Err(MaterializeSourceError::MethodNotSupported) => return Ok(None),
            Err(MaterializeSourceError::Failed(error)) => return Err(error),
        };

    let mut files = Vec::with_capacity(result.files.len());
    for file in result.files {
        if !is_safe_relative_path(&file.path) {
            return Err(format!(
                "materialize source kit returned unsafe output path `{}`",
                file.path.display()
            ));
        }
        let receipt: SourceTransformReceipt = serde_json::from_value(file.receipt)
            .map_err(|error| format!("decode materialize source receipt: {error}"))?;
        files.push(MaterializedFile {
            source_path: source_dir.join(&file.path),
            relative_path: file.path,
            content: file.content,
            receipt,
            fragments: Vec::new(),
            package_hint: None,
            compile_classpath: result.compile_classpath.clone(),
        });
    }
    if !files.is_empty() {
        eprintln!(
            "  {} source materialized by {} kit via RPC",
            "EMIT".green().bold(),
            target_lang,
        );
    }
    Ok(Some(files))
}

fn is_safe_relative_path(path: &Path) -> bool {
    !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

fn is_supported_source_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("ts" | "tsx" | "js" | "jsx" | "py" | "java")
    )
}

fn should_scan_entry(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return true;
    };
    !matches!(
        name,
        ".git"
            | ".mypy_cache"
            | ".next"
            | ".pytest_cache"
            | ".ruff_cache"
            | ".turbo"
            | ".venv"
            | ".vite"
            | "__pycache__"
            | "build"
            | "dist"
            | "node_modules"
            | "target"
            | "venv"
    )
}

fn materialize_source_text(
    project_root: &Path,
    target_lang: &str,
    library_tag: Option<&str>,
    raw: &str,
) -> Result<(String, SourceTransformReceipt, Vec<Json>), String> {
    let kit = MaterializeKit::new(target_lang, library_tag, project_root);
    // Phase E (`#1339`): use the refusal-collecting variant so a
    // `SiteOutcome::Refuse` becomes a first-class entry in the receipt
    // rather than aborting the run with a string Err.
    let (rewritten, sites_and_outcomes) = transform_source_text_collecting_refusals(raw, &kit)?;
    let receipt = build_receipt(
        &kit,
        target_lang,
        None,
        library_tag.unwrap_or(""),
        &sites_and_outcomes,
    );
    let fragments = kit.take_fragments();

    Ok((rewritten, receipt, fragments))
}

/// Extract the source-declared package/namespace so the assemble RPC can
/// reproduce it (`package_hint`). Java/Kotlin/Scala: `package x.y;` or
/// `package x.y`. Returns None when the source declares none. Substrate-honest
/// minimum — the kit owns the real package policy; this only forwards what the
/// consumer already wrote so the materialized unit lands in the same package.
fn extract_package_hint(raw: &str) -> Option<String> {
    for line in raw.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("package ") {
            let pkg = rest.trim().trim_end_matches(';').trim();
            if !pkg.is_empty() {
                return Some(pkg.to_string());
            }
        }
    }
    None
}

/// `SiteTransformKit` implementation for `provekit materialize`. The
/// materialize CLI is the N=1 specialization of the unified site-
/// transformation primitive: for each `provekit-concept:` carrier, build
/// a realize-request spec, dispatch through the LowerKit `execute_path`
/// composition (same flow Phase A's `realize_spec_via_path` used), and
/// map the realize transport's response onto the trichotomy outcome
/// (`Materialize`, `LoudlyLossy`, `Refuse`) per the substrate-honest
/// first-principle (#1334).
pub struct MaterializeKit<'root> {
    target_lang: String,
    library_tag: Option<String>,
    project_root: &'root Path,
    /// Per-site realization fragments, collected during `transform_site`, so
    /// the `--out-dir` path can hand them to the LANGUAGE KIT's assemble RPC
    /// (which owns imports/helper-hoisting/package/class-wrapping/dependency
    /// resolution; the substrate holds no language syntax). Each entry mirrors
    /// the cross-language fragment shape and is transported opaquely.
    /// `Mutex` (not `RefCell`) because `SiteTransformKit: Send + Sync`.
    fragments: std::sync::Mutex<Vec<Json>>,
}

impl<'root> MaterializeKit<'root> {
    pub fn new(target_lang: &str, library_tag: Option<&str>, project_root: &'root Path) -> Self {
        Self {
            target_lang: target_lang.to_string(),
            library_tag: library_tag.map(str::to_string),
            project_root,
            fragments: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Drain the fragments collected across this kit's `transform_site` calls.
    pub fn take_fragments(&self) -> Vec<Json> {
        std::mem::take(&mut self.fragments.lock().expect("fragments mutex"))
    }

    /// Execute the lower-kit composition path that turns a realize-request
    /// spec into a `RealizedSource`. This is the kit method that owns the
    /// `execute_path`+`KitRegistry`+`DispatchRealizeTransport` wiring Phase A's
    /// `realize_spec_via_path` free function did inline.
    fn realize_via_path(&self, spec: Json) -> Result<RealizedSource, String> {
        let mut inputs = HashMapInputCatalog::default();
        let input_cid = inputs.insert(Input::Spec(spec));
        let kit_name = self
            .library_tag
            .as_deref()
            .map(|tag| format!("lower-{}-{tag}", self.target_lang))
            .unwrap_or_else(|| format!("lower-{}", self.target_lang));
        let path = Input::Path(Box::new(CorePath {
            algebra: vec![PathAlgebra {
                name: "lower".to_string(),
                kit: kit_name.clone(),
                inputs: vec![input_cid],
                depends_on: vec![],
                verb: Verb::Transform,
            }],
        }));
        let mut registry = KitRegistry::default();
        registry.register(
            kit_name,
            LowerKit::new(
                self.project_root.to_path_buf(),
                self.target_lang.clone(),
                self.library_tag.clone(),
                DispatchRealizeTransport,
            ),
            ConformanceDeclaration::Carrier {
                fixtures_path: self.project_root.join(format!(
                    "implementations/{}/conformance/fixtures",
                    self.target_lang
                )),
            },
        );
        let chain = execute_path(&path, &registry, &inputs).map_err(|error| {
            error
                .composition_refusal()
                .and_then(|refusal| serde_json::to_string(refusal).ok())
                .unwrap_or_else(|| error.to_string())
        })?;
        LowerKit::<DispatchRealizeTransport>::realized_source_from_claim(chain.terminal_claim())
    }
}

impl SiteTransformKit for MaterializeKit<'_> {
    fn target_language(&self) -> &str {
        &self.target_lang
    }

    fn transform_site(&self, carrier: &CarrierComment) -> Result<SiteOutcome, String> {
        // #1364 chunk 2 / #1355: defensive concept-coverage check. When the
        // chosen realize manifest declares its `provides_concepts` and the
        // carrier's concept_name is NOT in that list, refuse-loudly BEFORE
        // dispatching. Surfaces per-kit coverage gaps at the materialize
        // boundary rather than at the realize plugin's is_stub fallback,
        // with an informative reason listing what the kit DOES provide.
        // Empty provides_concepts (no manifest declaration) ↔ today's
        // behavior (no enforcement; rely on is_stub).
        let carrier_library = carrier
            .library_tag
            .as_deref()
            .or(self.library_tag.as_deref())
            .unwrap_or("");
        if !carrier_library.is_empty() {
            let provides = provides_concepts_for_realize(
                self.project_root,
                &self.target_lang,
                carrier_library,
            );
            if !provides.is_empty() && !provides.iter().any(|c| c == &carrier.concept_name) {
                return Ok(SiteOutcome::Refuse {
                    reason: format!(
                        "library `{carrier_library}` (target {}) does not declare concept \
                         `{}` in its provides_concepts list. Declared concepts: {:?}. Either \
                         add the concept to the realize manifest's provides_concepts or \
                         point the @boundary at a different library.",
                        self.target_lang, carrier.concept_name, provides
                    ),
                    would_close_with_concept: carrier.concept_name.clone(),
                });
            }
        }
        // Reuse Phase A's permissive-defaults spec builder (which lives in
        // libprovekit) by re-serializing the carrier's raw payload. The
        // typed CarrierComment fields are equivalent; routing through
        // `realize_spec_from_payload` keeps a single permissive-defaults
        // surface across both code paths and preserves byte-identical
        // realize-request shape against Phase A.
        let spec = realize_spec_from_payload(&carrier.raw_payload)?;
        let realized = self.realize_via_path(spec)?;
        // String-formatted refusal sentence rather than a structured
        // gap-record memento: materialize is a build-pipeline workflow
        // that does not emit a receipt memento (unlike cmd_bind_migrate
        // which produces MigrateReceiptEnvelope with refusal_mementos per
        // `2026-05-18-refuse-leg-short-circuit-ruling`). Phase B's
        // `transform_source_text` propagates `SiteOutcome::Refuse` as
        // `Err(reason)` to the CLI, which suffices for the consumer surface.
        if realized.is_stub {
            return Ok(SiteOutcome::Refuse {
                reason: format!(
                    "realize plugin for `{}` library `{}` returned a stub",
                    self.target_lang,
                    self.library_tag.as_deref().unwrap_or("default")
                ),
                would_close_with_concept: carrier.concept_name.clone(),
            });
        }
        let binding_cid = realized.emitted_artifact_cid.clone().unwrap_or_default();
        // Collect the per-site fragment for the kit's assemble RPC. The kit's
        // assembler peels the wrapper class, dedupes imports, hoists helpers,
        // resolves package dependencies, and emits the package + compilation
        // unit. The substrate does not assemble or interpret language/package
        // semantics; it forwards what the realize plugin produced.
        self.fragments
            .lock()
            .expect("fragments mutex")
            .push(serde_json::json!({
                "concept_name": carrier.concept_name,
                "source": realized.source.clone(),
                "imports": realized.imports.clone(),
                "helpers": realized.helpers.clone(),
                "dependencies": realized.dependencies.clone(),
                "diagnostics": realized.diagnostics.clone(),
                "compile_unit_requirements": realized.compile_unit_requirements.clone(),
            }));
        if has_loss(&realized.observed_loss_record) {
            Ok(SiteOutcome::LoudlyLossy {
                body: realized.source,
                binding_cid,
                contract_cid: realized.contract_cid,
                declared_loss: extract_loss_dims(&realized.observed_loss_record),
            })
        } else {
            Ok(SiteOutcome::Materialize {
                body: realized.source,
                binding_cid,
                contract_cid: realized.contract_cid,
                loss_record: realized.observed_loss_record,
            })
        }
    }
}

/// A loss record is "loss-bearing" when it is neither JSON null nor an
/// empty object. `lower_plugin::loss_record_cid` uses the same predicate
/// shape for deciding whether to mint a loss-record CID; matching it here
/// keeps the `LoudlyLossy` vs `Materialize` split aligned with the kit
/// binding's own honesty gradient.
fn has_loss(record: &Json) -> bool {
    match record {
        Json::Null => false,
        Json::Object(map) => !map.is_empty(),
        _ => true,
    }
}

/// Project an `observed_loss_record` JSON value onto the list of declared
/// loss dimensions. Object keys are the named dimensions (matching the
/// shape `merge_observed_loss_records` builds in `lower_plugin`); non-object
/// records degrade to a single-element vec carrying the value's JSON
/// rendering, so consumers always get a non-empty `declared_loss` for
/// loss-bearing records.
fn extract_loss_dims(record: &Json) -> Vec<String> {
    match record {
        Json::Object(map) => map.keys().cloned().collect(),
        Json::Null => Vec::new(),
        other => vec![other.to_string()],
    }
}

fn print_dry_run(files: &[MaterializedFile]) -> Result<(), String> {
    let mut stdout = std::io::stdout().lock();
    for file in files {
        writeln!(stdout, "// file: {}", file.relative_path.display())
            .map_err(|error| format!("write stdout: {error}"))?;
        stdout
            .write_all(file.content.as_bytes())
            .map_err(|error| format!("write stdout: {error}"))?;
        if !file.content.ends_with('\n') {
            writeln!(stdout).map_err(|error| format!("write stdout: {error}"))?;
        }
    }
    Ok(())
}

fn write_in_place(files: &[MaterializedFile]) -> Result<(), String> {
    for file in files {
        std::fs::write(&file.source_path, &file.content)
            .map_err(|error| format!("write {}: {error}", file.source_path.display()))?;
    }
    Ok(())
}

/// Outcome of attempting kit-owned assembly for `--out-dir`.
enum EmitOutcome {
    /// The kit assembled (one or more files); inner is the compile-check exit
    /// code (EXIT_OK when --compile-check is off or the kit check passed).
    Assembled(u8),
}

/// Route each materialized file's fragments through the LANGUAGE KIT's
/// assemble RPC and write back what the kit returns, then (optionally)
/// ask the same kit to compile-check with its declared metadata. This is the
/// ONLY assembly/check path — the substrate never bakes language syntax or
/// compiler semantics.
///
fn emit_out_dir_via_kit_assemble(
    project_root: &Path,
    target_lang: &str,
    library_tag: Option<&str>,
    out_dir: &Path,
    files: &[MaterializedFile],
    compile_check: bool,
) -> EmitOutcome {
    use crate::kit_dispatch::{dispatch_assemble, AssembleError};

    if let Err(err) = std::fs::create_dir_all(out_dir) {
        eprintln!(
            "{}: failed to create --out-dir {}: {err}",
            "error".red().bold(),
            out_dir.display()
        );
        return EmitOutcome::Assembled(EXIT_USER_ERROR);
    }

    let mut aggregated_classpath: std::collections::BTreeSet<String> =
        std::collections::BTreeSet::new();
    for file in files {
        for cp in &file.compile_classpath {
            aggregated_classpath.insert(cp.clone());
        }
        // Files with no realizable fragments (refuse-only) have nothing for
        // the kit to assemble; preserve the in-place-rewritten content so the
        // refusal markers / untouched source still land in the out-dir.
        if file.fragments.is_empty() {
            let target = out_dir.join(&file.relative_path);
            if let Some(parent) = target.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Err(err) = std::fs::write(&target, &file.content) {
                eprintln!(
                    "{}: failed to write {}: {err}",
                    "error".red().bold(),
                    target.display()
                );
                return EmitOutcome::Assembled(EXIT_USER_ERROR);
            }
            continue;
        }

        let file_basename = file
            .relative_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("lib");
        let fragments_json =
            serde_json::to_string(&file.fragments).unwrap_or_else(|_| "[]".to_string());

        match dispatch_assemble(
            project_root,
            target_lang,
            library_tag,
            &fragments_json,
            file_basename,
            file.package_hint.as_deref(),
        ) {
            Ok(assembled) => {
                for af in &assembled.files {
                    // Preserve the source file's relative directory so the
                    // assembled unit lands next to the consumer.
                    let rel_parent = file.relative_path.parent();
                    let out_path = match rel_parent {
                        Some(p) if !p.as_os_str().is_empty() => out_dir.join(p).join(&af.path),
                        _ => out_dir.join(&af.path),
                    };
                    if let Some(parent) = out_path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    if let Err(err) = std::fs::write(&out_path, &af.content) {
                        eprintln!(
                            "{}: failed to write {}: {err}",
                            "error".red().bold(),
                            out_path.display()
                        );
                        return EmitOutcome::Assembled(EXIT_USER_ERROR);
                    }
                    eprintln!(
                        "  {} wrote {} (assembled by {} kit via RPC)",
                        "EMIT".green().bold(),
                        out_path.display(),
                        target_lang,
                    );
                }
                for cp in &assembled.compile_classpath {
                    aggregated_classpath.insert(cp.clone());
                }
            }
            Err(AssembleError::MethodNotSupported) => {
                eprintln!(
                    "{}: selected {target_lang} kit must implement assemble RPC for --out-dir materialize; refusing CLI-side target assembly for {}",
                    "error".red().bold(),
                    file.relative_path.display()
                );
                return EmitOutcome::Assembled(EXIT_VERIFY_FAIL);
            }
            Err(AssembleError::Failed(err)) => {
                eprintln!(
                    "{}: assemble RPC failed for {}: {err}",
                    "error".red().bold(),
                    file.relative_path.display()
                );
                return EmitOutcome::Assembled(EXIT_VERIFY_FAIL);
            }
        }
    }

    if compile_check {
        let classpath: Vec<String> = aggregated_classpath.iter().cloned().collect();
        let code =
            run_materialize_check(project_root, target_lang, library_tag, out_dir, &classpath);
        return EmitOutcome::Assembled(code);
    }
    EmitOutcome::Assembled(EXIT_OK)
}

fn sync_materialize_bridge_proof(
    project_root: &Path,
    files: &[MaterializedFile],
) -> Result<Option<PathBuf>, String> {
    let proof_dir = project_root.join(".provekit").join("materialize");
    let proof_pool = provekit_verifier::load_all_proofs::run(project_root);

    // NO global cleanup. The prior behavior wiped EVERY existing `.proof` in
    // this dir before rebuilding from just THIS invocation's files — which
    // silently deleted bridges emitted by earlier (or wider-scope) materialize
    // runs, so `prove` stopped enforcing already-adopted vendor contracts
    // after any partial scan (Codex P1 on #1566). Now: leave existing
    // envelopes in place and just write THIS invocation's envelope. The
    // verifier walks `.provekit/` and unions all bridge members across
    // envelopes; the filename stays content-addressed (`<cid>.proof`) so
    // idempotent re-runs overwrite themselves in place. Stale orphan
    // envelopes (whose members no longer correspond to any live source) are
    // a hygiene concern for a future garbage-collect verb, not data loss.
    let mut members = BTreeMap::new();
    for file in files {
        for site in &file.receipt.site_witnesses {
            let Some(contract_cid) = site.contract_cid.as_deref() else {
                continue;
            };
            let target_layer = if file.receipt.target_library.is_empty() {
                file.receipt.target_language.as_str()
            } else {
                file.receipt.target_library.as_str()
            };
            let body = build_bridge_body(
                "materialize",
                &site.function_name,
                &file.receipt.source_language,
                target_layer,
                contract_cid,
            );
            let body = with_target_proof_cid(body, &proof_pool, contract_cid)?;
            let (cid, bytes) = member_envelope_canonical("bridge", &body)?;
            members.entry(cid).or_insert(bytes);
        }
    }

    if members.is_empty() {
        return Ok(None);
    }

    std::fs::create_dir_all(&proof_dir)
        .map_err(|error| format!("create {}: {error}", proof_dir.display()))?;
    let signer = ed25519_pubkey_string(&MATERIALIZE_BRIDGE_SIGNER_SEED);
    let proof = build_proof_envelope(&ProofEnvelopeInput {
        name: "@provekit/materialize-bridges".to_string(),
        version: "0.1.0".to_string(),
        binary_cid: None,
        metadata: None,
        members,
        signer_cid: signer,
        signer_seed: MATERIALIZE_BRIDGE_SIGNER_SEED,
        declared_at: MATERIALIZE_BRIDGE_DECLARED_AT.to_string(),
    });
    let path = proof_dir.join(format!("{}.proof", proof.cid));
    std::fs::write(&path, &proof.bytes)
        .map_err(|error| format!("write {}: {error}", path.display()))?;
    Ok(Some(path))
}

fn with_target_proof_cid(
    mut body: Json,
    pool: &provekit_verifier::types::MementoPool,
    contract_cid: &str,
) -> Result<Json, String> {
    let Some(target) = pool.mementos.get(contract_cid) else {
        return Err(format!(
            "materialize bridge target contract `{contract_cid}` is not loaded from any project .proof"
        ));
    };
    if provekit_verifier::types::memento_kind(target) != Some("contract") {
        return Err(format!(
            "materialize bridge target `{contract_cid}` is not a contract memento"
        ));
    }
    let Some((bundle_cid, _)) = pool
        .bundle_members
        .iter()
        .find(|(_, members)| members.contains(contract_cid))
    else {
        return Err(format!(
            "materialize bridge target contract `{contract_cid}` has no containing proof bundle"
        ));
    };
    if let Json::Object(map) = &mut body {
        map.insert(
            "targetProofCid".to_string(),
            Json::String(bundle_cid.clone()),
        );
    }
    Ok(body)
}

// materialize_bridge_body / flat_member / canonical_json / canonical_value
// were moved to libprovekit::core::emit_obligation as build_bridge_body
// + member_envelope_canonical (#1579). cmd_materialize now imports them
// and shares one canonical authoring path with cmd_recognize.
