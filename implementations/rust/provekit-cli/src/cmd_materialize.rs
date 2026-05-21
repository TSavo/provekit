// SPDX-License-Identifier: Apache-2.0
//
// `provekit materialize` turns concept-citation carriers in source files into
// library-bound source by composing the existing LowerKit/realize path.

use std::collections::BTreeMap;
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

use crate::kit_dispatch::{
    provides_concepts_for_realize, scope_bringings_for_realize, DispatchRealizeTransport,
};
use crate::{OutputFlags, EXIT_OK, EXIT_USER_ERROR, EXIT_VERIFY_FAIL};

/// #1361 follow-up: `family=library` pair from --family-library clap arg.
#[derive(Debug, Clone)]
pub struct FamilyLibraryPair {
    pub family: String,
    pub library: String,
}

fn parse_family_library_pair(raw: &str) -> Result<FamilyLibraryPair, String> {
    let (family, library) = raw
        .split_once('=')
        .ok_or_else(|| format!("--family-library expects `family=library`, got: {raw}"))?;
    let family = family.trim();
    let library = library.trim();
    if family.is_empty() || library.is_empty() {
        return Err(format!(
            "--family-library expects non-empty family + library, got: {raw}"
        ));
    }
    Ok(FamilyLibraryPair {
        family: family.to_string(),
        library: library.to_string(),
    })
}

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
    /// it), the existing same-language path is used.
    #[arg(long = "source-lang")]
    pub source_lang: Option<String>,
    /// #1361 follow-up / #1355: per-family library override. Used to
    /// disambiguate AMBIGUOUS cross-language discovery sites when multiple
    /// target manifests declare the same `concept:family:X`. Syntax is
    /// `family=library` (e.g. `--family-library json=jackson` selects the
    /// jackson realization for any boundary with `family =
    /// concept:family:json`). Repeatable. The family suffix matches the
    /// trailing segment of the boundary's family pin (json matches
    /// concept:family:json). This is the substrate-honest compile-time
    /// choice mechanism: caller declares per-family realization at
    /// materialize, the proof envelope captures the selection forever.
    #[arg(long = "family-library", value_parser = parse_family_library_pair)]
    pub family_library: Vec<FamilyLibraryPair>,
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

    // #1361 chunk 2 part A / #1355: when --source-lang differs from --target,
    // run cross-language DISCOVERY mode — scan @boundary carriers in the
    // source-dir, query the target-language realize manifest catalog by
    // (family, concept_name), report which target manifests would resolve
    // and which would refuse. Does NOT emit target-language code yet (that's
    // part B — signature translation + per-target realize binary
    // invocation). The report is the substrate-honest "what would happen if
    // we materialized this rust source for python target" answer, surfaced
    // before the code-emission machinery exists.
    //
    // Same-language case (or omitted --source-lang) skips this branch and
    // continues to today's materialize path unchanged.
    if let Some(source_lang) = args.source_lang.as_deref() {
        if source_lang != target_lang {
            return run_cross_language_discovery(
                &project_root,
                &args.source_dir,
                source_lang,
                &target_lang,
                &args.family_library,
                args.out_dir.as_deref(),
            );
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

/// #1361 follow-up / #1355: accumulated target-language file content
/// for cross-language emission. One per source file → emits one composite
/// target-language file with imports + each RESOLVE'd boundary's body.
#[derive(Debug, Clone, Default)]
struct EmittedFile {
    bodies: Vec<String>,
    /// Scope-bringings deduplicated across all libraries used in this file.
    imports: std::collections::BTreeSet<String>,
}

/// Map a target language to a file extension. Substrate-honest fallback
/// "txt" for unknown languages (no per-lang knowledge encoded beyond the
/// declared extension; the kit could declare this in its manifest in a
/// follow-up substrate-mint).
fn target_lang_file_extension(target_lang: &str) -> &'static str {
    match target_lang {
        "rust" => "rs",
        "python" => "py",
        "typescript" => "ts",
        "java" => "java",
        "c" => "c",
        "csharp" => "cs",
        "go" => "go",
        "php" => "php",
        "ruby" => "rb",
        "zig" => "zig",
        _ => "txt",
    }
}

/// #1361 chunk 2 part B / #1355: invoke target kit's realize binary for a
/// RESOLVE'd boundary in cross-language discovery mode. The realize binary
/// owns the concept-hub → target-syntax translation internally; cmd_materialize
/// just routes the concept-hub-typed spec to it and gets back target source.
///
/// Returns None on any error (the discovery report continues without the
/// preview); the RESOLVE outcome itself is unchanged.
fn invoke_target_realize_for_discovery(
    project_root: &Path,
    target_lang: &str,
    target_library_tag: &str,
    carrier: &CarrierComment,
) -> Option<String> {
    use libprovekit::core::lower_plugin::request_from_spec;
    use libprovekit::core::RealizeTransport;
    // Reuse the same spec-construction path the same-language emission uses;
    // dispatch to the target's realize binary via the kit dispatcher.
    let spec = realize_spec_from_payload(&carrier.raw_payload).ok()?;
    let request = request_from_spec(&spec).ok()?;
    let transport = DispatchRealizeTransport;
    let response = transport
        .dispatch_realize(
            project_root,
            target_lang,
            Some(target_library_tag),
            &request,
        )
        .ok()?;
    if response.is_stub {
        return None;
    }
    Some(response.source)
}

/// #1361 chunk 2 part A / #1355: cross-language DISCOVERY mode.
///
/// When `--source-lang != --target`, scan the source directory for
/// `#[provekit::boundary(...)]` attribute call-sites, parse each into a
/// substrate-honest `CarrierComment`, and report which target-language
/// realize manifests would resolve their (family, concept_name)
/// tuples. Reports per-site outcome:
///
/// - **resolves**: exactly one target manifest matches (family + concept)
/// - **ambiguous**: multiple target manifests match — caller must
///   disambiguate via --library
/// - **refuses**: no target manifest matches — sister shim missing in
///   target language
///
/// This is foundation work for #1361 chunk 2 part B (signature translation
/// + per-target realize binary invocation + target-language file emission).
/// Part B picks up where this report leaves off — given the resolved
/// manifest list, emit target-language code.
///
/// Exit codes:
/// - `EXIT_OK` (0): scan completed, report printed.
/// - `EXIT_USER_ERROR` (>0): source-dir unreadable or no @boundary
///   attributes found (nothing to report on).
fn run_cross_language_discovery(
    project_root: &Path,
    source_dir: &Path,
    source_lang: &str,
    target_lang: &str,
    family_library_overrides: &[FamilyLibraryPair],
    out_dir: Option<&Path>,
) -> u8 {
    eprintln!(
        "{} cross-language discovery: {} -> {}",
        "materialize".cyan().bold(),
        source_lang,
        target_lang
    );

    let mut total_sites = 0usize;
    let mut resolves = 0usize;
    let mut ambiguous = 0usize;
    let mut refuses = 0usize;
    // #1361 follow-up: accumulate emitted bodies per source file when
    // --out-dir is set. Each (source_path, target_lang) → composite file
    // with imports (from realize manifest's scope_bringings) + all
    // RESOLVE'd boundaries' bodies concatenated.
    let mut emitted: BTreeMap<PathBuf, EmittedFile> = BTreeMap::new();

    for entry in WalkDir::new(source_dir).into_iter().flatten() {
        let path = entry.path();
        if !is_supported_source_file(path) || !should_scan_entry(path) {
            continue;
        }
        let raw = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        // inject_boundary_carriers turns @boundary attrs into
        // // provekit-concept: <payload> carriers — same surface used
        // by the same-language materialize path.
        let with_carriers = inject_boundary_carriers(&raw);
        for line in with_carriers.lines() {
            let Some((_indent, payload)) = concept_payload_from_line(line) else {
                continue;
            };
            let carrier = match CarrierComment::parse(payload) {
                Ok(c) => c,
                Err(_) => continue,
            };
            total_sites += 1;

            // Resolve via the catalog query: look for target_lang
            // manifests whose provides_concepts include this concept.
            // If family is pinned on the carrier, narrow to manifests
            // declaring that family too.
            let matches = find_target_manifests(
                project_root,
                target_lang,
                &carrier.concept_name,
                // family is carried in raw_payload — for now derive
                // from the substrate-honest carrier surface by parsing
                // raw_payload as JSON.
                family_from_payload(&carrier.raw_payload).as_deref(),
            );

            let rel = path.strip_prefix(source_dir).unwrap_or(path).display();
            match matches.len() {
                0 => {
                    refuses += 1;
                    eprintln!(
                        "  {} {} @ {} (concept={}, library={:?}): no {} manifest declares this concept",
                        "REFUSE".red().bold(),
                        carrier.concept_name,
                        rel,
                        carrier.concept_name,
                        carrier.library_tag.as_deref().unwrap_or("?"),
                        target_lang
                    );
                }
                1 => {
                    resolves += 1;
                    eprintln!(
                        "  {} {} @ {} → {} manifest `{}`",
                        "RESOLVE".green().bold(),
                        carrier.concept_name,
                        rel,
                        target_lang,
                        matches[0]
                    );
                    // #1361 chunk 2 part B / #1355: invoke the target kit's
                    // realize binary for the RESOLVE'd boundary. The realize
                    // binary owns its own concept-hub → target-syntax
                    // translation internally. cmd_materialize just routes
                    // the concept-hub-typed spec to it and gets target source.
                    // NO target-syntax knowledge in materialize.
                    if let Some(body) =
                        invoke_target_realize_for_discovery(project_root, target_lang, &matches[0], &carrier)
                    {
                        let preview: String = body.chars().take(120).collect();
                        let suffix = if body.chars().count() > 120 { "..." } else { "" };
                        eprintln!("      target body preview: {preview}{suffix}");
                        // #1361 follow-up: when --out-dir is set, accumulate
                        // the emitted body into the per-source-file composite.
                        // Imports come from the target realize manifest's
                        // scope_bringings (#1360).
                        if out_dir.is_some() {
                            let entry = emitted
                                .entry(path.to_path_buf())
                                .or_default();
                            entry.bodies.push(body);
                            let scope_imports = scope_bringings_for_realize(
                                project_root,
                                target_lang,
                                &matches[0],
                            );
                            for imp in scope_imports {
                                entry.imports.insert(imp);
                            }
                        }
                    }
                }
                _ => {
                    // #1361 follow-up: --family-library family=lib pairs are
                    // the substrate-honest compile-time decision mechanism.
                    // For each AMBIGUOUS site, look up the family override
                    // matching the carrier's family pin. The family suffix
                    // (after `concept:family:`) is the user-friendly handle.
                    // If exactly one candidate matches that library, RESOLVE
                    // to it. Otherwise report AMBIGUOUS — substrate refuses
                    // to silently pick beyond the user's explicit hint.
                    let carrier_family = family_from_payload(&carrier.raw_payload);
                    let picked = carrier_family.as_deref().and_then(|family| {
                        family_library_overrides
                            .iter()
                            .find(|p| family_matches_override(family, &p.family))
                            .and_then(|p| {
                                matches.iter().find(|m| m.as_str() == p.library).cloned()
                            })
                    });
                    if let Some(pick) = picked {
                        resolves += 1;
                        eprintln!(
                            "  {} {} @ {} → {} manifest `{}` (disambiguated by --family-library)",
                            "RESOLVE".green().bold(),
                            carrier.concept_name,
                            rel,
                            target_lang,
                            pick
                        );
                        if let Some(body) =
                            invoke_target_realize_for_discovery(project_root, target_lang, &pick, &carrier)
                        {
                            let preview: String = body.chars().take(120).collect();
                            let suffix = if body.chars().count() > 120 { "..." } else { "" };
                            eprintln!("      target body preview: {preview}{suffix}");
                            if out_dir.is_some() {
                                let entry = emitted
                                    .entry(path.to_path_buf())
                                    .or_default();
                                entry.bodies.push(body);
                                let scope_imports = scope_bringings_for_realize(
                                    project_root,
                                    target_lang,
                                    &pick,
                                );
                                for imp in scope_imports {
                                    entry.imports.insert(imp);
                                }
                            }
                        }
                        continue;
                    }
                    ambiguous += 1;
                    eprintln!(
                        "  {} {} @ {}: multiple {} manifests match — {:?}",
                        "AMBIGUOUS".yellow().bold(),
                        carrier.concept_name,
                        rel,
                        target_lang,
                        matches
                    );
                }
            }
        }
    }

    eprintln!(
        "{} discovery: {total_sites} site(s), {resolves} resolve + {ambiguous} ambiguous + {refuses} refused",
        "materialize".cyan().bold()
    );
    if total_sites == 0 {
        eprintln!(
            "{}: no @boundary attributes found in {}. Cross-language discovery requires \
             at least one #[provekit::boundary(...)] call-site.",
            "warn".yellow().bold(),
            source_dir.display()
        );
        return EXIT_USER_ERROR;
    }
    // #1361 follow-up: when --out-dir is set and we have any RESOLVE'd
    // emissions, write target-language composite files.
    if let Some(out_dir_path) = out_dir {
        if let Err(err) = std::fs::create_dir_all(out_dir_path) {
            eprintln!(
                "{}: failed to create --out-dir {}: {err}",
                "error".red().bold(),
                out_dir_path.display()
            );
            return EXIT_USER_ERROR;
        }
        let ext = target_lang_file_extension(target_lang);
        let mut files_written = 0usize;
        for (source_path, file) in &emitted {
            if file.bodies.is_empty() {
                continue;
            }
            let stem = source_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("materialized");
            let out_path = out_dir_path.join(format!("{stem}.{ext}"));
            let imports_block = file
                .imports
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");
            let bodies_block = file.bodies.join("\n\n");
            let content = if imports_block.is_empty() {
                format!("{bodies_block}\n")
            } else {
                format!("{imports_block}\n\n{bodies_block}\n")
            };
            if let Err(err) = std::fs::write(&out_path, content) {
                eprintln!(
                    "{}: failed to write {}: {err}",
                    "error".red().bold(),
                    out_path.display()
                );
                return EXIT_USER_ERROR;
            }
            eprintln!(
                "  {} wrote {} ({} body bodies, {} imports)",
                "EMIT".green().bold(),
                out_path.display(),
                file.bodies.len(),
                file.imports.len()
            );
            files_written += 1;
        }
        eprintln!(
            "{} cross-language emission: {files_written} file(s) written to {}",
            "materialize".green().bold(),
            out_dir_path.display()
        );
    } else {
        eprintln!(
            "{}: discovery-only mode. Add --out-dir <PATH> to emit target-language files.",
            "note".cyan().bold()
        );
    }
    EXIT_OK
}

/// Parse the carrier's raw_payload JSON and return the family field if
/// declared. Returns None when the payload either doesn't parse or omits
/// family (the substrate-honest signal for "family floats").
fn family_from_payload(payload: &str) -> Option<String> {
    let val: Json = serde_json::from_str(payload).ok()?;
    val.get("family")?.as_str().map(String::from)
}

/// #1361 follow-up: match a carrier's family pin (e.g.
/// `concept:family:json`) against a --family-library override key (e.g.
/// `json`). Accepts both forms: the full canonical name AND the user-
/// friendly suffix after `concept:family:`. Substrate-honest equality
/// uses the full canonical form; the suffix is sugar for CLI ergonomics.
fn family_matches_override(carrier_family: &str, override_key: &str) -> bool {
    if carrier_family == override_key {
        return true;
    }
    if let Some(suffix) = carrier_family.strip_prefix("concept:family:") {
        if suffix == override_key {
            return true;
        }
    }
    false
}

/// Find realize manifests in `target_lang` that declare the given
/// concept_name in their `provides_concepts`. When `family` is Some,
/// further narrow to manifests declaring the matching family. Returns
/// library_tag values for the matches.
fn find_target_manifests(
    project_root: &Path,
    target_lang: &str,
    concept_name: &str,
    family: Option<&str>,
) -> Vec<String> {
    use crate::kit_dispatch::registry_realize_candidates;
    let candidates = match registry_realize_candidates(project_root, target_lang) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let mut matches = Vec::new();
    for cand in &candidates {
        let provides = provides_concepts_for_realize(project_root, target_lang, &cand.tag);
        if !provides.iter().any(|c| c == concept_name) {
            continue;
        }
        if let Some(family_pin) = family {
            // When family is pinned on the carrier, the manifest must
            // declare the matching family to be considered. Loaded by
            // parsing the candidate's manifest source.
            if !manifest_declares_family(project_root, &cand.source, family_pin) {
                continue;
            }
        }
        matches.push(cand.tag.clone());
    }
    matches
}

/// Check whether the realize manifest at `manifest_source` declares the
/// given family. Resolves manifest_source against project_root when
/// relative, falls back to no-match when the file is unreadable.
fn manifest_declares_family(project_root: &Path, manifest_source: &str, family: &str) -> bool {
    let path = std::path::PathBuf::from(manifest_source);
    let resolved = if path.is_absolute() {
        path
    } else {
        project_root.join(&path)
    };
    if !resolved.is_file() {
        return false;
    }
    let Ok(raw) = std::fs::read_to_string(&resolved) else {
        return false;
    };
    // Line-based TOML probe matches the same shape kit_dispatch's
    // parse_manifest uses; the line `family = "<value>"` declares the pin.
    for line in raw.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("family") {
            let rest = rest.trim_start();
            if rest.starts_with('=') {
                let val = rest.trim_start_matches('=').trim();
                if val.trim_matches('"') == family {
                    return true;
                }
            }
        }
    }
    false
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
            if !provides.is_empty()
                && !provides.iter().any(|c| c == &carrier.concept_name)
            {
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
