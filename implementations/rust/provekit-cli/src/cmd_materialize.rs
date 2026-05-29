// SPDX-License-Identifier: Apache-2.0
//
// `provekit materialize` turns concept-citation carriers in source files into
// library-bound source by composing the existing LowerKit/realize path.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use clap::Parser;
use libprovekit::core::emit_obligation::{build_bridge_body, member_envelope_canonical};
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
use provekit_proof_envelope::{
    build_proof_envelope, ed25519_pubkey_string, Ed25519Seed, ProofEnvelopeInput,
};
use serde_json::Value as Json;
use walkdir::WalkDir;

use crate::kit_dispatch::{
    dispatch_materialize_check, provides_concepts_for_realize, scope_bringings_for_realize,
    DispatchRealizeTransport,
};
use crate::{OutputFlags, EXIT_OK, EXIT_USER_ERROR, EXIT_VERIFY_FAIL};

const MATERIALIZE_BRIDGE_DECLARED_AT: &str = "2026-05-27T00:00:00.000Z";
const MATERIALIZE_BRIDGE_SIGNER_SEED: Ed25519Seed = [0x6d; 32];

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
                library_tag.as_deref(),
                &args.family_library,
                args.out_dir.as_deref(),
                args.out.json,
                args.compile_check,
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

/// #1361 follow-up / #1355: accumulated target-language file content
/// for cross-language emission. One per source file → emits one composite
/// target-language file with imports + each RESOLVE'd boundary's body.
#[derive(Debug, Clone, Default)]
struct EmittedFile {
    bodies: Vec<String>,
    /// Scope-bringings deduplicated across all libraries used in this file.
    imports: std::collections::BTreeSet<String>,
    /// #1375 Milestone C: per-fragment objects for target-owned assembly.
    /// Each entry is the realize-fragment JSON shape (source + imports +
    /// helpers + dependencies + diagnostics + compile_unit_requirements +
    /// concept_name) that the target kit's assembler consumes to produce
    /// a compilation unit. Substrate keeps these so the selected kit owns
    /// target-source assembly.
    fragments: Vec<serde_json::Value>,
    /// Target manifest used for this file's emission (kept so the
    /// substrate knows which kit to call for assemble).
    target_manifest: Option<String>,
}

/// Discrimated outcome of a cross-language discovery realize-probe.
///
/// `None`-collapsed outcomes hide WHY the probe didn't succeed: a transport
/// error (broken manifest, missing binary, parse failure) looks the same as
/// a substrate-canonical semantic gap. Callers should treat them differently:
/// SemanticGap → REFUSE with substrate-gap message; TransportError → log
/// and skip without counting as a refuse; Preview → RESOLVE with emitted body.
#[derive(Debug)]
enum DiscoveryOutcome {
    /// Realize plugin successfully emitted a fragment — full RESOLVE.
    /// #1374: carries the per-fragment context (imports) so materialize
    /// can include realizer-declared imports in the emitted compilation
    /// unit. `source` is the fragment body; `imports` is the
    /// fully-qualified names the fragment uses from outside its own body.
    /// #1390: `helpers` is the list of static field declarations the
    /// fragment needs hoisted into the compilation unit's class body.
    Preview {
        source: String,
        imports: Vec<String>,
        helpers: Vec<String>,
    },
    /// Plugin ran but returned is_stub=true; the substrate has a real gap
    /// (no morphism for some sort CID, no body template for concept, etc.).
    SemanticGap,
    /// Plugin couldn't be invoked or its response couldn't be parsed.
    /// NOT a substrate gap — a kit operational failure.
    TransportError(String),
}

/// #1373 phase 2: machine-readable per-site resolution record. One entry
/// per @boundary site scanned; `resolution.status` is the discriminator
/// for downstream tooling.
#[derive(Debug, serde::Serialize)]
struct SiteReport {
    file: String,
    concept_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    family: Option<String>,
    resolution: SiteResolution,
}

#[derive(Debug, serde::Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum SiteResolution {
    Resolve {
        target_manifest: String,
        target_language: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        disambiguated_by: Option<String>,
    },
    Ambiguous {
        target_language: String,
        candidates: Vec<String>,
    },
    Refuse {
        target_manifest: String,
        target_language: String,
        reason: String,
    },
    Error {
        target_manifest: String,
        target_language: String,
        reason: String,
    },
}

#[derive(Debug, serde::Serialize)]
struct ResolutionSummary {
    total: usize,
    resolve: usize,
    ambiguous: usize,
    refuse: usize,
    error: usize,
}

#[derive(Debug, serde::Serialize)]
struct ResolutionReport {
    source_lang: String,
    target_lang: String,
    sites: Vec<SiteReport>,
    summary: ResolutionSummary,
}

/// #1361 chunk 2 part B / #1355: invoke target kit's realize binary for a
/// RESOLVE'd boundary in cross-language discovery mode. The realize binary
/// owns the concept-hub → target-syntax translation internally; cmd_materialize
/// just routes the concept-hub-typed spec to it and gets back target source.
fn invoke_target_realize_for_discovery(
    project_root: &Path,
    target_lang: &str,
    target_library_tag: &str,
    carrier: &CarrierComment,
) -> DiscoveryOutcome {
    use libprovekit::core::lower_plugin::request_from_spec;
    use libprovekit::core::RealizeTransport;
    let spec = match realize_spec_from_payload(&carrier.raw_payload) {
        Ok(s) => s,
        Err(e) => return DiscoveryOutcome::TransportError(format!("spec parse: {e}")),
    };
    let mut request = match request_from_spec(&spec) {
        Ok(r) => r,
        Err(e) => return DiscoveryOutcome::TransportError(format!("request build: {e}")),
    };
    request.target_library_tag = target_library_tag.to_string();
    let transport = DispatchRealizeTransport;
    let response = match transport.dispatch_realize(
        project_root,
        target_lang,
        Some(target_library_tag),
        &request,
    ) {
        Ok(r) => r,
        Err(e) => return DiscoveryOutcome::TransportError(format!("dispatch: {e:?}")),
    };
    if response.is_stub {
        return DiscoveryOutcome::SemanticGap;
    }
    // #1390: helpers is a list of strings in the response. The RealizedSource
    // schema (#1374) types it as Vec<Value>; pull out string entries.
    let helpers: Vec<String> = response
        .helpers
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();
    DiscoveryOutcome::Preview {
        source: response.source,
        imports: response.imports,
        helpers,
    }
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
    target_library_tag: Option<&str>,
    family_library_overrides: &[FamilyLibraryPair],
    out_dir: Option<&Path>,
    json_report: bool,
    compile_check: bool,
) -> u8 {
    if !json_report {
        eprintln!(
            "{} cross-language discovery: {} -> {}",
            "materialize".cyan().bold(),
            source_lang,
            target_lang
        );
    }

    let mut total_sites = 0usize;
    let mut resolves = 0usize;
    let mut ambiguous = 0usize;
    let mut refuses = 0usize;
    // #1373: TransportError is an ERROR, not a refuse. Broken manifests /
    // missing realize binaries / parse failures are kit operational issues,
    // not substrate gaps. Tally separately so the exit code reflects them.
    let mut transport_errors = 0usize;
    // #1373 phase 2: structured site results for --json report mode.
    let mut site_reports: Vec<SiteReport> = Vec::new();
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
            let carrier_family = family_from_payload(&carrier.raw_payload);
            let has_family_override = carrier_family.as_deref().is_some_and(|family| {
                family_library_overrides
                    .iter()
                    .any(|p| family_matches_override(family, &p.family))
            });
            // #34: --library precedence rule. --library is the DEFAULT for
            // multi-candidate sites without a more specific override, NOT a
            // hard filter on the whole site set. Order:
            //   1. Per-family override (--family-library): most specific. Keep
            //      all candidates so the ambiguous-site handler resolves via
            //      family.
            //   2. --library: if the requested library tag is among the
            //      candidates, pick it; otherwise leave the full set so the
            //      site resolves via its own structure (single-candidate
            //      concepts must not be filtered out by a top-level
            //      --library that doesn't apply to them).
            //   3. No hint: leave candidates intact; AMBIGUOUS if >1.
            //
            // Pre-#1383 behavior was case 3 only (--library was ignored at
            // this layer). #1383 added case 2 as a HARD filter which broke
            // every single-candidate concept whose manifest didn't match
            // --library. This restores case 2 as a SOFT preference.
            let matches = if has_family_override {
                matches
            } else if let Some(target_library_tag) = target_library_tag {
                let preferred: Vec<String> = matches
                    .iter()
                    .filter(|candidate| *candidate == target_library_tag)
                    .cloned()
                    .collect();
                if preferred.is_empty() {
                    matches
                } else {
                    preferred
                }
            } else {
                matches
            };

            let rel = path.strip_prefix(source_dir).unwrap_or(path).display();
            match matches.len() {
                0 => {
                    refuses += 1;
                    if !json_report {
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
                    site_reports.push(SiteReport {
                        file: rel.to_string(),
                        concept_name: carrier.concept_name.clone(),
                        family: family_from_payload(&carrier.raw_payload),
                        resolution: SiteResolution::Refuse {
                            target_manifest: String::new(),
                            target_language: target_lang.to_string(),
                            reason: format!(
                                "no {} manifest declares concept {}",
                                target_lang, carrier.concept_name
                            ),
                        },
                    });
                }
                1 => {
                    // Substrate-honest 2-stage outcome: dispatch ROUTE found
                    // (manifest matches family+concept) is RESOLVE; whether
                    // the target realize binary then accepts the concept-
                    // hub-typed spec OR refuses-loudly (is_stub due to
                    // missing concept-hub sort morphism) is reported
                    // separately. The "route found" + "type gap" outcome
                    // is REFUSE in the final tally — substrate surfaces
                    // gaps even when the family dispatch resolves.
                    let carrier_family = family_from_payload(&carrier.raw_payload);
                    match invoke_target_realize_for_discovery(
                        project_root,
                        target_lang,
                        &matches[0],
                        &carrier,
                    ) {
                        DiscoveryOutcome::Preview {
                            source: body,
                            imports: fragment_imports,
                            helpers: fragment_helpers,
                        } => {
                            resolves += 1;
                            if !json_report {
                                eprintln!(
                                    "  {} {} @ {} → {} manifest `{}`",
                                    "RESOLVE".green().bold(),
                                    carrier.concept_name,
                                    rel,
                                    target_lang,
                                    matches[0]
                                );
                                let preview: String = body.chars().take(120).collect();
                                let suffix = if body.chars().count() > 120 {
                                    "..."
                                } else {
                                    ""
                                };
                                eprintln!("      target body preview: {preview}{suffix}");
                            }
                            site_reports.push(SiteReport {
                                file: rel.to_string(),
                                concept_name: carrier.concept_name.clone(),
                                family: carrier_family.clone(),
                                resolution: SiteResolution::Resolve {
                                    target_manifest: matches[0].clone(),
                                    target_language: target_lang.to_string(),
                                    disambiguated_by: None,
                                },
                            });
                            if out_dir.is_some() {
                                let entry = emitted.entry(path.to_path_buf()).or_default();
                                // #1375 Milestone C / #1390: record the fragment
                                // shape (source + imports + helpers) so target-
                                // kit assemble can consume it.
                                entry.fragments.push(serde_json::json!({
                                    "concept_name": carrier.concept_name,
                                    "source": body.clone(),
                                    "imports": fragment_imports.clone(),
                                    "helpers": fragment_helpers.clone(),
                                }));
                                entry.target_manifest = Some(matches[0].clone());
                                entry.bodies.push(body);
                                for imp in fragment_imports {
                                    entry.imports.insert(imp);
                                }
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
                        DiscoveryOutcome::SemanticGap => {
                            refuses += 1;
                            if !json_report {
                                eprintln!(
                                    "  {} {} @ {} → {} manifest `{}` (substrate gap in concept-hub sort morphism)",
                                    "REFUSE".red().bold(),
                                    carrier.concept_name,
                                    rel,
                                    target_lang,
                                    matches[0]
                                );
                            }
                            site_reports.push(SiteReport {
                                file: rel.to_string(),
                                concept_name: carrier.concept_name.clone(),
                                family: carrier_family.clone(),
                                resolution: SiteResolution::Refuse {
                                    target_manifest: matches[0].clone(),
                                    target_language: target_lang.to_string(),
                                    reason: "substrate gap: realize plugin returned is_stub for concept-hub sort morphism".to_string(),
                                },
                            });
                        }
                        DiscoveryOutcome::TransportError(err) => {
                            transport_errors += 1;
                            if !json_report {
                                eprintln!(
                                    "  {} {} @ {} → {} manifest `{}` realize transport failed: {}",
                                    "ERROR".red().bold(),
                                    carrier.concept_name,
                                    rel,
                                    target_lang,
                                    matches[0],
                                    err,
                                );
                            }
                            site_reports.push(SiteReport {
                                file: rel.to_string(),
                                concept_name: carrier.concept_name.clone(),
                                family: carrier_family.clone(),
                                resolution: SiteResolution::Error {
                                    target_manifest: matches[0].clone(),
                                    target_language: target_lang.to_string(),
                                    reason: err,
                                },
                            });
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
                            .and_then(|p| matches.iter().find(|m| m.as_str() == p.library).cloned())
                    });
                    if let Some(pick) = picked {
                        match invoke_target_realize_for_discovery(
                            project_root,
                            target_lang,
                            &pick,
                            &carrier,
                        ) {
                            DiscoveryOutcome::Preview {
                                source: body,
                                imports: fragment_imports,
                                helpers: fragment_helpers,
                            } => {
                                resolves += 1;
                                if !json_report {
                                    eprintln!(
                                        "  {} {} @ {} → {} manifest `{}` (disambiguated by --family-library)",
                                        "RESOLVE".green().bold(),
                                        carrier.concept_name,
                                        rel,
                                        target_lang,
                                        pick
                                    );
                                    let preview: String = body.chars().take(120).collect();
                                    let suffix = if body.chars().count() > 120 {
                                        "..."
                                    } else {
                                        ""
                                    };
                                    eprintln!("      target body preview: {preview}{suffix}");
                                }
                                site_reports.push(SiteReport {
                                    file: rel.to_string(),
                                    concept_name: carrier.concept_name.clone(),
                                    family: carrier_family.clone(),
                                    resolution: SiteResolution::Resolve {
                                        target_manifest: pick.clone(),
                                        target_language: target_lang.to_string(),
                                        disambiguated_by: Some("--family-library".to_string()),
                                    },
                                });
                                if out_dir.is_some() {
                                    let entry = emitted.entry(path.to_path_buf()).or_default();
                                    // #1375 Milestone C / #1390: fragment with
                                    // source + imports + helpers.
                                    entry.fragments.push(serde_json::json!({
                                        "concept_name": carrier.concept_name,
                                        "source": body.clone(),
                                        "imports": fragment_imports.clone(),
                                        "helpers": fragment_helpers.clone(),
                                    }));
                                    entry.target_manifest = Some(pick.clone());
                                    entry.bodies.push(body);
                                    for imp in fragment_imports {
                                        entry.imports.insert(imp);
                                    }
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
                            DiscoveryOutcome::SemanticGap => {
                                refuses += 1;
                                if !json_report {
                                    eprintln!(
                                        "  {} {} @ {} → {} manifest `{}` (disambiguated by --family-library, substrate gap in concept-hub sort morphism)",
                                        "REFUSE".red().bold(),
                                        carrier.concept_name,
                                        rel,
                                        target_lang,
                                        pick
                                    );
                                }
                                site_reports.push(SiteReport {
                                    file: rel.to_string(),
                                    concept_name: carrier.concept_name.clone(),
                                    family: carrier_family.clone(),
                                    resolution: SiteResolution::Refuse {
                                        target_manifest: pick.clone(),
                                        target_language: target_lang.to_string(),
                                        reason: "substrate gap: realize plugin returned is_stub for concept-hub sort morphism".to_string(),
                                    },
                                });
                            }
                            DiscoveryOutcome::TransportError(err) => {
                                transport_errors += 1;
                                if !json_report {
                                    eprintln!(
                                        "  {} {} @ {} → {} manifest `{}` (disambiguated by --family-library) realize transport failed: {}",
                                        "ERROR".red().bold(),
                                        carrier.concept_name,
                                        rel,
                                        target_lang,
                                        pick,
                                        err,
                                    );
                                }
                                site_reports.push(SiteReport {
                                    file: rel.to_string(),
                                    concept_name: carrier.concept_name.clone(),
                                    family: carrier_family.clone(),
                                    resolution: SiteResolution::Error {
                                        target_manifest: pick.clone(),
                                        target_language: target_lang.to_string(),
                                        reason: err,
                                    },
                                });
                            }
                        }
                        continue;
                    }
                    ambiguous += 1;
                    if !json_report {
                        eprintln!(
                            "  {} {} @ {}: multiple {} manifests match — {:?}",
                            "AMBIGUOUS".yellow().bold(),
                            carrier.concept_name,
                            rel,
                            target_lang,
                            matches
                        );
                    }
                    site_reports.push(SiteReport {
                        file: rel.to_string(),
                        concept_name: carrier.concept_name.clone(),
                        family: carrier_family.clone(),
                        resolution: SiteResolution::Ambiguous {
                            target_language: target_lang.to_string(),
                            candidates: matches.clone(),
                        },
                    });
                }
            }
        }
    }

    if !json_report {
        eprintln!(
            "{} discovery: {total_sites} site(s), {resolves} resolve + {ambiguous} ambiguous + {refuses} refused + {transport_errors} transport-error",
            "materialize".cyan().bold()
        );
    }
    // #1373 phase 2: emit machine-readable JSON report when --json is set.
    if json_report {
        let report = ResolutionReport {
            source_lang: source_lang.to_string(),
            target_lang: target_lang.to_string(),
            sites: site_reports,
            summary: ResolutionSummary {
                total: total_sites,
                resolve: resolves,
                ambiguous,
                refuse: refuses,
                error: transport_errors,
            },
        };
        match serde_json::to_string_pretty(&report) {
            Ok(json) => println!("{json}"),
            Err(err) => {
                eprintln!(
                    "{}: failed to serialize report: {err}",
                    "error".red().bold()
                );
                return EXIT_USER_ERROR;
            }
        }
    }
    if total_sites == 0 {
        eprintln!(
            "{}: no @boundary attributes found in {}. Cross-language discovery requires \
             at least one #[provekit::boundary(...)] call-site.",
            "warn".yellow().bold(),
            source_dir.display()
        );
        return EXIT_USER_ERROR;
    }
    // #1373: strict resolution exit semantics.
    //   - ambiguous: caller didn't disambiguate via --family-library → error.
    //   - transport_errors: kit operational failures (broken manifest /
    //     missing binary / parse error) → error.
    //   - refuses: substrate gap (is_stub) → error (caller should mint the
    //     missing morphism or specify a different library).
    // Any of these means the cross-language demo did NOT fully resolve.
    // Emission only proceeds when ALL sites are RESOLVE.
    let has_failure = ambiguous > 0 || refuses > 0 || transport_errors > 0;
    if has_failure && out_dir.is_some() {
        eprintln!(
            "{}: --out-dir requested but {} site(s) did not resolve ({} ambiguous, {} refused, {} transport-error). \
             Emission aborted; the substrate refuses to write a partial compilation unit.",
            "error".red().bold(),
            ambiguous + refuses + transport_errors,
            ambiguous, refuses, transport_errors,
        );
        return EXIT_VERIFY_FAIL;
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
        let mut files_written = 0usize;
        // #1388: aggregate kit-declared compile metadata across all files so
        // --compile-check can pass it back to the same kit over RPC.
        let mut aggregated_classpath: std::collections::BTreeSet<String> =
            std::collections::BTreeSet::new();
        for (source_path, file) in &emitted {
            if file.bodies.is_empty() {
                continue;
            }
            let rel = source_path.strip_prefix(source_dir).unwrap_or(source_path);
            // #1375 Milestone C: route compilation-unit assembly to the
            // target kit. The kit decides file layout, package, imports,
            // and class/module wrapping. Missing assembly is a boundary
            // failure, not permission for the CLI to bake target syntax.
            let file_basename = rel.file_stem().and_then(|s| s.to_str()).unwrap_or("lib");
            let fragments_json =
                serde_json::to_string(&file.fragments).unwrap_or_else(|_| "[]".to_string());
            let Some(manifest) = file.target_manifest.as_deref() else {
                eprintln!(
                    "{}: no target manifest available to assemble {} via kit RPC",
                    "error".red().bold(),
                    rel.display()
                );
                return EXIT_VERIFY_FAIL;
            };
            use crate::kit_dispatch::{dispatch_assemble, AssembleError};
            match dispatch_assemble(
                project_root,
                target_lang,
                Some(manifest),
                &fragments_json,
                file_basename,
                None,
            ) {
                Ok(assembled_result) => {
                    for af in &assembled_result.files {
                        let assembled_path = out_dir_path.join(&af.path);
                        if let Some(parent) = assembled_path.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        if let Err(err) = std::fs::write(&assembled_path, &af.content) {
                            eprintln!(
                                "{}: failed to write {}: {err}",
                                "error".red().bold(),
                                assembled_path.display()
                            );
                            return EXIT_USER_ERROR;
                        }
                        eprintln!(
                            "  {} wrote {} (target-owned assembly via {} kit)",
                            "EMIT".green().bold(),
                            assembled_path.display(),
                            target_lang,
                        );
                        files_written += 1;
                    }
                    // #1388: collect kit-declared classpath entries.
                    for cp in &assembled_result.compile_classpath {
                        aggregated_classpath.insert(cp.clone());
                    }
                }
                Err(AssembleError::MethodNotSupported) => {
                    eprintln!(
                        "{}: selected {target_lang} kit must implement assemble RPC for --out-dir materialize; refusing CLI-side target assembly for {}",
                        "error".red().bold(),
                        rel.display()
                    );
                    return EXIT_VERIFY_FAIL;
                }
                Err(AssembleError::Failed(err)) => {
                    eprintln!(
                        "{}: target-kit assemble failed for {}: {err}",
                        "error".red().bold(),
                        rel.display()
                    );
                    return EXIT_VERIFY_FAIL;
                }
            }
        }
        eprintln!(
            "{} cross-language emission: {files_written} file(s) written to {}",
            "materialize".green().bold(),
            out_dir_path.display()
        );
        if compile_check {
            let classpath: Vec<String> = aggregated_classpath.iter().cloned().collect();
            let check_result = run_materialize_check(
                project_root,
                target_lang,
                target_library_tag,
                out_dir_path,
                &classpath,
            );
            if check_result != EXIT_OK {
                return check_result;
            }
        }
    } else {
        eprintln!(
            "{}: discovery-only mode. Add --out-dir <PATH> to emit target-language files.",
            "note".cyan().bold()
        );
    }
    // #1373: discovery-only mode also exits non-zero when sites didn't
    // resolve. The substrate-honest result of "9 sites scanned, 2 refuse"
    // is failure, not success.
    if ambiguous > 0 || refuses > 0 || transport_errors > 0 {
        EXIT_VERIFY_FAIL
    } else {
        EXIT_OK
    }
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
pub fn family_matches_override(carrier_family: &str, override_key: &str) -> bool {
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
    for language in ["typescript", "python", "rust", "java"] {
        // Same manifest-aware preference as the `--target` arm: a tag that
        // literally starts with `{language}-` (e.g. `java-io`) must not be
        // truncated when it names a real manifest.
        if library.starts_with(&format!("{language}-"))
            && realize_tag_exists(project_root, language, library)
        {
            return Ok((language.to_string(), Some(library.to_string())));
        }
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
        });
    }
    Ok(files)
}

fn is_supported_source_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("ts" | "tsx" | "js" | "jsx" | "py" | "rs" | "java" | "go")
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

    Ok((final_source, receipt, fragments))
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
    /// (which owns imports/helper-hoisting/package/class-wrapping — the
    /// substrate holds no language syntax). Each entry mirrors the cross-
    /// language fragment shape `{concept_name, source, imports, helpers}`.
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
        // Collect the per-site fragment for the kit's assemble RPC. Mirrors the
        // cross-language fragment shape (cmd_materialize.rs discovery loop): the
        // kit's assembler peels the wrapper class, dedupes imports, hoists
        // helpers, and emits the package + compilation unit. The substrate does
        // not assemble — it just forwards what the realize plugin produced.
        self.fragments
            .lock()
            .expect("fragments mutex")
            .push(serde_json::json!({
                "concept_name": carrier.concept_name,
                "source": realized.source.clone(),
                "imports": realized.imports.clone(),
                "helpers": realized.helpers.clone(),
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
/// compiler semantics. Same-language and cross-language both land here;
/// source-lang == target-lang changes the lift, not who assembles/checks.
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

// materialize_bridge_body / flat_member / canonical_json / canonical_value
// were moved to libprovekit::core::emit_obligation as build_bridge_body
// + member_envelope_canonical (#1579). cmd_materialize now imports them
// and shares one canonical authoring path with cmd_recognize.
