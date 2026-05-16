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

use base64::Engine;
use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value as CValue};
use provekit_ir_types::{EvidenceMemento, IrFormula, IrTerm, SourceKind};
use provekit_lift_contracts::{lift_file_with_docstring_evidence, lift_file_with_sig_evidence};
use provekit_walk::emit::{rust_function_term_json, shadow_proof_ir_cid, shadow_to_proof_ir};
use provekit_walk::{
    build_function_contract_with_file, build_shadow_source, lift_function_postcondition,
    lift_function_precondition, CalleeContract,
};
use serde_json::{json, Value};
use syn::spanned::Spanned;

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
                diagnostics.push(json!({
                    "kind": "read-error",
                    "path": path.display().to_string(),
                    "detail": e.to_string()
                }));
                continue;
            }
        };
        let file = match syn::parse_file(&src) {
            Ok(f) => f,
            Err(e) => {
                diagnostics.push(json!({
                    "kind": "parse-error",
                    "path": path.display().to_string(),
                    "detail": e.to_string()
                }));
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

        for target in collect_bind_lift_targets(&file) {
            let item_fn = &target.item_fn;
            let fn_name = &target.fn_name;
            let fn_line = target.line as u64;
            let (attr_pre, attr_post) = extract_contract_attrs(&item_fn.attrs);
            let concept_annotation = extract_concept_annotation(&src, &target.source_name);
            let term_shape = term_shape_for_fn(item_fn);
            let term_shape_cid = blake3_512_of(encode_jcs(&term_shape).as_bytes());
            let (param_names, param_types, return_type) = fn_signature(item_fn);
            let function_symbol = format!("{fn_name}@{rel}");
            let fallback_function_symbol = format!("{}@{rel}", target.source_name);
            let witnesses = witnesses_by_symbol
                .get(&function_symbol)
                .or_else(|| witnesses_by_symbol.get(&fallback_function_symbol))
                .cloned()
                .unwrap_or_default();

            entries.push(json!({
                "kind": "bind-lift-entry",
                "file": rel,
                "fn_name": fn_name,
                "fn_line": fn_line,
                "attr_pre": attr_pre,
                "attr_post": attr_post,
                "concept_annotation": concept_annotation,
                "param_names": param_names,
                "param_types": param_types,
                "return_type": return_type,
                "term_shape": cvalue_to_json(&term_shape),
                "term_shape_cid": term_shape_cid,
                "witnesses": witnesses,
            }));

            for (index, callsite) in collect_bound_library_calls(item_fn, &library_bindings)
                .into_iter()
                .enumerate()
            {
                let term_shape =
                    concept_citation_shape(&callsite.pattern.concept_cid, &callsite.resolved_args);
                let term_shape_cid = blake3_512_of(encode_jcs(&term_shape).as_bytes());
                entries.push(json!({
                    "kind": "bind-lift-entry",
                    "file": rel,
                    "fn_name": format!("{fn_name}__bound_call_{}", index + 1),
                    "fn_line": callsite.line as u64,
                    "attr_pre": Value::Null,
                    "attr_post": Value::Null,
                    "concept_annotation": concept_annotation_name(&callsite.pattern.concept_name),
                    "param_names": callsite.resolved_args,
                    "param_types": vec!["unknown"; callsite.arity],
                    "return_type": "unknown",
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

#[derive(Debug, Clone)]
struct BindLiftTarget {
    fn_name: String,
    source_name: String,
    line: usize,
    item_fn: syn::ItemFn,
}

fn collect_bind_lift_targets(file: &syn::File) -> Vec<BindLiftTarget> {
    let mut targets = Vec::new();
    collect_bind_lift_targets_in_items(&file.items, &mut targets);
    targets
}

fn collect_bind_lift_targets_in_items(items: &[syn::Item], targets: &mut Vec<BindLiftTarget>) {
    for item in items {
        match item {
            syn::Item::Fn(item_fn) => {
                let fn_name = item_fn.sig.ident.to_string();
                targets.push(BindLiftTarget {
                    source_name: fn_name.clone(),
                    line: span_line(item_fn),
                    fn_name,
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
                    targets.push(BindLiftTarget {
                        fn_name: format!("{qualifier}::{source_name}"),
                        source_name,
                        line: span_line(method),
                        item_fn: item_fn_from_impl_method(method),
                    });
                }
            }
            syn::Item::Mod(module) => {
                if let Some((_, nested_items)) = &module.content {
                    collect_bind_lift_targets_in_items(nested_items, targets);
                }
            }
            _ => {}
        }
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

fn span_line(node: &impl Spanned) -> usize {
    let line = node.span().start().line;
    if line == 0 {
        1
    } else {
        line
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
    concept_name: String,
    concept_cid: String,
    callee: String,
    min_params: Option<usize>,
    max_params: Option<usize>,
}

#[derive(Debug, Clone)]
struct BoundLibraryCall {
    pattern: LibraryCallPattern,
    resolved_args: Vec<String>,
    arity: usize,
    line: usize,
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
                        concept_name: concept_name.to_string(),
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
            self.record_call(&callee, &node.args, node.span().start().line);
        }
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        let receiver = expr_surface(&node.receiver);
        let callee = format!("{receiver}.{}", node.method);
        self.record_call(&callee, &node.args, node.span().start().line);
        syn::visit::visit_expr_method_call(self, node);
    }
}

impl BoundCallCollector<'_> {
    fn record_call(
        &mut self,
        callee: &str,
        args: &syn::punctuated::Punctuated<syn::Expr, syn::token::Comma>,
        line: usize,
    ) {
        let arity = args.len();
        let Some(pattern) = self.bindings.match_call(callee, arity) else {
            return;
        };
        let resolved_args = args.iter().map(expr_surface).collect::<Vec<_>>();
        self.calls.push(BoundLibraryCall {
            pattern: pattern.clone(),
            resolved_args,
            arity,
            line,
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

fn concept_annotation_name(concept_name: &str) -> String {
    concept_name
        .strip_prefix("concept:")
        .unwrap_or(concept_name)
        .to_string()
}

fn contract_witnesses_by_function_symbol(
    file: &syn::File,
    rel: &str,
    source_bytes: &[u8],
) -> BTreeMap<String, Vec<Value>> {
    let mut by_symbol: BTreeMap<String, Vec<(String, Value)>> = BTreeMap::new();
    for evidence in lift_file_with_sig_evidence(file, rel, source_bytes)
        .evidences
        .into_iter()
        .chain(
            lift_file_with_docstring_evidence(file, rel, source_bytes)
                .evidences
                .into_iter(),
        )
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
        "line": evidence.source_locator.span.start.line as u64,
        "col": evidence.source_locator.span.start.col as u64,
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

fn fn_signature(item_fn: &syn::ItemFn) -> (Vec<String>, Vec<String>, String) {
    let mut names = Vec::new();
    let mut types = Vec::new();
    for (i, arg) in item_fn.sig.inputs.iter().enumerate() {
        match arg {
            syn::FnArg::Receiver(_) => {
                names.push("__self".to_string());
                types.push("Self".to_string());
            }
            syn::FnArg::Typed(pt) => {
                let name = match &*pt.pat {
                    syn::Pat::Ident(p) => p.ident.to_string(),
                    _ => format!("__arg{}", i),
                };
                let ty_str = type_to_string(&pt.ty);
                names.push(name);
                types.push(ty_str);
            }
        }
    }
    let return_type = match &item_fn.sig.output {
        syn::ReturnType::Default => "()".to_string(),
        syn::ReturnType::Type(_, t) => type_to_string(t),
    };
    (names, types, return_type)
}

fn type_to_string(ty: &syn::Type) -> String {
    use quote::ToTokens;
    let s = ty.to_token_stream().to_string();
    normalize_ws(&s)
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

fn extract_contract_attrs(attrs: &[syn::Attribute]) -> (Option<String>, Option<String>) {
    let mut pre: Option<String> = None;
    let mut post: Option<String> = None;
    for attr in attrs {
        if let Some(name) = attr.path().get_ident().map(|i| i.to_string()) {
            if let syn::Meta::List(l) = &attr.meta {
                let text = normalize_ws(&l.tokens.to_string());
                match name.as_str() {
                    "requires" if pre.is_none() => pre = Some(text),
                    "ensures" if post.is_none() => post = Some(text),
                    _ => {}
                }
            }
        }
        if attr.path().is_ident("cfg_attr") {
            if let syn::Meta::List(l) = &attr.meta {
                let tokens_str = l.tokens.to_string();
                let rest = tokens_str
                    .strip_prefix("any ()")
                    .or_else(|| tokens_str.strip_prefix("any()"));
                if let Some(rest) = rest {
                    let rest = rest.trim().trim_start_matches(',').trim();
                    parse_kind_body(rest, &mut pre, &mut post);
                }
            }
        }
    }
    (pre, post)
}

fn parse_kind_body(s: &str, pre: &mut Option<String>, post: &mut Option<String>) {
    for kind in ["requires", "ensures"] {
        if let Some(rest) = s.strip_prefix(kind) {
            let rest = rest.trim_start();
            if rest.starts_with('(') && rest.ends_with(')') {
                let body = normalize_ws(&rest[1..rest.len() - 1]);
                match kind {
                    "requires" if pre.is_none() => *pre = Some(body),
                    "ensures" if post.is_none() => *post = Some(body),
                    _ => {}
                }
            }
        }
    }
}

fn extract_concept_annotation(src: &str, fn_name: &str) -> Option<String> {
    let needle = format!("fn {fn_name}(");
    let lines: Vec<&str> = src.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if line.contains(&needle) {
            let mut j = i;
            let mut comment_annotation: Option<String> = None;
            while j > 0 {
                let prev = lines[j - 1].trim_start();
                if let Some(tag) = extract_emitted_concept_tag(prev) {
                    return Some(tag);
                }
                if let Some(rest) = prev.strip_prefix("// concept:") {
                    if comment_annotation.is_none() {
                        comment_annotation = normalize_concept_annotation(rest);
                    }
                }
                if prev.starts_with("#[")
                    || prev.starts_with("// substrate-origin:")
                    || prev.starts_with("// memento-cid:")
                    || prev.starts_with("// witness-inherited-from:")
                    || prev.starts_with("// concept:")
                {
                    j -= 1;
                    continue;
                }
                break;
            }
            return comment_annotation;
        }
    }
    None
}

fn extract_emitted_concept_tag(line: &str) -> Option<String> {
    for marker in ["provekit_monitor", "provekit_emitter", "provekit_witness"] {
        let Some(marker_pos) = line.find(marker) else {
            continue;
        };
        let rest = &line[marker_pos + marker.len()..];
        let Some(concept_pos) = rest.find("concept") else {
            continue;
        };
        let after_key = rest[concept_pos + "concept".len()..].trim_start();
        let Some(after_eq) = after_key.strip_prefix('=') else {
            continue;
        };
        let after_eq = after_eq.trim_start();
        let Some(quoted) = after_eq.strip_prefix('"') else {
            continue;
        };
        let Some(end) = quoted.find('"') else {
            continue;
        };
        if let Some(name) = normalize_concept_annotation(&quoted[..end]) {
            return Some(name);
        }
    }
    None
}

fn normalize_concept_annotation(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let name = trimmed.strip_prefix("concept:").unwrap_or(trimmed).trim();
    if name.is_empty() || name.starts_with("UNNAMED-CONCEPT-") {
        None
    } else {
        Some(name.to_string())
    }
}

fn term_shape_for_fn(item_fn: &syn::ItemFn) -> std::sync::Arc<CValue> {
    let stmts: Vec<std::sync::Arc<CValue>> =
        item_fn.block.stmts.iter().map(shape_of_stmt).collect();
    CValue::object([
        ("kind", CValue::string("body")),
        ("stmts", CValue::array(stmts)),
    ])
}

fn shape_of_stmt(stmt: &syn::Stmt) -> std::sync::Arc<CValue> {
    match stmt {
        syn::Stmt::Expr(e, _) => shape_of_expr(e),
        syn::Stmt::Local(_) => CValue::object([("kind", CValue::string("let"))]),
        _ => CValue::object([("kind", CValue::string("opaque"))]),
    }
}

fn shape_of_expr(expr: &syn::Expr) -> std::sync::Arc<CValue> {
    match expr {
        syn::Expr::If(e) => {
            let mut kv: Vec<(&str, std::sync::Arc<CValue>)> = Vec::new();
            kv.push(("kind", CValue::string("if")));
            kv.push(("cond", shape_of_expr(&e.cond)));
            let then_stmts: Vec<std::sync::Arc<CValue>> =
                e.then_branch.stmts.iter().map(shape_of_stmt).collect();
            kv.push((
                "then",
                CValue::object([
                    ("kind", CValue::string("block")),
                    ("stmts", CValue::array(then_stmts)),
                ]),
            ));
            if let Some((_, else_expr)) = &e.else_branch {
                kv.push(("else", shape_of_expr(else_expr)));
            }
            CValue::object(kv)
        }
        syn::Expr::While(e) => {
            let body_stmts: Vec<std::sync::Arc<CValue>> =
                e.body.stmts.iter().map(shape_of_stmt).collect();
            CValue::object([
                ("kind", CValue::string("while")),
                ("cond", shape_of_expr(&e.cond)),
                (
                    "body",
                    CValue::object([
                        ("kind", CValue::string("block")),
                        ("stmts", CValue::array(body_stmts)),
                    ]),
                ),
            ])
        }
        syn::Expr::ForLoop(e) => {
            let body_stmts: Vec<std::sync::Arc<CValue>> =
                e.body.stmts.iter().map(shape_of_stmt).collect();
            CValue::object([
                ("kind", CValue::string("for")),
                (
                    "body",
                    CValue::object([
                        ("kind", CValue::string("block")),
                        ("stmts", CValue::array(body_stmts)),
                    ]),
                ),
            ])
        }
        syn::Expr::Return(_) | syn::Expr::Break(_) | syn::Expr::Continue(_) => {
            CValue::object([("kind", CValue::string("exit"))])
        }
        syn::Expr::Assign(_) => CValue::object([("kind", CValue::string("assign"))]),
        syn::Expr::Binary(e) => {
            let op = match &e.op {
                syn::BinOp::Add(_) | syn::BinOp::AddAssign(_) => "+",
                syn::BinOp::Sub(_) | syn::BinOp::SubAssign(_) => "-",
                syn::BinOp::Mul(_) | syn::BinOp::MulAssign(_) => "*",
                syn::BinOp::Div(_) | syn::BinOp::DivAssign(_) => "/",
                syn::BinOp::Rem(_) | syn::BinOp::RemAssign(_) => "%",
                syn::BinOp::Eq(_) => "==",
                syn::BinOp::Ne(_) => "!=",
                syn::BinOp::Lt(_) => "<",
                syn::BinOp::Le(_) => "<=",
                syn::BinOp::Gt(_) => ">",
                syn::BinOp::Ge(_) => ">=",
                _ => "opaque-op",
            };
            let is_rel = matches!(op, "==" | "!=" | "<" | "<=" | ">" | ">=");
            CValue::object([
                ("kind", CValue::string(if is_rel { "rel" } else { "bin" })),
                ("op", CValue::string(op.to_string())),
            ])
        }
        syn::Expr::Call(_) | syn::Expr::MethodCall(_) => {
            CValue::object([("kind", CValue::string("call"))])
        }
        syn::Expr::Block(b) => {
            let stmts: Vec<std::sync::Arc<CValue>> =
                b.block.stmts.iter().map(shape_of_stmt).collect();
            CValue::object([
                ("kind", CValue::string("block")),
                ("stmts", CValue::array(stmts)),
            ])
        }
        _ => CValue::object([("kind", CValue::string("opaque"))]),
    }
}

/// Render a canonicalizer `Value` as `serde_json::Value` for the RPC response.
fn cvalue_to_json(v: &CValue) -> Value {
    let s = encode_jcs(v);
    serde_json::from_str(&s).unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn bind_lift_marries_docstring_and_type_signature_evidence_as_witnesses() {
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
        let entry = entries
            .iter()
            .find(|entry| entry["fn_name"] == "wrap_positive")
            .expect("wrap_positive entry");
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
                .any(|w| w["source_kind"] == "type-signature"
                    && w["extension_fields"]["signature_position"] == "param:0"),
            "expected parameter type-signature evidence witness: {witnesses:#?}"
        );
        let return_type_witness = witnesses
            .iter()
            .find(|w| {
                w["source_kind"] == "type-signature"
                    && w["extension_fields"]["signature_position"] == "return"
            })
            .expect("return type-signature evidence witness");
        let predicate_text = return_type_witness["predicate_text"]
            .as_str()
            .expect("type-signature witnesses must carry predicate_text");
        assert!(
            predicate_text.contains("is_some(result)")
                && predicate_text.contains("is_none(result)")
                && !predicate_text.contains("\"kind\""),
            "type-signature predicate_text must be readable predicate text, not raw JSON: {predicate_text}"
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
    fn concept_annotation_reads_human_edited_emitted_monitor_tag() {
        let src = r#"
// concept: deposit-then-balance
// substrate-origin: annotation-lift
// memento-cid: blake3-512:abc
#[cfg_attr(any(), requires(amount > 0))]
#[cfg_attr(any(), provekit_monitor(concept = "ledger-deposit"))]
pub fn deposit(balance: i64, amount: i64) -> i64 {
    balance + amount
}
"#;

        assert_eq!(
            extract_concept_annotation(src, "deposit").as_deref(),
            Some("ledger-deposit")
        );
    }

    #[test]
    fn concept_annotation_strips_prefix_from_emitted_observation_tag() {
        let src = r#"
#[cfg_attr(any(), provekit_emitter(concept = "concept:ledger-deposit"))]
pub fn deposit(balance: i64, amount: i64) -> i64 {
    balance + amount
}
"#;

        assert_eq!(
            extract_concept_annotation(src, "deposit").as_deref(),
            Some("ledger-deposit")
        );
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
            .find(|entry| entry["concept_annotation"] == "http-request")
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

        assert_eq!(bound["file"], "src/lib.rs");
        assert_eq!(bound["fn_name"], "fetch_status__bound_call_1");
        assert_eq!(bound["param_names"], json!(["url"]));
        assert_eq!(bound["param_types"], json!(["unknown"]));
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
            .find(|entry| entry["concept_annotation"] == "http-request")
            .expect("second bound callsite entry");
        assert_eq!(bound_again["term_shape_cid"], bound["term_shape_cid"]);

        let _ = fs::remove_dir_all(root);
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

        let out = bind_lift(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": ["."]
        }))
        .expect("bind lift should succeed");

        let entries = out["ir"].as_array().expect("ir array");
        let fn_names = entries
            .iter()
            .filter_map(|entry| entry["fn_name"].as_str())
            .collect::<Vec<_>>();
        eprintln!("bind lift value.rs entries: {fn_names:?}");

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
                fn_names.iter().any(|name| *name == expected),
                "missing impl method entry {expected}; got {fn_names:?}"
            );
        }
        assert!(
            entries.len() >= 7,
            "expected at least seven bind lift entries from value.rs impl methods, got {}",
            entries.len()
        );

        let _ = fs::remove_dir_all(root);
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
}
