// SPDX-License-Identifier: Apache-2.0
//
// `provekit materialize` turns concept-citation carriers in source files into
// library-bound source by composing the existing LowerKit/realize path.

use std::io::Write;
use std::path::{Path, PathBuf};

use clap::Parser;
use libprovekit::core::{
    execute_path, HashMapInputCatalog, Input, KitRegistry, LowerKit, Path as CorePath, PathAlgebra,
    RealizedSource, Verb,
};
// Source-transform primitives live in libprovekit (#1335 Phase A). The glob
// re-export keeps the carrier-parsing surface available to `materialize_source_text`
// after the extract-and-move; Phase C reroutes the per-site loop through
// `transform_source_text` + the `SiteTransformKit` trait (#1337).
pub(crate) use libprovekit::core::source_transform::*;
use owo_colors::OwoColorize;
use serde_json::Value as Json;
use walkdir::WalkDir;

use crate::kit_dispatch::{scope_bringings_for_realize, DispatchRealizeTransport};
use crate::{OutputFlags, EXIT_OK, EXIT_USER_ERROR, EXIT_VERIFY_FAIL};

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
    /// #1361 / #1355: SOURCE language the @boundary stubs live in. When
    /// different from --target, the source kit's lifter produces ProofIR
    /// that's then consumed by the target kit's realizer, enabling cross-
    /// language materialization (e.g. Rust source → Python target). When
    /// equal to --target (today's default behavior; omit the flag to get
    /// it), the existing same-language path is used. Cross-language synthesis
    /// requires per-kit ProofIR exchange wired through (see #1361 chunk 2 +
    /// #1364 per-kit concept parity); this chunk plumbs the flag and
    /// refuses cross-language requests with a clear "not yet wired" message.
    #[arg(long = "source-lang")]
    pub source_lang: Option<String>,
    /// Write files in place. Omitted means dry-run to stdout.
    #[arg(long)]
    pub write: bool,
    /// Write materialized files under this directory, preserving paths relative to --source-dir.
    #[arg(long = "out-dir", conflicts_with = "write")]
    pub out_dir: Option<PathBuf>,
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

    // #1361 chunk 1 / #1355: when --source-lang differs from --target,
    // cross-language synthesis is required. Refuse loudly until #1361 chunk
    // 2 wires the source-kit lifter → ProofIR → target-kit realizer exchange.
    // Same-language case (or omitted --source-lang) keeps today's behavior.
    if let Some(source_lang) = args.source_lang.as_deref() {
        if source_lang != target_lang {
            eprintln!(
                "{}: cross-language materialize (--source-lang {source_lang} != --target {target_lang}) \
                 not yet wired. Tracked as #1361 chunk 2 + #1364 per-kit concept parity. \
                 Currently this CLI supports source_lang == target_lang only.",
                "refuse".red().bold()
            );
            return EXIT_USER_ERROR;
        }
    }

    let files = match materialize_source_dir(
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
        if let Err(error) = write_out_dir(out_dir, &files) {
            eprintln!("{}: {error}", "error".red().bold());
            return EXIT_USER_ERROR;
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
        let tag = library
            .strip_prefix(&format!("{target}-"))
            .unwrap_or(library);
        if tag.is_empty() {
            return Err(format!(
                "library surface `{library}` has empty library tag after stripping `{target}-` prefix"
            ));
        }
        return Ok((target.to_string(), Some(tag.to_string())));
    }
    for language in ["typescript", "python", "rust", "java"] {
        if let Some(tag) = library.strip_prefix(&format!("{language}-")) {
            if tag.is_empty() {
                return Err(format!("library surface `{library}` has empty library tag"));
            }
            return Ok((language.to_string(), Some(tag.to_string())));
        }
    }
    let language = detect_project_language(project_root).ok_or_else(|| {
        format!(
            "could not infer target language for library `{library}`; pass --target=<language> or use a language-prefixed library surface"
        )
    })?;
    Ok((language, Some(library.to_string())))
}

fn detect_project_language(project_root: &Path) -> Option<String> {
    let markers = [
        ("package.json", "typescript"),
        ("Cargo.toml", "rust"),
        ("pyproject.toml", "python"),
        ("requirements.txt", "python"),
        ("pom.xml", "java"),
        ("build.gradle", "java"),
        ("build.gradle.kts", "java"),
    ];
    markers
        .iter()
        .find(|(marker, _)| project_root.join(marker).exists())
        .map(|(_, language)| (*language).to_string())
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
        let (content, receipt) =
            materialize_source_text(project_root, target_lang, library_tag, &raw)
                .map_err(|error| format!("{}: {error}", path.display()))?;
        let replacements =
            receipt.aggregate_summary.exact + receipt.aggregate_summary.lossy;
        if replacements == 0 && receipt.aggregate_summary.refused == 0 {
            continue;
        }
        let relative_path = path.strip_prefix(source_dir).unwrap_or(path).to_path_buf();
        files.push(MaterializedFile {
            source_path: path.to_path_buf(),
            relative_path,
            content,
            receipt,
        });
    }
    Ok(files)
}

fn is_supported_source_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("ts" | "tsx" | "js" | "jsx" | "py" | "rs" | "java")
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

/// #1360 / #1355: Scan a Rust source for `#[provekit::boundary(... library = X ...)]`
/// attributes and return the unique set of `library` values declared. cmd_materialize
/// uses these to look up per-target scope_bringings from the realize manifests.
fn collect_boundary_libraries(raw: &str) -> Vec<String> {
    let mut libs: Vec<String> = Vec::new();
    let mut idx = 0;
    while let Some(start) = raw[idx..].find("#[provekit::boundary") {
        let abs = idx + start;
        // Find the closing `)]` (multi-line attribute supported).
        let Some(close_rel) = raw[abs..].find(")]") else {
            break;
        };
        let attr_text = &raw[abs..abs + close_rel + 2];
        idx = abs + close_rel + 2;
        // Extract `library = "..."` from the attribute body.
        if let Some(lib_pos) = attr_text.find("library = \"") {
            let after = &attr_text[lib_pos + "library = \"".len()..];
            if let Some(end) = after.find('"') {
                let lib = after[..end].to_string();
                if !libs.iter().any(|existing| existing == &lib) {
                    libs.push(lib);
                }
            }
        }
    }
    libs
}

/// #1360 / #1355: Splice a list of `use ...;` items into a Rust source.
/// Inserts after the last existing top-level `use` statement (or at the top
/// of the file if none). Items that are already present in the source are
/// skipped (deduplication against existing prelude).
fn splice_use_items(source: &str, items: &[String]) -> String {
    if items.is_empty() {
        return source.to_string();
    }
    // Find the last `use ...;` line in the file's prelude (consecutive
    // use statements at the top, possibly interspersed with comments /
    // doc strings / blank lines / pub use).
    let lines: Vec<&str> = source.lines().collect();
    let mut last_use_idx: Option<usize> = None;
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("use ") || trimmed.starts_with("pub use ") {
            last_use_idx = Some(i);
        }
    }

    // Deduplicate against existing uses (by exact line match after trim).
    let existing_uses: std::collections::HashSet<String> = lines
        .iter()
        .filter_map(|l| {
            let t = l.trim();
            if t.starts_with("use ") || t.starts_with("pub use ") {
                Some(t.to_string())
            } else {
                None
            }
        })
        .collect();
    let new_items: Vec<&String> = items
        .iter()
        .filter(|item| !existing_uses.contains(item.trim()))
        .collect();
    if new_items.is_empty() {
        return source.to_string();
    }

    let insertion_idx = last_use_idx.map(|i| i + 1).unwrap_or(0);
    let mut out = Vec::with_capacity(lines.len() + new_items.len());
    for (i, line) in lines.iter().enumerate() {
        if i == insertion_idx {
            for item in &new_items {
                out.push((*item).clone());
            }
        }
        out.push(line.to_string());
    }
    if insertion_idx == lines.len() {
        for item in &new_items {
            out.push((*item).clone());
        }
    }
    let mut joined = out.join("\n");
    // Preserve trailing newline if the input had one.
    if source.ends_with('\n') && !joined.ends_with('\n') {
        joined.push('\n');
    }
    joined
}

fn materialize_source_text(
    project_root: &Path,
    target_lang: &str,
    library_tag: Option<&str>,
    raw: &str,
) -> Result<(String, SourceTransformReceipt), String> {
    let kit = MaterializeKit::new(target_lang, library_tag, project_root);
    // Phase E (`#1339`): use the refusal-collecting variant so a
    // `SiteOutcome::Refuse` becomes a first-class entry in the receipt
    // rather than aborting the run with a string Err.
    let (rewritten, sites_and_outcomes) =
        transform_source_text_collecting_refusals(raw, &kit)?;
    let receipt = build_receipt(
        &kit,
        target_lang,
        None,
        library_tag.unwrap_or(""),
        &sites_and_outcomes,
    );

    // #1360 / #1355: collect per-target scope-bringings from each
    // @boundary site's named library and splice them into the rewritten
    // file's prelude. Deduplicated against existing `use` items.
    let libraries = collect_boundary_libraries(raw);
    let mut bringings: Vec<String> = Vec::new();
    for lib in &libraries {
        for bringing in scope_bringings_for_realize(project_root, target_lang, lib) {
            if !bringings.iter().any(|existing| existing == &bringing) {
                bringings.push(bringing);
            }
        }
    }
    let final_source = splice_use_items(&rewritten, &bringings);

    Ok((final_source, receipt))
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
}

impl<'root> MaterializeKit<'root> {
    pub fn new(
        target_lang: &str,
        library_tag: Option<&str>,
        project_root: &'root Path,
    ) -> Self {
        Self {
            target_lang: target_lang.to_string(),
            library_tag: library_tag.map(str::to_string),
            project_root,
        }
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
        registry.register_with_platform_semantics(
            kit_name,
            LowerKit::new(
                self.project_root.to_path_buf(),
                self.target_lang.clone(),
                self.library_tag.clone(),
                DispatchRealizeTransport,
            ),
            &self.target_lang,
            self.project_root.join(format!(
                "implementations/{}/conformance/fixtures",
                self.target_lang
            )),
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
        // Reuse Phase A's permissive-defaults spec builder (which lives in
        // libprovekit) by re-serializing the carrier's raw payload. The
        // typed CarrierComment fields are equivalent; routing through
        // `realize_spec_from_payload` keeps a single permissive-defaults
        // surface across both code paths and preserves byte-identical
        // realize-request shape against Phase A.
        let mut spec = realize_spec_from_payload(&carrier.raw_payload)?;
        augment_spec_with_shim_term_shape(&mut spec, self.project_root);
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
        if has_loss(&realized.observed_loss_record) {
            Ok(SiteOutcome::LoudlyLossy {
                body: realized.source,
                binding_cid,
                declared_loss: extract_loss_dims(&realized.observed_loss_record),
            })
        } else {
            Ok(SiteOutcome::Materialize {
                body: realized.source,
                binding_cid,
                loss_record: realized.observed_loss_record,
            })
        }
    }
}

/// If the carrier's `library` names a shim crate (i.e. there's a directory
/// `<project_root>/examples/<library>/` containing one or more `.proof`
/// envelopes), open the .proof, find the `library-sugar-binding-entry`
/// whose `concept_name` matches the spec's `concept_name`, and merge its
/// `term_shape` into the spec under `termShape`. The realize plugin's
/// dispatch already routes `term_shape`-bearing requests through
/// `emit_from_term_shape_with_bindings`; this is the only wire missing
/// for cross-platform @boundary stubs to materialize against their
/// sister shim's @sugar realization.
fn augment_spec_with_shim_term_shape(spec: &mut Json, project_root: &Path) {
    let Some(obj) = spec.as_object_mut() else {
        return;
    };
    let Some(library) = obj
        .get("library")
        .or_else(|| obj.get("libraryTag"))
        .and_then(Json::as_str)
        .map(str::to_string)
    else {
        return;
    };
    let Some(concept_name) = obj
        .get("concept_name")
        .or_else(|| obj.get("conceptName"))
        .and_then(Json::as_str)
        .map(str::to_string)
    else {
        return;
    };
    let shim_dir = project_root.join("examples").join(&library);
    if !shim_dir.is_dir() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(&shim_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with(".proof"))
        {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let Ok(catalog) = provekit_proof_envelope::cbor_decode::decode(&bytes) else {
            continue;
        };
        let Some(root) = catalog.as_map() else {
            continue;
        };
        let Some(members) = root.get("members").and_then(|v| v.as_map()) else {
            continue;
        };
        for (_cid, member) in members {
            // Each member is a CBOR byte string whose contents are the
            // member's JCS-JSON encoding (`{header, body, schemaVersion}`).
            let Some(member_bytes) = member.as_bstr() else {
                continue;
            };
            let Ok(member_text) = std::str::from_utf8(member_bytes) else {
                continue;
            };
            let Ok(member_json) = serde_json::from_str::<Json>(member_text) else {
                continue;
            };
            let Some(body) = member_json.get("body") else {
                continue;
            };
            if body.get("kind").and_then(Json::as_str) != Some("library-sugar-binding-entry") {
                continue;
            }
            if body.get("concept_name").and_then(Json::as_str) != Some(concept_name.as_str()) {
                continue;
            }
            if body.get("target_language").and_then(Json::as_str) != Some("rust") {
                continue;
            }
            if let Some(term_shape) = body.get("term_shape") {
                obj.insert("termShape".to_string(), term_shape.clone());
            }
            if let Some(operand_bindings) = body.get("operand_bindings") {
                obj.insert("operandBindings".to_string(), operand_bindings.clone());
            }
            // #1357: when the consumer @boundary floated `family` or
            // `library_version` (absent in the spec), surface the shim's
            // declared values into the spec. Boundary-pinned values win
            // (don't overwrite); shim values fill in floating axes. This
            // is the "consumer floats; shim declares" resolution.
            if !obj.contains_key("family") {
                if let Some(family) = body.get("family").cloned() {
                    obj.insert("family".to_string(), family);
                }
            }
            if !obj.contains_key("library_version") {
                if let Some(version) = body.get("library_version").cloned() {
                    obj.insert("library_version".to_string(), version);
                }
            }
            if let Some(cid) = body.get("signature_shape_cid").cloned() {
                obj.insert("signatureShapeCid".to_string(), cid);
            }
            return;
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

fn write_out_dir(out_dir: &Path, files: &[MaterializedFile]) -> Result<(), String> {
    for file in files {
        let target = out_dir.join(&file.relative_path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("create {}: {error}", parent.display()))?;
        }
        std::fs::write(&target, &file.content)
            .map_err(|error| format!("write {}: {error}", target.display()))?;
    }
    Ok(())
}
