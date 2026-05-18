// SPDX-License-Identifier: Apache-2.0
//
// `provekit-walk-rpc`: minimal JSON-RPC 2.0 server over stdio. Each line
// of stdin is one JSON-RPC request; each response is one line of JSON
// on stdout. Methods:
//
//   walk.lift_pre        { src, fn_name }            → IrFormula
//   walk.lift_post       { src, fn_name }            → IrFormula
//   walk.contract        { src, fn_name, file? }     → FunctionContractMemento
//   walk.term            { src, fn_name, source? }   → rust algebra term JSON
//   walk.shadow_source   { src, callee, caller }     → { cid, slots, arrivals_total, bundle_cid }
//   walk.proof_ir        { src, callee, caller }     → { cid, bytes_b64, length }
//
// All requests must include `jsonrpc: "2.0"` and an `id`. Errors return
// JSON-RPC error objects.
//
// This makes the substrate's wire-format gap closed end-to-end: any
// program that speaks line-delimited JSON-RPC can drive provekit-walk
// and pull back proof.ir bytes ready for the substrate's lift / mint /
// linker pipeline.

use std::collections::BTreeMap;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use base64::Engine;
use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value as CValue};
use provekit_ir_types::{EvidenceMemento, ExamManifestMemento, IrFormula, IrTerm, SourceKind};
use provekit_lift_contracts::lift_file_with_docstring_evidence;
use provekit_walk::emit::{rust_function_term_json, shadow_proof_ir_cid, shadow_to_proof_ir};
use provekit_walk::{
    build_function_contract_with_file, build_shadow_source, lift_function_postcondition,
    lift_function_precondition, CalleeContract,
};
use serde_json::{json, Value};

const CONCEPT_SHAPES_CATALOG_INDEX_JSON: &str =
    include_str!("../../../../../menagerie/concept-shapes/catalog/index.json");
const EXAM_MANIFEST_CID: &str = libprovekit::exam_manifest::DEFAULT_EXAM_MANIFEST_CID;

static CONCEPT_OP_CIDS: OnceLock<BTreeMap<String, String>> = OnceLock::new();
static EXAM_MANIFEST: OnceLock<Option<ExamManifestMemento>> = OnceLock::new();

fn main() -> io::Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    eprintln!("provekit-walk-rpc listening on stdio (JSON-RPC 2.0, line-delimited)");
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let response = handle_line(&line);
        let response_str = serde_json::to_string(&response).unwrap_or_else(|e| {
            json!({"jsonrpc": "2.0", "error": {"code": -32603, "message": e.to_string()}})
                .to_string()
        });
        writeln!(stdout, "{}", response_str)?;
        stdout.flush()?;
    }
    Ok(())
}

fn handle_line(line: &str) -> Value {
    let req: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            return json!({
                "jsonrpc": "2.0",
                "error": { "code": -32700, "message": format!("parse error: {}", e) },
                "id": Value::Null,
            });
        }
    };
    let id = req.get("id").cloned().unwrap_or(Value::Null);
    let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let params = req.get("params").cloned().unwrap_or(json!({}));

    let result = match method {
        // Bind-IR lift surface (PEP 1.7.0 `kind = "lift"` over the legacy-retained
        // `initialize`/`lift`/`shutdown` JSON-RPC shape per
        // `2026-04-30-lift-plugin-protocol.md`). cmd_bind dispatches Verb 1 here.
        "initialize" => Ok(initialize_result()),
        "lift" => bind_lift(&params),
        "shutdown" => Ok(Value::Null),
        // Walk-internal RPC (substrate-private; not part of any plugin protocol).
        "walk.lift_pre" => lift_pre(&params),
        "walk.lift_post" => lift_post(&params),
        "walk.contract" => contract(&params),
        "walk.term" => term(&params),
        "walk.shadow_source" => shadow_source(&params),
        "walk.proof_ir" => proof_ir(&params),
        _ => Err(format!("unknown method: {}", method)),
    };

    match result {
        Ok(value) => json!({
            "jsonrpc": "2.0",
            "result": value,
            "id": id,
        }),
        Err(message) => json!({
            "jsonrpc": "2.0",
            "error": { "code": -32602, "message": message },
            "id": id,
        }),
    }
}

fn lift_pre(params: &Value) -> Result<Value, String> {
    let src = params
        .get("src")
        .and_then(|v| v.as_str())
        .ok_or("missing `src`")?;
    let fn_name = params
        .get("fn_name")
        .and_then(|v| v.as_str())
        .ok_or("missing `fn_name`")?;
    let item = parse_fn(src, fn_name)?;
    let pre = lift_function_precondition(&item);
    serde_json::to_value(pre.as_formula()).map_err(|e| e.to_string())
}

fn lift_post(params: &Value) -> Result<Value, String> {
    let src = params
        .get("src")
        .and_then(|v| v.as_str())
        .ok_or("missing `src`")?;
    let fn_name = params
        .get("fn_name")
        .and_then(|v| v.as_str())
        .ok_or("missing `fn_name`")?;
    let item = parse_fn(src, fn_name)?;
    let post = lift_function_postcondition(&item);
    serde_json::to_value(post.as_formula()).map_err(|e| e.to_string())
}

fn contract(params: &Value) -> Result<Value, String> {
    let src = params
        .get("src")
        .and_then(|v| v.as_str())
        .ok_or("missing `src`")?;
    let fn_name = params
        .get("fn_name")
        .and_then(|v| v.as_str())
        .ok_or("missing `fn_name`")?;
    let file = params.get("file").and_then(|v| v.as_str());
    let item = parse_fn(src, fn_name)?;
    let contract = build_function_contract_with_file(&item, None, file);
    serde_json::from_slice(&contract.canonical_bytes).map_err(|e| e.to_string())
}

fn term(params: &Value) -> Result<Value, String> {
    let src = params
        .get("src")
        .and_then(|v| v.as_str())
        .ok_or("missing `src`")?;
    let fn_name = params
        .get("fn_name")
        .and_then(|v| v.as_str())
        .ok_or("missing `fn_name`")?;
    let source = params
        .get("source")
        .and_then(|v| v.as_str())
        .unwrap_or("<rpc>");
    let item = parse_fn(src, fn_name)?;
    let bytes = rust_function_term_json(&item, source)?;
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

fn shadow_source(params: &Value) -> Result<Value, String> {
    let (s, _bytes) = build_bundle(params)?;
    Ok(json!({
        "cid": s.cid,
        "fn_name": s.fn_name,
        "slots": s.slots.len(),
        "arrivals_total": s.slots.iter().map(|sl| sl.arrivals.len()).sum::<usize>(),
        "bundle_cid": shadow_proof_ir_cid(&s),
    }))
}

fn proof_ir(params: &Value) -> Result<Value, String> {
    let (s, _bytes) = build_bundle(params)?;
    let bytes = shadow_to_proof_ir(&s);
    let cid = shadow_proof_ir_cid(&s);
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(json!({
        "cid": cid,
        "bytes_b64": b64,
        "length": bytes.len(),
        "shadow_source_cid": s.cid,
    }))
}

fn build_bundle(params: &Value) -> Result<(provekit_walk::ShadowSource, Vec<u8>), String> {
    let src = params
        .get("src")
        .and_then(|v| v.as_str())
        .ok_or("missing `src`")?;
    let callee_name = params
        .get("callee")
        .and_then(|v| v.as_str())
        .ok_or("missing `callee`")?;
    let caller_name = params
        .get("caller")
        .and_then(|v| v.as_str())
        .ok_or("missing `caller`")?;
    let callee_fn = parse_fn(src, callee_name)?;
    let caller_fn = parse_fn(src, caller_name)?;
    let pre = lift_function_precondition(&callee_fn);
    let formal_params = all_param_names(&callee_fn);
    let s = build_shadow_source(
        &caller_fn,
        &[CalleeContract {
            callee_name: callee_name.to_string(),
            formal_params,
            precondition: pre,
        }],
    );
    Ok((s, Vec::new()))
}

fn parse_fn(src: &str, name: &str) -> Result<syn::ItemFn, String> {
    let file: syn::File = syn::parse_str(src).map_err(|e| format!("parse error: {}", e))?;
    file.items
        .into_iter()
        .find_map(|item| match item {
            syn::Item::Fn(f) if f.sig.ident == name => Some(f),
            _ => None,
        })
        .ok_or_else(|| format!("function `{}` not found", name))
}

fn all_param_names(item_fn: &syn::ItemFn) -> Vec<String> {
    // Must return length == arity. Non-Ident patterns get a stable placeholder.
    item_fn
        .sig
        .inputs
        .iter()
        .enumerate()
        .map(|(i, arg)| match arg {
            syn::FnArg::Receiver(_) => "__self".to_string(),
            syn::FnArg::Typed(pt) => match &*pt.pat {
                syn::Pat::Ident(p) => p.ident.to_string(),
                _ => format!("__arg{}", i),
            },
        })
        .collect()
}

// ============================================================================
// Bind-IR lift surface (PEP 1.7.0 kind = "lift")
//
// Implements the legacy-retained `initialize` / `lift` / `shutdown` JSON-RPC
// shape from `2026-04-30-lift-plugin-protocol.md`, returning bind-lift entries
// per `2026-05-13-bind-ir-lift-result.md`. cmd_bind dispatches Verb 1 (Lift)
// to this surface so the bind pipeline carries zero language knowledge in
// the CLI core.
// ============================================================================

fn initialize_result() -> Value {
    json!({
        "name": "provekit-walk-rpc",
        "version": env!("CARGO_PKG_VERSION"),
        "protocol_version": "pep/1.7.0",
        "capabilities": {
            "authoring_surfaces": ["rust", "rust-bind"],
            "ir_version": "bind-ir/1.0.0",
            "emits_signed_mementos": false
        }
    })
}

fn bind_lift(params: &Value) -> Result<Value, String> {
    let workspace_root = params
        .get("workspace_root")
        .and_then(|v| v.as_str())
        .ok_or("missing `workspace_root`")?;
    let source_paths: Vec<String> = params
        .get("source_paths")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_else(|| vec![".".to_string()]);

    let root = PathBuf::from(workspace_root);
    let mut entries: Vec<Value> = Vec::new();
    let mut diagnostics: Vec<Value> = Vec::new();
    let library_bindings = LibraryBindingLookup::load(&root)?;

    let scan_roots: Vec<PathBuf> = if source_paths.is_empty() {
        vec![root.clone()]
    } else {
        source_paths
            .iter()
            .map(|p| {
                let candidate = root.join(p);
                if candidate.is_dir() {
                    candidate
                } else {
                    root.clone()
                }
            })
            .collect()
    };

    let mut visited: std::collections::BTreeSet<PathBuf> = Default::default();
    for scan_root in &scan_roots {
        let src_dir = scan_root.join("src");
        let walk_root: &Path = if src_dir.is_dir() {
            &src_dir
        } else {
            scan_root.as_path()
        };
        collect_rs_files(walk_root, &mut visited);
        // Also include top-level *.rs files when scan_root != walk_root.
        if walk_root != scan_root.as_path() {
            collect_rs_files_shallow(scan_root.as_path(), &mut visited);
        }
    }

    for path in &visited {
        let src = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                diagnostics.push(cited_diagnostic(
                    "read-error",
                    path.display().to_string(),
                    e.to_string(),
                    "morphism",
                    "concept:source-unit",
                    "rust",
                ));
                continue;
            }
        };
        let file = match syn::parse_file(&src) {
            Ok(f) => f,
            Err(e) => {
                diagnostics.push(cited_diagnostic(
                    "parse-error",
                    path.display().to_string(),
                    e.to_string(),
                    "morphism",
                    "concept:source-unit",
                    "rust",
                ));
                continue;
            }
        };
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .display()
            .to_string()
            .replace('\\', "/");
        let witnesses_by_symbol =
            contract_witnesses_by_function_symbol(&file, &rel, src.as_bytes());

        for target in collect_bind_lift_targets_with_source(&file, &src) {
            let item_fn = &target.item_fn;
            let fn_name = &target.fn_name;
            let comment_shapes = comment_shapes_for_fn_source(&src, &target.source_name);
            let comment_count = comment_shapes.len();
            let base_term_shape = term_shape_for_fn(item_fn);
            let base_has_operator = !is_non_operation_shape(&base_term_shape);
            let term_shape = term_shape_with_comments(base_term_shape, comment_shapes);
            let term_shape_cid = blake3_512_of(encode_jcs(&term_shape).as_bytes());
            let operand_bindings = operand_bindings_for_fn_with_comment_prefix(
                item_fn,
                comment_count,
                base_has_operator,
            );
            let param_names = fn_param_names(item_fn);
            let function_symbol = format!("{fn_name}@{rel}");
            let fallback_function_symbol = format!("{}@{rel}", target.source_name);
            let witnesses = witnesses_by_symbol
                .get(&function_symbol)
                .or_else(|| witnesses_by_symbol.get(&fallback_function_symbol))
                .cloned()
                .unwrap_or_default();

            let mut entry = json!({
                "kind": "bind-lift-entry",
                "param_names": param_names,
                "term_shape": cvalue_to_json(&term_shape),
                "term_shape_cid": term_shape_cid,
                "operand_bindings": operand_bindings,
                "source_function_name": target.source_name,
                "witnesses": witnesses,
            });
            if let Some(concept_annotation) = &target.concept_annotation {
                entry["concept_annotation"] = json!(concept_annotation);
            }
            entries.push(entry);

            for callsite in collect_bound_library_calls(item_fn, &library_bindings) {
                let term_shape =
                    concept_citation_shape(&callsite.pattern.concept_cid, &callsite.resolved_args);
                let term_shape_cid = blake3_512_of(encode_jcs(&term_shape).as_bytes());
                entries.push(json!({
                    "kind": "bind-lift-entry",
                    "param_names": callsite.resolved_args,
                    "term_shape": cvalue_to_json(&term_shape),
                    "term_shape_cid": term_shape_cid,
                    "witnesses": [],
                }));
            }
        }
    }

    Ok(json!({
        "kind": "ir-document",
        "ir": entries,
        "diagnostics": diagnostics,
    }))
}

fn cited_diagnostic(
    kind: &str,
    path: String,
    detail: String,
    question_kind: &str,
    concept: &str,
    language: &str,
) -> Value {
    let mut diagnostic = json!({
        "kind": kind,
        "path": path,
        "detail": detail,
    });
    if let Some(question_cid) = exam_question_cid_for(question_kind, concept, language) {
        diagnostic["exam_manifest_cid"] = json!(EXAM_MANIFEST_CID);
        diagnostic["exam_question_cid"] = json!(question_cid);
    } else {
        diagnostic["exam_citation_diagnostic"] = json!({
            "kind": "exam-question-citation-missing",
            "question_kind": question_kind,
            "concept": concept,
            "language": language,
        });
    }
    diagnostic
}

fn exam_question_cid_for(kind: &str, concept: &str, language: &str) -> Option<String> {
    let manifest =
        EXAM_MANIFEST.get_or_init(|| libprovekit::exam_manifest::load_default_exam_manifest().ok());
    manifest.as_ref().and_then(|manifest| {
        libprovekit::exam_manifest::exam_question_cid_for(manifest, kind, concept, language)
            .ok()
            .flatten()
    })
}

#[derive(Debug, Clone)]
struct BindLiftTarget {
    fn_name: String,
    source_name: String,
    concept_annotation: Option<String>,
    item_fn: syn::ItemFn,
}

fn collect_bind_lift_targets(file: &syn::File) -> Vec<BindLiftTarget> {
    collect_bind_lift_targets_with_source(file, "")
}

fn collect_bind_lift_targets_with_source(file: &syn::File, source: &str) -> Vec<BindLiftTarget> {
    let mut targets = Vec::new();
    collect_bind_lift_targets_in_items(&file.items, source, &mut targets);
    targets
}

fn collect_bind_lift_targets_in_items(
    items: &[syn::Item],
    source: &str,
    targets: &mut Vec<BindLiftTarget>,
) {
    for item in items {
        match item {
            syn::Item::Fn(item_fn) => {
                let fn_name = item_fn.sig.ident.to_string();
                targets.push(BindLiftTarget {
                    source_name: fn_name.clone(),
                    fn_name,
                    concept_annotation: concept_annotation_for_fn(source, item_fn),
                    item_fn: item_fn.clone(),
                });
            }
            syn::Item::Impl(impl_block) => {
                let Some(qualifier) = impl_function_qualifier(impl_block) else {
                    continue;
                };
                for impl_item in &impl_block.items {
                    let syn::ImplItem::Fn(method) = impl_item else {
                        continue;
                    };
                    if !is_liftable_impl_method(impl_block, method) {
                        continue;
                    }
                    let source_name = method.sig.ident.to_string();
                    let item_fn = item_fn_from_impl_method(method);
                    targets.push(BindLiftTarget {
                        fn_name: format!("{qualifier}::{source_name}"),
                        source_name,
                        concept_annotation: concept_annotation_for_fn(source, &item_fn),
                        item_fn,
                    });
                }
            }
            syn::Item::Mod(module) => {
                if let Some((_, nested_items)) = &module.content {
                    collect_bind_lift_targets_in_items(nested_items, source, targets);
                }
            }
            _ => {}
        }
    }
}

fn concept_annotation_for_fn(source: &str, item_fn: &syn::ItemFn) -> Option<String> {
    if source.trim().is_empty() {
        return None;
    }
    let fn_line = item_fn.sig.fn_token.span.start().line;
    concept_annotation_before_line(source, fn_line)
}

fn concept_annotation_before_line(source: &str, fn_line: usize) -> Option<String> {
    if fn_line <= 1 {
        return None;
    }
    let lines = source.lines().collect::<Vec<_>>();
    let mut idx = fn_line.checked_sub(2)?;
    loop {
        let trimmed = lines.get(idx)?.trim();
        if trimmed.is_empty() {
            return None;
        }
        if trimmed.starts_with("#[") {
            if idx == 0 {
                return None;
            }
            idx -= 1;
            continue;
        }
        if let Some(comment_body) = rust_line_comment_body(trimmed) {
            if let Some(name) = parse_concept_comment_body(comment_body) {
                return Some(name);
            }
            if idx == 0 {
                return None;
            }
            idx -= 1;
            continue;
        }
        return None;
    }
}

fn rust_line_comment_body(trimmed: &str) -> Option<&str> {
    trimmed
        .strip_prefix("///")
        .or_else(|| trimmed.strip_prefix("//"))
        .map(str::trim_start)
}

fn parse_concept_comment_body(body: &str) -> Option<String> {
    let raw = body.strip_prefix("concept:")?.trim();
    let token = raw.split_whitespace().next()?;
    let bare = token.strip_prefix("concept:").unwrap_or(token);
    if bare.is_empty() {
        None
    } else {
        Some(bare.to_string())
    }
}

fn is_liftable_impl_method(impl_block: &syn::ItemImpl, method: &syn::ImplItemFn) -> bool {
    impl_block.trait_.is_some() || matches!(method.vis, syn::Visibility::Public(_))
}

fn impl_function_qualifier(impl_block: &syn::ItemImpl) -> Option<String> {
    let self_ty = rust_symbol_surface(&impl_block.self_ty);
    if self_ty.is_empty() {
        return None;
    }
    match &impl_block.trait_ {
        Some((_, trait_path, _)) => {
            let trait_name = rust_symbol_surface(trait_path);
            if trait_name.is_empty() {
                Some(self_ty)
            } else {
                Some(format!("<{self_ty} as {trait_name}>"))
            }
        }
        None => Some(self_ty),
    }
}

fn item_fn_from_impl_method(method: &syn::ImplItemFn) -> syn::ItemFn {
    syn::ItemFn {
        attrs: method.attrs.clone(),
        vis: method.vis.clone(),
        sig: method.sig.clone(),
        block: Box::new(method.block.clone()),
    }
}

fn rust_symbol_surface(node: &impl quote::ToTokens) -> String {
    normalize_rust_symbol(&node.to_token_stream().to_string())
}

fn normalize_rust_symbol(raw: &str) -> String {
    let mut s = normalize_ws(raw);
    for (from, to) in [
        (" :: ", "::"),
        (" < ", "<"),
        (" >", ">"),
        (" <", "<"),
        (" ,", ","),
        (" & ", "&"),
        (" * ", "*"),
    ] {
        s = s.replace(from, to);
    }
    s
}

#[derive(Debug, Clone)]
struct LibraryBindingLookup {
    patterns: Vec<LibraryCallPattern>,
}

#[derive(Debug, Clone)]
struct LibraryCallPattern {
    concept_cid: String,
    callee: String,
    min_params: Option<usize>,
    max_params: Option<usize>,
}

#[derive(Debug, Clone)]
struct BoundLibraryCall {
    pattern: LibraryCallPattern,
    resolved_args: Vec<String>,
}

impl LibraryBindingLookup {
    fn load(workspace_root: &Path) -> Result<Self, String> {
        let config_path = workspace_root
            .join(".provekit")
            .join("library-bindings.json");
        if !config_path.is_file() {
            return Ok(Self {
                patterns: Vec::new(),
            });
        }
        let raw = std::fs::read_to_string(&config_path)
            .map_err(|e| format!("read {}: {e}", config_path.display()))?;
        let config: Value = serde_json::from_str(&raw)
            .map_err(|e| format!("parse {}: {e}", config_path.display()))?;
        let language = config
            .get("language")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| {
                format!(
                    "{} must contain non-empty string field `language`",
                    config_path.display()
                )
            })?;
        let bindings = config
            .get("bindings")
            .and_then(Value::as_object)
            .ok_or_else(|| {
                format!(
                    "{} must contain object field `bindings`",
                    config_path.display()
                )
            })?;

        let mut pairs = bindings.iter().collect::<Vec<_>>();
        pairs.sort_by(|a, b| a.0.cmp(b.0));
        let mut patterns = Vec::new();
        for (concept_name, surface_value) in pairs {
            if !concept_name.starts_with("concept:") {
                return Err(format!(
                    "{} binding key `{concept_name}` must start with `concept:`",
                    config_path.display()
                ));
            }
            let surface = surface_value.as_str().ok_or_else(|| {
                format!(
                    "{} binding `{concept_name}` must be a string library surface",
                    config_path.display()
                )
            })?;
            let (surface_language, library_tag) = split_library_surface(surface)?;
            if surface_language != language {
                return Err(format!(
                    "{} binding `{concept_name}` points to `{surface}` but top-level language is `{language}`",
                    config_path.display()
                ));
            }
            let concept_cid = concept_shape_cid(workspace_root, concept_name)?;
            let entries = body_template_entries(workspace_root, &surface_language, &library_tag)?;
            let mut matched = 0usize;
            for entry in entries
                .into_iter()
                .filter(|entry| concept_names_match(&entry.concept_name, concept_name))
            {
                for callee in infer_call_patterns_from_template(&entry.emission_template) {
                    matched += 1;
                    patterns.push(LibraryCallPattern {
                        concept_cid: concept_cid.clone(),
                        callee,
                        min_params: entry.min_params,
                        max_params: entry.max_params,
                    });
                }
            }
            if matched == 0 {
                return Err(format!(
                    "body template for `{surface}` contains no callable emission template for `{concept_name}`"
                ));
            }
        }
        Ok(Self { patterns })
    }

    fn match_call(&self, callee: &str, arity: usize) -> Option<&LibraryCallPattern> {
        let normalized = normalize_call_pattern(callee);
        self.patterns.iter().find(|pattern| {
            pattern.callee == normalized
                && pattern.min_params.is_none_or(|min| arity >= min)
                && pattern.max_params.is_none_or(|max| arity <= max)
        })
    }
}

#[derive(Debug)]
struct BodyTemplateCallEntry {
    concept_name: String,
    emission_template: String,
    min_params: Option<usize>,
    max_params: Option<usize>,
}

fn split_library_surface(surface: &str) -> Result<(String, String), String> {
    let Some((language, tag)) = surface.split_once('-') else {
        return Err(format!(
            "library surface `{surface}` must look like `<language>-<library>`"
        ));
    };
    if language.is_empty() || tag.is_empty() {
        return Err(format!(
            "library surface `{surface}` must have non-empty language and library"
        ));
    }
    Ok((language.to_string(), tag.to_string()))
}

fn body_template_entries(
    workspace_root: &Path,
    language: &str,
    library_tag: &str,
) -> Result<Vec<BodyTemplateCallEntry>, String> {
    let rel = PathBuf::from("menagerie")
        .join(format!("{language}-language-signature"))
        .join("specs")
        .join("body-templates")
        .join(format!("{language}-canonical-bodies-{library_tag}.json"));
    let path = find_repo_file(workspace_root, &rel)
        .ok_or_else(|| format!("missing body template {}", rel.display()))?;
    let raw =
        std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let root: Value =
        serde_json::from_str(&raw).map_err(|e| format!("parse {}: {e}", path.display()))?;
    let entries = root
        .pointer("/header/content/entries")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{} missing /header/content/entries", path.display()))?;
    Ok(entries
        .iter()
        .filter_map(|entry| {
            let concept_name = entry.get("concept_name")?.as_str()?.to_string();
            let emission_template = entry
                .get("emission_template")?
                .get("template")?
                .as_str()?
                .to_string();
            let guard = entry.get("signature_guard");
            Some(BodyTemplateCallEntry {
                concept_name,
                emission_template,
                min_params: guard
                    .and_then(|g| g.get("min_params"))
                    .and_then(Value::as_u64)
                    .map(|n| n as usize),
                max_params: guard
                    .and_then(|g| g.get("max_params"))
                    .and_then(Value::as_u64)
                    .map(|n| n as usize),
            })
        })
        .collect())
}

fn concept_shape_cid(workspace_root: &Path, concept_name: &str) -> Result<String, String> {
    let rel = Path::new("menagerie")
        .join("concept-shapes")
        .join("cids.tsv");
    let path =
        find_repo_file(workspace_root, &rel).ok_or_else(|| format!("missing {}", rel.display()))?;
    let raw =
        std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    for line in raw.lines() {
        let cols = line.split('\t').collect::<Vec<_>>();
        if cols.len() >= 3 && cols[0] == "shape" && cols[1] == concept_name {
            return Ok(cols[2].to_string());
        }
    }
    Err(format!(
        "{} has no shape CID entry for `{concept_name}`",
        path.display()
    ))
}

fn find_repo_file(workspace_root: &Path, relative: &Path) -> Option<PathBuf> {
    let mut bases = vec![workspace_root.to_path_buf()];
    if let Some(root) = std::env::var_os("PROVEKIT_REPO_ROOT") {
        bases.push(PathBuf::from(root));
    }
    if let Ok(cwd) = std::env::current_dir() {
        bases.extend(cwd.ancestors().map(Path::to_path_buf));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            bases.extend(parent.ancestors().map(Path::to_path_buf));
        }
    }
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    bases.extend(manifest_dir.ancestors().map(Path::to_path_buf));

    for base in bases {
        let candidate = base.join(relative);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn infer_call_patterns_from_template(template: &str) -> Vec<String> {
    let bytes = template.as_bytes();
    let mut out = Vec::new();
    for (idx, ch) in template.char_indices() {
        if ch != '(' {
            continue;
        }
        let mut start = idx;
        while start > 0 {
            let prev = bytes[start - 1] as char;
            if prev.is_ascii_alphanumeric() || matches!(prev, '_' | '.' | ':') {
                start -= 1;
            } else {
                break;
            }
        }
        let token = &template[start..idx];
        if token.contains('.') || token.contains("::") {
            let normalized = normalize_call_pattern(token);
            if !normalized.is_empty() && !out.contains(&normalized) {
                out.push(normalized);
            }
        }
    }
    out
}

fn normalize_call_pattern(raw: &str) -> String {
    raw.trim()
        .replace("::", ".")
        .split_whitespace()
        .collect::<String>()
}

fn concept_names_match(entry_name: &str, requested: &str) -> bool {
    entry_name == requested
        || entry_name
            .strip_prefix("concept:")
            .is_some_and(|name| name == requested)
        || requested
            .strip_prefix("concept:")
            .is_some_and(|name| name == entry_name)
}

fn collect_bound_library_calls(
    item_fn: &syn::ItemFn,
    bindings: &LibraryBindingLookup,
) -> Vec<BoundLibraryCall> {
    if bindings.patterns.is_empty() {
        return Vec::new();
    }
    let mut collector = BoundCallCollector {
        bindings,
        calls: Vec::new(),
    };
    syn::visit::Visit::visit_item_fn(&mut collector, item_fn);
    collector.calls
}

struct BoundCallCollector<'a> {
    bindings: &'a LibraryBindingLookup,
    calls: Vec<BoundLibraryCall>,
}

impl<'ast> syn::visit::Visit<'ast> for BoundCallCollector<'_> {
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let Some(callee) = callee_name_from_expr(&node.func) {
            self.record_call(&callee, &node.args);
        }
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        let receiver = expr_surface(&node.receiver);
        let callee = format!("{receiver}.{}", node.method);
        self.record_call(&callee, &node.args);
        syn::visit::visit_expr_method_call(self, node);
    }
}

impl BoundCallCollector<'_> {
    fn record_call(
        &mut self,
        callee: &str,
        args: &syn::punctuated::Punctuated<syn::Expr, syn::token::Comma>,
    ) {
        let arity = args.len();
        let Some(pattern) = self.bindings.match_call(callee, arity) else {
            return;
        };
        let resolved_args = args.iter().map(expr_surface).collect::<Vec<_>>();
        self.calls.push(BoundLibraryCall {
            pattern: pattern.clone(),
            resolved_args,
        });
    }
}

fn callee_name_from_expr(expr: &syn::Expr) -> Option<String> {
    match expr {
        syn::Expr::Path(path) => Some(
            path.path
                .segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .collect::<Vec<_>>()
                .join("."),
        ),
        _ => Some(expr_surface(expr)),
    }
}

fn expr_surface(expr: &syn::Expr) -> String {
    use quote::ToTokens;
    normalize_ws(&expr.to_token_stream().to_string())
}

fn concept_citation_shape(concept_cid: &str, resolved_args: &[String]) -> std::sync::Arc<CValue> {
    CValue::object([
        (
            "args",
            CValue::array(
                resolved_args
                    .iter()
                    .map(|arg| CValue::string(arg.clone()))
                    .collect(),
            ),
        ),
        ("concept_cid", CValue::string(concept_cid.to_string())),
        ("kind", CValue::string("concept-citation")),
    ])
}

fn contract_witnesses_by_function_symbol(
    file: &syn::File,
    rel: &str,
    source_bytes: &[u8],
) -> BTreeMap<String, Vec<Value>> {
    let mut by_symbol: BTreeMap<String, Vec<(String, Value)>> = BTreeMap::new();
    for evidence in lift_file_with_docstring_evidence(file, rel, source_bytes)
        .evidences
        .into_iter()
    {
        let Some(symbol) = evidence_function_symbol(&evidence) else {
            continue;
        };
        let Some(witness) = bind_contract_witness_from_evidence(&evidence) else {
            continue;
        };
        by_symbol
            .entry(symbol.to_string())
            .or_default()
            .push((evidence.cid.clone(), witness));
    }

    by_symbol
        .into_iter()
        .map(|(symbol, mut witnesses)| {
            witnesses.sort_by(|a, b| a.0.cmp(&b.0));
            (
                symbol,
                witnesses.into_iter().map(|(_, witness)| witness).collect(),
            )
        })
        .collect()
}

fn evidence_function_symbol(evidence: &EvidenceMemento) -> Option<&str> {
    evidence
        .extension_fields
        .get("function_symbol")
        .and_then(|value| value.as_str())
}

fn bind_contract_witness_from_evidence(evidence: &EvidenceMemento) -> Option<Value> {
    let role = evidence_role(evidence)?;
    let source_kind: String = evidence.source_kind.clone().into();
    let predicate = serde_json::to_value(&evidence.predicate).ok()?;
    let predicate_text = evidence
        .extension_fields
        .get("raw_text")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| ir_formula_to_text(&evidence.predicate));
    let mut extension_fields = evidence.extension_fields.clone();
    extension_fields.remove("role");

    Some(json!({
        "role": role,
        "predicate": predicate,
        "predicate_text": predicate_text,
        "source_kind": source_kind,
        "confidence_basis_points": evidence.confidence_basis_points,
        "extension_fields": extension_fields,
    }))
}

fn evidence_role(evidence: &EvidenceMemento) -> Option<String> {
    if let Some(role) = evidence
        .extension_fields
        .get("role")
        .and_then(|value| value.as_str())
        .filter(|role| !role.trim().is_empty())
    {
        return Some(role.trim().to_string());
    }

    match &evidence.source_kind {
        SourceKind::Docstring => evidence
            .extension_fields
            .get("pattern_kind")
            .and_then(|value| value.as_str())
            .and_then(|pattern_kind| match pattern_kind {
                "requires" | "arguments_must_be" => Some("pre".to_string()),
                "returns_if" => Some("post".to_string()),
                "panics_if" => Some("panic".to_string()),
                _ => None,
            }),
        SourceKind::TypeSignature => evidence
            .extension_fields
            .get("signature_position")
            .and_then(|value| value.as_str())
            .map(|position| {
                if position == "return" {
                    "post".to_string()
                } else {
                    "pre".to_string()
                }
            }),
        _ => None,
    }
}

fn ir_formula_to_text(formula: &IrFormula) -> String {
    match formula {
        IrFormula::Atomic { name, args } if args.is_empty() => name.clone(),
        IrFormula::Atomic { name, args } => {
            let args = args
                .iter()
                .map(ir_term_to_text)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{name}({args})")
        }
        IrFormula::And { operands } => join_formula_operands(operands, " && "),
        IrFormula::Or { operands } => join_formula_operands(operands, " || "),
        IrFormula::Not { operands } => {
            let inner = join_formula_operands(operands, ", ");
            format!("!({inner})")
        }
        IrFormula::Implies { operands } => join_formula_operands(operands, " -> "),
        IrFormula::Forall { name, body, .. } => {
            format!("forall {name}. {}", ir_formula_to_text(body))
        }
        IrFormula::Exists { name, body, .. } => {
            format!("exists {name}. {}", ir_formula_to_text(body))
        }
        IrFormula::Choice { var_name, body, .. } => {
            format!("choice {var_name}. {}", ir_formula_to_text(body))
        }
        IrFormula::Substitute { target, var, term } => {
            format!(
                "{}[{} := {}]",
                ir_formula_to_text(target),
                var,
                ir_term_to_text(term)
            )
        }
        IrFormula::Apply { r#fn, args } => {
            let args = args
                .iter()
                .map(ir_formula_to_text)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}({args})", r#fn)
        }
        IrFormula::DivergenceBetween { source, target } => format!(
            "divergence_between({}, {})",
            ir_formula_to_text(source),
            ir_formula_to_text(target)
        ),
    }
}

fn join_formula_operands(operands: &[IrFormula], separator: &str) -> String {
    operands
        .iter()
        .map(|operand| format!("({})", ir_formula_to_text(operand)))
        .collect::<Vec<_>>()
        .join(separator)
}

fn ir_term_to_text(term: &IrTerm) -> String {
    match term {
        IrTerm::Var { name } => name.clone(),
        // Human-readable compatibility text for predicate_text only. The
        // authoritative predicate remains the structured IrFormula above.
        IrTerm::Const { value, .. } => match value {
            Value::String(s) => format!("{s:?}"),
            _ => value.to_string(),
        },
        IrTerm::Ctor { name, args } => {
            let args = args
                .iter()
                .map(ir_term_to_text)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{name}({args})")
        }
        IrTerm::Lambda {
            param_name, body, ..
        } => {
            format!("lambda {param_name}. {}", ir_term_to_text(body))
        }
        IrTerm::Let { bindings, body } => {
            let bindings = bindings
                .iter()
                .map(|binding| {
                    format!(
                        "{} = {}",
                        binding.name,
                        ir_term_to_text(&binding.bound_term)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("let {bindings} in {}", ir_term_to_text(body))
        }
    }
}

fn collect_rs_files(dir: &Path, visited: &mut std::collections::BTreeSet<PathBuf>) {
    if !dir.is_dir() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, visited);
        } else if path.extension().map(|x| x == "rs").unwrap_or(false) {
            visited.insert(path);
        }
    }
}

fn collect_rs_files_shallow(dir: &Path, visited: &mut std::collections::BTreeSet<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().map(|x| x == "rs").unwrap_or(false) {
            visited.insert(path);
        }
    }
}

fn fn_param_names(item_fn: &syn::ItemFn) -> Vec<String> {
    let mut names = Vec::new();
    for (i, arg) in item_fn.sig.inputs.iter().enumerate() {
        match arg {
            syn::FnArg::Receiver(_) => {
                names.push("__self".to_string());
            }
            syn::FnArg::Typed(pt) => {
                let name = match &*pt.pat {
                    syn::Pat::Ident(p) => p.ident.to_string(),
                    _ => format!("__arg{}", i),
                };
                names.push(name);
            }
        }
    }
    names
}

fn normalize_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_ws = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !prev_ws && !out.is_empty() {
                out.push(' ');
            }
            prev_ws = true;
        } else {
            out.push(c);
            prev_ws = false;
        }
    }
    out.trim().to_string()
}

fn comment_shapes_for_fn_source(src: &str, fn_name: &str) -> Vec<Arc<CValue>> {
    let Some(body) = function_body_source(src, fn_name) else {
        return Vec::new();
    };
    comment_surfaces_in_source(body)
        .into_iter()
        .filter_map(|surface| comment_shape(&surface))
        .collect()
}

fn comment_shape(surface: &str) -> Option<Arc<CValue>> {
    let op_cid = concept_op_cid("concept:comment")?;
    Some(CValue::object([
        (
            "args",
            CValue::array(vec![CValue::object([
                ("kind", CValue::string("literal")),
                ("value", CValue::string(surface.to_string())),
            ])]),
        ),
        ("concept_name", CValue::string("concept:comment")),
        ("op_cid", CValue::string(op_cid.to_string())),
    ]))
}

fn function_body_source<'a>(src: &'a str, fn_name: &str) -> Option<&'a str> {
    let start = src.find(&format!("fn {fn_name}"))?;
    let open = src[start..].find('{')? + start;
    let close = matching_brace(src, open)?;
    src.get(open + 1..close)
}

fn matching_brace(src: &str, open: usize) -> Option<usize> {
    let bytes = src.as_bytes();
    let mut depth = 0usize;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(i);
                }
                i += 1;
            }
            b'"' => i = skip_quoted(bytes, i, b'"'),
            b'\'' => i = skip_quoted(bytes, i, b'\''),
            b'/' if bytes.get(i + 1) == Some(&b'/') => i = skip_line_comment(bytes, i),
            b'/' if bytes.get(i + 1) == Some(&b'*') => i = skip_block_comment(bytes, i),
            _ => i += 1,
        }
    }
    None
}

fn comment_surfaces_in_source(src: &str) -> Vec<String> {
    let bytes = src.as_bytes();
    let mut surfaces = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => i = skip_quoted(bytes, i, b'"'),
            b'\'' => i = skip_quoted(bytes, i, b'\''),
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                let end = line_comment_end(bytes, i);
                if let Some(surface) = src.get(i..end).map(str::trim_end) {
                    if !is_provekit_comment_carrier(surface) {
                        surfaces.push(surface.to_string());
                    }
                }
                i = end;
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                let end = block_comment_end(bytes, i);
                if let Some(surface) = src.get(i..end).map(str::trim) {
                    if !is_provekit_comment_carrier(surface) {
                        surfaces.push(surface.to_string());
                    }
                }
                i = end;
            }
            _ => i += 1,
        }
    }
    surfaces
}

fn is_provekit_comment_carrier(surface: &str) -> bool {
    let mut payload = surface.trim();
    if let Some(rest) = payload.strip_prefix("//") {
        payload = rest.trim();
    } else if payload.starts_with("/*") && payload.ends_with("*/") {
        payload = payload[2..payload.len() - 2].trim();
    }
    [
        "provekit:concept:",
        "provekit:concept-payload-cid:",
        "provekit-concept:",
        "provekit-concept-payload-cid:",
        "provekit-contract:",
        "provekit-contract-payload-cid:",
    ]
    .iter()
    .any(|prefix| payload.starts_with(prefix))
}

fn skip_quoted(bytes: &[u8], start: usize, quote: u8) -> usize {
    let mut i = start + 1;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i = (i + 2).min(bytes.len());
        } else if bytes[i] == quote {
            return i + 1;
        } else {
            i += 1;
        }
    }
    bytes.len()
}

fn skip_line_comment(bytes: &[u8], start: usize) -> usize {
    line_comment_end(bytes, start)
}

fn line_comment_end(bytes: &[u8], start: usize) -> usize {
    let mut i = start + 2;
    while i < bytes.len() && bytes[i] != b'\n' {
        i += 1;
    }
    i
}

fn skip_block_comment(bytes: &[u8], start: usize) -> usize {
    block_comment_end(bytes, start)
}

fn block_comment_end(bytes: &[u8], start: usize) -> usize {
    let mut i = start + 2;
    while i + 1 < bytes.len() {
        if bytes[i] == b'*' && bytes[i + 1] == b'/' {
            return i + 2;
        }
        i += 1;
    }
    bytes.len()
}

fn term_shape_for_fn(item_fn: &syn::ItemFn) -> Arc<CValue> {
    let ctx = ShapeContext::for_fn(item_fn);
    shape_of_block(&item_fn.block, &ctx)
}

fn term_shape_with_comments(
    base_term_shape: Arc<CValue>,
    mut comment_shapes: Vec<Arc<CValue>>,
) -> Arc<CValue> {
    if !is_non_operation_shape(&base_term_shape) {
        comment_shapes.push(base_term_shape);
    }
    collapse_operation_shapes(comment_shapes)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OperandBinding {
    position: Vec<usize>,
    symbol: String,
}

#[derive(Debug, Clone, Default)]
struct BindingResult {
    has_operator: bool,
    bindings: Vec<OperandBinding>,
}

fn operand_bindings_for_fn(item_fn: &syn::ItemFn) -> Vec<Value> {
    let ctx = ShapeContext::for_fn(item_fn);
    let mut bindings = bindings_of_block(&item_fn.block, &ctx).bindings;
    bindings.sort_by(|left, right| left.position.cmp(&right.position));
    bindings
        .into_iter()
        .map(|binding| {
            json!({
                "position": binding.position,
                "symbol": binding.symbol,
            })
        })
        .collect()
}

fn operand_bindings_for_fn_with_comment_prefix(
    item_fn: &syn::ItemFn,
    comment_count: usize,
    base_has_operator: bool,
) -> Vec<Value> {
    let mut bindings = operand_bindings_for_fn(item_fn);
    if comment_count == 0 || !base_has_operator {
        return bindings;
    }
    for binding in &mut bindings {
        if let Some(position) = binding.get_mut("position").and_then(Value::as_array_mut) {
            position.insert(0, json!(comment_count));
        }
    }
    bindings
}

fn bindings_of_block(block: &syn::Block, ctx: &ShapeContext) -> BindingResult {
    let mut block_ctx = ctx.clone();
    let mut groups = Vec::new();
    for stmt in &block.stmts {
        let result = bindings_of_stmt(stmt, &block_ctx);
        if result.has_operator {
            groups.push(result.bindings);
        }
        update_context_from_stmt(stmt, &mut block_ctx);
    }
    collapse_binding_groups(groups)
}

fn collapse_binding_groups(groups: Vec<Vec<OperandBinding>>) -> BindingResult {
    match groups.len() {
        0 => BindingResult::default(),
        1 => BindingResult {
            has_operator: true,
            bindings: groups.into_iter().next().unwrap_or_default(),
        },
        _ => {
            let bindings = groups
                .into_iter()
                .enumerate()
                .flat_map(|(index, group)| prefix_bindings(group, index))
                .collect();
            BindingResult {
                has_operator: true,
                bindings,
            }
        }
    }
}

fn bindings_of_stmt(stmt: &syn::Stmt, ctx: &ShapeContext) -> BindingResult {
    match stmt {
        syn::Stmt::Expr(e, _) => bindings_of_expr(e, ctx),
        syn::Stmt::Local(local) => local
            .init
            .as_ref()
            .map(|init| bindings_of_expr(&init.expr, ctx))
            .unwrap_or_default(),
        _ => BindingResult::default(),
    }
}

fn bindings_of_expr(expr: &syn::Expr, ctx: &ShapeContext) -> BindingResult {
    match expr {
        syn::Expr::If(e) => {
            let else_bindings = e
                .else_branch
                .as_ref()
                .map(|(_, else_expr)| bindings_of_expr(else_expr, ctx))
                .unwrap_or_default();
            operation_binding_result(vec![
                bindings_of_expr(&e.cond, ctx),
                bindings_of_block(&e.then_branch, ctx),
                else_bindings,
            ])
        }
        syn::Expr::While(e) => operation_binding_result(vec![
            bindings_of_expr(&e.cond, ctx),
            bindings_of_block(&e.body, ctx),
        ]),
        syn::Expr::ForLoop(e) => operation_binding_result(vec![bindings_of_block(&e.body, ctx)]),
        syn::Expr::Return(e) => e
            .expr
            .as_ref()
            .map(|expr| bindings_of_expr(expr, ctx))
            .unwrap_or_default(),
        syn::Expr::Break(e) => {
            let args = e
                .expr
                .as_ref()
                .map(|expr| vec![bindings_of_expr(expr, ctx)])
                .unwrap_or_default();
            operation_binding_result(args)
        }
        syn::Expr::Continue(_) => BindingResult {
            has_operator: true,
            bindings: Vec::new(),
        },
        syn::Expr::Assign(e) => operation_binding_result(vec![
            bindings_of_expr(&e.left, ctx),
            bindings_of_expr(&e.right, ctx),
        ]),
        syn::Expr::Binary(e) => {
            if binary_operator_concept_name(&e.op).is_none() {
                return BindingResult::default();
            }
            operation_binding_result(vec![
                bindings_of_expr(&e.left, ctx),
                bindings_of_expr(&e.right, ctx),
            ])
        }
        syn::Expr::Unary(e) => {
            if unary_operator_concept_name(&e.op, expr_sort(&e.expr, ctx)).is_none() {
                return BindingResult::default();
            }
            operation_binding_result(vec![bindings_of_expr(&e.expr, ctx)])
        }
        syn::Expr::Call(e) => operation_binding_result(
            e.args
                .iter()
                .map(|arg| bindings_of_expr(arg, ctx))
                .collect::<Vec<_>>(),
        ),
        syn::Expr::MethodCall(e) => {
            let mut args = vec![bindings_of_expr(&e.receiver, ctx)];
            args.extend(e.args.iter().map(|arg| bindings_of_expr(arg, ctx)));
            operation_binding_result(args)
        }
        syn::Expr::Block(b) => bindings_of_block(&b.block, ctx),
        syn::Expr::Paren(e) => bindings_of_expr(&e.expr, ctx),
        syn::Expr::Group(e) => bindings_of_expr(&e.expr, ctx),
        _ => operand_symbol(expr)
            .map(|symbol| BindingResult {
                has_operator: false,
                bindings: vec![OperandBinding {
                    position: Vec::new(),
                    symbol,
                }],
            })
            .unwrap_or_default(),
    }
}

fn operation_binding_result(args: Vec<BindingResult>) -> BindingResult {
    let bindings = args
        .into_iter()
        .enumerate()
        .flat_map(|(index, arg)| prefix_bindings(arg.bindings, index))
        .collect();
    BindingResult {
        has_operator: true,
        bindings,
    }
}

fn prefix_bindings(bindings: Vec<OperandBinding>, prefix: usize) -> Vec<OperandBinding> {
    bindings
        .into_iter()
        .map(|mut binding| {
            binding.position.insert(0, prefix);
            binding
        })
        .collect()
}

fn operand_symbol(expr: &syn::Expr) -> Option<String> {
    match expr {
        syn::Expr::Path(path) => path.path.get_ident().map(|ident| ident.to_string()),
        syn::Expr::Lit(lit) => literal_symbol(&lit.lit),
        _ => None,
    }
}

fn literal_symbol(lit: &syn::Lit) -> Option<String> {
    match lit {
        syn::Lit::Bool(value) => Some(value.value().to_string()),
        syn::Lit::Int(value) => Some(value.base10_digits().to_string()),
        syn::Lit::Str(value) => Some(format!("{:?}", value.value())),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShapeSort {
    Bool,
    Int,
    Unit,
}

#[derive(Debug, Clone, Default)]
struct ShapeContext {
    vars: BTreeMap<String, ShapeSort>,
}

impl ShapeContext {
    fn for_fn(item_fn: &syn::ItemFn) -> Self {
        let mut ctx = Self::default();
        for arg in &item_fn.sig.inputs {
            let syn::FnArg::Typed(pat_type) = arg else {
                continue;
            };
            let syn::Pat::Ident(ident) = &*pat_type.pat else {
                continue;
            };
            ctx.set_local(ident.ident.to_string(), sort_from_type(&pat_type.ty));
        }
        ctx
    }

    fn set_local(&mut self, name: String, sort: Option<ShapeSort>) {
        if let Some(sort) = sort {
            self.vars.insert(name, sort);
        } else {
            self.vars.remove(&name);
        }
    }
}

fn shape_of_block(block: &syn::Block, ctx: &ShapeContext) -> Arc<CValue> {
    let mut block_ctx = ctx.clone();
    let mut shapes = Vec::with_capacity(block.stmts.len());
    for stmt in &block.stmts {
        shapes.push(shape_of_stmt(stmt, &block_ctx));
        update_context_from_stmt(stmt, &mut block_ctx);
    }
    collapse_operation_shapes(shapes)
}

fn update_context_from_stmt(stmt: &syn::Stmt, ctx: &mut ShapeContext) {
    let syn::Stmt::Local(local) = stmt else {
        return;
    };
    if let Some((name, sort)) = local_binding_sort(local, ctx) {
        ctx.set_local(name, sort);
    }
}

fn local_binding_sort(
    local: &syn::Local,
    ctx: &ShapeContext,
) -> Option<(String, Option<ShapeSort>)> {
    let inferred = local
        .init
        .as_ref()
        .and_then(|init| expr_sort(&init.expr, ctx));
    match &local.pat {
        syn::Pat::Ident(ident) => Some((ident.ident.to_string(), inferred)),
        syn::Pat::Type(pat_type) => {
            let syn::Pat::Ident(ident) = &*pat_type.pat else {
                return None;
            };
            Some((
                ident.ident.to_string(),
                sort_from_type(&pat_type.ty).or(inferred),
            ))
        }
        _ => None,
    }
}

fn shape_of_stmt(stmt: &syn::Stmt, ctx: &ShapeContext) -> Arc<CValue> {
    match stmt {
        syn::Stmt::Expr(e, _) => shape_of_expr(e, ctx),
        syn::Stmt::Local(local) => local
            .init
            .as_ref()
            .map(|init| shape_of_expr(&init.expr, ctx))
            .unwrap_or_else(non_operation_shape),
        _ => non_operation_shape(),
    }
}

fn shape_of_expr(expr: &syn::Expr, ctx: &ShapeContext) -> Arc<CValue> {
    match expr {
        syn::Expr::If(e) => {
            let else_shape = e
                .else_branch
                .as_ref()
                .map(|(_, else_expr)| shape_of_expr(else_expr, ctx))
                .unwrap_or_else(non_operation_shape);
            gamma_operation(
                "concept:conditional",
                vec![
                    shape_of_expr(&e.cond, ctx),
                    shape_of_block(&e.then_branch, ctx),
                    else_shape,
                ],
            )
        }
        syn::Expr::While(e) => gamma_operation(
            "concept:while",
            vec![shape_of_expr(&e.cond, ctx), shape_of_block(&e.body, ctx)],
        ),
        syn::Expr::ForLoop(e) => gamma_operation("concept:for", vec![shape_of_block(&e.body, ctx)]),
        syn::Expr::Return(e) => e
            .expr
            .as_ref()
            .map(|expr| shape_of_expr(expr, ctx))
            .unwrap_or_else(non_operation_shape),
        syn::Expr::Break(e) => {
            let args = e
                .expr
                .as_ref()
                .map(|expr| vec![shape_of_expr(expr, ctx)])
                .unwrap_or_default();
            gamma_operation("concept:break", args)
        }
        syn::Expr::Continue(_) => gamma_operation("concept:continue", Vec::new()),
        syn::Expr::Assign(e) => gamma_operation(
            "concept:assign",
            vec![shape_of_expr(&e.left, ctx), shape_of_expr(&e.right, ctx)],
        ),
        syn::Expr::Binary(e) => {
            let Some(concept_name) = binary_operator_concept_name(&e.op) else {
                return non_operation_shape();
            };
            gamma_operation(
                concept_name,
                vec![shape_of_expr(&e.left, ctx), shape_of_expr(&e.right, ctx)],
            )
        }
        syn::Expr::Unary(e) => {
            let Some(concept_name) = unary_operator_concept_name(&e.op, expr_sort(&e.expr, ctx))
            else {
                return non_operation_shape();
            };
            gamma_operation(concept_name, vec![shape_of_expr(&e.expr, ctx)])
        }
        syn::Expr::Call(e) => gamma_operation(
            "concept:call",
            e.args
                .iter()
                .map(|arg| shape_of_expr(arg, ctx))
                .collect::<Vec<_>>(),
        ),
        syn::Expr::MethodCall(e) => {
            let mut args = vec![shape_of_expr(&e.receiver, ctx)];
            args.extend(e.args.iter().map(|arg| shape_of_expr(arg, ctx)));
            gamma_operation("concept:call", args)
        }
        syn::Expr::Block(b) => shape_of_block(&b.block, ctx),
        syn::Expr::Paren(e) => shape_of_expr(&e.expr, ctx),
        syn::Expr::Group(e) => shape_of_expr(&e.expr, ctx),
        _ => non_operation_shape(),
    }
}

fn expr_sort(expr: &syn::Expr, ctx: &ShapeContext) -> Option<ShapeSort> {
    match expr {
        syn::Expr::Lit(lit) => match &lit.lit {
            syn::Lit::Bool(_) => Some(ShapeSort::Bool),
            syn::Lit::Int(_) => Some(ShapeSort::Int),
            _ => None,
        },
        syn::Expr::Path(path) => path
            .path
            .get_ident()
            .and_then(|ident| ctx.vars.get(&ident.to_string()).copied()),
        syn::Expr::Paren(paren) => expr_sort(&paren.expr, ctx),
        syn::Expr::Group(group) => expr_sort(&group.expr, ctx),
        syn::Expr::Block(block) => {
            block_tail_expr(&block.block).and_then(|expr| expr_sort(expr, ctx))
        }
        syn::Expr::Unary(unary) => match &unary.op {
            syn::UnOp::Neg(_) => {
                (expr_sort(&unary.expr, ctx) == Some(ShapeSort::Int)).then_some(ShapeSort::Int)
            }
            syn::UnOp::Not(_) => match expr_sort(&unary.expr, ctx) {
                Some(ShapeSort::Bool) => Some(ShapeSort::Bool),
                Some(ShapeSort::Int) => Some(ShapeSort::Int),
                _ => None,
            },
            _ => None,
        },
        syn::Expr::Binary(binary) => binary_result_sort(binary, ctx),
        _ => None,
    }
}

fn block_tail_expr(block: &syn::Block) -> Option<&syn::Expr> {
    match block.stmts.last()? {
        syn::Stmt::Expr(expr, None) => Some(expr),
        _ => None,
    }
}

fn binary_result_sort(binary: &syn::ExprBinary, ctx: &ShapeContext) -> Option<ShapeSort> {
    match &binary.op {
        syn::BinOp::Add(_)
        | syn::BinOp::Sub(_)
        | syn::BinOp::Mul(_)
        | syn::BinOp::Div(_)
        | syn::BinOp::Rem(_)
        | syn::BinOp::BitAnd(_)
        | syn::BinOp::BitOr(_)
        | syn::BinOp::BitXor(_)
        | syn::BinOp::Shl(_)
        | syn::BinOp::Shr(_) => {
            operands_have_sort(&binary.left, &binary.right, ctx, ShapeSort::Int)
                .then_some(ShapeSort::Int)
        }
        syn::BinOp::Eq(_)
        | syn::BinOp::Ne(_)
        | syn::BinOp::Lt(_)
        | syn::BinOp::Le(_)
        | syn::BinOp::Gt(_)
        | syn::BinOp::Ge(_) => operands_have_sort(&binary.left, &binary.right, ctx, ShapeSort::Int)
            .then_some(ShapeSort::Bool),
        syn::BinOp::And(_) | syn::BinOp::Or(_) => {
            operands_have_sort(&binary.left, &binary.right, ctx, ShapeSort::Bool)
                .then_some(ShapeSort::Bool)
        }
        _ => None,
    }
}

fn operands_have_sort(
    left: &syn::Expr,
    right: &syn::Expr,
    ctx: &ShapeContext,
    sort: ShapeSort,
) -> bool {
    expr_sort(left, ctx) == Some(sort) && expr_sort(right, ctx) == Some(sort)
}

fn sort_from_type(ty: &syn::Type) -> Option<ShapeSort> {
    match ty {
        syn::Type::Path(path) if path.qself.is_none() => {
            let ident = path.path.segments.last()?.ident.to_string();
            sort_from_type_name(&ident)
        }
        syn::Type::Paren(paren) => sort_from_type(&paren.elem),
        syn::Type::Group(group) => sort_from_type(&group.elem),
        syn::Type::Tuple(tuple) if tuple.elems.is_empty() => Some(ShapeSort::Unit),
        _ => None,
    }
}

fn sort_from_type_name(name: &str) -> Option<ShapeSort> {
    match name {
        "bool" => Some(ShapeSort::Bool),
        "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64" | "u128"
        | "usize" => Some(ShapeSort::Int),
        _ => None,
    }
}

fn collapse_operation_shapes(shapes: impl IntoIterator<Item = Arc<CValue>>) -> Arc<CValue> {
    let operations = shapes
        .into_iter()
        .filter(|shape| !is_non_operation_shape(shape))
        .collect::<Vec<_>>();
    match operations.as_slice() {
        [] => non_operation_shape(),
        [only] => only.clone(),
        _ => gamma_operation("concept:seq", operations),
    }
}

fn binary_operator_concept_name(op: &syn::BinOp) -> Option<&'static str> {
    match op {
        syn::BinOp::Add(_) | syn::BinOp::AddAssign(_) => Some("concept:add"),
        syn::BinOp::Sub(_) | syn::BinOp::SubAssign(_) => Some("concept:sub"),
        syn::BinOp::Mul(_) | syn::BinOp::MulAssign(_) => Some("concept:mul"),
        syn::BinOp::Div(_) | syn::BinOp::DivAssign(_) => Some("concept:div"),
        syn::BinOp::Rem(_) | syn::BinOp::RemAssign(_) => Some("concept:mod"),
        syn::BinOp::BitAnd(_) | syn::BinOp::BitAndAssign(_) => Some("concept:bitand"),
        syn::BinOp::BitOr(_) | syn::BinOp::BitOrAssign(_) => Some("concept:bitor"),
        syn::BinOp::BitXor(_) | syn::BinOp::BitXorAssign(_) => Some("concept:bitxor"),
        syn::BinOp::Shl(_) | syn::BinOp::ShlAssign(_) => Some("concept:shl"),
        syn::BinOp::Shr(_) | syn::BinOp::ShrAssign(_) => Some("concept:shr"),
        syn::BinOp::Eq(_) => Some("concept:eq"),
        syn::BinOp::Ne(_) => Some("concept:ne"),
        syn::BinOp::Lt(_) => Some("concept:lt"),
        syn::BinOp::Le(_) => Some("concept:le"),
        syn::BinOp::Gt(_) => Some("concept:gt"),
        syn::BinOp::Ge(_) => Some("concept:ge"),
        syn::BinOp::And(_) => Some("concept:and"),
        syn::BinOp::Or(_) => Some("concept:or"),
        _ => None,
    }
}

fn unary_operator_concept_name(
    op: &syn::UnOp,
    operand_sort: Option<ShapeSort>,
) -> Option<&'static str> {
    match op {
        syn::UnOp::Deref(_) => Some("concept:deref"),
        syn::UnOp::Not(_) => match operand_sort {
            Some(ShapeSort::Int) => Some("concept:bitnot"),
            _ => Some("concept:not"),
        },
        syn::UnOp::Neg(_) => Some("concept:neg"),
        _ => None,
    }
}

fn gamma_operation(concept_name: &str, args: Vec<Arc<CValue>>) -> Arc<CValue> {
    let Some(op_cid) = concept_op_cid(concept_name) else {
        return non_operation_shape();
    };
    CValue::object([
        ("args", CValue::array(args)),
        ("concept_name", CValue::string(concept_name.to_string())),
        ("op_cid", CValue::string(op_cid.to_string())),
    ])
}

fn concept_op_cid(concept_name: &str) -> Option<&'static str> {
    CONCEPT_OP_CIDS
        .get_or_init(load_concept_op_cids)
        .get(concept_name)
        .map(String::as_str)
}

fn load_concept_op_cids() -> BTreeMap<String, String> {
    let index: Value = serde_json::from_str(CONCEPT_SHAPES_CATALOG_INDEX_JSON)
        .expect("embedded concept-shapes catalog index is valid JSON");
    let mut cids = BTreeMap::new();
    let Some(entries) = index.get("entries").and_then(Value::as_object) else {
        return cids;
    };
    for (cid, meta) in entries {
        if meta.get("kind").and_then(Value::as_str) != Some("algorithm") {
            continue;
        }
        let Some(name) = meta.get("name").and_then(Value::as_str) else {
            continue;
        };
        if !name.starts_with("concept:") {
            continue;
        }
        let resolved_cid = meta.get("cid").and_then(Value::as_str).unwrap_or(cid);
        cids.insert(name.to_string(), resolved_cid.to_string());
    }
    cids
}

fn is_non_operation_shape(shape: &CValue) -> bool {
    encode_jcs(shape) == "{}"
}

fn non_operation_shape() -> Arc<CValue> {
    CValue::object(Vec::<(&str, Arc<CValue>)>::new())
}

/// Render a canonicalizer `Value` as `serde_json::Value` for the RPC response.
fn cvalue_to_json(v: &CValue) -> Value {
    let s = encode_jcs(v);
    serde_json::from_str(&s).unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use libprovekit::core::{bind_result_payload, bind_term_document, BindOptions, Term};
    use provekit_ir_types::Sort;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn bind_lift_erases_signature_types_from_bind_ir_entries() {
        let root = temp_workspace("bind_lift_type_erasure");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        fs::write(
            src_dir.join("lib.rs"),
            r#"
pub fn add(left: i64, right: i64) -> i64 {
    left + right
}
"#,
        )
        .expect("write source");

        let out = bind_lift(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": ["."]
        }))
        .expect("bind lift should succeed");

        let entries = out["ir"].as_array().expect("ir array");
        let entry = entries.first().expect("add entry");
        assert_no_forbidden_bind_lift_entry_fields(entry);
        assert_eq!(entry["param_names"], json!(["left", "right"]));
        assert!(entry.get("param_types").is_none());
        assert!(entry.get("return_type").is_none());
        assert!(
            entry["witnesses"]
                .as_array()
                .expect("witnesses array")
                .iter()
                .all(|witness| witness["source_kind"] != "type-signature"),
            "signature type evidence must not cross the bind lift boundary"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn bind_lift_marries_docstring_evidence_as_witnesses() {
        assert_eq!(
            initialize_result()["capabilities"]["ir_version"],
            "bind-ir/1.0.0",
            "the Rust kit must not advertise a schema bump ahead of sibling kits"
        );

        let root = temp_workspace("bind_lift_witnesses");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        fs::write(
            src_dir.join("lib.rs"),
            r#"
/// Requires amount > 0
/// Returns Some(...) if amount > 0
/// Panics if amount == 0
pub fn wrap_positive(amount: usize) -> Option<usize> {
    Some(amount)
}
"#,
        )
        .expect("write source");

        let out = bind_lift(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": ["."]
        }))
        .expect("bind lift should succeed");

        let entries = out["ir"].as_array().expect("ir array");
        let entry = entries.first().expect("wrap_positive entry");
        assert_no_forbidden_bind_lift_entry_fields(entry);
        let witnesses = entry["witnesses"].as_array().expect("witnesses array");

        assert!(
            witnesses.iter().any(|w| w["source_kind"] == "docstring"
                && w["role"] == "pre"
                && w["extension_fields"]["pattern_kind"] == "requires"),
            "expected Requires docstring evidence to be married as a pre witness: {witnesses:#?}"
        );
        assert!(
            witnesses.iter().any(|w| w["source_kind"] == "docstring"
                && w["role"] == "post"
                && w["extension_fields"]["pattern_kind"] == "returns_if"),
            "expected Returns docstring evidence to be married as a post witness: {witnesses:#?}"
        );
        assert!(
            witnesses.iter().any(|w| w["source_kind"] == "docstring"
                && w["role"] == "panic"
                && w["extension_fields"]["pattern_kind"] == "panics_if"),
            "expected Panics docstring evidence to be preserved under the panic role: {witnesses:#?}"
        );
        assert!(
            witnesses
                .iter()
                .all(|w| w["source_kind"] != "type-signature"),
            "type-signature witnesses must not cross the bind lift boundary: {witnesses:#?}"
        );
        assert!(
            witnesses.iter().all(|w| {
                w["extension_fields"]
                    .as_object()
                    .map(|fields| !fields.contains_key("role"))
                    .unwrap_or(false)
            }),
            "bind witness wire entries must keep role top-level only: {witnesses:#?}"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn bind_lift_applies_operator_library_binding_to_bound_callsites() {
        let root = temp_workspace("bind_lift_library_bindings");
        fs::create_dir_all(root.join(".provekit")).expect("create .provekit");
        fs::write(
            root.join(".provekit").join("library-bindings.json"),
            r#"{
  "language": "rust",
  "bindings": {
    "concept:http-request": "rust-reqwest"
  }
}
"#,
        )
        .expect("write library bindings");
        let template_dir = root
            .join("menagerie")
            .join("rust-language-signature")
            .join("specs")
            .join("body-templates");
        fs::create_dir_all(&template_dir).expect("create body-template dir");
        fs::write(
            template_dir.join("rust-canonical-bodies-reqwest.json"),
            r#"{
  "header": {
    "content": {
      "entries": [
        {
          "concept_name": "concept:http-request",
          "emission_template": {
            "kind": "verbatim",
            "template": "return reqwest::blocking::get(${param0});"
          },
          "signature_guard": {
            "min_params": 1,
            "max_params": 1
          }
        }
      ],
      "target_language": "rust",
      "template_name": "rust-canonical-bodies-reqwest"
    }
  }
}
"#,
        )
        .expect("write body template");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        fs::write(
            src_dir.join("lib.rs"),
            r#"
pub fn fetch_status(url: &str) -> i64 {
    reqwest::blocking::get(url)
}
"#,
        )
        .expect("write source");

        let out = bind_lift(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": ["."]
        }))
        .expect("bind lift should succeed");

        let entries = out["ir"].as_array().expect("ir array");
        let bound = entries
            .iter()
            .find(|entry| entry["term_shape"]["kind"] == "concept-citation")
            .expect("bound callsite entry");
        let expected_shape = CValue::object([
            ("args", CValue::array(vec![CValue::string("url")])),
            (
                "concept_cid",
                CValue::string("blake3-512:784dab96537ebae452cba5fdbcf88e07395d5e0634099055008d819f21d0fb51930fc29877afda069cdf0c1ec893fba5de47b025717fd024919c687381baee43"),
            ),
            ("kind", CValue::string("concept-citation")),
        ]);
        let expected_cid = blake3_512_of(encode_jcs(&expected_shape).as_bytes());

        assert!(
            bound.get("file").is_none(),
            "file key should be absent in γ canonical"
        );
        assert_no_forbidden_bind_lift_entry_fields(bound);
        assert_eq!(bound["param_names"], json!(["url"]));
        assert!(bound.get("param_types").is_none());
        assert!(bound.get("return_type").is_none());
        assert_eq!(bound["term_shape"], cvalue_to_json(&expected_shape));
        assert_eq!(bound["term_shape_cid"], expected_cid);

        let out_again = bind_lift(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": ["."]
        }))
        .expect("second bind lift should succeed");
        let bound_again = out_again["ir"]
            .as_array()
            .expect("second ir array")
            .iter()
            .find(|entry| entry["term_shape"]["kind"] == "concept-citation")
            .expect("second bound callsite entry");
        assert_eq!(bound_again["term_shape_cid"], bound["term_shape_cid"]);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn term_shape_simple_add_is_canonical_gamma_literal() {
        let shape = term_shape_json(
            r#"
pub fn add(x: i64, y: i64) -> i64 {
    x + y
}
"#,
        );

        assert_eq!(
            shape,
            json!({
                "args": [{}, {}],
                "concept_name": "concept:add",
                "op_cid": "blake3-512:95fc70e63a5550fd2e25142f13932919c59d085654ab387789c798886b0111c61d28fe533fc98b50df70eea9428a9af8aa75372c8b1c1deb3acc1a4094790468"
            })
        );
    }

    #[test]
    fn bind_lift_emits_operand_binding_sidecar_with_integer_positions() {
        let root = temp_workspace("bind_lift_operand_binding_sidecar");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        fs::write(
            src_dir.join("lib.rs"),
            r#"
pub fn add(x: i64, y: i64) -> i64 {
    x + y
}
"#,
        )
        .expect("write source");

        let out = bind_lift(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": ["."]
        }))
        .expect("bind lift should succeed");

        let entries = out["ir"].as_array().expect("ir array");
        let entry = entries.first().expect("add entry");
        assert_no_forbidden_bind_lift_entry_fields(entry);
        assert_eq!(entry["source_function_name"], json!("add"));
        assert_eq!(
            entry["operand_bindings"],
            json!([
                {"position": [0], "symbol": "x"},
                {"position": [1], "symbol": "y"},
            ])
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn bind_lift_line_comments_as_concept_comment_terms() {
        let root = temp_workspace("bind_lift_line_comments");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        fs::write(
            src_dir.join("lib.rs"),
            r#"
pub fn commented(value: i64) -> i64 {
    // first line comment
    let next = value + 1;
    // second line comment
    // third line comment
    next
}
"#,
        )
        .expect("write source");

        let out = bind_lift(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": ["."]
        }))
        .expect("bind lift should succeed");

        let entry = out["ir"]
            .as_array()
            .expect("ir array")
            .first()
            .expect("entry");
        assert_eq!(
            comment_surfaces(&entry["term_shape"]),
            vec![
                "// first line comment".to_string(),
                "// second line comment".to_string(),
                "// third line comment".to_string(),
            ]
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn bind_lift_excludes_concept_carrier_line_comments() {
        let root = temp_workspace("bind_lift_comment_carriers");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        fs::write(
            src_dir.join("lib.rs"),
            r#"
pub fn commented(value: i64) -> i64 {
    // provekit:concept:skip
    // provekit-concept: {}
    // provekit-concept-payload-cid: blake3-512:dead
    // ordinary line comment
    value
}
"#,
        )
        .expect("write source");

        let out = bind_lift(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": ["."]
        }))
        .expect("bind lift should succeed");

        let entry = out["ir"]
            .as_array()
            .expect("ir array")
            .first()
            .expect("entry");
        assert_eq!(
            comment_surfaces(&entry["term_shape"]),
            vec!["// ordinary line comment".to_string()]
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn bind_lift_block_comments_as_concept_comment_terms() {
        let root = temp_workspace("bind_lift_block_comments");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        fs::write(
            src_dir.join("lib.rs"),
            r#"
pub fn commented(value: i64) -> i64 {
    /* first block comment */
    let next = value + 1;
    /* second
       block comment */
    /* third block comment */
    next
}
"#,
        )
        .expect("write source");

        let out = bind_lift(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": ["."]
        }))
        .expect("bind lift should succeed");

        let entry = out["ir"]
            .as_array()
            .expect("ir array")
            .first()
            .expect("entry");
        assert_eq!(
            comment_surfaces(&entry["term_shape"]),
            vec![
                "/* first block comment */".to_string(),
                "/* second\n       block comment */".to_string(),
                "/* third block comment */".to_string(),
            ]
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn bind_lift_ignores_comment_markers_inside_strings() {
        let root = temp_workspace("bind_lift_comment_string_markers");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        fs::write(
            src_dir.join("lib.rs"),
            r#"
pub fn commented() -> &'static str {
    let text = "// not a comment /* still not a comment */";
    /* actual block */
    // actual line
    text
}
"#,
        )
        .expect("write source");

        let out = bind_lift(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": ["."]
        }))
        .expect("bind lift should succeed");

        let entry = out["ir"]
            .as_array()
            .expect("ir array")
            .first()
            .expect("entry");
        assert_eq!(
            comment_surfaces(&entry["term_shape"]),
            vec![
                "/* actual block */".to_string(),
                "// actual line".to_string()
            ]
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn term_shape_let_rhs_operator_is_canonical_gamma() {
        let shape = term_shape_json(
            r#"
pub fn add_via_let(a: i64, b: i64) -> i64 {
    let q = a + b;
    q
}
"#,
        );

        assert_eq!(
            shape,
            json!({
                "args": [{}, {}],
                "concept_name": "concept:add",
                "op_cid": "blake3-512:95fc70e63a5550fd2e25142f13932919c59d085654ab387789c798886b0111c61d28fe533fc98b50df70eea9428a9af8aa75372c8b1c1deb3acc1a4094790468"
            })
        );
        assert_no_forbidden_term_shape_fields(&shape);
    }

    #[test]
    fn term_shape_top_level_operator_matches_let_rhs_gamma() {
        let top_level = term_shape_json(
            r#"
pub fn f(a: i64, b: i64) -> i64 {
    a + b
}
"#,
        );
        let let_rhs = term_shape_json(
            r#"
pub fn f(a: i64, b: i64) -> i64 {
    let q = a + b;
    q
}
"#,
        );

        assert_eq!(top_level, let_rhs);
        assert_no_forbidden_term_shape_fields(&top_level);
    }

    #[test]
    fn safe_divide_then_double_emits_gamma_without_unnamed_concepts() {
        let shape = term_shape_json(
            r#"
pub fn safe_divide_then_double(num: i64, denom: i64) -> i64 {
    if denom == 0 {
        -1
    } else {
        let q = num / denom;
        if q < 0 { -1 } else { q * 2 }
    }
}
"#,
        );

        assert_no_forbidden_term_shape_fields(&shape);
        let mut concepts = Vec::new();
        collect_concept_names(&shape, &mut concepts);
        for expected in [
            "concept:conditional",
            "concept:eq",
            "concept:neg",
            "concept:div",
            "concept:lt",
            "concept:mul",
        ] {
            assert!(
                concepts.iter().any(|actual| actual == expected),
                "missing {expected} from canonical gamma shape {shape:#?}"
            );
        }
        assert!(
            !serde_json::to_string(&shape)
                .expect("shape stringifies")
                .contains("UNNAMED-CONCEPT"),
            "gamma shape must not contain unnamed concept wrappers: {shape:#?}"
        );
    }

    #[test]
    fn bind_lift_output_strips_source_locations_from_term_shape_and_lines() {
        let root = temp_workspace("bind_lift_source_location_strip");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        fs::write(
            src_dir.join("lib.rs"),
            r#"
pub fn add(x: i64, y: i64) -> i64 {
    x + y
}
"#,
        )
        .expect("write source");

        let out = bind_lift(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": ["."]
        }))
        .expect("bind lift should succeed");

        let entries = out["ir"].as_array().expect("ir array");
        let entry = entries.first().expect("add entry");
        assert_no_forbidden_bind_lift_entry_fields(entry);
        assert!(
            entry.get("fn_line").is_none(),
            "bind payload must not carry function source lines: {entry:#?}"
        );
        assert!(
            entry.get("file").is_none() || entry["file"] == "",
            "bind payload must not carry source paths: {entry:#?}"
        );
        assert_no_forbidden_term_shape_fields(&entry["term_shape"]);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn bind_lift_relifts_edited_concept_comment_into_named_substrate_binding() {
        let root = temp_workspace("bind_lift_concept_comment_lifecycle");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        fs::write(
            src_dir.join("lib.rs"),
            r#"
// concept: my-thing
pub fn shaped(x: i64) -> i64 {
    (x * 7) + 3
}
"#,
        )
        .expect("write source");

        let out = bind_lift(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": ["."]
        }))
        .expect("bind lift should succeed");

        let entries = out["ir"].as_array().expect("ir array");
        let entry = entries.first().expect("shaped entry");
        assert_eq!(entry["concept_annotation"], "my-thing");

        let named = bind_term_document(&out, &BindOptions::default())
            .expect("bind term document builds from relifted source");
        assert_eq!(named.terms[0].concept_name, "concept:my-thing");

        let original_term = Term::Const {
            value: out,
            sort: primitive_sort("LiftPluginResponse"),
        };
        let payload = bind_result_payload(original_term, &named).expect("bind payload builds");
        let payload_bytes =
            libprovekit::canonical::serializable_jcs(&payload).expect("payload canonicalizes");
        assert!(
            !payload_bytes.contains("concept_annotation"),
            "bind payload must not retain lift-side annotation scaffolding: {payload_bytes}"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn bind_lift_concept_comment_survives_adjacent_attrs_and_metadata_comments() {
        let root = temp_workspace("bind_lift_concept_comment_attrs");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        fs::write(
            src_dir.join("lib.rs"),
            r#"
// concept: attr-backed-name
// substrate-origin: annotation-lift
#[cfg_attr(any(), requires(x > 0))]
pub fn shaped(x: i64) -> i64 {
    x
}
"#,
        )
        .expect("write source");

        let out = bind_lift(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": ["."]
        }))
        .expect("bind lift should succeed");

        let entries = out["ir"].as_array().expect("ir array");
        let entry = entries.first().expect("shaped entry");
        assert_eq!(entry["concept_annotation"], "attr-backed-name");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn bind_lift_discrimination_ignores_concept_comment_separated_by_blank_line() {
        let entry = single_entry_for_source(
            "concept_comment_blank_line",
            r#"
// concept: too-far-away

pub fn shaped(x: i64) -> i64 {
    x
}
"#,
        );

        assert!(
            entry.get("concept_annotation").is_none(),
            "blank-separated concept comment must not bind: {entry:#?}"
        );
    }

    #[test]
    fn bind_lift_discrimination_ignores_inline_trailing_concept_comment() {
        let entry = single_entry_for_source(
            "concept_comment_inline",
            r#"
pub fn shaped(x: i64) -> i64 { // concept: inline-name
    x
}
"#,
        );

        assert!(
            entry.get("concept_annotation").is_none(),
            "inline trailing concept comment must not bind: {entry:#?}"
        );
    }

    #[test]
    fn bind_lift_discrimination_ignores_empty_concept_comment() {
        let entry = single_entry_for_source(
            "concept_comment_empty",
            r#"
// concept:
pub fn shaped(x: i64) -> i64 {
    x
}
"#,
        );

        assert!(
            entry.get("concept_annotation").is_none(),
            "empty concept comment must not bind: {entry:#?}"
        );
    }

    #[test]
    fn bind_lift_contract_attrs_do_not_enter_hashed_bind_payload() {
        let root = temp_workspace("bind_lift_contract_attrs_payload");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        fs::write(
            src_dir.join("lib.rs"),
            r#"
#[cfg_attr(any(), requires(x > 0))]
pub fn positive_id(x: i64) -> i64 {
    x
}
"#,
        )
        .expect("write source");

        let out = bind_lift(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": ["."]
        }))
        .expect("bind lift should succeed");

        let entries = out["ir"].as_array().expect("ir array");
        assert_eq!(entries.len(), 1);
        assert_no_forbidden_bind_lift_entry_fields(&entries[0]);

        let original_term = Term::Const {
            value: out,
            sort: primitive_sort("LiftPluginResponse"),
        };
        let named = bind_term_document(
            match &original_term {
                Term::Const { value, .. } => value,
                _ => unreachable!("test constructs const term"),
            },
            &BindOptions::default(),
        )
        .expect("bind term document builds");
        let payload = bind_result_payload(original_term, &named).expect("bind payload builds");
        let payload_bytes =
            libprovekit::canonical::serializable_jcs(&payload).expect("payload canonicalizes");

        // `fn_name` is allowed inside gap-record subtrees because TransportGapMemento's
        // schema (see protocol/specs/2026-05-14-transport-gap-and-partial-morphism-protocol.md)
        // names the gap by its own fn_name identifier (e.g., "gap:unknown:bind:...:wp-rule").
        // That is legitimate citation, not a contract-attr leak. Walk the JSON tree and
        // forbid `fn_name` everywhere EXCEPT under a `gapRecords` ancestor.
        for forbidden in [
            "attr_pre",
            "attr_post",
            "concept_annotation",
            "operand_bindings",
            "source_function_name",
        ] {
            assert!(
                !payload_bytes.contains(forbidden),
                "bind payload hashed bytes contain forbidden field `{forbidden}`: {payload_bytes}"
            );
        }
        let payload_json: Value =
            serde_json::from_str(&payload_bytes).expect("payload bytes parse back to JSON");
        assert_no_fn_name_outside_gap_records(&payload_json, &[]);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn unary_not_disambiguates_bool_logical_and_integer_bitwise() {
        let logical_shape = term_shape_json(
            r#"
pub fn logical_not() -> bool {
    !true
}
"#,
        );
        let bitwise_shape = term_shape_json(
            r#"
pub fn bitwise_not() -> i64 {
    !0i64
}
"#,
        );

        assert_eq!(logical_shape["concept_name"], "concept:not");
        assert_eq!(bitwise_shape["concept_name"], "concept:bitnot");
        assert_ne!(
            logical_shape["op_cid"], bitwise_shape["op_cid"],
            "logical not and bitwise not must resolve to distinct concept atoms"
        );
        assert_no_forbidden_term_shape_fields(&logical_shape);
        assert_no_forbidden_term_shape_fields(&bitwise_shape);
    }

    #[test]
    fn bind_lift_includes_public_impl_methods_from_canonicalizer_value() {
        let root = temp_workspace("bind_lift_impl_methods");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        fs::write(
            src_dir.join("value.rs"),
            include_str!("../../../provekit-canonicalizer/src/value.rs"),
        )
        .expect("write value.rs fixture");
        let file = syn::parse_file(include_str!("../../../provekit-canonicalizer/src/value.rs"))
            .expect("value fixture parses");
        let target_names = collect_bind_lift_targets(&file)
            .into_iter()
            .map(|target| target.fn_name)
            .collect::<Vec<_>>();

        let out = bind_lift(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": ["."]
        }))
        .expect("bind lift should succeed");

        let entries = out["ir"].as_array().expect("ir array");
        for entry in entries {
            assert_no_forbidden_bind_lift_entry_fields(entry);
        }

        for expected in [
            "Value::kind",
            "Value::null",
            "Value::boolean",
            "Value::integer",
            "Value::string",
            "Value::array",
            "Value::object",
        ] {
            assert!(
                target_names.iter().any(|name| name == expected),
                "missing impl method target {expected}; got {target_names:?}"
            );
        }
        assert!(
            entries.len() >= 7,
            "expected at least seven bind lift entries from value.rs impl methods, got {}",
            entries.len()
        );

        let _ = fs::remove_dir_all(root);
    }

    fn term_shape_json(src: &str) -> Value {
        let file = syn::parse_file(src).expect("fixture parses");
        let item_fn = file
            .items
            .iter()
            .find_map(|item| {
                if let syn::Item::Fn(item_fn) = item {
                    Some(item_fn)
                } else {
                    None
                }
            })
            .expect("fixture contains function");
        cvalue_to_json(&term_shape_for_fn(item_fn))
    }

    fn comment_surfaces(value: &Value) -> Vec<String> {
        let mut surfaces = Vec::new();
        collect_comment_surfaces(value, &mut surfaces);
        surfaces
    }

    fn collect_comment_surfaces(value: &Value, surfaces: &mut Vec<String>) {
        match value {
            Value::Object(object) => {
                if object.get("concept_name").and_then(Value::as_str) == Some("concept:comment") {
                    if let Some(surface) = object
                        .get("args")
                        .and_then(Value::as_array)
                        .and_then(|args| args.first())
                        .and_then(|arg| arg.get("value"))
                        .and_then(Value::as_str)
                    {
                        surfaces.push(surface.to_string());
                    }
                }
                for child in object.values() {
                    collect_comment_surfaces(child, surfaces);
                }
            }
            Value::Array(items) => {
                for child in items {
                    collect_comment_surfaces(child, surfaces);
                }
            }
            _ => {}
        }
    }

    fn assert_no_forbidden_term_shape_fields(value: &Value) {
        match value {
            Value::Object(object) => {
                for forbidden in [
                    "kind",
                    "op",
                    "file",
                    "line",
                    "column",
                    "fn_line",
                    "concept_annotation",
                    "attr_pre",
                    "attr_post",
                    "concept_citations",
                ] {
                    assert!(
                        !object.contains_key(forbidden),
                        "term_shape contains forbidden field `{forbidden}` in {value:#?}"
                    );
                }
                for child in object.values() {
                    assert_no_forbidden_term_shape_fields(child);
                }
            }
            Value::Array(values) => {
                for child in values {
                    assert_no_forbidden_term_shape_fields(child);
                }
            }
            _ => {}
        }
    }

    fn collect_concept_names(value: &Value, out: &mut Vec<String>) {
        match value {
            Value::Object(object) => {
                if let Some(name) = object.get("concept_name").and_then(Value::as_str) {
                    out.push(name.to_string());
                }
                for child in object.values() {
                    collect_concept_names(child, out);
                }
            }
            Value::Array(values) => {
                for child in values {
                    collect_concept_names(child, out);
                }
            }
            _ => {}
        }
    }

    fn assert_no_forbidden_bind_lift_entry_fields(value: &Value) {
        let object = value.as_object().expect("bind entry object");
        for forbidden in ["attr_pre", "attr_post", "concept_annotation", "fn_name"] {
            assert!(
                !object.contains_key(forbidden),
                "bind lift entry contains forbidden field `{forbidden}` in {value:#?}"
            );
        }
    }

    fn assert_no_fn_name_outside_gap_records(value: &Value, path: &[&str]) {
        let under_gap_records = path.iter().any(|segment| *segment == "gapRecords");
        match value {
            Value::Object(map) => {
                if !under_gap_records {
                    assert!(
                        !map.contains_key("fn_name"),
                        "bind payload contains forbidden `fn_name` outside gap-record context at path {path:?}: {value:#?}"
                    );
                }
                for (key, child) in map {
                    let mut next_path: Vec<&str> = path.to_vec();
                    next_path.push(key.as_str());
                    assert_no_fn_name_outside_gap_records(child, &next_path);
                }
            }
            Value::Array(items) => {
                for child in items {
                    assert_no_fn_name_outside_gap_records(child, path);
                }
            }
            _ => {}
        }
    }

    fn primitive_sort(name: &str) -> Sort {
        Sort::Primitive {
            name: name.to_string(),
        }
    }

    fn temp_workspace(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("{name}_{nanos}"));
        fs::create_dir_all(&root).expect("create temp workspace");
        root
    }

    fn single_entry_for_source(name: &str, source: &str) -> Value {
        let root = temp_workspace(name);
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        fs::write(src_dir.join("lib.rs"), source).expect("write source");

        let out = bind_lift(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": ["."]
        }))
        .expect("bind lift should succeed");
        let entry = out["ir"]
            .as_array()
            .expect("ir array")
            .first()
            .expect("single entry")
            .clone();
        let _ = fs::remove_dir_all(root);
        entry
    }

    #[test]
    fn rust_lifter_parse_refusal_cites_v1_1_exam_question() {
        let dir = rust_lifter_parse_refusal_workspace("cites");

        let result = bind_lift(&json!({
            "workspace_root": dir,
            "source_paths": ["."]
        }))
        .expect("bind lift returns document");
        let diagnostic = result["diagnostics"][0]
            .as_object()
            .expect("diagnostic object");
        let expected = exam_question_cid_for("morphism", "concept:source-unit", "rust")
            .expect("source-unit rust question exists");

        assert_eq!(diagnostic["kind"], "parse-error");
        assert_eq!(diagnostic["exam_manifest_cid"], EXAM_MANIFEST_CID);
        assert_eq!(diagnostic["exam_question_cid"], expected);
    }

    #[test]
    fn rust_lifter_parse_refusal_does_not_fire_read_error_variant() {
        let dir = rust_lifter_parse_refusal_workspace("discrim");

        let result = bind_lift(&json!({
            "workspace_root": dir,
            "source_paths": ["."]
        }))
        .expect("bind lift returns document");

        assert_eq!(result["diagnostics"][0]["kind"], "parse-error");
        assert!(result["diagnostics"]
            .as_array()
            .expect("diagnostics array")
            .iter()
            .all(|item| item["kind"] != "read-error"));
    }

    #[test]
    fn rust_lifter_parse_refusal_cites_source_unit_not_related_add() {
        let dir = rust_lifter_parse_refusal_workspace("structural");

        let result = bind_lift(&json!({
            "workspace_root": dir,
            "source_paths": ["."]
        }))
        .expect("bind lift returns document");
        let refusal_cid = result["diagnostics"][0]["exam_question_cid"]
            .as_str()
            .expect("exam question cid");
        let related =
            exam_question_cid_for("morphism", "concept:add", "rust").expect("add rust exists");

        assert_ne!(refusal_cid, related);
    }

    fn rust_lifter_parse_refusal_workspace(label: &str) -> PathBuf {
        let unique = format!(
            "provekit-walk-citation-{}-{}",
            label,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        );
        let dir = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(dir.join("src")).expect("create src");
        std::fs::write(dir.join("src/lib.rs"), "fn broken(").expect("write source");
        dir
    }
}
