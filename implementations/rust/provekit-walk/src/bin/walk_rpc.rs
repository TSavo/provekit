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

        for item in &file.items {
            if let syn::Item::Fn(item_fn) = item {
                let fn_name = item_fn.sig.ident.to_string();
                let fn_line = line_for_fn(&src, &fn_name) as u64;
                let (attr_pre, attr_post) = extract_contract_attrs(&item_fn.attrs);
                let concept_annotation = extract_concept_annotation(&src, &fn_name);
                let term_shape = term_shape_for_fn(item_fn);
                let term_shape_cid = blake3_512_of(encode_jcs(&term_shape).as_bytes());
                let (param_names, param_types, return_type) = fn_signature(item_fn);
                let function_symbol = format!("{fn_name}@{rel}");
                let witnesses = witnesses_by_symbol
                    .get(&function_symbol)
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
            }
        }
    }

    Ok(json!({
        "kind": "ir-document",
        "ir": entries,
        "diagnostics": diagnostics,
    }))
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

fn line_for_fn(src: &str, fn_name: &str) -> usize {
    let needle = format!("fn {fn_name}(");
    for (i, line) in src.lines().enumerate() {
        if line.contains(&needle) {
            return i + 1;
        }
    }
    1
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
            while j > 0 {
                let prev = lines[j - 1].trim_start();
                if let Some(rest) = prev.strip_prefix("// concept:") {
                    let trimmed = rest.trim().to_string();
                    if trimmed.starts_with("UNNAMED-CONCEPT-") {
                        return None;
                    }
                    return Some(trimmed);
                }
                if prev.starts_with("#[")
                    || prev.starts_with("// substrate-origin:")
                    || prev.starts_with("// memento-cid:")
                    || prev.starts_with("// witness-inherited-from:")
                {
                    j -= 1;
                    continue;
                }
                break;
            }
            return None;
        }
    }
    None
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
