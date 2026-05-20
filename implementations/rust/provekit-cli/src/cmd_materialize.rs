// SPDX-License-Identifier: Apache-2.0
//
// `provekit materialize` turns concept-citation carriers in source files into
// library-bound source by composing the existing LowerKit/realize path.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::Parser;
use libprovekit::core::{
    execute_path, HashMapInputCatalog, Input, KitRegistry, LowerKit, Path as CorePath, PathAlgebra,
    Verb,
};
use owo_colors::OwoColorize;
use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value as CanonicalValue};
use serde_json::Value as Json;
use walkdir::WalkDir;

use crate::kit_dispatch::DispatchRealizeTransport;
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
    replacements: usize,
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

    if !args.out.quiet && (args.write || args.out_dir.is_some()) {
        let replacements: usize = files.iter().map(|file| file.replacements).sum();
        println!(
            "{} materialized {replacements} concept citation(s) across {} file(s)",
            "materialize".green().bold(),
            files.len()
        );
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
        let (content, replacements) =
            materialize_source_text(project_root, target_lang, library_tag, &raw)
                .map_err(|error| format!("{}: {error}", path.display()))?;
        if replacements == 0 {
            continue;
        }
        let relative_path = path.strip_prefix(source_dir).unwrap_or(path).to_path_buf();
        files.push(MaterializedFile {
            source_path: path.to_path_buf(),
            relative_path,
            content,
            replacements,
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

fn materialize_source_text(
    project_root: &Path,
    target_lang: &str,
    library_tag: Option<&str>,
    raw: &str,
) -> Result<(String, usize), String> {
    let mut out = String::new();
    let mut replacements = 0usize;
    let lines = raw.split_inclusive('\n').collect::<Vec<_>>();
    let mut idx = 0usize;
    while idx < lines.len() {
        let line = lines[idx];
        if let Some((indent, payload)) = concept_payload_from_line(line) {
            // Consume the carrier comment, optional payload-cid line, and
            // (if present) the following stub function declaration. The
            // materialized realization is emitted in their place. This is
            // the in-place body replacement strategy described in #1331:
            // every carrier+stub pair is replaced by a single materialized
            // function declaration carrying the realized body.
            let mut consumed = 1usize;
            if idx + consumed < lines.len()
                && concept_payload_cid_from_line(lines[idx + consumed]).is_some()
            {
                let declared_cid = concept_payload_cid_from_line(lines[idx + consumed]).unwrap();
                verify_payload_cid(payload, declared_cid)?;
                consumed += 1;
            }
            consumed += skip_stub_function_block(&lines[idx + consumed..]);

            let spec = realize_spec_from_payload(payload)?;
            let realized = realize_spec_via_path(project_root, target_lang, library_tag, spec)?;
            let indented = indent_realized_source(&realized, indent);
            out.push_str(&indented);
            if !indented.ends_with('\n') {
                out.push('\n');
            }
            replacements += 1;
            idx += consumed;
            continue;
        }
        out.push_str(line);
        idx += 1;
    }
    Ok((out, replacements))
}

/// Skip the stub function block that follows a carrier comment, if one is
/// present. Returns the number of lines consumed.
///
/// A stub block starts with a line whose first non-whitespace tokens form a
/// Rust function declaration (`fn`, `pub fn`, `pub(...) fn`, `async fn`,
/// `const fn`, `unsafe fn`, or combinations thereof) and ends at the
/// matching closing brace of the function body.
///
/// If no stub function follows the carrier (the carrier sits above a non-fn
/// item, or no item at all), this returns 0 and the carrier replacement is
/// inserted without consuming subsequent content.
fn skip_stub_function_block(lines: &[&str]) -> usize {
    let Some(first_line) = lines.first() else {
        return 0;
    };
    if !line_starts_function_declaration(first_line) {
        return 0;
    }
    // Track brace depth from the first line forward until we balance back
    // to zero. The opening brace may be on the same line as `fn ...` or on
    // a subsequent line (Rust style with `fn foo()\n{`).
    let mut depth: i32 = 0;
    let mut saw_open = false;
    for (offset, line) in lines.iter().enumerate() {
        for ch in line.chars() {
            match ch {
                '{' => {
                    depth += 1;
                    saw_open = true;
                }
                '}' => {
                    depth -= 1;
                    if saw_open && depth == 0 {
                        return offset + 1;
                    }
                }
                _ => {}
            }
        }
    }
    // Unbalanced or never opened. Consume nothing rather than swallow the
    // rest of the file; the realized source is still inserted above.
    0
}

fn line_starts_function_declaration(line: &str) -> bool {
    let trimmed = line.trim_start();
    const KEYWORDS_TO_STRIP: &[&str] = &[
        "pub ", "async ", "const ", "unsafe ", "extern ", "default ",
    ];
    let mut remaining = trimmed;
    loop {
        let mut stripped = false;
        if remaining.starts_with("pub(") {
            if let Some(rest) = remaining.split_once(')').map(|(_, r)| r.trim_start()) {
                remaining = rest;
                stripped = true;
            }
        }
        for kw in KEYWORDS_TO_STRIP {
            if let Some(rest) = remaining.strip_prefix(kw) {
                remaining = rest.trim_start();
                stripped = true;
                break;
            }
        }
        if !stripped {
            break;
        }
    }
    remaining.starts_with("fn ") || remaining.starts_with("fn(")
}

fn concept_payload_from_line(line: &str) -> Option<(&str, &str)> {
    let indent_len = line.len() - line.trim_start().len();
    let indent = &line[..indent_len];
    let normalized = strip_comment_prefix(line.trim_start())?;
    normalized
        .strip_prefix("provekit-concept: ")
        .map(str::trim)
        .map(|payload| (indent, payload))
}

fn concept_payload_cid_from_line(line: &str) -> Option<&str> {
    let normalized = strip_comment_prefix(line.trim_start())?;
    normalized
        .strip_prefix("provekit-concept-payload-cid: ")
        .map(str::trim)
}

fn strip_comment_prefix(line: &str) -> Option<&str> {
    let body = line
        .strip_prefix("//")
        .or_else(|| line.strip_prefix('#'))
        .or_else(|| line.strip_prefix("/*"))?
        .trim_start();
    Some(
        body.trim_end()
            .strip_suffix("*/")
            .map(str::trim_end)
            .unwrap_or(body),
    )
}

fn indent_realized_source(source: &str, indent: &str) -> String {
    if indent.is_empty() {
        return source.to_string();
    }
    source
        .split_inclusive('\n')
        .map(|line| {
            if line.trim().is_empty() {
                line.to_string()
            } else {
                format!("{indent}{line}")
            }
        })
        .collect()
}

fn verify_payload_cid(payload: &str, declared_cid: &str) -> Result<(), String> {
    let parsed: Json = serde_json::from_str(payload)
        .map_err(|error| format!("parse provekit-concept payload JSON: {error}"))?;
    let canonical = canonical_value_from_json(&parsed)?;
    let actual_cid = blake3_512_of(encode_jcs(canonical.as_ref()).as_bytes());
    if actual_cid != declared_cid {
        return Err(format!(
            "provekit-concept-payload-cid mismatch: declared {declared_cid}, computed {actual_cid}"
        ));
    }
    Ok(())
}

fn canonical_value_from_json(value: &Json) -> Result<Arc<CanonicalValue>, String> {
    match value {
        Json::Null => Ok(CanonicalValue::null()),
        Json::Bool(value) => Ok(CanonicalValue::boolean(*value)),
        Json::Number(value) => value.as_i64().map(CanonicalValue::integer).ok_or_else(|| {
            format!("provekit-concept payload contains non-integer number `{value}`")
        }),
        Json::String(value) => Ok(CanonicalValue::string(value)),
        Json::Array(values) => values
            .iter()
            .map(canonical_value_from_json)
            .collect::<Result<Vec<_>, _>>()
            .map(CanonicalValue::array),
        Json::Object(entries) => entries
            .iter()
            .map(|(key, value)| canonical_value_from_json(value).map(|value| (key.clone(), value)))
            .collect::<Result<Vec<_>, _>>()
            .map(CanonicalValue::object),
    }
}

// Permissive-defaults for carrier payloads. The materialize command synthesizes
// defaults for missing carrier-payload fields (function, params, param_types,
// return_type) to reduce friction during development. The substrate-honest
// alternative is refuse-on-missing-fields: require each carrier author to
// provide a complete payload. The permissive shape is intentional for the
// build-pipeline ergonomics; the trade-off is that incomplete carriers may
// produce stub-like realize output. A future strict mode (e.g., a
// --strict-payloads flag) could refuse incomplete carriers up front.
fn realize_spec_from_payload(payload: &str) -> Result<Json, String> {
    let mut value: Json = serde_json::from_str(payload)
        .map_err(|error| format!("parse provekit-concept payload JSON: {error}"))?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| "provekit-concept payload must be a JSON object".to_string())?;
    let concept_name = object
        .get("concept_name")
        .or_else(|| object.get("conceptName"))
        .and_then(Json::as_str)
        .ok_or_else(|| "provekit-concept payload missing concept_name".to_string())?
        .to_string();
    object.insert(
        "kind".to_string(),
        Json::String("RealizeRequest".to_string()),
    );
    object.insert("concept_name".to_string(), Json::String(concept_name));
    if !object.contains_key("function") {
        object.insert(
            "function".to_string(),
            Json::String("provekit_materialized".to_string()),
        );
    }
    if !object.contains_key("params") {
        object.insert("params".to_string(), Json::Array(Vec::new()));
    }
    if !object.contains_key("param_types") {
        if let Some(param_types) = object.remove("paramTypes") {
            object.insert("param_types".to_string(), param_types);
        } else {
            object.insert("param_types".to_string(), Json::Array(Vec::new()));
        }
    }
    if !object.contains_key("return_type") {
        if let Some(return_type) = object.remove("returnType") {
            object.insert("return_type".to_string(), return_type);
        } else {
            object.insert("return_type".to_string(), Json::String("void".to_string()));
        }
    }
    if !object.contains_key("named_term_tree") {
        if let Some(named_term_tree) = object.remove("namedTermTree") {
            object.insert("named_term_tree".to_string(), named_term_tree);
        }
    }
    Ok(value)
}

fn realize_spec_via_path(
    project_root: &Path,
    target_lang: &str,
    library_tag: Option<&str>,
    spec: Json,
) -> Result<String, String> {
    let mut inputs = HashMapInputCatalog::default();
    let input_cid = inputs.insert(Input::Spec(spec));
    let kit_name = library_tag
        .map(|tag| format!("lower-{target_lang}-{tag}"))
        .unwrap_or_else(|| format!("lower-{target_lang}"));
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
            project_root.to_path_buf(),
            target_lang.to_string(),
            library_tag.map(str::to_string),
            DispatchRealizeTransport,
        ),
        target_lang,
        project_root.join(format!(
            "implementations/{target_lang}/conformance/fixtures"
        )),
    );
    let chain = execute_path(&path, &registry, &inputs).map_err(|error| {
        error
            .composition_refusal()
            .and_then(|refusal| serde_json::to_string(refusal).ok())
            .unwrap_or_else(|| error.to_string())
    })?;
    let realized =
        LowerKit::<DispatchRealizeTransport>::realized_source_from_claim(chain.terminal_claim())?;
    // String-formatted CLI error rather than a structured gap-record memento:
    // materialize is a build-pipeline workflow that does not emit a receipt
    // memento (unlike cmd_bind_migrate which produces MigrateReceiptEnvelope
    // with refusal_mementos per `2026-05-18-refuse-leg-short-circuit-ruling`).
    // The CLI surface is the consumer; a string error suffices. If a future
    // caller needs structured refusal for build-pipeline consumption, extend
    // materialize to optionally emit a refusal receipt and route through the
    // existing RefusalMemento machinery.
    if realized.is_stub {
        return Err(format!(
            "realize plugin for `{target_lang}` library `{}` returned a stub",
            library_tag.unwrap_or("default")
        ));
    }
    Ok(realized.source)
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
