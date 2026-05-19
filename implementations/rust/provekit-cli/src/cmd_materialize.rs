// SPDX-License-Identifier: Apache-2.0
//
// `provekit materialize` turns concept-citation carriers in source files into
// library-bound source by composing the existing LowerKit/realize path.

use std::io::Write;
use std::path::{Path, PathBuf};

use clap::Parser;
use libprovekit::core::{
    execute_path, HashMapInputCatalog, Input, KitRegistry, LowerKit, Path as CorePath, PathAlgebra,
    Verb,
};
use owo_colors::OwoColorize;
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
        return Ok((target.to_string(), Some(library.to_string())));
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
    for entry in WalkDir::new(source_dir).into_iter().filter_map(Result::ok) {
        let path = entry.path();
        if !path.is_file() || !is_supported_source_file(path) {
            continue;
        }
        let raw = std::fs::read_to_string(path)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        let (content, replacements) =
            materialize_source_text(project_root, target_lang, library_tag, &raw)?;
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
        if let Some(payload) = concept_payload_from_line(line) {
            let spec = realize_spec_from_payload(payload)?;
            let realized = realize_spec_via_path(project_root, target_lang, library_tag, spec)?;
            out.push_str(&realized);
            if !realized.ends_with('\n') {
                out.push('\n');
            }
            replacements += 1;
            if idx + 1 < lines.len() && concept_payload_cid_from_line(lines[idx + 1]).is_some() {
                idx += 2;
            } else {
                idx += 1;
            }
            continue;
        }
        out.push_str(line);
        idx += 1;
    }
    Ok((out, replacements))
}

fn concept_payload_from_line(line: &str) -> Option<&str> {
    let normalized = strip_comment_prefix(line.trim_start())?;
    normalized.strip_prefix("provekit-concept: ").map(str::trim)
}

fn concept_payload_cid_from_line(line: &str) -> Option<&str> {
    let normalized = strip_comment_prefix(line.trim_start())?;
    normalized
        .strip_prefix("provekit-concept-payload-cid: ")
        .map(str::trim)
}

fn strip_comment_prefix(line: &str) -> Option<&str> {
    line.strip_prefix("//")
        .or_else(|| line.strip_prefix('#'))
        .or_else(|| line.strip_prefix("/*"))
        .map(str::trim_start)
}

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
            let arity = object
                .get("params")
                .and_then(Json::as_array)
                .map(Vec::len)
                .unwrap_or(0);
            object.insert(
                "param_types".to_string(),
                Json::Array(
                    (0..arity)
                        .map(|_| Json::String("unknown".to_string()))
                        .collect(),
                ),
            );
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
