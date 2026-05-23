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

use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use base64::Engine;
use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value as CValue};
use provekit_ir_types::{EvidenceMemento, ExamManifestMemento, IrFormula, IrTerm, SourceKind};
use provekit_lift_contracts::lift_file_with_docstring_evidence;
use provekit_walk::emit::{
    rust_function_term_json_for_file, shadow_proof_ir_cid, shadow_to_proof_ir,
};
use provekit_walk::{
    build_function_contract_with_file, build_shadow_source, lift_function_postcondition,
    lift_function_precondition, CalleeContract,
};
use serde_json::{json, Value};
use syn::spanned::Spanned;

const CONCEPT_SHAPES_CATALOG_INDEX_JSON: &str =
    include_str!("../../../../../menagerie/concept-shapes/catalog/index.json");
const EXAM_MANIFEST_CID: &str = libprovekit::exam_manifest::DEFAULT_EXAM_MANIFEST_CID;
const SORT_BOOL_CID: &str = "blake3-512:0ee13bf3fd6b7ecfbee72dfbfc18a7c0ea7f1663de6cca43cefb36f5b4c03665452646094a7c296e819e75d683c6ce4821f3d7db3c3c78ae97f2d4e3451d2074";
const SORT_BYTES_CID: &str = "blake3-512:7116ef6e62e6739b213a8394f975a53c771b89f08c36d27143827acfcfebc0e39e5b82c530be668c3cfd5ec6966ccaa42930b37fdb1f4ac25652a970be10fb6b";
const SORT_FLOAT_CID: &str = "blake3-512:b979e70c4d5e53d9bdf13d6f08330be3c5b0714b8c770d69bbd05946b86c36df5274be8145a2683cc29c278155c9c1ee65b6897913524eecb9e4c89c71862f57";
const SORT_INT_CID: &str = "blake3-512:30ffc51350121a7172f3e4064a33c45bbd345756979fccff6875cd2ab33e4964d098a99df80cfbdf1ec1a0738c5ac3476f0ff8f75589ea511d1acd82c74ecd58";
const SORT_STRING_CID: &str = "blake3-512:be8721d24849feb74c4721520bdba02d352a94f49253a627cd509127472aa1c47cbe99cb705cac4159b5365abcce0c9aaa4901fe67630827deb6be1f9daeea10";

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
    let file: syn::File = syn::parse_str(src).map_err(|e| format!("parse error: {}", e))?;
    let bytes = rust_function_term_json_for_file(&file, fn_name, source)?;
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
            "ir_version": "bind-ir/2.0.0",
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
            let param_types = sugar_param_types(item_fn);
            let original_param_types = sugar_original_param_types(item_fn);
            let return_type = sugar_return_type(item_fn);
            let generic_params = sugar_generic_params(item_fn);
            let visibility = match &item_fn.vis {
                syn::Visibility::Public(_) => "pub",
                syn::Visibility::Restricted(_) => "pub(crate)",
                syn::Visibility::Inherited => "",
            };
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
                "param_types": param_types,
                "original_param_types": original_param_types,
                "return_type": return_type,
                "generic_params": generic_params,
                "visibility": visibility,
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

        for sugar_target in collect_sugar_targets(&file, &src) {
            let SugarTarget {
                concept,
                library,
                version,
                family,
                loss,
                observed_dimension,
                item_fn,
            } = sugar_target;
            let param_names = fn_param_names(&item_fn);
            let param_types = sugar_param_types(&item_fn);
            let original_param_types = sugar_original_param_types(&item_fn);
            let generic_params = sugar_generic_params(&item_fn);
            let return_type = sugar_return_type(&item_fn);
            let term_shape = term_shape_for_fn(&item_fn);
            let term_shape_cid = blake3_512_of(encode_jcs(&term_shape).as_bytes());
            let operand_bindings = operand_bindings_for_fn(&item_fn);
            let sig_shape = CValue::object([
                (
                    "param_names",
                    CValue::array(
                        param_names
                            .iter()
                            .map(|name| CValue::string(name.clone()))
                            .collect(),
                    ),
                ),
                (
                    "param_types",
                    CValue::array(
                        param_types
                            .iter()
                            .map(|param_type| CValue::string(param_type.clone()))
                            .collect(),
                    ),
                ),
                ("return_type", CValue::string(return_type.clone())),
            ]);
            let signature_shape_cid = blake3_512_of(encode_jcs(&sig_shape).as_bytes());

            // #1361 chunk 2 part B / #1355: emit concept-hub sort CIDs for
            // each parameter type. The rust kit's @sugar lift translates
            // its source syntax to substrate-canonical sort identities AT
            // the kit/substrate boundary. Parallel to source_transform's
            // @boundary carrier emission and JavaBindLifter's @sugar emission.
            // Kit-internal sort labels (rust:Int, rust:Str, ...) stay inside
            // the rust kit; only concept-hub CIDs cross to substrate. Empty
            // string in a slot signals "kit has no morphism for this type" —
            // substrate-honest gap signal for downstream refusal.
            let mut parametric_sort_expansions: Vec<
                libprovekit::core::lower_plugin::ParametricSortExpansion,
            > = Vec::new();
            let param_sort_cids: Vec<String> = param_types
                .iter()
                .map(|t| {
                    rust_source_type_to_concept_hub_sort_cid(t, &mut parametric_sort_expansions)
                        .unwrap_or_default()
                })
                .collect();
            let return_sort_cid = rust_source_type_to_concept_hub_sort_cid(
                &return_type,
                &mut parametric_sort_expansions,
            )
            .unwrap_or_default();
            let mut entry = json!({
                "kind": "library-sugar-binding-entry",
                "target_language": "rust",
                "target_library_tag": library,
                "concept_name": concept,
                "source_function_name": item_fn.sig.ident.to_string(),
                "visibility": match &item_fn.vis {
                    syn::Visibility::Public(_) => "pub",
                    syn::Visibility::Restricted(_) => "pub(crate)",
                    syn::Visibility::Inherited => "",
                },
                "generic_params": generic_params,
                "original_param_types": original_param_types,
                "param_names": param_names,
                "param_types": param_types,
                "param_sort_cids": param_sort_cids,
                "return_type": return_type,
                "return_sort_cid": return_sort_cid,
                "term_shape": cvalue_to_json(&term_shape),
                "term_shape_cid": term_shape_cid,
                "operand_bindings": operand_bindings,
                "signature_shape_cid": signature_shape_cid,
                "loss_record_contribution": {
                    "form": "literal",
                    "value": { "entries": loss },
                },
                "body_source": sugar_body_source(&rel, &src, &item_fn),
            });
            // #1369: parametric content-addressing — emit expansions for any
            // composite CIDs the signature contains. Realize plugin reads
            // these to decompose composite CIDs into (constructor, args)
            // for parameterized morphism dispatch.
            if !parametric_sort_expansions.is_empty() {
                entry["parametric_sort_expansions"] = serde_json::to_value(
                    &parametric_sort_expansions,
                )
                .unwrap_or_else(|_| json!([]));
            }
            if let Some(observed) = observed_dimension {
                entry["observed_dimension"] = json!(observed);
            }
            // #1357: surface the optional version + family pins on the
            // binding entry so downstream materialize dispatch (#1359) can
            // narrow by them. Absent on the annotation → absent in the
            // emitted JSON (NOT empty strings — null/missing is the substrate
            // signal for "this axis floats").
            if let Some(v) = version {
                entry["library_version"] = json!(v);
            }
            if let Some(f) = family {
                entry["family"] = json!(f);
            }
            entries.push(entry);
        }

        for refuse_target in collect_refuse_targets(&file) {
            let RefuseTarget {
                surface,
                concept,
                reason,
                would_close_with_cluster,
            } = refuse_target;
            entries.push(json!({
                "kind": "refusal-memento",
                "target_language": "rust",
                "surface": surface,
                "concept": concept,
                "reason": reason,
                "would_close_with_cluster": would_close_with_cluster,
            }));
        }

        // Boundary lane: #[provekit::boundary] annotations. Each marks
        // a function as the EDGE where a concept binds to a per-language
        // library. Emitted as `realization-memento` (Boundary variant)
        // entries so cmd_mint can mint them into the envelope; the
        // materializer reads them when retargeting downstream consumers
        // to other languages and substitutes the per-target sister
        // library at each boundary callsite.
        for boundary_target in collect_boundary_targets(&file) {
            let BoundaryTarget {
                concept,
                library,
                version,
                family,
                api,
                boundary_contract,
                loss,
                source_function_name,
            } = boundary_target;
            let mut entry = json!({
                "kind": "realization-memento",
                "realization_kind": "boundary",
                "target_language": "rust",
                "concept_name": concept,
                "library": library,
                "source_function_name": source_function_name,
                "loss_record_contribution": {
                    "form": "literal",
                    "value": { "entries": loss },
                },
            });
            if let Some(api_str) = api {
                entry["api"] = json!(api_str);
            }
            if let Some(bc) = boundary_contract {
                entry["boundary_contract"] = json!(bc);
            }
            // #1357: parallel to the sugar emission above.
            if let Some(v) = version {
                entry["library_version"] = json!(v);
            }
            if let Some(f) = family {
                entry["family"] = json!(f);
            }
            entries.push(entry);
        }

        // Trait declarations: walk top-level pub trait items and emit
        // `trait-decl` entries so the target plugin can synthesize an
        // interface matching the rust trait. Substrate-honest: the
        // AdapterLifter interface in the java wrapper comes from the
        // rust trait declaration, not from hand-written java code.
        use quote::ToTokens;
        for item in &file.items {
            if let syn::Item::Trait(t) = item {
                let trait_name = t.ident.to_string();
                let visibility = match &t.vis {
                    syn::Visibility::Public(_) => "pub",
                    syn::Visibility::Restricted(_) => "pub(crate)",
                    syn::Visibility::Inherited => "",
                };
                let mut methods: Vec<Value> = Vec::new();
                for trait_item in &t.items {
                    if let syn::TraitItem::Fn(m) = trait_item {
                        let name = m.sig.ident.to_string();
                        let return_type = match &m.sig.output {
                            syn::ReturnType::Default => "()".to_string(),
                            syn::ReturnType::Type(_, ty) => ty.to_token_stream().to_string().replace(' ', ""),
                        };
                        let mut param_names: Vec<String> = Vec::new();
                        let mut param_types: Vec<String> = Vec::new();
                        for input in &m.sig.inputs {
                            match input {
                                syn::FnArg::Receiver(_) => {
                                    // Skip `&self` / `&mut self` — java interface methods are implicit.
                                }
                                syn::FnArg::Typed(pt) => {
                                    if let syn::Pat::Ident(pi) = &*pt.pat {
                                        param_names.push(pi.ident.to_string());
                                    } else {
                                        param_names.push(pt.pat.to_token_stream().to_string());
                                    }
                                    param_types.push(pt.ty.to_token_stream().to_string().replace(' ', ""));
                                }
                            }
                        }
                        methods.push(json!({
                            "name": name,
                            "param_names": param_names,
                            "param_types": param_types,
                            "return_type": return_type,
                        }));
                    }
                }
                entries.push(json!({
                    "kind": "trait-decl",
                    "name": trait_name,
                    "visibility": visibility,
                    "methods": methods,
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

#[derive(Debug, Clone)]
struct SugarTarget {
    concept: String,
    library: String,
    /// #1357: per-#1355, the @sugar annotation may carry a `version`
    /// pin (e.g. "0.39.0") and a `family` pin (e.g.
    /// "concept:family:sql"). Both float when absent; the dispatch
    /// query in #1359 narrows the candidate set using these when present.
    version: Option<String>,
    family: Option<String>,
    loss: Vec<String>,
    observed_dimension: Option<String>,
    item_fn: syn::ItemFn,
}

#[derive(Debug, Clone)]
struct RefuseTarget {
    surface: String,
    concept: String,
    reason: String,
    would_close_with_cluster: String,
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

fn collect_sugar_targets(file: &syn::File, _src: &str) -> Vec<SugarTarget> {
    let mut targets = Vec::new();
    collect_sugar_targets_in_items(&file.items, &mut targets);
    targets
}

fn collect_sugar_targets_in_items(items: &[syn::Item], targets: &mut Vec<SugarTarget>) {
    for item in items {
        match item {
            syn::Item::Fn(item_fn) => {
                if let Some(parsed) = extract_sugar_attr(item_fn) {
                    targets.push(SugarTarget {
                        concept: parsed.concept,
                        library: parsed.library,
                        version: parsed.version,
                        family: parsed.family,
                        loss: parsed.loss,
                        observed_dimension: parsed.observed_dimension,
                        item_fn: item_fn.clone(),
                    });
                }
            }
            syn::Item::Mod(module) => {
                if let Some((_, nested_items)) = &module.content {
                    collect_sugar_targets_in_items(nested_items, targets);
                }
            }
            _ => {}
        }
    }
}

fn collect_refuse_targets(file: &syn::File) -> Vec<RefuseTarget> {
    let mut targets = Vec::new();
    collect_refuse_targets_in_items(&file.items, &mut targets);
    targets
}

fn collect_refuse_targets_in_items(items: &[syn::Item], targets: &mut Vec<RefuseTarget>) {
    for item in items {
        if let syn::Item::Mod(module) = item {
            if let Some(parsed) = extract_refuse_attr(module) {
                targets.push(parsed);
            }
            if let Some((_, nested_items)) = &module.content {
                collect_refuse_targets_in_items(nested_items, targets);
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
struct SugarAttrParsed {
    concept: String,
    library: String,
    /// #1357: optional `version` named arg (e.g. "0.39.0"). Absent ↔ floating.
    version: Option<String>,
    /// #1357: optional `family` named arg (e.g. "concept:family:sql").
    /// Absent ↔ floating (the platform_profile or dispatcher may supply it).
    family: Option<String>,
    loss: Vec<String>,
    observed_dimension: Option<String>,
}

fn extract_sugar_attr(item_fn: &syn::ItemFn) -> Option<SugarAttrParsed> {
    for attr in &item_fn.attrs {
        let path = attr.path();
        let segments: Vec<_> = path.segments.iter().collect();
        if segments.len() == 2 && segments[0].ident == "provekit" && segments[1].ident == "sugar" {
            if let Ok(meta_list) = attr.meta.require_list() {
                let args = parse_attr_named_args(&meta_list.tokens);
                let concept = args.string("concept").unwrap_or_default();
                let library = args.string("library").unwrap_or_default();
                if !concept.is_empty() && !library.is_empty() {
                    return Some(SugarAttrParsed {
                        concept,
                        library,
                        version: args.string("version"),
                        family: args.string("family"),
                        loss: args.string_array("loss"),
                        observed_dimension: args.string("observed_dimension"),
                    });
                }
            }
        }
    }
    None
}

/// One `#[provekit::boundary]` target discovered by walking the source.
/// Each boundary annotation marks a function as the EDGE where a
/// concept binds to a per-language library. The lifter promotes it to
/// a `realization-memento` (Boundary variant); the materializer reads
/// it when retargeting downstream consumers to other languages,
/// substituting the per-target sister library at that callsite.
#[derive(Debug, Clone, Default)]
struct BoundaryTarget {
    concept: String,
    library: String,
    /// #1357: optional version and family pins, parallel to SugarTarget.
    version: Option<String>,
    family: Option<String>,
    api: Option<String>,
    boundary_contract: Option<String>,
    loss: Vec<String>,
    source_function_name: String,
}

fn collect_boundary_targets(file: &syn::File) -> Vec<BoundaryTarget> {
    let mut targets = Vec::new();
    collect_boundary_targets_in_items(&file.items, &mut targets);
    targets
}

fn collect_boundary_targets_in_items(
    items: &[syn::Item],
    targets: &mut Vec<BoundaryTarget>,
) {
    for item in items {
        match item {
            syn::Item::Fn(item_fn) => {
                if let Some(mut parsed) = extract_boundary_attr(item_fn) {
                    parsed.source_function_name = item_fn.sig.ident.to_string();
                    targets.push(parsed);
                }
            }
            syn::Item::Mod(module) => {
                if let Some((_, nested_items)) = &module.content {
                    collect_boundary_targets_in_items(nested_items, targets);
                }
            }
            _ => {}
        }
    }
}

fn extract_boundary_attr(item_fn: &syn::ItemFn) -> Option<BoundaryTarget> {
    for attr in &item_fn.attrs {
        let path = attr.path();
        let segments: Vec<_> = path.segments.iter().collect();
        if segments.len() == 2
            && segments[0].ident == "provekit"
            && segments[1].ident == "boundary"
        {
            if let Ok(meta_list) = attr.meta.require_list() {
                let args = parse_attr_named_args(&meta_list.tokens);
                let concept = args.string("concept").unwrap_or_default();
                let library = args.string("library").unwrap_or_default();
                if !concept.is_empty() && !library.is_empty() {
                    return Some(BoundaryTarget {
                        concept,
                        library,
                        version: args.string("version"),
                        family: args.string("family"),
                        api: args.string("api"),
                        boundary_contract: args.string("boundary_contract"),
                        loss: args.string_array("loss"),
                        source_function_name: String::new(),
                    });
                }
            }
        }
    }
    None
}

fn extract_refuse_attr(item_mod: &syn::ItemMod) -> Option<RefuseTarget> {
    for attr in &item_mod.attrs {
        let path = attr.path();
        let segments: Vec<_> = path.segments.iter().collect();
        if segments.len() == 2 && segments[0].ident == "provekit" && segments[1].ident == "refuse" {
            if let Ok(meta_list) = attr.meta.require_list() {
                let args = parse_attr_named_args(&meta_list.tokens);
                let surface = args.string("surface").unwrap_or_default();
                let concept = args.string("concept").unwrap_or_default();
                let reason = args.string("reason").unwrap_or_default();
                let would_close_with_cluster =
                    args.string("would_close_with_cluster").unwrap_or_default();
                if !surface.is_empty()
                    && !concept.is_empty()
                    && !reason.is_empty()
                    && !would_close_with_cluster.is_empty()
                {
                    return Some(RefuseTarget {
                        surface,
                        concept,
                        reason,
                        would_close_with_cluster,
                    });
                }
            }
        }
    }
    None
}

#[derive(Debug, Default)]
struct ParsedAttrArgs {
    strings: std::collections::BTreeMap<String, String>,
    string_arrays: std::collections::BTreeMap<String, Vec<String>>,
}

impl ParsedAttrArgs {
    fn string(&self, key: &str) -> Option<String> {
        self.strings.get(key).cloned()
    }

    fn string_array(&self, key: &str) -> Vec<String> {
        self.string_arrays.get(key).cloned().unwrap_or_default()
    }
}

fn parse_attr_named_args(tokens: &proc_macro2::TokenStream) -> ParsedAttrArgs {
    let mut out = ParsedAttrArgs::default();
    let tokens: Vec<_> = tokens.clone().into_iter().collect();
    let mut i = 0;
    while i < tokens.len() {
        let key = match &tokens[i] {
            proc_macro2::TokenTree::Ident(ident) => ident.to_string(),
            _ => {
                i += 1;
                continue;
            }
        };
        let is_eq = matches!(tokens.get(i + 1), Some(proc_macro2::TokenTree::Punct(p)) if p.as_char() == '=');
        if !is_eq {
            i += 1;
            continue;
        }
        match tokens.get(i + 2) {
            Some(proc_macro2::TokenTree::Literal(lit)) => {
                if let Some(unquoted) = unquote_string_literal(&lit.to_string()) {
                    out.strings.insert(key, unquoted);
                }
                i += 3;
            }
            Some(proc_macro2::TokenTree::Group(group))
                if group.delimiter() == proc_macro2::Delimiter::Bracket =>
            {
                let entries: Vec<String> = group
                    .stream()
                    .into_iter()
                    .filter_map(|tt| match tt {
                        proc_macro2::TokenTree::Literal(lit) => {
                            unquote_string_literal(&lit.to_string())
                        }
                        _ => None,
                    })
                    .collect();
                out.string_arrays.insert(key, entries);
                i += 3;
            }
            _ => {
                i += 1;
                continue;
            }
        }
        if let Some(proc_macro2::TokenTree::Punct(p)) = tokens.get(i) {
            if p.as_char() == ',' {
                i += 1;
            }
        }
    }
    out
}

fn unquote_string_literal(raw: &str) -> Option<String> {
    if raw.starts_with('"') && raw.ends_with('"') && raw.len() >= 2 {
        Some(raw[1..raw.len() - 1].to_string())
    } else {
        None
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

/// Catalog-driven rust-source-syntax → concept-hub sort CID (#1370).
///
/// NO hardcoded source-token names. Reads kit-source-alias mementos via
/// libprovekit::core::lower_plugin::load_kit_source_aliases("rust") and
/// dispatches via the recursive resolver. Parametric types emit composite
/// CIDs computed via content-addressing; expansions are accumulated for
/// realize-side dispatch.
fn rust_source_type_to_concept_hub_sort_cid(
    rust_type: &str,
    expansions: &mut Vec<libprovekit::core::lower_plugin::ParametricSortExpansion>,
) -> Option<String> {
    let aliases = RUST_ALIASES.get_or_init(|| {
        libprovekit::core::lower_plugin::load_kit_source_aliases("rust")
    });
    libprovekit::core::lower_plugin::rust_type_to_concept_hub_sort_cid(
        rust_type, aliases, expansions,
    )
}

static RUST_ALIASES: OnceLock<
    std::collections::BTreeMap<String, libprovekit::core::lower_plugin::KitSourceAliasEntry>,
> = OnceLock::new();

fn sugar_param_types(item_fn: &syn::ItemFn) -> Vec<String> {
    // Build a map from type-parameter ident to its FIRST trait bound.
    // For `fn f<A: AdapterLifter>(a: A)`, the param_type for `a` is
    // emitted as "AdapterLifter" so the lower side has the right
    // method-resolution target instead of the erased Object.
    let mut bounds: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for gp in &item_fn.sig.generics.params {
        if let syn::GenericParam::Type(tp) = gp {
            let name = tp.ident.to_string();
            for b in &tp.bounds {
                if let syn::TypeParamBound::Trait(tb) = b {
                    if let Some(last) = tb.path.segments.last() {
                        bounds.insert(name.clone(), last.ident.to_string());
                        break;
                    }
                }
            }
        }
    }
    item_fn
        .sig
        .inputs
        .iter()
        .filter_map(|arg| match arg {
            syn::FnArg::Typed(pat_type) => {
                let raw = sugar_type_surface(&pat_type.ty);
                // Strip `&` / `&mut ` prefix for the bound lookup, then
                // re-apply for non-bound types.
                let stripped = raw
                    .trim_start_matches("&mut")
                    .trim_start_matches("&")
                    .trim()
                    .to_string();
                if let Some(bound) = bounds.get(&stripped) {
                    Some(bound.clone())
                } else {
                    Some(raw)
                }
            }
            _ => None,
        })
        .collect()
}

fn sugar_return_type(item_fn: &syn::ItemFn) -> String {
    match &item_fn.sig.output {
        syn::ReturnType::Default => "()".to_string(),
        syn::ReturnType::Type(_, ty) => sugar_type_surface(ty),
    }
}

/// Original param types as written in source (no trait-bound substitution).
/// Preserves generic-param references like `&A` so realize can emit the
/// signature byte-identical. param_types (above) carries the substituted
/// form for body-template matching.
fn sugar_original_param_types(item_fn: &syn::ItemFn) -> Vec<String> {
    item_fn
        .sig
        .inputs
        .iter()
        .filter_map(|arg| match arg {
            syn::FnArg::Typed(pat_type) => Some(sugar_type_surface(&pat_type.ty)),
            _ => None,
        })
        .collect()
}

/// Generic parameter declarations as a single string suitable for
/// inserting between `fn name` and `(`. Empty if no generics.
/// Example: `<A: AdapterLifter, T>`.
fn sugar_generic_params(item_fn: &syn::ItemFn) -> String {
    use quote::ToTokens;
    if item_fn.sig.generics.params.is_empty() {
        String::new()
    } else {
        item_fn.sig.generics.to_token_stream().to_string()
    }
}

fn sugar_type_surface(ty: &syn::Type) -> String {
    use quote::ToTokens;
    ty.to_token_stream().to_string().replace(' ', "")
}

fn sugar_body_source(rel: &str, src: &str, item_fn: &syn::ItemFn) -> Value {
    let start = item_fn.sig.fn_token.span.start();
    let end = item_fn.block.span().end();
    let lines: Vec<&str> = src.lines().collect();
    let span_text = if start.line > 0 && end.line >= start.line && end.line <= lines.len() {
        lines[start.line - 1..end.line].join("\n") + "\n"
    } else {
        String::new()
    };
    // Extract just the body block (between the outermost `{` and matching
    // `}`) so cmd_mint can project a substrate-honest body-templates JSON
    // from the envelope without re-reading source files at mint time. The
    // full span_text is still hashed for `source_cid`.
    let body_block = extract_block_body(&span_text);
    json!({
        "file": rel,
        "span": {
            "start_line": start.line,
            "start_col": start.column,
            "end_line": end.line,
            "end_col": end.column,
        },
        "source_cid": blake3_512_of(span_text.as_bytes()),
        "body_text": body_block,
    })
}

/// Extract the contents between the outermost `{` and matching `}` of a
/// function span. Returns the trimmed inner block. The matching tracks
/// nested braces but does not parse strings or comments; the shim's
/// wrapper-fn bodies are short and balanced, so simple matching suffices.
fn extract_block_body(span_text: &str) -> String {
    let bytes = span_text.as_bytes();
    let mut start: Option<usize> = None;
    let mut depth: i32 = 0;
    let mut end: Option<usize> = None;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'{' => {
                if start.is_none() {
                    start = Some(i + 1);
                }
                depth += 1;
            }
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    match (start, end) {
        (Some(s), Some(e)) if e >= s => span_text[s..e].trim().to_string(),
        _ => String::new(),
    }
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
        syn::Stmt::Local(local) => {
            let Some(init) = local.init.as_ref() else {
                return BindingResult::default();
            };
            // `let _ = X;` mirrors shape_of_stmt: args = [wildcard_literal,
            // init_shape]. No operand binding for the literal slot.
            if matches!(&local.pat, syn::Pat::Wild(_)) {
                return operation_binding_result(vec![
                    BindingResult::default(),
                    bindings_of_expr(&init.expr, ctx),
                ]);
            }
            let Some(symbol) = local_binding_symbol(local) else {
                return bindings_of_expr(&init.expr, ctx);
            };
            operation_binding_result(vec![
                binding_result_for_symbol(symbol),
                bindings_of_expr(&init.expr, ctx),
            ])
        }
        _ => BindingResult::default(),
    }
}

fn bindings_of_expr(expr: &syn::Expr, ctx: &ShapeContext) -> BindingResult {
    match expr {
        // Structural control flow — thread bindings through each substrate
        // operator's argument layout to match shape_of_expr's emission.
        // Without this, references inside condition/body lose their
        // operand_binding entries and downstream address resolution can't
        // see them at the expected positions.
        syn::Expr::If(e) => {
            let cond = bindings_of_expr(&e.cond, ctx);
            let then_branch = bindings_of_block(&e.then_branch, ctx);
            let else_branch = match &e.else_branch {
                Some((_, else_expr)) => bindings_of_expr(else_expr, ctx),
                None => BindingResult { has_operator: true, bindings: Vec::new() },
            };
            operation_binding_result(vec![cond, then_branch, else_branch])
        }
        syn::Expr::While(e) => {
            // Match the shape side's treatment of `while let` — emit
            // [empty_var_slot, value_bindings, body_bindings] so the
            // arities align with concept:while-let.
            if let syn::Expr::Let(let_expr) = &*e.cond {
                let value_bindings = bindings_of_expr(&let_expr.expr, ctx);
                let body = bindings_of_block(&e.body, ctx);
                return operation_binding_result(vec![
                    BindingResult::default(),
                    value_bindings,
                    body,
                ]);
            }
            let cond = bindings_of_expr(&e.cond, ctx);
            let body = bindings_of_block(&e.body, ctx);
            operation_binding_result(vec![cond, body])
        }
        syn::Expr::ForLoop(e) => {
            // concept:for-each(var, iterable, body); var is a symbol leaf (no binding slot).
            let iterable = bindings_of_expr(&e.expr, ctx);
            let body = bindings_of_block(&e.body, ctx);
            operation_binding_result(vec![BindingResult::default(), iterable, body])
        }
        syn::Expr::Loop(e) => {
            // concept:while(true_literal_leaf, body)
            let body = bindings_of_block(&e.body, ctx);
            operation_binding_result(vec![BindingResult::default(), body])
        }
        syn::Expr::Match(e) => {
            // concept:match(scrutinee, arm1, arm2, ...)
            // Each arm: concept:match-arm(pattern_leaf, body) — pattern is leaf, body has bindings.
            let mut args = vec![bindings_of_expr(&e.expr, ctx)];
            for arm in &e.arms {
                let body_bindings = bindings_of_expr(&arm.body, ctx);
                args.push(operation_binding_result(vec![
                    BindingResult::default(),
                    body_bindings,
                ]));
            }
            operation_binding_result(args)
        }
        syn::Expr::Cast(c) => {
            // concept:cast(value, type_leaf)
            operation_binding_result(vec![
                bindings_of_expr(&c.expr, ctx),
                BindingResult::default(),
            ])
        }
        syn::Expr::Index(idx) => {
            // concept:index(receiver, index)
            operation_binding_result(vec![
                bindings_of_expr(&idx.expr, ctx),
                bindings_of_expr(&idx.index, ctx),
            ])
        }
        syn::Expr::Field(f) => {
            // concept:field(receiver, field_leaf)
            operation_binding_result(vec![
                bindings_of_expr(&f.base, ctx),
                BindingResult::default(),
            ])
        }
        syn::Expr::Return(e) => {
            let args = e
                .expr
                .as_ref()
                .map(|expr| vec![bindings_of_expr(expr, ctx)])
                .unwrap_or_default();
            operation_binding_result(args)
        }
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
        syn::Expr::Call(e) => {
            // Callee path leaf is at args[0] (no operand binding — it's an
            // inline identifier, not a parameter symbol). Call arguments
            // are at args[1..], matching the shape_of_expr layout below.
            let mut args = vec![BindingResult::default()]; // slot for callee leaf
            args.extend(e.args.iter().map(|arg| bindings_of_expr(arg, ctx)));
            operation_binding_result(args)
        }
        syn::Expr::MethodCall(e) => {
            // Receiver is at args[0]. Method ident leaf is at args[1] (no
            // operand binding). Call arguments are at args[2..], matching
            // the shape_of_expr layout below.
            let mut args = vec![
                bindings_of_expr(&e.receiver, ctx),
                BindingResult::default(), // slot for method ident leaf
            ];
            args.extend(e.args.iter().map(|arg| bindings_of_expr(arg, ctx)));
            operation_binding_result(args)
        }
        // concept:array-repeat: args[0]=elem bindings, args[1]=len (no symbol).
        syn::Expr::Repeat(e) => operation_binding_result(vec![
            bindings_of_expr(&e.expr, ctx),
            BindingResult::default(), // len is a leaf, no operand binding slot
        ]),
        // concept:ref: args[0]=inner bindings, args[1]=mutability leaf (no symbol).
        syn::Expr::Reference(e) => operation_binding_result(vec![
            bindings_of_expr(&e.expr, ctx),
            BindingResult::default(), // mutability leaf, no operand binding slot
        ]),
        // Expr::Macro is lifted as a concept:literal source_text leaf — no
        // inner operand bindings to thread through.
        syn::Expr::Macro(_) => BindingResult::default(),
        // concept:closure: args[0]=body bindings, args[1..]=param literal slots
        // (no operand bindings — they're concept:literal source_text leaves).
        // Closure-introduced param symbols flow through the existing
        // Expr::Path -> operand_symbol path when referenced in the body;
        // that's the McCarthy address resolution.
        syn::Expr::Closure(e) => {
            let mut arg_bindings = vec![bindings_of_expr(&e.body, ctx)];
            for _ in &e.inputs {
                arg_bindings.push(BindingResult::default());
            }
            operation_binding_result(arg_bindings)
        }
        syn::Expr::Block(b) => bindings_of_block(&b.block, ctx),
        syn::Expr::Paren(e) => bindings_of_expr(&e.expr, ctx),
        syn::Expr::Group(e) => bindings_of_expr(&e.expr, ctx),
        _ => {
            // Single-identifier path → only emit a binding when the
            // identifier is a KNOWN scoped name (function param / local).
            // Free identifiers (None, Some, Vec::new, etc.) fall through
            // to the structural kind:"symbol" leaf in shape_of_expr —
            // emitting a binding for them shadowed the leaf at use site
            // because realize checks operand_bindings BEFORE kind:symbol.
            if let Some(symbol) = operand_symbol(expr) {
                // Use scoped_names (all in-scope identifiers) instead of
                // ctx.vars (only sort-inferred). Params/locals of non-modeled
                // types (refs, String, custom) belong in scope but aren't
                // in vars — without scoped_names they were treated as free
                // symbols and lost their operand_binding.
                if ctx.scoped_names.contains(&symbol) {
                    return binding_result_for_symbol(symbol);
                }
            }
            BindingResult::default()
        }
    }
}

fn binding_result_for_symbol(symbol: String) -> BindingResult {
    BindingResult {
        has_operator: false,
        bindings: vec![OperandBinding {
            position: Vec::new(),
            symbol,
        }],
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
        syn::Lit::Int(value) => Some(value.to_string()),
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
    /// Identifier → inferred sort. Only populated when the type is in
    /// ShapeSort's modeled set (Int/Float/Bool/String/Bytes/...). Used
    /// for sort-aware lifts (e.g. binary op result-sort inference).
    vars: BTreeMap<String, ShapeSort>,
    /// All scoped identifiers, regardless of whether their type is sort-
    /// inferable. Distinct from `vars` because params/locals with non-
    /// modeled types (refs, String, custom types) belong in scope but
    /// not in `vars`. Used by bindings_of_expr to distinguish scoped
    /// identifiers (emit operand_binding) from free identifiers
    /// (None/Some/path leaves — let kind:"symbol" handle them).
    scoped_names: BTreeSet<String>,
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
        // Always track the name in scope, even when sort isn't inferable —
        // bindings_of_expr needs this to recognize the name as scoped.
        self.scoped_names.insert(name.clone());
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

fn local_binding_symbol(local: &syn::Local) -> Option<String> {
    match &local.pat {
        syn::Pat::Ident(ident) => Some(ident.ident.to_string()),
        syn::Pat::Type(pat_type) => {
            let syn::Pat::Ident(ident) = &*pat_type.pat else {
                return None;
            };
            Some(ident.ident.to_string())
        }
        _ => None,
    }
}

fn local_binding_is_mut(local: &syn::Local) -> bool {
    match &local.pat {
        syn::Pat::Ident(ident) => ident.mutability.is_some(),
        syn::Pat::Type(pat_type) => {
            let syn::Pat::Ident(ident) = &*pat_type.pat else {
                return false;
            };
            ident.mutability.is_some()
        }
        _ => false,
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
        // Function-local item declaration: `const X: T = expr;`, `static X`,
        // inner `fn`, etc. First-class concept:item-decl carrying the
        // verbatim source. Realize-side emits it back into the function body.
        // (Full structural lift of items is future work — preserving source
        // verbatim is the minimal byte-identical surface.)
        syn::Stmt::Item(item) => {
            use quote::ToTokens;
            let source = item.to_token_stream().to_string();
            gamma_operation("concept:item-decl", vec![
                CValue::object([
                    ("kind", CValue::string("symbol")),
                    ("text", CValue::string(source)),
                ]),
            ])
        }
        syn::Stmt::Local(local) => {
            let Some(init) = local.init.as_ref() else {
                return non_operation_shape();
            };
            // `let _ = X;` — Pat::Wild discard binding. Target is a symbol
            // leaf "_" (substrate-canonical name; same shape walk_rpc uses
            // for other named bindings). The realize side detects "_" and
            // emits the wildcard form. No source_text — the underscore IS
            // the substrate name, not a kit-specific source token.
            if matches!(&local.pat, syn::Pat::Wild(_)) {
                let target_leaf = CValue::object([
                    ("kind", CValue::string("symbol")),
                    ("text", CValue::string("_")),
                ]);
                return gamma_operation(
                    "concept:assign",
                    vec![target_leaf, shape_of_expr(&init.expr, ctx)],
                );
            }
            // Struct destructuring: `let TypeName { field1, field2 } = expr`
            // → first-class concept:destructure-struct(value, type_leaf,
            //   field1_name_leaf, ..., fieldN_name_leaf).
            // Realize-side: rust emits the source's destructure pattern;
            // java/etc emit field-by-field getter calls.
            if let syn::Pat::Struct(struct_pat) = &local.pat {
                use quote::ToTokens;
                let type_text = struct_pat.path.to_token_stream().to_string();
                let mut args: Vec<Arc<CValue>> = Vec::new();
                args.push(shape_of_expr(&init.expr, ctx));
                args.push(CValue::object([
                    ("kind", CValue::string("type")),
                    ("text", CValue::string(type_text)),
                ]));
                for field in &struct_pat.fields {
                    let field_name = match &field.member {
                        syn::Member::Named(ident) => ident.to_string(),
                        syn::Member::Unnamed(idx) => idx.index.to_string(),
                    };
                    let binding_name = match &*field.pat {
                        syn::Pat::Ident(pi) => pi.ident.to_string(),
                        _ => field_name.clone(),
                    };
                    args.push(CValue::object([
                        ("kind", CValue::string("symbol")),
                        ("text", CValue::string(binding_name)),
                        ("field_name", CValue::string(field_name)),
                    ]));
                }
                return gamma_operation("concept:destructure-struct", args);
            }
            // Tuple destructuring: `let (a, b, c) = expr` — first-class
            // concept:destructure-tuple(value, name1, name2, name3, ...).
            // Realize-side translates to `let (a, b, c) = expr` (rust-native)
            // OR to a temp + indexed assigns (java/etc).
            if let syn::Pat::Tuple(tuple_pat) = &local.pat {
                let mut name_leaves: Vec<Arc<CValue>> = Vec::new();
                for elem in &tuple_pat.elems {
                    let name = match elem {
                        syn::Pat::Ident(pi) => pi.ident.to_string(),
                        syn::Pat::Wild(_) => "_".to_string(),
                        other => {
                            use quote::ToTokens;
                            other.to_token_stream().to_string()
                        }
                    };
                    name_leaves.push(CValue::object([
                        ("kind", CValue::string("symbol")),
                        ("text", CValue::string(name)),
                    ]));
                }
                if !name_leaves.is_empty() {
                    let mut args = vec![shape_of_expr(&init.expr, ctx)];
                    args.extend(name_leaves);
                    return gamma_operation("concept:destructure-tuple", args);
                }
            }
            if let Some(binding_name) = local_binding_symbol(local) {
                // Substrate-honest let-binding: the binding NAME is data,
                // emit it as a kind:"symbol" leaf in the target slot.
                // (Previously emitted as non_operation_shape {} relying on
                // operand_bindings position-resolution — fragile because
                // for nested lets in loop / conditional bodies, the bindings
                // collector doesn't always thread the binding name to the
                // expected position.)
                // args[0] = target (symbol leaf with binding name)
                // args[1] = value expression
                // args[2] = mutability flag leaf when `let mut`, omitted otherwise
                // Preserve explicit `: Type` annotation when present.
                // Stored on the symbol leaf as `let_type` so the lower side
                // can emit `let X: Type = value` byte-identical with source.
                let explicit_type = if let syn::Pat::Type(pat_type) = &local.pat {
                    use quote::ToTokens;
                    Some(pat_type.ty.to_token_stream().to_string())
                } else {
                    None
                };
                let target_leaf = if let Some(ty) = explicit_type {
                    CValue::object([
                        ("kind", CValue::string("symbol")),
                        ("text", CValue::string(binding_name)),
                        ("let_type", CValue::string(ty)),
                    ])
                } else {
                    CValue::object([
                        ("kind", CValue::string("symbol")),
                        ("text", CValue::string(binding_name)),
                    ])
                };
                let mut assign_args = vec![
                    target_leaf,
                    shape_of_expr(&init.expr, ctx),
                ];
                if local_binding_is_mut(local) {
                    // Emit a concept:literal boolean true as the mutability flag.
                    let Some(op_cid) = concept_op_cid("concept:literal") else {
                        return non_operation_shape();
                    };
                    assign_args.push(CValue::object([
                        ("args", CValue::array(Vec::new())),
                        ("concept_name", CValue::string("concept:literal")),
                        ("op_cid", CValue::string(op_cid)),
                        ("sort", CValue::string(SORT_BOOL_CID)),
                        ("value", CValue::boolean(true)),
                    ]));
                }
                return gamma_operation("concept:assign", assign_args);
            }
            shape_of_expr(&init.expr, ctx)
        }
        _ => non_operation_shape(),
    }
}

fn shape_of_expr(expr: &syn::Expr, ctx: &ShapeContext) -> Arc<CValue> {
    match expr {
        // Structural control-flow lifts (source_text fallback removed):
        // - if/else → concept:conditional(cond, then, else)
        // - while → concept:while(cond, body)
        // - for x in iter → concept:for-each(var, iterable, body)
        // - loop → concept:while(true, body)  (decomposed via existing primitives)
        syn::Expr::If(e) => {
            // Detect `if let PATTERN = EXPR { body }` — rust models this
            // as ExprIf with cond = Expr::Let(pat, expr). Re-encode as
            //   { let var = EXPR; if var != null { body } else { else } }
            // using existing catalog concepts.
            if let syn::Expr::Let(let_expr) = &*e.cond {
                // First-class concept:if-let(pattern_leaf, expr, then, else).
                // Pattern text preserved so lower emits the source idiom.
                use quote::ToTokens;
                let pattern_text = let_expr.pat.to_token_stream().to_string();
                let pattern_leaf = CValue::object([
                    ("kind", CValue::string("symbol")),
                    ("text", CValue::string(pattern_text)),
                ]);
                let value = shape_of_expr(&let_expr.expr, ctx);
                let then_shape = shape_of_block(&e.then_branch, ctx);
                let else_shape = match &e.else_branch {
                    Some((_, expr)) => shape_of_expr(expr, ctx),
                    None => gamma_operation("concept:skip", Vec::new()),
                };
                return gamma_operation(
                    "concept:if-let",
                    vec![pattern_leaf, value, then_shape, else_shape],
                );
            }
            let cond = shape_of_expr(&e.cond, ctx);
            let then_shape = shape_of_block(&e.then_branch, ctx);
            let else_shape = match &e.else_branch {
                Some((_, expr)) => shape_of_expr(expr, ctx),
                None => gamma_operation("concept:skip", Vec::new()),
            };
            gamma_operation("concept:conditional", vec![cond, then_shape, else_shape])
        }
        syn::Expr::While(e) => {
            // Detect `while let PATTERN = EXPR { body }` — rust models this
            // as ExprWhile with cond = Expr::Let(pat, expr). The pattern
            // binds a variable that's in scope inside body. Lift as
            // concept:while-let(var_leaf, expr, body) so the lower side
            // can emit `while ((var = expr) != null) { body }` or the
            // equivalent in each target.
            if let syn::Expr::Let(let_expr) = &*e.cond {
                // First-class concept:while-let(pattern_leaf, expr, body).
                // The pattern's full text is preserved (e.g. `Some(line)`)
                // so the lower side can emit the original `while let
                // PATTERN = EXPR { body }` byte-identical with source.
                // Synthetic decomposition into while-true + assign + break
                // is a target-side translation, not a substrate concept.
                use quote::ToTokens;
                let pattern_text = let_expr.pat.to_token_stream().to_string();
                let pattern_leaf = CValue::object([
                    ("kind", CValue::string("symbol")),
                    ("text", CValue::string(pattern_text)),
                ]);
                let value = shape_of_expr(&let_expr.expr, ctx);
                let body = shape_of_block(&e.body, ctx);
                return gamma_operation(
                    "concept:while-let",
                    vec![pattern_leaf, value, body],
                );
            }
            let cond = shape_of_expr(&e.cond, ctx);
            let body = shape_of_block(&e.body, ctx);
            gamma_operation("concept:while", vec![cond, body])
        }
        syn::Expr::ForLoop(e) => {
            // `for pat in expr { body }` — concept:for-each(var, iterable, body).
            // The pattern is captured as a leaf binding by name (full pattern
            // destructuring resolution is follow-up; matches walk_rpc's
            // existing pattern-as-leaf convention).
            let var = match &*e.pat {
                syn::Pat::Ident(p) => {
                    // Preserve `for mut x in ...` mutability marker.
                    let mut fields = vec![
                        ("kind", CValue::string("symbol")),
                        ("text", CValue::string(p.ident.to_string())),
                    ];
                    if p.mutability.is_some() {
                        fields.push(("mut", CValue::boolean(true)));
                    }
                    CValue::object(fields)
                }
                other => {
                    use quote::ToTokens;
                    CValue::object([
                        ("kind", CValue::string("symbol")),
                        ("text", CValue::string(other.to_token_stream().to_string())),
                    ])
                }
            };
            let iterable = shape_of_expr(&e.expr, ctx);
            let body = shape_of_block(&e.body, ctx);
            gamma_operation("concept:for-each", vec![var, iterable, body])
        }
        syn::Expr::Loop(e) => {
            // `loop { body }` ≡ `while true { body }` — decompose via concept:literal(true).
            let true_lit = {
                let Some(op_cid) = concept_op_cid("concept:literal") else {
                    return non_operation_shape();
                };
                CValue::object([
                    ("args", CValue::array(Vec::new())),
                    ("concept_name", CValue::string("concept:literal")),
                    ("op_cid", CValue::string(op_cid)),
                    ("sort", CValue::string(SORT_BOOL_CID)),
                    ("value", CValue::boolean(true)),
                ])
            };
            let body = shape_of_block(&e.body, ctx);
            gamma_operation("concept:while", vec![true_lit, body])
        }
        syn::Expr::Return(e) => {
            let args = e
                .expr
                .as_ref()
                .map(|expr| vec![shape_of_expr(expr, ctx)])
                .unwrap_or_default();
            gamma_operation("concept:return", args)
        }
        // `(a, b, c)` — rust tuple literal. No first-class tuple concept
        // in the catalog; encode as concept:call with synthetic path leaf
        // `__provekit_tuple_new`. The lower side detects this name and
        // emits target-appropriate tuple constructor (e.g. Object[] in java).
        syn::Expr::Tuple(e) => {
            let callee = CValue::object([
                ("kind", CValue::string("path")),
                ("text", CValue::string("__provekit_tuple_new")),
            ]);
            let mut args = vec![callee];
            for elem in &e.elems {
                args.push(shape_of_expr(elem, ctx));
            }
            gamma_operation("concept:call", args)
        }
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
        syn::Expr::Call(e) => {
            // Extract callee path text from `func`. Only Expr::Path callees are
            // recognized; anything else (closures, parens, etc.) falls through to
            // non_operation_shape so we never fabricate a callee.
            let callee_text = if let syn::Expr::Path(path_expr) = &*e.func {
                path_expr
                    .path
                    .segments
                    .iter()
                    .map(|seg| seg.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::")
            } else {
                return non_operation_shape();
            };
            // args[0]: callee path leaf (kind:"path", text:"blake3::Hasher::new")
            // args[1..]: call arguments, matching bindings_of_expr layout above.
            let callee_leaf = CValue::object([
                ("kind", CValue::string("path")),
                ("text", CValue::string(callee_text)),
            ]);
            let mut args = vec![callee_leaf];
            args.extend(e.args.iter().map(|arg| shape_of_expr(arg, ctx)));
            gamma_operation("concept:call", args)
        }
        syn::Expr::MethodCall(e) => {
            // args[0]: receiver shape, matching bindings_of_expr layout above.
            // args[1]: canonical method-concept leaf (kind:"method",
            // concept_name:"method:<name>", arity:<n>, op_cid:<derived>).
            // The CID is determined by structure — no minting required.
            // args[2..]: call arguments.
            let method_leaf = method_concept_leaf(&e.method.to_string(), e.args.len());
            let mut args = vec![
                shape_of_expr(&e.receiver, ctx),
                method_leaf,
            ];
            args.extend(e.args.iter().map(|arg| shape_of_expr(arg, ctx)));
            gamma_operation("concept:call", args)
        }
        syn::Expr::Lit(lit) => literal_shape(&lit.lit),
        // `expr?` — rust's Try operator. First-class concept:try(inner)
        // preserves the source form for byte-identical rust round-trip;
        // target plugins translate to language-appropriate unwrap (java
        // gets .try_unwrap() via the method-concept catalog mapping).
        syn::Expr::Try(e) => {
            let inner = shape_of_expr(&e.expr, ctx);
            gamma_operation("concept:try", vec![inner])
        }
        // [elem; count] syntax — emitted as concept:array-repeat with args [elem, len].
        syn::Expr::Repeat(e) => gamma_operation(
            "concept:array-repeat",
            vec![shape_of_expr(&e.expr, ctx), shape_of_expr(&e.len, ctx)],
        ),
        // &expr and &mut expr — emitted as concept:ref with args [inner, mutability_leaf].
        // mutability_leaf: {kind:"mutability", text:"mut"} or {kind:"mutability", text:""}.
        syn::Expr::Reference(e) => {
            let mut_text = if e.mutability.is_some() { "mut" } else { "" };
            let mut_leaf = CValue::object([
                ("kind", CValue::string("mutability")),
                ("text", CValue::string(mut_text)),
            ]);
            gamma_operation(
                "concept:ref",
                vec![shape_of_expr(&e.expr, ctx), mut_leaf],
            )
        }
        // Structural match: concept:match(scrutinee, arm1, arm2, ...).
        // Each arm is concept:match-arm(pattern, body). Pattern is a
        // symbol leaf carrying the textual form (full pattern decomposition
        // to concept:literal-pattern / concept:constructor-pattern / etc.
        // is follow-up substrate-mint work; for now patterns live as
        // symbol-leaf substrate names with kit-side pattern parsing).
        syn::Expr::Match(e) => {
            use quote::ToTokens;
            let mut args = vec![shape_of_expr(&e.expr, ctx)];
            for arm in &e.arms {
                let mut pattern_text = arm.pat.to_token_stream().to_string();
                // Carry the guard inline in the pattern text so the
                // realize side reconstructs `Pattern if Cond => Body`.
                // Storing it in the pattern leaf keeps the substrate
                // shape stable (no new concept-arity yet); future work
                // can split guard into its own structural slot.
                if let Some((_, guard)) = &arm.guard {
                    pattern_text.push_str(" if ");
                    pattern_text.push_str(&guard.to_token_stream().to_string());
                }
                let pattern_leaf = CValue::object([
                    ("kind", CValue::string("symbol")),
                    ("text", CValue::string(pattern_text)),
                ]);
                let body = shape_of_expr(&arm.body, ctx);
                args.push(gamma_operation("concept:match-arm", vec![pattern_leaf, body]));
            }
            gamma_operation("concept:match", args)
        }
        // Macro invocation: `writeln!(handle, "{}", line)` and similar.
        // concept:macro-call(path, args...) — path is symbol leaf for the
        // macro name; args are the tokens as a single symbol leaf
        // (structural decomposition of macro tokens to substrate primitives
        // is follow-up; macros are syntactic — their structure isn't
        // semantically meaningful until the macro expands).
        syn::Expr::Macro(e) => {
            use quote::ToTokens;
            let path = e.mac.path.to_token_stream().to_string().replace(' ', "");
            let path_leaf = CValue::object([
                ("kind", CValue::string("symbol")),
                ("text", CValue::string(path)),
            ]);
            let tokens = e.mac.tokens.to_string();
            let formatted = format_macro_tokens(&tokens);
            let args_leaf = CValue::object([
                ("kind", CValue::string("symbol")),
                ("text", CValue::string(formatted)),
            ]);
            gamma_operation("concept:macro-call", vec![path_leaf, args_leaf])
        }
        // |param1, param2, ...| body — emitted as concept:closure with
        // args = [body_shape, param1_literal, param2_literal, ...]. Body
        // at args[0], param-name literals at args[1..]. No new substrate
        // concept:closure (already in catalog as abstraction). Param names
        // are symbol leaves — substrate-canonical names, NOT source text.
        // The realize side spells each per its own convention.
        //
        // Closure-introduced bindings flow through the existing
        // Expr::Path -> operand_symbol path at use site (McCarthy address
        // resolution); we do NOT pre-bind closure params here.
        syn::Expr::Closure(e) => {
            let mut closure_args: Vec<Arc<CValue>> = Vec::new();
            // Record source form choice: block-body vs expression-body.
            // `|e| { ... }` is Expr::Block (block-form); `|e| expr` is
            // any other expression. The lower side uses this to emit
            // byte-identical surface — block form lets rustfmt split
            // long lines, expression form keeps them inline.
            let is_block_body = matches!(&*e.body, syn::Expr::Block(_));
            let body_shape = shape_of_expr(&e.body, ctx);
            // Attach closure_block_body marker on a wrapper. Cloning the
            // inner object's fields and re-wrapping is simpler than
            // get_mut-and-mutate (Arc has multiple refs in the IR tree).
            let body_shape = if is_block_body {
                let cloned = (*body_shape).clone();
                match cloned {
                    CValue::Object(mut fields) => {
                        // CValue::Object is Vec<(String, Arc<Value>)>
                        let val: Arc<CValue> = CValue::boolean(true);
                        fields.push(("closure_block_body".to_string(), val));
                        Arc::new(CValue::Object(fields))
                    }
                    other => Arc::new(other),
                }
            } else {
                body_shape
            };
            closure_args.push(body_shape);
            for input in &e.inputs {
                let syn::Pat::Ident(pat) = input else {
                    return non_operation_shape();
                };
                closure_args.push(CValue::object([
                    ("kind", CValue::string("symbol")),
                    ("text", CValue::string(pat.ident.to_string())),
                ]));
            }
            gamma_operation("concept:closure", closure_args)
        }
        syn::Expr::Block(b) => shape_of_block(&b.block, ctx),
        syn::Expr::Paren(e) => shape_of_expr(&e.expr, ctx),
        syn::Expr::Group(e) => shape_of_expr(&e.expr, ctx),
        // Cast: `value as TargetType` → concept:cast(value_shape, type_symbol_leaf).
        syn::Expr::Cast(c) => {
            use quote::ToTokens;
            let value = shape_of_expr(&c.expr, ctx);
            let type_text = c.ty.to_token_stream().to_string().replace(' ', "");
            let type_leaf = CValue::object([
                ("kind", CValue::string("symbol")),
                ("text", CValue::string(type_text)),
            ]);
            gamma_operation("concept:cast", vec![value, type_leaf])
        }
        // Indexed access: `receiver[index]` → concept:index(receiver, index).
        syn::Expr::Index(idx) => {
            let receiver = shape_of_expr(&idx.expr, ctx);
            let index = shape_of_expr(&idx.index, ctx);
            gamma_operation("concept:index", vec![receiver, index])
        }
        // Field access: `receiver.field` (and `receiver.0` tuple field) →
        // concept:field(receiver_shape, field_symbol_leaf).
        syn::Expr::Field(f) => {
            let receiver = shape_of_expr(&f.base, ctx);
            let field_text = match &f.member {
                syn::Member::Named(ident) => ident.to_string(),
                syn::Member::Unnamed(idx) => idx.index.to_string(),
            };
            let field_leaf = CValue::object([
                ("kind", CValue::string("symbol")),
                ("text", CValue::string(field_text)),
            ]);
            gamma_operation("concept:field", vec![receiver, field_leaf])
        }
        // Free path identifier (e.g. None, Some, Vec::new, Ordering::Less).
        // Emit as substrate-canonical symbol leaf so deeper consumers (match
        // arm bodies, conditional branches) can lower them without depending
        // on operand_bindings position threading.
        //
        // Note: parameter references ALSO go through Expr::Path. Those are
        // handled at use site by the realize binary's operand_bindings
        // lookup (term_shape_leaf_expression checks operand_bindings.get(
        // position) BEFORE checking kind=symbol). So this symbol-leaf
        // emission is the FALLBACK for free identifiers not bound as params.
        syn::Expr::Path(path) => {
            use quote::ToTokens;
            let text = path.to_token_stream().to_string().replace(' ', "");
            CValue::object([
                ("kind", CValue::string("symbol")),
                ("text", CValue::string(text)),
            ])
        }
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

/// Pretty-print a `syn::Expr` via prettyplease by wrapping it in a dummy fn,
/// formatting the resulting `syn::File`, and extracting the body. This is
/// the byte-exact source-form reproduction used by Expr::Match and other
/// shapes that aren't structurally lifted in the substrate primitives.
/// Outer-scope identifier references inside the expression survive because
/// they're textual; the materialized boundary stub's surrounding scope
/// must provide them by name (which is what `let` bindings emitted by
/// earlier seq children give us).
fn pretty_print_expr(expr: &syn::Expr) -> String {
    use quote::quote;
    let wrapped = quote! {
        fn __provekit_pp() { #expr }
    };
    let Ok(file) = syn::parse2::<syn::File>(wrapped) else {
        return String::new();
    };
    let formatted = prettyplease::unparse(&file);
    // Extract the body between `fn __provekit_pp() {` and the matching `}`,
    // then strip the 4-space indent prettyplease added.
    let Some(open) = formatted.find('{') else {
        return formatted;
    };
    let body_start = open + 1;
    let Some(close) = formatted.rfind('}') else {
        return formatted;
    };
    let inner = formatted[body_start..close].trim_matches('\n');
    inner
        .lines()
        .map(|l| l.strip_prefix("    ").unwrap_or(l))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Canonical-spacing format for proc_macro2::TokenStream::to_string() output.
/// The default TokenStream renderer emits space-separated tokens (e.g.
/// `handle , "{}" , line`). Apply minimal normalization to match common
/// Rust source conventions: drop spaces before `,` and around `(` `)` `[`
/// `]` and `;`. This is byte-correct for the macro forms the L0 shims use.
fn format_macro_tokens(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_was_space = false;
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c == ' ' {
            // Look at the next non-space character: if it's `,` `(` `)` `[` `]` `;` `.`
            // or we just emitted `(` `[` `.`, drop this space.
            let next_non_space = chars.iter().skip(i + 1).find(|&&c| c != ' ');
            let prev = out.chars().last();
            let skip = matches!(next_non_space, Some(',' | ')' | ']' | ';' | '.'))
                || matches!(prev, Some('(' | '[' | '.'));
            if !skip {
                if !prev_was_space {
                    out.push(' ');
                }
                prev_was_space = true;
            }
        } else {
            out.push(c);
            prev_was_space = false;
        }
    }
    out.trim().to_string()
}

/// Build a substrate-canonical method-concept leaf.
///
/// A method's identity comes from its STRUCTURE — the canonical shape
/// is `{kind:"method-concept", name:"<name>", arity:<n>}`, and its
/// op_cid is `blake3_512(JCS(that))`. No catalog minting required:
/// the structure IS the identity. Any source language emitting a
/// method with the same (name, arity) gets the same CID automatically.
///
/// The leaf also keeps `text` for legacy readers that haven't migrated
/// to `concept_name` yet. New readers should prefer `concept_name`
/// + `op_cid`.
fn method_concept_leaf(method_name: &str, arity: usize) -> Arc<CValue> {
    let concept_name = format!("method:{}", method_name);
    // Canonical content-addressable shape (no text/legacy fields,
    // no op_cid yet — those are derived/auxiliary).
    let canonical = CValue::object([
        ("arity", CValue::integer(arity as i64)),
        ("concept_name", CValue::string(concept_name.clone())),
        ("kind", CValue::string("method-concept")),
    ]);
    let op_cid = blake3_512_of(encode_jcs(&canonical).as_bytes());
    // Emitted leaf includes op_cid (self-describing) AND keeps text/
    // kind="method" for backwards compatibility with existing readers
    // (e.g. the java realize plugin's pattern-match on "kind":"method").
    CValue::object([
        ("arity", CValue::integer(arity as i64)),
        ("concept_name", CValue::string(concept_name)),
        ("kind", CValue::string("method")),
        ("op_cid", CValue::string(op_cid.to_string())),
        ("text", CValue::string(method_name.to_string())),
    ])
}

fn gamma_operation(concept_name: &str, args: Vec<Arc<CValue>>) -> Arc<CValue> {
    let op_cid = match concept_op_cid(concept_name) {
        Some(cid) => cid.to_string(),
        None => {
            // Not in the live catalogue yet — derive a CID from the
            // concept name. The substrate's accretion-over-time model:
            // new concepts come into existence at lift time, get their
            // CID from structure (here just the name), and join the
            // catalogue. Catalog memento files can be generated later
            // from observed CIDs.
            format!("blake3-512:{}", blake3_512_of(concept_name.as_bytes()))
        }
    };
    CValue::object([
        ("args", CValue::array(args)),
        ("concept_name", CValue::string(concept_name.to_string())),
        ("op_cid", CValue::string(op_cid)),
    ])
}

fn literal_shape(lit: &syn::Lit) -> Arc<CValue> {
    match lit {
        syn::Lit::Bool(value) => {
            concept_literal_shape(CValue::boolean(value.value()), SORT_BOOL_CID)
        }
        syn::Lit::Int(value) => {
            // Substrate-canonical literal: (sort=concept:Int, value=N, integer_width=W).
            // The kit's realize side reconstructs source spelling from these
            // — no source_text side channel. integer_width covers width
            // refinements (u8/i32/usize/etc.) so the spelling is fully
            // derivable from (value + width).
            let Some(decoded) = value.base10_parse::<i64>().ok() else {
                return non_operation_shape();
            };
            let Some(op_cid) = concept_op_cid("concept:literal") else {
                return non_operation_shape();
            };
            let suffix = value.suffix();
            let integer_width = if suffix.is_empty() {
                "inferred".to_string()
            } else {
                suffix.to_string()
            };
            // Preserve source radix (hex, oct, bin) so realize reproduces
            // `0x0F` not `15`. base10_digits gives "15" for any radix; we
            // peek at the original token text for the prefix.
            let token_text = value.to_string();
            let radix = if token_text.starts_with("0x") || token_text.starts_with("0X") {
                "hex"
            } else if token_text.starts_with("0o") || token_text.starts_with("0O") {
                "oct"
            } else if token_text.starts_with("0b") || token_text.starts_with("0B") {
                "bin"
            } else {
                "dec"
            };
            CValue::object([
                ("args", CValue::array(Vec::new())),
                ("concept_name", CValue::string("concept:literal")),
                ("op_cid", CValue::string(op_cid)),
                ("sort", CValue::string(SORT_INT_CID)),
                ("value", CValue::integer(decoded)),
                ("integer_width", CValue::string(integer_width)),
                ("radix", CValue::string(radix)),
            ])
        }
        syn::Lit::Float(value) => {
            concept_literal_shape(CValue::string(value.base10_digits()), SORT_FLOAT_CID)
        }
        syn::Lit::Str(value) => {
            // Substrate-canonical: sort + value (the decoded string).
            // Source-form escape choices (`"\\\""` vs `"\""`) are kit
            // presentation, not substrate state.
            let Some(op_cid) = concept_op_cid("concept:literal") else {
                return non_operation_shape();
            };
            CValue::object([
                ("args", CValue::array(Vec::new())),
                ("concept_name", CValue::string("concept:literal")),
                ("op_cid", CValue::string(op_cid)),
                ("sort", CValue::string(SORT_STRING_CID)),
                ("value", CValue::string(value.value())),
            ])
        }
        syn::Lit::ByteStr(value) => {
            concept_literal_shape(byte_array_value(value.value()), SORT_BYTES_CID)
        }
        syn::Lit::CStr(value) => concept_literal_shape(
            byte_array_value(value.value().as_bytes_with_nul().to_vec()),
            SORT_BYTES_CID,
        ),
        syn::Lit::Byte(value) => {
            concept_literal_shape(CValue::integer(i64::from(value.value())), SORT_INT_CID)
        }
        // Substrate-canonical char: sort + value (the actual character as a
        // single-char string). Source spelling (`'a'` vs `'\u{61}'`) is kit
        // presentation; the substrate carries semantic identity only.
        syn::Lit::Char(value) => {
            let Some(op_cid) = concept_op_cid("concept:literal") else {
                return non_operation_shape();
            };
            CValue::object([
                ("args", CValue::array(Vec::new())),
                ("concept_name", CValue::string("concept:literal")),
                ("op_cid", CValue::string(op_cid)),
                ("sort", CValue::string(SORT_STRING_CID)),
                ("value", CValue::string(value.value().to_string())),
            ])
        }
        syn::Lit::Verbatim(_) => non_operation_shape(),
        // syn::Lit is #[non_exhaustive]; future variants refuse the lift
        // until the substrate adds an explicit shape claim for them.
        _ => non_operation_shape(),
    }
}

fn concept_literal_shape(value: Arc<CValue>, sort_cid: &str) -> Arc<CValue> {
    let Some(op_cid) = concept_op_cid("concept:literal") else {
        return non_operation_shape();
    };
    CValue::object([
        ("args", CValue::array(Vec::new())),
        ("concept_name", CValue::string("concept:literal")),
        ("op_cid", CValue::string(op_cid.to_string())),
        ("sort", CValue::string(sort_cid.to_string())),
        ("value", value),
    ])
}

fn byte_array_value(bytes: Vec<u8>) -> Arc<CValue> {
    CValue::array(
        bytes
            .into_iter()
            .map(|byte| CValue::integer(i64::from(byte)))
            .collect(),
    )
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
        // Both `algorithm` and `abstraction` entries are usable as
        // term-shape node identities. Algorithms are operations (e.g.
        // concept:assign, concept:call); abstractions are values-as-
        // operations in syntactic position (e.g. concept:closure —
        // syntactically a closure-creation node, semantically a value).
        let kind = meta.get("kind").and_then(Value::as_str).unwrap_or("");
        if kind != "algorithm" && kind != "abstraction" {
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
            "bind-ir/2.0.0",
            "the Rust kit must advertise the current bind IR schema"
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
    fn sugar_attr_annotated_fn_emits_library_sugar_binding_entry() {
        let root = temp_workspace("sugar_positive");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        let src = r#"
#[provekit::sugar(concept = "concept:http-request", library = "reqwest")]
async fn fetch_status(url: String) -> i64 {
    0
}
"#;
        fs::write(src_dir.join("lib.rs"), src).expect("write source");
        let out = bind_lift(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": ["."],
        }))
        .expect("bind lift should succeed");
        let ir = out["ir"].as_array().expect("ir array");
        let sugar: Vec<_> = ir
            .iter()
            .filter(|e| e["kind"] == "library-sugar-binding-entry")
            .collect();
        assert_eq!(
            sugar.len(),
            1,
            "expected exactly one sugar entry, got: {sugar:?}"
        );
        let e = &sugar[0];
        assert_eq!(e["kind"], "library-sugar-binding-entry");
        assert_eq!(e["target_language"], "rust");
        assert_eq!(e["target_library_tag"], "reqwest");
        assert_eq!(e["concept_name"], "concept:http-request");
        assert_eq!(e["source_function_name"], "fetch_status");
        assert!(
            e["signature_shape_cid"]
                .as_str()
                .expect("signature cid")
                .starts_with("blake3-512:"),
            "bad sig cid"
        );
        assert!(
            e["body_source"]["source_cid"]
                .as_str()
                .expect("source cid")
                .starts_with("blake3-512:"),
            "bad source cid"
        );
        assert_eq!(e["body_source"]["span"]["start_line"], 3);
        assert_eq!(e["loss_record_contribution"]["form"], "literal");
        assert_eq!(e["loss_record_contribution"]["value"]["entries"], json!([]));
        assert!(
            e.get("signature_shape").is_none(),
            "must not emit full signature_shape doc"
        );
        assert!(
            e["body_source"].get("locator").is_none(),
            "must use span not locator"
        );

        let _ = fs::remove_dir_all(root);
    }

    // ---------------------------------------------------------------------
    // #1357 / #1355: family + version axes on @sugar / @boundary annotations
    // ---------------------------------------------------------------------

    #[test]
    fn sugar_attr_with_family_and_version_emits_into_binding_entry() {
        let root = temp_workspace("sugar_family_version");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        let src = r#"
#[provekit::sugar(
    concept = "concept:sql-query",
    library = "rusqlite",
    version = "0.39.0",
    family = "concept:family:sql",
)]
pub fn query(conn: &i64, sql: &str) -> i64 {
    0
}
"#;
        fs::write(src_dir.join("lib.rs"), src).expect("write source");
        let out = bind_lift(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": ["."],
        }))
        .expect("bind lift should succeed");
        let ir = out["ir"].as_array().expect("ir array");
        let sugar: Vec<_> = ir
            .iter()
            .filter(|e| e["kind"] == "library-sugar-binding-entry")
            .collect();
        assert_eq!(sugar.len(), 1, "expected one sugar entry, got: {sugar:?}");
        let e = &sugar[0];
        assert_eq!(e["target_library_tag"], "rusqlite");
        assert_eq!(e["library_version"], "0.39.0");
        assert_eq!(e["family"], "concept:family:sql");
        assert_eq!(e["concept_name"], "concept:sql-query");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn sugar_attr_without_family_or_version_omits_those_fields() {
        // Back-compat: existing shims without family/version annotations must
        // still mint, with the new fields simply absent (NOT empty strings).
        let root = temp_workspace("sugar_no_family_version");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        let src = r#"
#[provekit::sugar(concept = "concept:http-request", library = "reqwest")]
async fn fetch_status(url: String) -> i64 {
    0
}
"#;
        fs::write(src_dir.join("lib.rs"), src).expect("write source");
        let out = bind_lift(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": ["."],
        }))
        .expect("bind lift should succeed");
        let ir = out["ir"].as_array().expect("ir array");
        let e = ir
            .iter()
            .find(|e| e["kind"] == "library-sugar-binding-entry")
            .expect("sugar entry");
        assert!(
            e.get("library_version").is_none() || e["library_version"].is_null(),
            "library_version must not be emitted when absent on annotation"
        );
        assert!(
            e.get("family").is_none() || e["family"].is_null(),
            "family must not be emitted when absent on annotation"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn boundary_attr_with_family_and_version_emits_into_realization_memento() {
        let root = temp_workspace("boundary_family_version");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        let src = r#"
#[provekit::boundary(
    concept = "concept:sql-query",
    library = "rusqlite",
    version = "0.39.0",
    family = "concept:family:sql",
    boundary_contract = "boundary:sql-execute",
)]
pub fn query_stub(_conn: &i64, _sql: &str) -> i64 {
    unimplemented!()
}
"#;
        fs::write(src_dir.join("lib.rs"), src).expect("write source");
        let out = bind_lift(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": ["."],
        }))
        .expect("bind lift should succeed");
        let ir = out["ir"].as_array().expect("ir array");
        let memento = ir
            .iter()
            .find(|e| e["kind"] == "realization-memento")
            .expect("realization memento");
        assert_eq!(memento["library"], "rusqlite");
        assert_eq!(memento["library_version"], "0.39.0");
        assert_eq!(memento["family"], "concept:family:sql");
        assert_eq!(memento["concept_name"], "concept:sql-query");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn unannotated_fn_produces_zero_sugar_entries() {
        let root = temp_workspace("sugar_discrim");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        let src = r#"
fn plain_fn(x: i64) -> i64 {
    x + 1
}
"#;
        fs::write(src_dir.join("lib.rs"), src).expect("write source");
        let out = bind_lift(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": ["."],
        }))
        .expect("bind lift should succeed");
        let ir = out["ir"].as_array().expect("ir array");
        let sugar: Vec<_> = ir
            .iter()
            .filter(|e| e["kind"] == "library-sugar-binding-entry")
            .collect();
        assert_eq!(
            sugar.len(),
            0,
            "unannotated fn must produce zero sugar entries"
        );
        let bind: Vec<_> = ir
            .iter()
            .filter(|e| e["kind"] == "bind-lift-entry")
            .collect();
        assert_eq!(
            bind.len(),
            1,
            "regular bind-lift-entry must still be emitted"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn two_sugar_annotated_fns_produce_two_entries() {
        let root = temp_workspace("sugar_multi");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        let src = r#"
#[provekit::sugar(concept = "concept:http-request", library = "reqwest")]
fn fetch_one(url: String) -> i64 {
    0
}

#[provekit::sugar(concept = "concept:sql-query", library = "rusqlite")]
fn query_db(sql: String) -> String {
    String::new()
}
"#;
        fs::write(src_dir.join("lib.rs"), src).expect("write source");
        let out = bind_lift(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": ["."],
        }))
        .expect("bind lift should succeed");
        let ir = out["ir"].as_array().expect("ir array");
        let sugar: Vec<_> = ir
            .iter()
            .filter(|e| e["kind"] == "library-sugar-binding-entry")
            .collect();
        assert_eq!(
            sugar.len(),
            2,
            "two annotated fns must produce two sugar entries"
        );
        let concepts: Vec<_> = sugar
            .iter()
            .map(|e| e["concept_name"].as_str().expect("concept string"))
            .collect();
        assert!(concepts.contains(&"concept:http-request"), "{concepts:?}");
        assert!(concepts.contains(&"concept:sql-query"), "{concepts:?}");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn malformed_sugar_attr_missing_concept_or_library_produces_zero_entries() {
        let root = temp_workspace("sugar_malformed");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        let src_missing_lib = r#"
#[provekit::sugar(concept = "concept:http-request")]
fn missing_lib(url: String) -> i64 { 0 }
"#;
        fs::write(src_dir.join("lib.rs"), src_missing_lib).expect("write source");
        let out = bind_lift(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": ["."],
        }))
        .expect("bind lift should succeed");
        let ir = out["ir"].as_array().expect("ir array");
        let sugar: Vec<_> = ir
            .iter()
            .filter(|e| e["kind"] == "library-sugar-binding-entry")
            .collect();
        assert_eq!(
            sugar.len(),
            0,
            "missing library must produce zero sugar entries"
        );

        let src_missing_concept = r#"
#[provekit::sugar(library = "reqwest")]
fn missing_concept(url: String) -> i64 { 0 }
"#;
        fs::write(src_dir.join("lib.rs"), src_missing_concept).expect("write source");
        let out = bind_lift(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": ["."],
        }))
        .expect("bind lift should succeed");
        let ir = out["ir"].as_array().expect("ir array");
        let sugar: Vec<_> = ir
            .iter()
            .filter(|e| e["kind"] == "library-sugar-binding-entry")
            .collect();
        assert_eq!(
            sugar.len(),
            0,
            "missing concept must produce zero sugar entries"
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

        // Substrate-canonical shape: free identifier args (x, y) lift as
        // kind:"symbol" leaves carrying the name. (Previously emitted as
        // non_operation_shape {} + supplemented by operand_bindings — the
        // 2026-05-21 substrate-honest pass moved names into the shape so
        // deeper consumers don't need position-threading to resolve them.)
        assert_eq!(
            shape,
            json!({
                "args": [
                    {"kind": "symbol", "text": "x"},
                    {"kind": "symbol", "text": "y"},
                ],
                "concept_name": "concept:add",
                "op_cid": "blake3-512:95fc70e63a5550fd2e25142f13932919c59d085654ab387789c798886b0111c61d28fe533fc98b50df70eea9428a9af8aa75372c8b1c1deb3acc1a4094790468"
            })
        );
    }

    // ---------------------------------------------------------------------
    // #1363 / #1355: integer-width metadata on concept:literal shapes
    // ---------------------------------------------------------------------

    #[test]
    fn integer_literal_with_u8_suffix_emits_integer_width_u8() {
        let shape = term_shape_json(
            r#"
pub fn first() -> i64 {
    let x = 0u8;
    1
}
"#,
        );
        let json_text = serde_json::to_string(&shape).expect("shape stringifies");
        assert!(
            json_text.contains("\"integer_width\":\"u8\""),
            "u8-suffixed literal must carry integer_width=u8: {json_text}"
        );
    }

    #[test]
    fn integer_literal_with_i32_suffix_emits_integer_width_i32() {
        let shape = term_shape_json(
            r#"
pub fn first() -> i64 {
    let x = 5i32;
    1
}
"#,
        );
        let json_text = serde_json::to_string(&shape).expect("shape stringifies");
        assert!(
            json_text.contains("\"integer_width\":\"i32\""),
            "i32-suffixed literal must carry integer_width=i32: {json_text}"
        );
    }

    #[test]
    fn integer_literal_without_suffix_emits_integer_width_inferred() {
        let shape = term_shape_json(
            r#"
pub fn first() -> i64 {
    let x = 42;
    1
}
"#,
        );
        let json_text = serde_json::to_string(&shape).expect("shape stringifies");
        assert!(
            json_text.contains("\"integer_width\":\"inferred\""),
            "unsuffixed literal must mark inferred: {json_text}"
        );
    }

    #[test]
    fn term_shape_rust_char_literal_lifts_as_concept_literal_with_value() {
        // Substrate-canonical char literal shape: (sort=concept:String CID,
        // value=<char as string>). Source-form spelling ('a' vs '\u{61}')
        // is kit presentation; substrate carries semantic identity only.
        // (2026-05-21: source_text dropped from substrate channel; the rust
        // realize binary re-spells via literal_term_with_width.)
        let shape = term_shape_json(
            r#"
pub fn first() -> char {
    'a'
}
"#,
        );
        assert_eq!(
            shape.get("concept_name").and_then(|v| v.as_str()),
            Some("concept:literal"),
            "Char literal must lift as concept:literal: {shape}"
        );
        assert_eq!(
            shape.get("value").and_then(|v| v.as_str()),
            Some("a"),
            "char value must carry semantic identity (the actual character)"
        );
        assert!(
            shape.get("source_text").is_none(),
            "source_text must not appear in substrate channel: {shape}"
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
    fn term_shape_let_binding_preserves_assignment_boundary() {
        let shape = term_shape_json(
            r#"
pub fn add_via_let(a: i64, b: i64) -> i64 {
    let q = a + b;
    q
}
"#,
        );
        let seq_cid = concept_op_cid("concept:seq").expect("seq cid");
        let assign_cid = concept_op_cid("concept:assign").expect("assign cid");
        let add_cid = concept_op_cid("concept:add").expect("add cid");

        // Substrate-canonical shape after 2026-05-21:
        // - body is concept:seq containing [assign, tail-expression]
        // - assign target is kind:symbol "q" (was {} relying on operand_bindings)
        // - free identifier references (a, b, q) are kind:symbol leaves
        assert_eq!(
            shape,
            json!({
                "args": [
                    {
                        "args": [
                            {"kind": "symbol", "text": "q"},
                            {
                                "args": [
                                    {"kind": "symbol", "text": "a"},
                                    {"kind": "symbol", "text": "b"},
                                ],
                                "concept_name": "concept:add",
                                "op_cid": add_cid
                            }
                        ],
                        "concept_name": "concept:assign",
                        "op_cid": assign_cid
                    },
                    {"kind": "symbol", "text": "q"}
                ],
                "concept_name": "concept:seq",
                "op_cid": seq_cid
            })
        );
        assert_no_forbidden_term_shape_fields(&shape);
    }

    #[test]
    fn term_shape_top_level_operator_differs_from_let_assignment_boundary() {
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

        assert_ne!(top_level, let_rhs);
        assert_eq!(
            top_level["concept_name"],
            json!("concept:add"),
            "top-level tail expression remains an add shape"
        );
        // After 2026-05-21: let+tail-expression body is concept:seq
        // ([assign, tail-symbol]) — both substrate-meaningful nodes.
        // The assignment boundary is preserved INSIDE the seq, not at top.
        assert_eq!(
            let_rhs["concept_name"],
            json!("concept:seq"),
            "let+tail body lifts as concept:seq with the assign as a child"
        );
        assert_eq!(
            let_rhs["args"][0]["concept_name"],
            json!("concept:assign"),
            "seq's first child is the concept:assign for the let-binding"
        );
        assert_no_forbidden_term_shape_fields(&top_level);
        assert_no_forbidden_term_shape_fields(&let_rhs);
    }

    #[test]
    fn term_shape_explicit_return_preserves_return_boundary() {
        let shape = term_shape_json(
            r#"
pub fn f(a: i64) -> i64 {
    return a;
}
"#,
        );

        assert_eq!(shape["concept_name"], json!("concept:return"));
        assert_no_forbidden_term_shape_fields(&shape);
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

        // Substrate-canonical (2026-05-21): Expr::If lifts structurally
        // as concept:conditional(cond, then, else). No more
        // concept:literal source_text fallback — source strings don't
        // belong in the substrate channel. The structural shape carries
        // identity; each kit re-spells from concept-hub identities.
        assert_no_forbidden_term_shape_fields(&shape);
        assert_eq!(
            shape.get("concept_name").and_then(|v| v.as_str()),
            Some("concept:conditional"),
            "if-as-tail-expression lifts as concept:conditional: {shape:#?}"
        );
        // Collect every concept_name in the tree — verify the structural
        // shape carries the expected operator chain (eq for the equality
        // check, conditional for nested if, div for /, lt for <, etc.).
        let mut names = Vec::new();
        collect_concept_names(&shape, &mut names);
        for expected in ["concept:conditional", "concept:eq", "concept:div", "concept:lt"] {
            assert!(
                names.contains(&expected.to_string()),
                "expected operator {expected} in shape names: {names:?}\nshape: {shape:#?}"
            );
        }
        assert!(
            !serde_json::to_string(&shape)
                .expect("shape stringifies")
                .contains("UNNAMED-CONCEPT"),
            "shape must not contain unnamed concept wrappers: {shape:#?}"
        );
        // Substrate-honest invariant: no source_text leaves anywhere.
        let json_text = serde_json::to_string(&shape).expect("shape stringifies");
        assert!(
            !json_text.contains("\"source_text\""),
            "substrate channel must not carry source_text: {json_text}"
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
                // `kind` is legitimately used as a leaf-disambiguator (path/
                // method/mutability/symbol/literal) and is NOT forbidden;
                // remaining forbiddens are lift-side annotations and source
                // locations that have no place in the substrate channel.
                for forbidden in [
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

    // ---- substrate-honest precursor wire tests (loss, observed_dimension, refusal-memento) ----

    #[test]
    fn sugar_attr_loss_array_populates_loss_record_entries() {
        let root = temp_workspace("sugar_loss_array");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        let src = r#"
#[provekit::sugar(
    concept = "concept:sql-query",
    library = "rusqlite",
    loss = ["sync-vs-async", "row-cardinality"],
)]
fn query(conn: String, sql: String) -> i64 { 0 }
"#;
        fs::write(src_dir.join("lib.rs"), src).expect("write source");
        let out = bind_lift(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": ["."],
        }))
        .expect("bind lift should succeed");
        let ir = out["ir"].as_array().expect("ir array");
        let sugar: Vec<_> = ir
            .iter()
            .filter(|e| e["kind"] == "library-sugar-binding-entry")
            .collect();
        assert_eq!(sugar.len(), 1, "expected one sugar entry");
        assert_eq!(
            sugar[0]["loss_record_contribution"]["value"]["entries"],
            json!(["sync-vs-async", "row-cardinality"])
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn sugar_attr_without_loss_still_emits_empty_entries() {
        let root = temp_workspace("sugar_no_loss");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        let src = r#"
#[provekit::sugar(concept = "concept:sql-query", library = "rusqlite")]
fn query(conn: String, sql: String) -> i64 { 0 }
"#;
        fs::write(src_dir.join("lib.rs"), src).expect("write source");
        let out = bind_lift(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": ["."],
        }))
        .expect("bind lift should succeed");
        let ir = out["ir"].as_array().expect("ir array");
        let sugar: Vec<_> = ir
            .iter()
            .filter(|e| e["kind"] == "library-sugar-binding-entry")
            .collect();
        assert_eq!(sugar.len(), 1);
        assert_eq!(
            sugar[0]["loss_record_contribution"]["value"]["entries"],
            json!([])
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn sugar_attr_observed_dimension_propagates_to_entry() {
        let root = temp_workspace("sugar_observed_dim");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        let src = r#"
#[provekit::sugar(
    concept = "concept:contract-observation",
    library = "rusqlite",
    observed_dimension = "autocommit-mode",
)]
fn is_autocommit(conn: String) -> bool { false }
"#;
        fs::write(src_dir.join("lib.rs"), src).expect("write source");
        let out = bind_lift(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": ["."],
        }))
        .expect("bind lift should succeed");
        let ir = out["ir"].as_array().expect("ir array");
        let sugar: Vec<_> = ir
            .iter()
            .filter(|e| e["kind"] == "library-sugar-binding-entry")
            .collect();
        assert_eq!(sugar.len(), 1);
        assert_eq!(sugar[0]["observed_dimension"], "autocommit-mode");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn refuse_attr_emits_refusal_memento_with_all_fields() {
        let root = temp_workspace("refuse_full");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        let src = r#"
#[provekit::refuse(
    surface = "rusqlite::Connection::backup",
    concept = "concept:sql-physical-backup",
    reason = "SQLite-binary-specific physical backup; N=1 cluster.",
    would_close_with_cluster = "Connection-level physical-backup method on >=2 SQL drivers",
)]
pub mod refused_backup {}
"#;
        fs::write(src_dir.join("lib.rs"), src).expect("write source");
        let out = bind_lift(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": ["."],
        }))
        .expect("bind lift should succeed");
        let ir = out["ir"].as_array().expect("ir array");
        let refusals: Vec<_> = ir
            .iter()
            .filter(|e| e["kind"] == "refusal-memento")
            .collect();
        assert_eq!(refusals.len(), 1, "expected one refusal-memento entry");
        let r = &refusals[0];
        assert_eq!(r["surface"], "rusqlite::Connection::backup");
        assert_eq!(r["concept"], "concept:sql-physical-backup");
        assert_eq!(
            r["reason"],
            "SQLite-binary-specific physical backup; N=1 cluster."
        );
        assert_eq!(
            r["would_close_with_cluster"],
            "Connection-level physical-backup method on >=2 SQL drivers"
        );
        assert_eq!(r["target_language"], "rust");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn refuse_attr_missing_field_produces_zero_memento() {
        let root = temp_workspace("refuse_missing");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        let src_missing_reason = r#"
#[provekit::refuse(
    surface = "rusqlite::Connection::backup",
    concept = "concept:sql-physical-backup",
    would_close_with_cluster = "Cross-driver analog",
)]
pub mod refused_backup {}
"#;
        fs::write(src_dir.join("lib.rs"), src_missing_reason).expect("write source");
        let out = bind_lift(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": ["."],
        }))
        .expect("bind lift should succeed");
        let ir = out["ir"].as_array().expect("ir array");
        let refusals: Vec<_> = ir
            .iter()
            .filter(|e| e["kind"] == "refusal-memento")
            .collect();
        assert_eq!(
            refusals.len(),
            0,
            "missing required field must produce zero refusal mementos"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn non_refuse_module_does_not_emit_memento() {
        let root = temp_workspace("refuse_discrim");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        let src = r#"
pub mod plain_module {}
"#;
        fs::write(src_dir.join("lib.rs"), src).expect("write source");
        let out = bind_lift(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": ["."],
        }))
        .expect("bind lift should succeed");
        let ir = out["ir"].as_array().expect("ir array");
        let refusals: Vec<_> = ir
            .iter()
            .filter(|e| e["kind"] == "refusal-memento")
            .collect();
        assert_eq!(refusals.len(), 0);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn three_speech_acts_compose_in_one_source_file() {
        // The load-bearing test for PR A: a single Rust source file declares
        // (1) a substrate-exact binding (sugar with loss = []), (2) a lossy
        // binding (sugar with loss = ["sync-vs-async"]), and (3) a refusal
        // (refuse module). One bind_lift call extracts all three.
        let root = temp_workspace("three_speech_acts");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        let src = r#"
#[provekit::sugar(concept = "concept:sql-execute", library = "rusqlite", loss = [])]
fn execute(conn: String, sql: String) -> i64 { 0 }

#[provekit::sugar(
    concept = "concept:sql-query",
    library = "rusqlite",
    loss = ["sync-vs-async", "row-cardinality"],
)]
fn query_row(conn: String, sql: String) -> String { String::new() }

#[provekit::refuse(
    surface = "rusqlite::Connection::backup",
    concept = "concept:sql-physical-backup",
    reason = "SQLite-specific; cluster N=1.",
    would_close_with_cluster = "Cross-driver backup method on >=2 SQL drivers",
)]
pub mod refused_backup {}
"#;
        fs::write(src_dir.join("lib.rs"), src).expect("write source");
        let out = bind_lift(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": ["."],
        }))
        .expect("bind lift should succeed");
        let ir = out["ir"].as_array().expect("ir array");

        let sugar: Vec<_> = ir
            .iter()
            .filter(|e| e["kind"] == "library-sugar-binding-entry")
            .collect();
        assert_eq!(sugar.len(), 2, "expected two sugar entries (exact + lossy)");

        let exact = sugar
            .iter()
            .find(|e| e["source_function_name"] == "execute")
            .expect("exact binding present");
        assert_eq!(
            exact["loss_record_contribution"]["value"]["entries"],
            json!([])
        );

        let lossy = sugar
            .iter()
            .find(|e| e["source_function_name"] == "query_row")
            .expect("lossy binding present");
        assert_eq!(
            lossy["loss_record_contribution"]["value"]["entries"],
            json!(["sync-vs-async", "row-cardinality"])
        );

        let refusals: Vec<_> = ir
            .iter()
            .filter(|e| e["kind"] == "refusal-memento")
            .collect();
        assert_eq!(refusals.len(), 1, "expected one refusal-memento entry");
        assert_eq!(refusals[0]["surface"], "rusqlite::Connection::backup");

        let _ = fs::remove_dir_all(root);
    }
}
