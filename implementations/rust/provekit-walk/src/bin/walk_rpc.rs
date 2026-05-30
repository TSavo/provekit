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

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use base64::Engine;
use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value as CValue};
use provekit_ir_types::{EvidenceMemento, IrFormula, IrTerm, SourceKind};
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
use tracing::{debug, info, trace};

// Tier 2b native semantic oracle (spec 2026-05-30-callee-resolution-tiers §2.T2b).
// The RA LSP client now lives in the `provekit_walk::ra_oracle` library module so
// BOTH this per-mint binary AND the resident `provekit-linkerd` daemon import the
// same framing/quiescence/resolve logic with no copy-paste. This binary no longer
// COLD-SPAWNS rust-analyzer per mint; it asks the warm resident daemon via
// `resolveReceiverCrate` (see `resolve_method_calls_via_oracle`). The oracle is
// opt-in (PROVEKIT_RESOLVE_ORACLE=rust-analyzer) and refuses (leaves
// callee_crate = None) when the daemon is unreachable or not yet ready, so the
// fast path and CI are unaffected. The RA LSP client itself lives in
// `provekit_walk::ra_oracle` and is imported by the daemon, not this binary.

// The daemon client lives alongside this binary (std-only, synchronous NDJSON).
#[path = "../ra_daemon_client.rs"]
mod ra_daemon_client;

const CONCEPT_SHAPES_CATALOG_INDEX_JSON: &str =
    include_str!("../../../../../menagerie/concept-shapes/catalog/index.json");
const SORT_BOOL_CID: &str = "blake3-512:0ee13bf3fd6b7ecfbee72dfbfc18a7c0ea7f1663de6cca43cefb36f5b4c03665452646094a7c296e819e75d683c6ce4821f3d7db3c3c78ae97f2d4e3451d2074";
const SORT_BYTES_CID: &str = "blake3-512:7116ef6e62e6739b213a8394f975a53c771b89f08c36d27143827acfcfebc0e39e5b82c530be668c3cfd5ec6966ccaa42930b37fdb1f4ac25652a970be10fb6b";
const SORT_FLOAT_CID: &str = "blake3-512:b979e70c4d5e53d9bdf13d6f08330be3c5b0714b8c770d69bbd05946b86c36df5274be8145a2683cc29c278155c9c1ee65b6897913524eecb9e4c89c71862f57";
const SORT_INT_CID: &str = "blake3-512:30ffc51350121a7172f3e4064a33c45bbd345756979fccff6875cd2ab33e4964d098a99df80cfbdf1ec1a0738c5ac3476f0ff8f75589ea511d1acd82c74ecd58";
const SORT_STRING_CID: &str = "blake3-512:be8721d24849feb74c4721520bdba02d352a94f49253a627cd509127472aa1c47cbe99cb705cac4159b5365abcce0c9aaa4901fe67630827deb6be1f9daeea10";
const BODY_TEXT_CANONICALIZATION: &str = "trim-outer-whitespace-v1";

static CONCEPT_OP_CIDS: OnceLock<BTreeMap<String, String>> = OnceLock::new();

fn main() -> io::Result<()> {
    // Logs go to stderr only; stdout is the JSON-RPC channel and must stay
    // byte-clean. Default level: warn. Set RUST_LOG to override:
    //   RUST_LOG=info  -> phase summaries
    //   RUST_LOG=debug -> per-callsite decisions, per-RPC method
    //   RUST_LOG=provekit_walk::ra_oracle=trace -> every RA LSP query
    // Note: ra_oracle now lives in the provekit_walk library, so its event
    // target is provekit_walk::ra_oracle.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(tracing_subscriber::filter::LevelFilter::WARN.into())
                .from_env_lossy(),
        )
        .init();
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    info!("provekit-walk-rpc listening on stdio (JSON-RPC 2.0, line-delimited)");
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
        // Recognizer foundation (#81, #82) per protocol §4.2.5. The lift
        // binary handles this too because it already owns the syn AST
        // machinery that recognize needs — same kit, same language.
        "provekit.plugin.recognize" => recognize(&params),
        // Implication lifter (#97). For every call expression in every
        // function body in the supplied source files, emit a kind:bridge
        // memento that links the call site (sourceSymbol = callee ident)
        // to a contract resolved by ctor-name index over the supplied
        // contract_bindings. Same kit, same AST walker; new memento kind.
        // This is the structural callsite obligation pass: the verb that
        // says "this call expression exists, an obligation forms here,
        // here is the contract it pins to."
        "provekit.plugin.lift_implications" => lift_implications(&params),
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

/// Recognizer foundation (#81, #82) per protocol §4.2.5.
///
/// Walk user source files, compute their function bodies' identifier-
/// canonical AST templates with the same `block_to_ast_template` the
/// sugar lifter uses, and match by `template_cid` against the request's
/// `binding_templates`. An exact CID match means the user's function
/// body IS the shim's sugar body (modulo whitespace + alpha-equivalence
/// on params) — tier `exact`. Tiers `structural`, `probable`, `refused`
/// are reserved for follow-up tier-2/3 work.
///
/// The kit owns the AST machinery; the substrate sees only the tag set.
/// This is the language-blind invariant: the substrate forwards
/// `binding_templates` opaquely and collects tags opaquely; only the
/// kit reads or writes syn shapes.
fn recognize(params: &Value) -> Result<Value, String> {
    use std::collections::HashMap;

    let project_root = params
        .get("project_root")
        .and_then(|v| v.as_str())
        .ok_or("missing `project_root`")?;
    let project_root = std::path::PathBuf::from(project_root);

    let source_paths: Vec<String> = params
        .get("source_paths")
        .and_then(|v| v.as_array())
        .ok_or("missing `source_paths` array")?
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();

    let empty: Vec<Value> = Vec::new();
    let binding_templates: &Vec<Value> = params
        .get("binding_templates")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty);

    // Index bindings by template_cid for O(1) lookup. The kit reads the
    // template_cid the lifter emitted (or, equivalently, recomputes it
    // from the supplied ast_template; we trust the supplied value because
    // the substrate's §4.2.5 contract requires it to verify the CID).
    let mut bindings_by_cid: HashMap<String, &Value> = HashMap::new();
    for binding in binding_templates {
        if let Some(cid) = binding.get("template_cid").and_then(|v| v.as_str()) {
            bindings_by_cid.insert(cid.to_string(), binding);
        }
    }

    let mut tags: Vec<Value> = Vec::new();

    for rel_path in &source_paths {
        let full_path = project_root.join(rel_path);
        let src = match std::fs::read_to_string(&full_path) {
            Ok(s) => s,
            Err(_) => continue, // missing files are not errors at the recognize layer
        };
        let file = match syn::parse_file(&src) {
            Ok(f) => f,
            Err(_) => continue, // unparseable files cannot host AST recognition
        };

        recognize_walk_items(&file.items, rel_path, &bindings_by_cid, &mut tags);
    }

    Ok(json!({ "tags": tags }))
}

/// Recursively visit items + nested modules, collecting recognize tags.
fn recognize_walk_items(
    items: &[syn::Item],
    rel_path: &str,
    bindings_by_cid: &std::collections::HashMap<String, &Value>,
    tags: &mut Vec<Value>,
) {
    for item in items {
        match item {
            syn::Item::Fn(item_fn) => {
                if let Some(tag) = recognize_match_item_fn(item_fn, rel_path, bindings_by_cid) {
                    tags.push(tag);
                }
            }
            syn::Item::Mod(item_mod) => {
                if let Some((_, ref nested)) = item_mod.content {
                    recognize_walk_items(nested, rel_path, bindings_by_cid, tags);
                }
            }
            _ => {}
        }
    }
}

/// Compute the candidate's identifier-canonical AST template and look it
/// up in `bindings_by_cid`. Returns a tag JSON if matched at tier `exact`.
fn recognize_match_item_fn(
    item_fn: &syn::ItemFn,
    rel_path: &str,
    bindings_by_cid: &std::collections::HashMap<String, &Value>,
) -> Option<Value> {
    let param_names: Vec<String> = item_fn
        .sig
        .inputs
        .iter()
        .filter_map(|arg| match arg {
            syn::FnArg::Typed(pat_ty) => match &*pat_ty.pat {
                syn::Pat::Ident(pid) => Some(pid.ident.to_string()),
                _ => None,
            },
            syn::FnArg::Receiver(_) => None,
        })
        .collect();

    let candidate_template = block_to_ast_template(&item_fn.block, &param_names);
    let candidate_cid = blake3_512_of(candidate_template.to_string().as_bytes());

    let binding = bindings_by_cid.get(&candidate_cid)?;

    let start = item_fn.sig.fn_token.span.start();
    let end = item_fn.block.brace_token.span.close().end();

    let param_bindings: Vec<Value> = param_names
        .iter()
        .enumerate()
        .map(|(i, n)| {
            json!({
                "index": i + 1,
                "source_text": n,
            })
        })
        .collect();

    Some(json!({
        "file": rel_path,
        "span": {
            "start_line": start.line,
            "start_col": start.column,
            "end_line": end.line,
            "end_col": end.column,
        },
        "function_name": item_fn.sig.ident.to_string(),
        "concept_name": binding.get("concept_name").cloned().unwrap_or(Value::Null),
        "library_tag": binding.get("library_tag").cloned().unwrap_or(Value::Null),
        "family": binding.get("family").cloned().unwrap_or(Value::Null),
        "template_cid": candidate_cid,
        "contract_cid": binding.get("contract_cid").cloned().unwrap_or(Value::Null),
        "match_tier": "exact",
        "param_bindings": param_bindings,
    }))
}

// ---------------------------------------------------------------------------
// Implication lifter (#97).
//
// The substrate has three lift surfaces:
//
//   1. The sugar lifter (rust-bind, above) walks #[provekit::sugar] /
//      #[provekit::boundary] annotations and emits bind-IR entries plus
//      identity-ctor sibling contracts at the sugar definitions. That
//      surface NAMES the vendor contract.
//
//   2. The test lifter (provekit-lift-rust-tests) walks #[test] / panic /
//      early-return shapes and emits one contract per asserted callsite,
//      named "<callee>@<file>:<line>:<col>". That surface DERIVES contracts
//      from observed asserts and pins them to the production-code call site
//      the assertion witnessed.
//
//   3. THIS LIFTER walks production code (no test-runner involvement, no
//      sugar annotation requirement). For every Expr::Call and
//      Expr::MethodCall in every function body, it emits a kind:bridge
//      memento that pins the call site by sourceSymbol = callee identifier
//      to a contract supplied via `contract_bindings`. The implication
//      forms STRUCTURALLY: the AST already contains the call expression;
//      the lifter just makes it explicit as a substrate-named callsite
//      anchor that enumerate_callsites can find via pool.bridges_by_symbol.
//
// Without this lifter, the verifier's enumerate_callsites stage walks the
// loaded contracts looking for ctor refs whose names hit pool.bridges_by_symbol,
// finds zero hits when the project has no vendor-shim recognize pass, and
// reports "no callsites". The bind-lift surface and the test surface emit
// contracts; this surface emits the bridges that turn those contracts into
// enumerable callsites.

#[derive(Debug, Clone)]
struct CallSite {
    /// The bare callee leaf: last path segment for `Expr::Call`, method ident
    /// for `Expr::MethodCall`.
    callee: String,
    /// The crate the callee resolves to (Tier-1 qualification). `Some("std")`,
    /// `Some("libprovekit")`, etc. for a path or use-resolved free function;
    /// `None` when it could not be resolved syntactically (a method call whose
    /// receiver type is unknown, or a bare call with no matching `use`). A
    /// `None` here is treated as the current crate by the matcher, which keeps
    /// intra-crate bare calls resolving as before; a `Some(other)` is what
    /// distinguishes a cross-crate callee from a same-named local one.
    callee_crate: Option<String>,
    /// `true` for an `Expr::MethodCall` (`x.method()`), `false` for an
    /// `Expr::Call` (free function / associated function). Only method calls
    /// whose `callee_crate` stayed `None` after Tier 2a are eligible for the
    /// Tier 2b semantic-oracle fallback; a `None` on a free call is a glob
    /// import and must keep its current-crate fallback, never the oracle.
    is_method: bool,
    file: String,
    line: usize,
    col: usize,
}

/// Map of `leaf ident -> crate root` for the `use` imports in one file.
/// `use libprovekit::core::{address, cid_of_value}` yields
/// `{address: libprovekit, cid_of_value: libprovekit}`. `crate`/`self`/`super`
/// roots map to `"crate"` (the current crate). Lets a bare call `address(x)`
/// recover that it is `libprovekit::address`, robust to internal re-exports
/// because only the ROOT (not the full module path) is needed to disambiguate.
fn build_use_crate_map(file: &syn::File) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for item in &file.items {
        if let syn::Item::Use(item_use) = item {
            collect_use_tree(&item_use.tree, None, &mut map);
        }
    }
    map
}

/// Walk a `use` tree, threading the crate root (first path segment) down to
/// each imported leaf. `root` is `None` until the first `Path` segment fixes it.
fn collect_use_tree(tree: &syn::UseTree, root: Option<&str>, map: &mut HashMap<String, String>) {
    match tree {
        syn::UseTree::Path(p) => {
            let seg = p.ident.to_string();
            // The crate root is the FIRST segment. `crate`/`self`/`super`
            // normalize to the current crate.
            let next_root = root.map(|r| r.to_string()).unwrap_or_else(|| {
                if seg == "self" || seg == "super" || seg == "crate" {
                    "crate".to_string()
                } else {
                    seg.clone()
                }
            });
            collect_use_tree(&p.tree, Some(&next_root), map);
        }
        syn::UseTree::Name(n) => {
            if let Some(r) = root {
                map.insert(n.ident.to_string(), r.to_string());
            }
        }
        syn::UseTree::Rename(r) => {
            // `use a::b as c`: the in-scope name is `c`, still rooted at `root`.
            if let Some(rt) = root {
                map.insert(r.rename.to_string(), rt.to_string());
            }
        }
        syn::UseTree::Group(g) => {
            for item in &g.items {
                collect_use_tree(item, root, map);
            }
        }
        syn::UseTree::Glob(_) => {
            // A glob import (`use foo::*`) brings unknown leaves; we cannot map
            // individual names, so callees relying on it stay unresolved (None)
            // and fall back to the current crate. Honest under-approximation.
        }
    }
}

fn resolve_path_root_crate(
    root: &str,
    use_map: &HashMap<String, String>,
    current_crate: &str,
) -> String {
    if root == "crate" || root == "self" || root == "super" {
        current_crate.to_string()
    } else if let Some(mapped) = use_map.get(root) {
        if mapped == "crate" {
            current_crate.to_string()
        } else {
            normalize_crate_root(mapped)
        }
    } else {
        normalize_crate_root(root)
    }
}

fn type_crate_for(
    ty: &syn::Type,
    use_map: &HashMap<String, String>,
    local_type_names: &BTreeSet<String>,
    current_crate: &str,
) -> Option<String> {
    let syn::Type::Path(tp) = ty else {
        return None;
    };
    let segs: Vec<String> = tp
        .path
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect();
    let first = segs.first()?;
    if segs.len() >= 2 {
        Some(resolve_path_root_crate(first, use_map, current_crate))
    } else {
        use_map
            .get(first)
            .map(|root| resolve_path_root_crate(root, use_map, current_crate))
            .or_else(|| {
                if !current_crate.is_empty() && local_type_names.contains(first) {
                    Some(current_crate.to_string())
                } else {
                    None
                }
            })
    }
}

fn collect_local_type_names(file: &syn::File) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    collect_local_type_names_in_items(&file.items, &mut names);
    names
}

fn collect_local_type_names_in_items(items: &[syn::Item], names: &mut BTreeSet<String>) {
    for item in items {
        match item {
            syn::Item::Struct(item) => {
                names.insert(item.ident.to_string());
            }
            syn::Item::Enum(item) => {
                names.insert(item.ident.to_string());
            }
            syn::Item::Union(item) => {
                names.insert(item.ident.to_string());
            }
            syn::Item::Type(item) => {
                names.insert(item.ident.to_string());
            }
            syn::Item::Mod(item_mod) => {
                if let Some((_, ref nested)) = item_mod.content {
                    collect_local_type_names_in_items(nested, names);
                }
            }
            _ => {}
        }
    }
}

fn function_return_crates(
    file: &syn::File,
    use_map: &HashMap<String, String>,
    local_type_names: &BTreeSet<String>,
    current_crate: &str,
) -> HashMap<String, String> {
    let mut returns = HashMap::new();
    collect_function_return_crates_in_items(
        &file.items,
        use_map,
        local_type_names,
        current_crate,
        &mut returns,
    );
    returns
}

fn collect_function_return_crates_in_items(
    items: &[syn::Item],
    use_map: &HashMap<String, String>,
    local_type_names: &BTreeSet<String>,
    current_crate: &str,
    returns: &mut HashMap<String, String>,
) {
    for item in items {
        match item {
            syn::Item::Fn(item_fn) => {
                if let Some(krate) = return_type_crate(
                    &item_fn.sig.output,
                    use_map,
                    local_type_names,
                    current_crate,
                ) {
                    returns.insert(item_fn.sig.ident.to_string(), krate);
                }
            }
            syn::Item::Impl(item_impl) => {
                for impl_item in &item_impl.items {
                    if let syn::ImplItem::Fn(method) = impl_item {
                        if let Some(krate) = return_type_crate(
                            &method.sig.output,
                            use_map,
                            local_type_names,
                            current_crate,
                        ) {
                            returns.insert(method.sig.ident.to_string(), krate);
                        }
                    }
                }
            }
            syn::Item::Mod(item_mod) => {
                if let Some((_, ref nested)) = item_mod.content {
                    collect_function_return_crates_in_items(
                        nested,
                        use_map,
                        local_type_names,
                        current_crate,
                        returns,
                    );
                }
            }
            _ => {}
        }
    }
}

fn return_type_crate(
    output: &syn::ReturnType,
    use_map: &HashMap<String, String>,
    local_type_names: &BTreeSet<String>,
    current_crate: &str,
) -> Option<String> {
    let syn::ReturnType::Type(_, ty) = output else {
        return None;
    };
    type_crate_for(ty, use_map, local_type_names, current_crate)
}

/// Recursively collect every call expression in every function body inside
/// `file`. Each entry carries the bare callee leaf, the crate it resolves to
/// (via the file's `use` map / path qualification), and the source position.
fn collect_callsites_in_items(
    items: &[syn::Item],
    rel_path: &str,
    use_map: &HashMap<String, String>,
    fn_return_crates: &HashMap<String, String>,
    local_type_names: &BTreeSet<String>,
    current_crate: &str,
    out: &mut Vec<CallSite>,
) {
    for item in items {
        match item {
            syn::Item::Fn(item_fn) => {
                collect_callsites_in_block(
                    &item_fn.block,
                    rel_path,
                    use_map,
                    fn_return_crates,
                    local_type_names,
                    current_crate,
                    out,
                );
            }
            syn::Item::Impl(item_impl) => {
                for impl_item in &item_impl.items {
                    if let syn::ImplItem::Fn(method) = impl_item {
                        collect_callsites_in_block(
                            &method.block,
                            rel_path,
                            use_map,
                            fn_return_crates,
                            local_type_names,
                            current_crate,
                            out,
                        );
                    }
                }
            }
            syn::Item::Mod(item_mod) => {
                if let Some((_, ref nested)) = item_mod.content {
                    collect_callsites_in_items(
                        nested,
                        rel_path,
                        use_map,
                        fn_return_crates,
                        local_type_names,
                        current_crate,
                        out,
                    );
                }
            }
            _ => {}
        }
    }
}

fn collect_callsites_in_block(
    block: &syn::Block,
    rel_path: &str,
    use_map: &HashMap<String, String>,
    fn_return_crates: &HashMap<String, String>,
    local_type_names: &BTreeSet<String>,
    current_crate: &str,
    out: &mut Vec<CallSite>,
) {
    use syn::visit::Visit;
    struct V<'a> {
        rel_path: &'a str,
        use_map: &'a HashMap<String, String>,
        fn_return_crates: &'a HashMap<String, String>,
        local_type_names: &'a BTreeSet<String>,
        current_crate: &'a str,
        local_types: HashMap<String, String>,
        out: &'a mut Vec<CallSite>,
    }
    impl<'ast, 'a> Visit<'ast> for V<'a> {
        fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
            if let Some((callee, callee_crate)) =
                call_expr_callee(&node.func, self.use_map, self.current_crate)
            {
                let start = node.func.span().start();
                self.out.push(CallSite {
                    callee,
                    callee_crate,
                    is_method: false,
                    file: self.rel_path.to_string(),
                    line: start.line,
                    col: start.column,
                });
            }
            syn::visit::visit_expr_call(self, node);
        }
        fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
            let callee = node.method.to_string();
            let start = node.method.span().start();
            self.out.push(CallSite {
                callee,
                callee_crate: receiver_crate_for_expr(
                    &node.receiver,
                    &self.local_types,
                    self.use_map,
                    self.fn_return_crates,
                    self.local_type_names,
                    self.current_crate,
                ),
                is_method: true,
                file: self.rel_path.to_string(),
                line: start.line,
                col: start.column,
            });
            syn::visit::visit_expr_method_call(self, node);
        }
        fn visit_local(&mut self, node: &'ast syn::Local) {
            let explicit = pat_type_crate(
                &node.pat,
                self.use_map,
                self.local_type_names,
                self.current_crate,
            );
            let inferred = node.init.as_ref().and_then(|init| {
                self.visit_expr(&init.expr);
                expr_return_crate(
                    &init.expr,
                    self.use_map,
                    self.fn_return_crates,
                    self.local_type_names,
                    self.current_crate,
                )
            });
            if let (Some(name), Some(krate)) = (pat_ident_name(&node.pat), explicit.or(inferred)) {
                self.local_types.insert(name, krate);
            }
        }
    }
    let mut v = V {
        rel_path,
        use_map,
        fn_return_crates,
        local_type_names,
        current_crate,
        local_types: HashMap::new(),
        out,
    };
    v.visit_block(block);
}

fn pat_ident_name(pat: &syn::Pat) -> Option<String> {
    match pat {
        syn::Pat::Ident(ident) => Some(ident.ident.to_string()),
        syn::Pat::Type(pat_type) => pat_ident_name(&pat_type.pat),
        _ => None,
    }
}

fn pat_type_crate(
    pat: &syn::Pat,
    use_map: &HashMap<String, String>,
    local_type_names: &BTreeSet<String>,
    current_crate: &str,
) -> Option<String> {
    match pat {
        syn::Pat::Type(pat_type) => {
            type_crate_for(&pat_type.ty, use_map, local_type_names, current_crate)
        }
        _ => None,
    }
}

fn receiver_crate_for_expr(
    expr: &syn::Expr,
    local_types: &HashMap<String, String>,
    use_map: &HashMap<String, String>,
    fn_return_crates: &HashMap<String, String>,
    local_type_names: &BTreeSet<String>,
    current_crate: &str,
) -> Option<String> {
    match expr {
        syn::Expr::Path(path) if path.path.segments.len() == 1 => path
            .path
            .segments
            .first()
            .and_then(|seg| local_types.get(&seg.ident.to_string()).cloned()),
        syn::Expr::Paren(paren) => receiver_crate_for_expr(
            &paren.expr,
            local_types,
            use_map,
            fn_return_crates,
            local_type_names,
            current_crate,
        ),
        _ => expr_return_crate(
            expr,
            use_map,
            fn_return_crates,
            local_type_names,
            current_crate,
        ),
    }
}

fn expr_return_crate(
    expr: &syn::Expr,
    use_map: &HashMap<String, String>,
    fn_return_crates: &HashMap<String, String>,
    local_type_names: &BTreeSet<String>,
    current_crate: &str,
) -> Option<String> {
    match expr {
        syn::Expr::Call(call) => {
            if let Some(krate) =
                associated_type_crate_for_call(&call.func, use_map, local_type_names, current_crate)
            {
                return Some(krate);
            }
            let (leaf, _) = call_expr_callee(&call.func, use_map, current_crate)?;
            fn_return_crates.get(&leaf).cloned()
        }
        syn::Expr::Paren(paren) => expr_return_crate(
            &paren.expr,
            use_map,
            fn_return_crates,
            local_type_names,
            current_crate,
        ),
        _ => None,
    }
}

fn associated_type_crate_for_call(
    func: &syn::Expr,
    use_map: &HashMap<String, String>,
    local_type_names: &BTreeSet<String>,
    current_crate: &str,
) -> Option<String> {
    let syn::Expr::Path(path) = func else {
        return None;
    };
    if path.path.segments.len() < 2 {
        return None;
    }
    let root = path.path.segments.first()?.ident.to_string();
    use_map
        .get(&root)
        .map(|mapped| resolve_path_root_crate(mapped, use_map, current_crate))
        .or_else(|| {
            if !current_crate.is_empty() && local_type_names.contains(&root) {
                Some(current_crate.to_string())
            } else {
                None
            }
        })
}

/// Extract the callee leaf AND its resolved crate from an `Expr::Call`'s
/// `func`. The leaf is the last path segment (the natural ctor name the
/// contract lifters use); the crate is recovered as follows:
///   - a multi-segment path `a::b::fn` is rooted at `a` (an extern crate),
///     unless `a` is `crate`/`self`/`super`, in which case it is the current
///     crate;
///   - a single-segment path `fn` is looked up in the file `use` map; a hit
///     gives its crate root, a miss defaults to the current crate.
/// Returns None for callees that are not simple paths (closures, macro
/// expansions, dynamic dispatch).
fn call_expr_callee(
    expr: &syn::Expr,
    use_map: &HashMap<String, String>,
    current_crate: &str,
) -> Option<(String, Option<String>)> {
    match expr {
        syn::Expr::Path(p) => {
            let segs: Vec<String> = p
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect();
            let leaf = segs.last()?.clone();
            let krate = if segs.len() >= 2 {
                let root = &segs[0];
                Some(resolve_path_root_crate(root, use_map, current_crate))
            } else {
                // Bare call: resolve via the file's `use` map.
                use_map.get(&leaf).map(|r| {
                    if r == "crate" {
                        current_crate.to_string()
                    } else {
                        normalize_crate_root(r)
                    }
                })
            };
            Some((leaf, krate))
        }
        syn::Expr::Paren(p) => call_expr_callee(&p.expr, use_map, current_crate),
        _ => None,
    }
}

/// Crate roots in source use `_`-free hyphenless identifiers; a Cargo package
/// `provekit-cli` is referenced in code as `provekit_cli`. Normalize to the
/// underscore form so call-site roots and the Cargo-derived current crate name
/// compare equal.
fn normalize_crate_root(root: &str) -> String {
    root.replace('-', "_")
}

/// Read the `[package].name` of the crate rooted at `dir` (its `Cargo.toml`),
/// normalized to the underscore identifier form used in source. Returns None
/// when there is no readable package manifest.
fn crate_name_for(dir: &Path) -> Option<String> {
    // Line-scan the manifest rather than pull in a TOML parser: we only need
    // `[package].name`. Robust to the standard `name = "..."` form.
    let manifest = std::fs::read_to_string(dir.join("Cargo.toml")).ok()?;
    let mut in_package = false;
    for line in manifest.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_package = t == "[package]";
            continue;
        }
        if in_package {
            if let Some(rest) = t.strip_prefix("name") {
                if let Some(rest) = rest.trim_start().strip_prefix('=') {
                    let v = rest.trim().trim_matches('"').trim_matches('\'');
                    if !v.is_empty() {
                        return Some(normalize_crate_root(v));
                    }
                }
            }
        }
    }
    None
}

/// JSON-RPC handler for `provekit.plugin.lift_implications`.
///
/// Request params:
///   {
///     "workspace_root": "/abs/path",
///     "source_paths":   ["src/lib.rs", "src/cmd_mint.rs", ...],
///     "contract_bindings": [
///       { "name": "<callee>@<file>:<line>:<col>", "contract_cid": "blake3-512:..." },
///       ...
///     ]
///   }
///
/// Response (PEP 1.7.0 `kind = "ir-document"`):
///   {
///     "kind": "ir-document",
///     "ir":   [ <bridge memento>, ... ],
///     "diagnostics": [ <lift-gap>, ... ]
///   }
///
/// Each `bridge` memento pins exactly one call site to exactly one
/// contract. Call sites that find no matching binding emit a `lift-gap`
/// diagnostic and skip the bridge — substrate-honesty over silently
/// producing the wrong edge.
///
/// Tier 2b (§2.T2b): for the still-unresolved method calls in `callsites`
/// (`is_method && callee_crate.is_none()`), consult the rust-analyzer semantic
/// oracle in one warm session and stamp the resolved (normalized) crate. Free
/// calls left `None` by Tier 1 are NOT sent to the oracle: those are glob
/// imports whose current-crate fallback is intended. The oracle refuses
/// (leaves `None`) when disabled or unavailable, so this is a pure upgrade over
/// Tier 2a with no behavior change when off.
fn resolve_method_calls_via_oracle(
    workspace_root: &Path,
    callsites: &mut [(CallSite, PathBuf)],
) {
    use ra_daemon_client::DaemonQuery;

    // Opt-in stays identical to the cold path: a mint with the oracle off must
    // never spawn or contact the daemon's RA host. When off we leave every
    // unresolved method call to the syntactic tiers (Tier 1/2a) and return.
    let oracle_on =
        std::env::var("PROVEKIT_RESOLVE_ORACLE").unwrap_or_default() == "rust-analyzer";
    if !oracle_on {
        debug!("oracle: off (PROVEKIT_RESOLVE_ORACLE != rust-analyzer); leaving method calls to Tier 1/2a");
        return;
    }

    // Gather the eligible positions. proc-macro2 spans are 1-based line /
    // 0-based column; LSP wants 0-based line, 0-based char. The method ident's
    // span start already points at the ident (not the dot), so the column maps
    // directly. A line of 0 should never occur for a real call; guard anyway.
    let mut queries: Vec<DaemonQuery> = Vec::new();
    for (cs, full_path) in callsites.iter() {
        if cs.is_method && cs.callee_crate.is_none() && cs.line >= 1 {
            debug!(
                callee = %cs.callee,
                file = %full_path.display(),
                line = cs.line,
                col = cs.col,
                "oracle query: unresolved method call"
            );
            queries.push(DaemonQuery {
                file: full_path.to_string_lossy().into_owned(),
                line: (cs.line - 1) as u32,
                col: cs.col as u32,
            });
        }
    }
    let total_queries = queries.len();
    if queries.is_empty() {
        debug!("oracle: no unresolved method calls, skipping");
        return;
    }
    debug!(
        count = total_queries,
        "oracle: asking resident daemon (provekit-linkerd) to resolve method calls"
    );
    // The resident warm rust-analyzer indexes the workspace ONCE inside the
    // daemon and is reused across mints, fronted by a content-addressed cache.
    // On a cold daemon this returns empty (ready:false) and we refuse to the
    // syntactic tiers; the next mint resolves warm. NEVER blocks for the index.
    let resolved = ra_daemon_client::resolve_receiver_crates(workspace_root, &queries);
    let resolved_count = resolved.len();
    let unavailable_count = total_queries - resolved_count;
    if resolved.is_empty() {
        debug!(
            total = total_queries,
            "oracle: daemon resolved nothing (cold/not-ready or all refused); \
             leaving method calls to Tier 1/2a"
        );
        return;
    }
    for (cs, full_path) in callsites.iter_mut() {
        if cs.is_method && cs.callee_crate.is_none() && cs.line >= 1 {
            let key = (
                full_path.to_string_lossy().into_owned(),
                (cs.line - 1) as u32,
                cs.col as u32,
            );
            if let Some(krate) = resolved.get(&key) {
                debug!(
                    callee = %cs.callee,
                    resolved_crate = %krate,
                    file = %full_path.display(),
                    line = cs.line,
                    "oracle resolved method call (resident daemon)"
                );
                cs.callee_crate = Some(krate.clone());
            } else {
                debug!(
                    callee = %cs.callee,
                    file = %full_path.display(),
                    line = cs.line,
                    "oracle refused: no resolution for method call"
                );
            }
        }
    }
    info!(
        resolved = resolved_count,
        total = total_queries,
        unavailable = unavailable_count,
        "oracle resolved {}/{} method calls via resident daemon ({} refused/unavailable)",
        resolved_count,
        total_queries,
        unavailable_count
    );
}

fn lift_implications(params: &Value) -> Result<Value, String> {
    use std::collections::HashMap;

    let workspace_root = params
        .get("workspace_root")
        .and_then(|v| v.as_str())
        .ok_or("missing `workspace_root`")?;
    let workspace_root = std::path::PathBuf::from(workspace_root);

    let source_paths: Vec<String> = params
        .get("source_paths")
        .and_then(|v| v.as_array())
        .ok_or("missing `source_paths` array")?
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();

    let empty: Vec<Value> = Vec::new();
    let contract_bindings: &Vec<Value> = params
        .get("contract_bindings")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty);

    let span = tracing::info_span!(
        "lift_implications",
        workspace = %workspace_root.display(),
        source_files = source_paths.len(),
        contract_bindings = contract_bindings.len(),
    );
    let _enter = span.enter();

    info!(
        source_files = source_paths.len(),
        contract_bindings = contract_bindings.len(),
        workspace = %workspace_root.display(),
        "lift_implications: starting callsite collection"
    );

    trace!(
        source_paths = ?source_paths,
        "lift_implications: source file list"
    );

    // The crate currently being lifted. Producer contracts and intra-crate
    // call sites resolve to this; a dependency call site resolves to its own
    // crate. Reading it from the project Cargo.toml is what lets the matcher
    // tell this crate's `foo` from a same-named dependency `foo` (Tier 1).
    let current_crate = crate_name_for(&workspace_root).unwrap_or_default();
    debug!(current_crate = %current_crate, "lift_implications: resolved current crate");

    // Index contracts by (crate, leaf), not bare leaf. The leaf is the substring
    // before the first '@' in the contract `name` (`<callee>@<file>:<line>:<col>`
    // for test contracts, a bare `<fn>` for function-contracts). The crate is
    // the binding's `library` field (stamped by the lifter that produced it);
    // a binding with no `library` is treated as this crate (a producer
    // contract). Keying by (crate, leaf) is what stops a bare callee from
    // resolving to a same-named contract in the WRONG crate -- the cross-crate
    // ambiguity that forced same-name dependency contracts to be dropped at
    // mint. Within one key the body-bearing binding wins over an inv-only
    // witness (a bridge to the body-bearing target is dischargeable; a bridge
    // to the `inv` witness vacuous-passes); among same-tier bindings the first
    // wins (stable). Body-discharge-ineligible bindings are indexed separately
    // so the diagnostic distinguishes "known callee, not a real obligation"
    // from "no contract exists for this callee".
    let mut contracts_by_key: HashMap<(String, String), &Value> = HashMap::new();
    let mut ineligible_by_key: HashMap<(String, String), &Value> = HashMap::new();
    for binding in contract_bindings {
        if let Some(name) = binding.get("name").and_then(|v| v.as_str()) {
            let leaf = name.split('@').next().unwrap_or(name).trim().to_string();
            if leaf.is_empty() {
                continue;
            }
            let body_discharge_eligible = binding
                .get("bodyDischargeEligible")
                .or_else(|| binding.get("body_discharge_eligible"))
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let library = binding
                .get("library")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(normalize_crate_root)
                .unwrap_or_else(|| current_crate.clone());
            let key = (library, leaf);
            if !body_discharge_eligible {
                ineligible_by_key.insert(key, binding);
                continue;
            }
            let is_body_bearing = binding
                .get("body_bearing")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            match contracts_by_key.get(&key) {
                None => {
                    contracts_by_key.insert(key, binding);
                }
                Some(existing) => {
                    let existing_body_bearing = existing
                        .get("body_bearing")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    // Upgrade to the body-bearing binding; never downgrade.
                    if is_body_bearing && !existing_body_bearing {
                        contracts_by_key.insert(key, binding);
                    }
                }
            }
        }
    }

    let mut entries: Vec<Value> = Vec::new();
    let mut diagnostics: Vec<Value> = Vec::new();

    // Collect every call site across every file FIRST, carrying each one's
    // absolute path. The Tier-1/2a crate is set during collection; the Tier-2b
    // oracle (if enabled) then resolves the still-unresolved method calls in one
    // warm rust-analyzer session, before the matcher runs. Collecting up front is
    // what lets the oracle be spawned + indexed exactly once per lift run rather
    // than once per call site.
    let mut all_callsites: Vec<(CallSite, PathBuf)> = Vec::new();
    for (rel_path, full_path) in resolve_rs_source_files(&workspace_root, &source_paths) {
        let src = match std::fs::read_to_string(&full_path) {
            Ok(s) => s,
            Err(_) => continue, // missing files are not lift errors here
        };
        let file = match syn::parse_file(&src) {
            Ok(f) => f,
            Err(_) => continue, // unparseable files cannot host implication lifting
        };

        let use_map = build_use_crate_map(&file);
        let local_type_names = collect_local_type_names(&file);
        let fn_return_crates =
            function_return_crates(&file, &use_map, &local_type_names, &current_crate);
        let mut callsites: Vec<CallSite> = Vec::new();
        collect_callsites_in_items(
            &file.items,
            &rel_path,
            &use_map,
            &fn_return_crates,
            &local_type_names,
            &current_crate,
            &mut callsites,
        );
        let file_callsite_count = callsites.len();
        if file_callsite_count > 0 {
            debug!(
                file = %rel_path,
                callsites = file_callsite_count,
                "lift_implications: collected callsites from file"
            );
        }
        for cs in callsites {
            all_callsites.push((cs, full_path.clone()));
        }
    }

    info!(
        total_callsites = all_callsites.len(),
        "lift_implications: callsite collection complete"
    );

    // Tier 2b (§2.T2b): for method calls Tier 2a could not resolve
    // (`callee_crate == None` AND `is_method`), ask rust-analyzer which crate
    // the receiver's method resolves into, and stamp the normalized crate. The
    // oracle is opt-in and refuses when unavailable; a refusal leaves the crate
    // `None`, which the matcher below treats exactly as before (current-crate
    // fallback / lift-gap). Tier 1/2a are untouched: only None-on-method sites
    // are sent to the oracle, so the fast path is never slowed by it.
    resolve_method_calls_via_oracle(&workspace_root, &mut all_callsites);

    {
        for (cs, _full_path) in all_callsites {
            // Resolve the call site to a (crate, leaf) key. An unresolved crate
            // (None: a method call, or a glob-imported bare call) defaults to
            // the current crate, preserving the prior intra-crate behavior; a
            // resolved cross-crate callee keys into that dependency's contracts.
            let resolved_crate = cs
                .callee_crate
                .clone()
                .unwrap_or_else(|| current_crate.clone());
            let key = (resolved_crate, cs.callee.clone());
            let Some(binding) = contracts_by_key.get(&key) else {
                if let Some(ineligible) = ineligible_by_key.get(&key) {
                    debug!(
                        callee = %cs.callee,
                        crate_ = %key.0,
                        file = %cs.file,
                        line = cs.line,
                        "lift-gap: body-discharge-ineligible callee"
                    );
                    diagnostics.push(json!({
                        "kind": "lift-gap",
                        "reason": "body-discharge-ineligible",
                        "detail": ineligible
                            .get("bodyDischargeRefusalReason")
                            .or_else(|| ineligible.get("body_discharge_refusal_reason"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("callee contract is not body-discharge eligible"),
                        "callee": cs.callee,
                        "calleeCrate": key.0,
                        "file": cs.file,
                        "line": cs.line,
                        "col": cs.col,
                    }));
                    continue;
                }
                debug!(
                    callee = %cs.callee,
                    crate_ = %key.0,
                    file = %cs.file,
                    line = cs.line,
                    "lift-gap: no contract for callee"
                );
                diagnostics.push(json!({
                    "kind": "lift-gap",
                    "reason": "no-contract-for-callee",
                    "callee": cs.callee,
                    "calleeCrate": key.0,
                    "file": cs.file,
                    "line": cs.line,
                    "col": cs.col,
                }));
                continue;
            };
            let Some(target_cid) = binding
                .get("contract_cid")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            else {
                debug!(
                    callee = %cs.callee,
                    file = %cs.file,
                    line = cs.line,
                    "lift-gap: binding missing contract_cid"
                );
                diagnostics.push(json!({
                    "kind": "lift-gap",
                    "reason": "binding-missing-contract-cid",
                    "callee": cs.callee,
                    "file": cs.file,
                    "line": cs.line,
                    "col": cs.col,
                }));
                continue;
            };
            debug!(
                callee = %cs.callee,
                crate_ = %key.0,
                target_cid = %target_cid,
                file = %cs.file,
                line = cs.line,
                "lift_implications: emitting bridge for callsite"
            );
            // Forward pin: a binding harvested from a dependency proof carries
            // `target_proof_cid` (that proof's bundle CID); stamp it on the
            // bridge so the verifier enforces ConsequentBundlePinned against
            // the dependency bundle. An intra-crate binding has none -> the
            // field is omitted and the verifier enforces same-bundle
            // co-membership (self-pinned). This is the only path; there is no
            // unpinned bridge.
            let mut bridge = json!({
                "kind": "bridge",
                "name": format!(
                    "intra-body:rust:{}@{}:{}:{}",
                    cs.callee, cs.file, cs.line, cs.col
                ),
                "schemaVersion": "1",
                "sourceContractCid": target_cid,
                "sourceLayer": "rust",
                "sourceSymbol": cs.callee,
                "target": { "cid": target_cid, "kind": "contract" },
                "targetContractCid": target_cid,
                "targetLayer": "rust-tests",
                "callsite": {
                    "file": cs.file,
                    "start_line": cs.line,
                    "start_col": cs.col,
                },
            });
            if let Some(tpc) = binding
                .get("target_proof_cid")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                bridge["targetProofCid"] = json!(tpc);
            }
            entries.push(bridge);
        }
    }

    let bridge_count = entries.len();
    let gap_count = diagnostics.iter().filter(|d| d.get("kind").and_then(|v| v.as_str()) == Some("lift-gap")).count();
    info!(
        bridges_emitted = bridge_count,
        lift_gaps = gap_count,
        "lift_implications: complete"
    );

    Ok(json!({
        "kind": "ir-document",
        "ir": entries,
        "diagnostics": diagnostics,
    }))
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
                "authoring_surfaces": ["rust", "rust-bind", "rust-walk-contracts"],
            "ir_version": "bind-ir/2.0.0",
            "emits_signed_mementos": false
        }
    })
}

fn bind_lift(params: &Value) -> Result<Value, String> {
    if lift_emit_mode(params) == Some("ir-document") {
        return function_contract_lift(params);
    }

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
                diagnostics.push(diagnostic(
                    "read-error",
                    path.display().to_string(),
                    e.to_string(),
                ));
                continue;
            }
        };
        let file = match syn::parse_file(&src) {
            Ok(f) => f,
            Err(e) => {
                diagnostics.push(diagnostic(
                    "parse-error",
                    path.display().to_string(),
                    e.to_string(),
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

            let doc_lines = sugar_doc_lines(item_fn);
            // #1075/A9 federation invariant: the bind-lift-entry is a
            // CID-bearing surface (it is embedded verbatim as arg[0] of the
            // federated `concept:bind-result` payload and feeds NamedTerm via
            // `bind_term_document`). Declared source signature types MUST NOT
            // ride here, or typed-Rust and untyped-Python bind to DIFFERENT
            // CIDs and seam-4 federation byte-identity breaks (regression
            // 48b343a43). The Java boundary emitter genuinely needs these
            // types to realize the interface signature, so they travel on the
            // CID-INVISIBLE realize sidecar (`realize_*`, stripped by
            // `strip_realize_sidecar_from_lift_term` before hashing) — the
            // same channel #1448 already uses for the sugar/realizer path.
            let mut entry = json!({
                "kind": "bind-lift-entry",
                "param_names": param_names,
                "realize_param_types": param_types,
                "realize_original_param_types": original_param_types,
                "realize_return_type": return_type,
                "generic_params": generic_params,
                "visibility": visibility,
                "term_shape": cvalue_to_json(&term_shape),
                "term_shape_cid": term_shape_cid,
                "operand_bindings": operand_bindings,
                "source_function_name": target.source_name,
                "witnesses": witnesses,
                "doc_lines": doc_lines,
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
            // #1396 PR-0: singleton-concept lift-gap validator.  When the
            // @sugar annotation claims a portable `concept:X`, the substrate
            // expects ≥2 realizations in the catalog (one is a singleton
            // dressed in cross-language clothing).  `library:X` prefixes are
            // already-honest kit-specific identifiers — we skip them
            // entirely.  This is GAP-EMITTING, not refusing: the existing
            // `library-sugar-binding-entry` is still emitted below.
            if concept.starts_with("concept:") {
                let counts = concept_realization_counts();
                let realization_count = counts.get(&concept).copied().unwrap_or(0);
                if realization_count < 2 {
                    let fn_name = item_fn.sig.ident.to_string();
                    let line = item_fn.sig.fn_token.span.start().line;
                    diagnostics.push(singleton_concept_gap(
                        &fn_name,
                        &rel,
                        line,
                        &concept,
                        realization_count,
                    ));
                }
            }
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
            let doc_lines_sb = sugar_doc_lines(&item_fn);
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
                "doc_lines": doc_lines_sb,
            });
            // #1369: parametric content-addressing — emit expansions for any
            // composite CIDs the signature contains. Realize plugin reads
            // these to decompose composite CIDs into (constructor, args)
            // for parameterized morphism dispatch.
            if !parametric_sort_expansions.is_empty() {
                entry["parametric_sort_expansions"] =
                    serde_json::to_value(&parametric_sort_expansions).unwrap_or_else(|_| json!([]));
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

            // #1580: emit a SIBLING `contract` decl per
            // `#[provekit::sugar(...)]` annotation. cmd_mint mints
            // this as a regular (non-body-bearing) contract memento.
            // The post is the trivial identity ctor — `function_name(<vars>)`
            // — which makes the verifier's enumerate_callsites find a
            // callsite at this ctor name. The bridge that resolves it
            // is emitted by the recognize lane (or by other downstream
            // consumers that want to point at this contract).
            //
            // Why NOT `function-contract` (which would auto-mint a
            // bridge): kind=function-contract triggers body-discharge,
            // which substrate-honestly refuses contracts that have
            // formals but no precondition (rather than reporting a
            // vacuous pass). Without a real body-derived precondition
            // — which walk_rpc doesn't have without invoking a deeper
            // lifter — staying out of body-discharge is the honest
            // path. The contract still publishes the sugar function as
            // a substrate-named entity; bridges resolve to it.
            let fn_name = item_fn.sig.ident.to_string();
            let arg_terms: Vec<Value> = param_names
                .iter()
                .map(|name| json!({ "kind": "var", "name": name }))
                .collect();
            let post = json!({
                "kind": "atomic",
                "args": [{
                    "kind": "ctor",
                    "name": fn_name,
                    "args": arg_terms,
                }],
            });
            entries.push(json!({
                "kind": "contract",
                "name": fn_name,
                "post": post,
                "outBinding": "out",
            }));
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

        // Module-level item declarations: const, static, struct, enum,
        // trait. Each becomes its own substrate IR entry so the target
        // plugin can emit native equivalents (java class+interface+
        // constants from rust mod-level items, no hand-written code).
        use quote::ToTokens;
        for item in &file.items {
            // const X: T = value;
            if let syn::Item::Const(c) = item {
                let name = c.ident.to_string();
                let ty = c.ty.to_token_stream().to_string().replace(' ', "");
                let value = c.expr.to_token_stream().to_string();
                let visibility = match &c.vis {
                    syn::Visibility::Public(_) => "pub",
                    syn::Visibility::Restricted(_) => "pub(crate)",
                    syn::Visibility::Inherited => "",
                };
                entries.push(json!({
                    "kind": "const-decl",
                    "name": name,
                    "type": ty,
                    "value": value,
                    "visibility": visibility,
                }));
            }
            // struct X { field: T, ... }
            if let syn::Item::Struct(s) = item {
                let name = s.ident.to_string();
                let visibility = match &s.vis {
                    syn::Visibility::Public(_) => "pub",
                    syn::Visibility::Restricted(_) => "pub(crate)",
                    syn::Visibility::Inherited => "",
                };
                let mut fields: Vec<Value> = Vec::new();
                if let syn::Fields::Named(named) = &s.fields {
                    for f in &named.named {
                        let fname = f.ident.as_ref().map(|i| i.to_string()).unwrap_or_default();
                        let fty = f.ty.to_token_stream().to_string().replace(' ', "");
                        fields.push(json!({"name": fname, "type": fty}));
                    }
                }
                entries.push(json!({
                    "kind": "struct-decl",
                    "name": name,
                    "visibility": visibility,
                    "fields": fields,
                }));
            }
            // enum X { Variant1(T), Variant2 }
            if let syn::Item::Enum(e) = item {
                let name = e.ident.to_string();
                let visibility = match &e.vis {
                    syn::Visibility::Public(_) => "pub",
                    syn::Visibility::Restricted(_) => "pub(crate)",
                    syn::Visibility::Inherited => "",
                };
                let mut variants: Vec<Value> = Vec::new();
                for v in &e.variants {
                    let vname = v.ident.to_string();
                    let mut payload_types: Vec<String> = Vec::new();
                    match &v.fields {
                        syn::Fields::Unnamed(unn) => {
                            for f in &unn.unnamed {
                                payload_types
                                    .push(f.ty.to_token_stream().to_string().replace(' ', ""));
                            }
                        }
                        syn::Fields::Named(_) => {
                            // struct-style variant; skip detail for now (named-fields
                            // variant data is future work — pull as a struct with the
                            // variant's name).
                        }
                        syn::Fields::Unit => {}
                    }
                    variants.push(json!({
                        "name": vname,
                        "payload_types": payload_types,
                    }));
                }
                entries.push(json!({
                    "kind": "enum-decl",
                    "name": name,
                    "visibility": visibility,
                    "variants": variants,
                }));
            }
            // Original trait-decl handling kept below — fall through.
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
                            syn::ReturnType::Type(_, ty) => {
                                ty.to_token_stream().to_string().replace(' ', "")
                            }
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
                                    param_types
                                        .push(pt.ty.to_token_stream().to_string().replace(' ', ""));
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

fn lift_emit_mode(params: &Value) -> Option<&str> {
    params
        .get("options")
        .and_then(|options| options.get("emit"))
        .and_then(|value| value.as_str())
}

fn function_contract_lift(params: &Value) -> Result<Value, String> {
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
    // The crate these contracts belong to (Tier 1): the Cargo package name of
    // the project being lifted, normalized. Stamped onto every contract so a
    // consumer that vendors this proof can key a call site by (crate, leaf).
    // Single-crate self-application projects (the only ones minted today) have
    // one package per workspace_root; a true multi-crate workspace would need
    // per-scan-root names, noted as a limitation.
    let current_crate = crate_name_for(&root).unwrap_or_default();
    let mut entries: Vec<Value> = Vec::new();
    let mut diagnostics: Vec<Value> = Vec::new();
    let mut visited: std::collections::BTreeSet<PathBuf> = Default::default();

    for scan_root in lift_scan_roots(&root, &source_paths) {
        let src_dir = scan_root.join("src");
        let walk_root: &Path = if src_dir.is_dir() {
            &src_dir
        } else {
            scan_root.as_path()
        };
        collect_rs_files(walk_root, &mut visited);
        if walk_root != scan_root.as_path() {
            collect_rs_files_shallow(scan_root.as_path(), &mut visited);
        }
    }

    for path in &visited {
        let src = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                diagnostics.push(diagnostic(
                    "read-error",
                    path.display().to_string(),
                    e.to_string(),
                ));
                continue;
            }
        };
        let file = match syn::parse_file(&src) {
            Ok(f) => f,
            Err(e) => {
                diagnostics.push(diagnostic(
                    "parse-error",
                    path.display().to_string(),
                    e.to_string(),
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

        for target in collect_function_contract_targets(&file) {
            let contract =
                build_function_contract_with_file(&target.item_fn, None, Some(rel.as_str()));
            let (body_discharge_eligible, refusal_reason) =
                body_discharge_eligibility(&contract.post, &contract.formals);
            let mut entry: Value =
                serde_json::from_slice(&contract.canonical_bytes).map_err(|e| e.to_string())?;
            entry["name"] = json!(target.fn_name.clone());
            entry["fn_name"] = json!(target.fn_name.clone());
            entry["bridgeSourceSymbol"] = json!(target.source_name.clone());
            entry["bodyDischargeEligible"] = json!(body_discharge_eligible);
            if let Some(reason) = refusal_reason {
                entry["bodyDischargeRefusalReason"] = json!(reason.clone());
                diagnostics.push(json!({
                    "kind": "body-discharge-gap",
                    "reason": reason,
                    "function": target.fn_name,
                    "file": rel,
                }));
            }
            if !current_crate.is_empty() {
                entry["library"] = json!(current_crate.clone());
            }
            entries.push(entry);
        }
    }

    Ok(json!({
        "kind": "ir-document",
        "ir": entries,
        "diagnostics": diagnostics,
        "refusals": [],
    }))
}

fn body_discharge_eligibility(post: &IrFormula, formals: &[String]) -> (bool, Option<String>) {
    let Some(value_expr) = libprovekit::wp::find_result_equation(post, "result") else {
        return (false, Some("missing-result-equation".to_string()));
    };
    let allowed_vars: BTreeSet<&str> = formals.iter().map(String::as_str).collect();
    match body_discharge_term_refusal(&value_expr, &allowed_vars) {
        Some(reason) => (false, Some(reason)),
        None => (true, None),
    }
}

fn body_discharge_term_refusal(term: &IrTerm, allowed_vars: &BTreeSet<&str>) -> Option<String> {
    match term {
        IrTerm::Const { .. } => None,
        IrTerm::Var { name } => {
            if allowed_vars.contains(name.as_str()) {
                None
            } else {
                Some(format!("unsupported-free-var:{name}"))
            }
        }
        IrTerm::Ctor { name, args } => {
            if !body_discharge_supported_ctor(name) {
                return Some(format!("unsupported-term:{name}"));
            }
            args.iter()
                .find_map(|arg| body_discharge_term_refusal(arg, allowed_vars))
        }
        IrTerm::Lambda { .. } => Some("unsupported-term:lambda".to_string()),
        IrTerm::Let { .. } => Some("unsupported-term:let".to_string()),
    }
}

fn body_discharge_supported_ctor(name: &str) -> bool {
    matches!(
        name,
        "+" | "-" | "*" | "neg" | "ite" | "=" | "≠" | "<" | "≤" | ">" | "≥" | "and" | "or" | "not"
    )
}

fn lift_scan_roots(root: &Path, source_paths: &[String]) -> Vec<PathBuf> {
    if source_paths.is_empty() {
        return vec![root.to_path_buf()];
    }
    source_paths
        .iter()
        .map(|p| {
            let candidate = root.join(p);
            if candidate.is_dir() {
                candidate
            } else {
                root.to_path_buf()
            }
        })
        .collect()
}

fn diagnostic(kind: &str, path: String, detail: String) -> Value {
    json!({
        "kind": kind,
        "path": path,
        "detail": detail,
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
struct FunctionContractLiftTarget {
    fn_name: String,
    source_name: String,
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

fn collect_bind_lift_targets_with_source(file: &syn::File, source: &str) -> Vec<BindLiftTarget> {
    let mut targets = Vec::new();
    collect_bind_lift_targets_in_items(&file.items, source, &mut targets);
    targets
}

fn collect_function_contract_targets(file: &syn::File) -> Vec<FunctionContractLiftTarget> {
    let mut targets = Vec::new();
    collect_function_contract_targets_in_items(&file.items, false, &mut targets);
    targets
}

fn collect_function_contract_targets_in_items(
    items: &[syn::Item],
    in_test_context: bool,
    targets: &mut Vec<FunctionContractLiftTarget>,
) {
    for item in items {
        match item {
            syn::Item::Fn(item_fn) => {
                if in_test_context || is_rust_test_fn(item_fn) {
                    continue;
                }
                let fn_name = item_fn.sig.ident.to_string();
                targets.push(FunctionContractLiftTarget {
                    source_name: fn_name.clone(),
                    fn_name,
                    item_fn: item_fn.clone(),
                });
            }
            syn::Item::Impl(impl_block) => {
                if in_test_context || attrs_include_cfg_test(&impl_block.attrs) {
                    continue;
                }
                let Some(qualifier) = impl_function_qualifier(impl_block) else {
                    continue;
                };
                for impl_item in &impl_block.items {
                    let syn::ImplItem::Fn(method) = impl_item else {
                        continue;
                    };
                    let item_fn = item_fn_from_impl_method(method);
                    if !is_liftable_impl_method(impl_block, method) || is_rust_test_fn(&item_fn) {
                        continue;
                    }
                    let source_name = method.sig.ident.to_string();
                    targets.push(FunctionContractLiftTarget {
                        fn_name: format!("{qualifier}::{source_name}"),
                        source_name,
                        item_fn,
                    });
                }
            }
            syn::Item::Mod(module) => {
                let nested_test_context = in_test_context || attrs_include_cfg_test(&module.attrs);
                if let Some((_, nested_items)) = &module.content {
                    collect_function_contract_targets_in_items(
                        nested_items,
                        nested_test_context,
                        targets,
                    );
                }
            }
            _ => {}
        }
    }
}

fn is_rust_test_fn(item_fn: &syn::ItemFn) -> bool {
    item_fn.attrs.iter().any(|attr| {
        let path = attr
            .path()
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>()
            .join("::");
        matches!(path.as_str(), "test" | "tokio::test" | "async_std::test")
            || is_cfg_test_attr(attr)
    })
}

fn attrs_include_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(is_cfg_test_attr)
}

fn is_cfg_test_attr(attr: &syn::Attribute) -> bool {
    let syn::Meta::List(meta) = &attr.meta else {
        return false;
    };
    attr.path().is_ident("cfg") && meta.tokens.to_string().trim() == "test"
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

fn collect_boundary_targets_in_items(items: &[syn::Item], targets: &mut Vec<BoundaryTarget>) {
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
        if segments.len() == 2 && segments[0].ident == "provekit" && segments[1].ident == "boundary"
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
    if language == "rust" {
        let entries = rust_kit_body_template_entries(library_tag);
        if !entries.is_empty() {
            return Ok(entries);
        }
    }

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
        .filter_map(body_template_call_entry_from_json)
        .collect())
}

fn rust_kit_body_template_entries(library_tag: &str) -> Vec<BodyTemplateCallEntry> {
    let response = provekit_realize_rust_core::dispatch(&json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "provekit.plugin.body_template_entries",
        "params": {
            "target_library_tag": library_tag,
        }
    }));
    response
        .pointer("/result/entries")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(body_template_call_entry_from_json)
                .collect()
        })
        .unwrap_or_default()
}

fn body_template_call_entry_from_json(entry: &Value) -> Option<BodyTemplateCallEntry> {
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

fn resolve_rs_source_files(
    workspace_root: &Path,
    source_paths: &[String],
) -> Vec<(String, PathBuf)> {
    let mut visited: std::collections::BTreeSet<PathBuf> = Default::default();
    let scan_roots: Vec<PathBuf> = if source_paths.is_empty() {
        vec![workspace_root.to_path_buf()]
    } else {
        source_paths
            .iter()
            .map(|p| workspace_root.join(p))
            .collect()
    };
    for scan_root in &scan_roots {
        if scan_root.is_file() {
            if scan_root.extension().map(|x| x == "rs").unwrap_or(false) {
                visited.insert(scan_root.clone());
            }
            continue;
        }
        if !scan_root.is_dir() {
            continue;
        }
        let src_dir = scan_root.join("src");
        let walk_root: &Path = if src_dir.is_dir() {
            &src_dir
        } else {
            scan_root
        };
        collect_rs_files(walk_root, &mut visited);
        if walk_root != scan_root.as_path() {
            collect_rs_files_shallow(scan_root, &mut visited);
        }
    }
    visited
        .into_iter()
        .map(|abs| {
            let rel = abs
                .strip_prefix(workspace_root)
                .unwrap_or(&abs)
                .to_string_lossy()
                .to_string();
            (rel, abs)
        })
        .collect()
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
    let aliases = RUST_ALIASES
        .get_or_init(|| libprovekit::core::lower_plugin::load_kit_source_aliases("rust"));
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

/// #1391 follow-on: extract the `///` doc-comment lines that appear
/// AFTER the `#[provekit::sugar(...)]` attribute on a fn (syn surfaces
/// these as `#[doc = "..."]` attributes interleaved with sugar). Doc
/// comments BEFORE the sugar attribute belong to the rust source-level
/// concept declaration block (a different surface that measure_fn skips)
/// and are NOT round-tripped through the cycle's body channel.
///
/// Returns the doc body lines (without the `/// ` prefix and without
/// `\n`), preserving source order. Empty when the function has no
/// post-sugar docs.
fn sugar_doc_lines(item_fn: &syn::ItemFn) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen_sugar = false;
    for attr in &item_fn.attrs {
        let path = attr.path();
        // Detect the `#[provekit::sugar(...)]` attribute by its two-segment
        // path.
        let segs: Vec<_> = path.segments.iter().collect();
        if segs.len() == 2 && segs[0].ident == "provekit" && segs[1].ident == "sugar" {
            seen_sugar = true;
            continue;
        }
        if !path.is_ident("doc") {
            continue;
        }
        if !seen_sugar {
            // Doc BEFORE sugar — belongs to the rust-source concept block;
            // skip for the cycle's emit (the block precedes the cycle's
            // function-level surface).
            continue;
        }
        if let syn::Meta::NameValue(nv) = &attr.meta {
            if let syn::Expr::Lit(elit) = &nv.value {
                if let syn::Lit::Str(s) = &elit.lit {
                    out.push(s.value());
                }
            }
        }
    }
    out
}

fn sugar_type_surface(ty: &syn::Type) -> String {
    use quote::ToTokens;
    ty.to_token_stream().to_string().replace(' ', "")
}

fn sugar_body_source(rel: &str, src: &str, item_fn: &syn::ItemFn) -> Value {
    let start = item_fn.sig.fn_token.span.start();
    let end = item_fn.block.brace_token.span.close().end();
    let body_text = block_inner_source(src, &item_fn.block)
        .map(canonical_sugar_body_text)
        .unwrap_or_default()
        .to_string();
    // Phase 2 / Recognizer foundation (#81, #82): emit an identifier-canonical
    // AST template alongside the body text. Same source-pass produces both:
    // body_text drives `materialize` (the splice-in form), ast_template
    // drives `recognize` (the structural pattern match against user code).
    // Identifier canonicalization replaces the sugar's named params with
    // $1, $2, … positional markers so user variants (`conn.execute(sql,args)`
    // vs `c.execute(s,a)`) match the same template after alpha-equivalence.
    let param_names: Vec<String> = item_fn
        .sig
        .inputs
        .iter()
        .filter_map(|arg| match arg {
            syn::FnArg::Typed(pat_ty) => match &*pat_ty.pat {
                syn::Pat::Ident(pid) => Some(pid.ident.to_string()),
                _ => None,
            },
            syn::FnArg::Receiver(_) => None,
        })
        .collect();
    let ast_template = block_to_ast_template(&item_fn.block, &param_names);
    let template_text = ast_template.to_string();
    json!({
        "file": rel,
        "span": {
            "start_line": start.line,
            "start_col": start.column,
            "end_line": end.line,
            "end_col": end.column,
        },
        "source_cid": blake3_512_of(body_text.as_bytes()),
        "body_text": body_text,
        "body_text_canonicalization": BODY_TEXT_CANONICALIZATION,
        "ast_template": ast_template,
        "template_cid": blake3_512_of(template_text.as_bytes()),
        "param_names": param_names,
    })
}

fn canonical_sugar_body_text(body: &str) -> &str {
    body.trim()
}

/// Identifier-canonical AST template serializer for the Recognizer
/// foundation (#81, #82). Walks a `syn::Block` and emits a structured JSON
/// tree where each node is `{kind, ...}`. Sugar param names are replaced
/// with `$1`, `$2`, … so user-code variants alpha-equivalent to the sugar
/// match the same template.
///
/// The format is intentionally a small union over the syn variants that
/// actually show up in sugar bodies (calls, method calls, paths, refs,
/// ?, blocks, literals, identifiers). Unhandled variants fall through to
/// `{kind: "other", variant: "<name>"}` catch-all so the template is
/// never lossy in a way that drops a node — only in a way that opaqueifies
/// the variant. The recognizer then refuses any candidate site containing
/// an opaqued node.
fn block_to_ast_template(block: &syn::Block, params: &[String]) -> Value {
    let stmts: Vec<Value> = block
        .stmts
        .iter()
        .map(|stmt| stmt_to_template(stmt, params))
        .collect();
    json!({ "kind": "block", "stmts": stmts })
}

fn stmt_to_template(stmt: &syn::Stmt, params: &[String]) -> Value {
    use syn::Stmt;
    match stmt {
        Stmt::Local(local) => {
            let pat = pat_to_template(&local.pat, params);
            let init = local
                .init
                .as_ref()
                .map(|init| expr_to_template(&init.expr, params))
                .unwrap_or(Value::Null);
            json!({ "kind": "let", "pat": pat, "init": init })
        }
        Stmt::Item(_) => json!({ "kind": "item" }),
        Stmt::Expr(expr, semi) => {
            let inner = expr_to_template(expr, params);
            let trailing = semi.is_some();
            json!({ "kind": "expr_stmt", "expr": inner, "trailing_semi": trailing })
        }
        Stmt::Macro(m) => {
            let path = path_to_template(&m.mac.path);
            json!({ "kind": "macro_stmt", "path": path })
        }
    }
}

fn expr_to_template(expr: &syn::Expr, params: &[String]) -> Value {
    use syn::Expr;
    match expr {
        Expr::Call(c) => {
            let func = expr_to_template(&c.func, params);
            let args: Vec<Value> = c.args.iter().map(|a| expr_to_template(a, params)).collect();
            json!({ "kind": "call", "func": func, "args": args })
        }
        Expr::MethodCall(m) => {
            let receiver = expr_to_template(&m.receiver, params);
            let method = m.method.to_string();
            let args: Vec<Value> = m.args.iter().map(|a| expr_to_template(a, params)).collect();
            json!({
                "kind": "method_call",
                "receiver": receiver,
                "method": method,
                "args": args,
            })
        }
        Expr::Path(p) => {
            let segs: Vec<String> = p
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect();
            if segs.len() == 1 {
                if let Some(idx) = params.iter().position(|n| n == &segs[0]) {
                    return json!({ "kind": "param_ref", "index": idx + 1 });
                }
                return json!({ "kind": "ident", "name": segs[0] });
            }
            json!({ "kind": "path", "segments": segs })
        }
        Expr::Lit(l) => lit_to_template(&l.lit),
        Expr::Reference(r) => {
            let inner = expr_to_template(&r.expr, params);
            json!({ "kind": "ref", "mutability": r.mutability.is_some(), "expr": inner })
        }
        Expr::Try(t) => {
            let inner = expr_to_template(&t.expr, params);
            json!({ "kind": "try", "expr": inner })
        }
        Expr::Block(b) => block_to_ast_template(&b.block, params),
        Expr::Paren(p) => expr_to_template(&p.expr, params),
        Expr::Tuple(t) => {
            let elems: Vec<Value> = t
                .elems
                .iter()
                .map(|e| expr_to_template(e, params))
                .collect();
            json!({ "kind": "tuple", "elems": elems })
        }
        Expr::Array(a) => {
            let elems: Vec<Value> = a
                .elems
                .iter()
                .map(|e| expr_to_template(e, params))
                .collect();
            json!({ "kind": "array", "elems": elems })
        }
        Expr::Closure(_) => json!({ "kind": "closure" }),
        Expr::Match(_) => json!({ "kind": "match" }),
        Expr::If(_) => json!({ "kind": "if" }),
        Expr::Return(r) => {
            let inner = r
                .expr
                .as_ref()
                .map(|e| expr_to_template(e, params))
                .unwrap_or(Value::Null);
            json!({ "kind": "return", "expr": inner })
        }
        Expr::Binary(b) => {
            let left = expr_to_template(&b.left, params);
            let right = expr_to_template(&b.right, params);
            let op = format!("{:?}", b.op);
            json!({ "kind": "binary", "op": op, "left": left, "right": right })
        }
        Expr::Unary(u) => {
            let inner = expr_to_template(&u.expr, params);
            let op = format!("{:?}", u.op);
            json!({ "kind": "unary", "op": op, "expr": inner })
        }
        Expr::Field(f) => {
            let base = expr_to_template(&f.base, params);
            let member = match &f.member {
                syn::Member::Named(n) => n.to_string(),
                syn::Member::Unnamed(u) => u.index.to_string(),
            };
            json!({ "kind": "field", "base": base, "member": member })
        }
        Expr::Macro(m) => {
            let path = path_to_template(&m.mac.path);
            json!({ "kind": "macro", "path": path })
        }
        other => json!({
            "kind": "other",
            "variant": format!("{:?}", std::mem::discriminant(other)),
        }),
    }
}

fn pat_to_template(pat: &syn::Pat, params: &[String]) -> Value {
    use syn::Pat;
    match pat {
        Pat::Ident(pi) => {
            let name = pi.ident.to_string();
            if let Some(idx) = params.iter().position(|n| n == &name) {
                json!({ "kind": "param_ref", "index": idx + 1 })
            } else {
                json!({ "kind": "binding", "name": name })
            }
        }
        Pat::Wild(_) => json!({ "kind": "wildcard" }),
        Pat::Tuple(t) => {
            let elems: Vec<Value> = t.elems.iter().map(|p| pat_to_template(p, params)).collect();
            json!({ "kind": "pat_tuple", "elems": elems })
        }
        _ => json!({ "kind": "pat_other" }),
    }
}

fn path_to_template(path: &syn::Path) -> Value {
    let segs: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
    json!({ "segments": segs })
}

fn lit_to_template(lit: &syn::Lit) -> Value {
    match lit {
        syn::Lit::Str(s) => json!({ "kind": "lit_str", "value": s.value() }),
        syn::Lit::Int(i) => json!({ "kind": "lit_int", "value": i.base10_digits() }),
        syn::Lit::Bool(b) => json!({ "kind": "lit_bool", "value": b.value }),
        syn::Lit::Char(c) => json!({ "kind": "lit_char", "value": c.value().to_string() }),
        syn::Lit::Float(f) => json!({ "kind": "lit_float", "value": f.base10_digits() }),
        _ => json!({ "kind": "lit_other" }),
    }
}

fn block_inner_source<'a>(src: &'a str, block: &syn::Block) -> Option<&'a str> {
    let open_end = block.brace_token.span.open().end();
    let close_start = block.brace_token.span.close().start();
    source_slice_between(src, open_end, close_start)
}

fn source_slice_between(
    src: &str,
    start: proc_macro2::LineColumn,
    end: proc_macro2::LineColumn,
) -> Option<&str> {
    let start = line_column_to_byte_offset(src, start)?;
    let end = line_column_to_byte_offset(src, end)?;
    if start <= end {
        src.get(start..end)
    } else {
        None
    }
}

fn line_column_to_byte_offset(src: &str, loc: proc_macro2::LineColumn) -> Option<usize> {
    if loc.line == 0 {
        return None;
    }

    let mut line_starts = vec![0usize];
    for (idx, byte) in src.bytes().enumerate() {
        if byte == b'\n' {
            line_starts.push(idx + 1);
        }
    }

    let line_start = *line_starts.get(loc.line - 1)?;
    let line_end = line_starts
        .get(loc.line)
        .copied()
        .map(|next_start| next_start.saturating_sub(1))
        .unwrap_or(src.len());
    let line = src.get(line_start..line_end)?;

    if loc.column == 0 {
        return Some(line_start);
    }

    match line.char_indices().nth(loc.column) {
        Some((offset, _)) => Some(line_start + offset),
        None if line.chars().count() == loc.column => Some(line_end),
        None => None,
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
    comment_surfaces_in_source(&body)
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

fn function_body_source(src: &str, fn_name: &str) -> Option<String> {
    let file = syn::parse_file(src).ok()?;
    let item_fn = find_item_fn_by_name(&file.items, fn_name)?;
    block_inner_source(src, &item_fn.block).map(str::to_string)
}

fn find_item_fn_by_name<'a>(items: &'a [syn::Item], fn_name: &str) -> Option<&'a syn::ItemFn> {
    for item in items {
        match item {
            syn::Item::Fn(item_fn) if item_fn.sig.ident == fn_name => return Some(item_fn),
            syn::Item::Mod(item_mod) => {
                if let Some((_, nested_items)) = &item_mod.content {
                    if let Some(item_fn) = find_item_fn_by_name(nested_items, fn_name) {
                        return Some(item_fn);
                    }
                }
            }
            _ => {}
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

fn line_comment_end(bytes: &[u8], start: usize) -> usize {
    let mut i = start + 2;
    while i < bytes.len() && bytes[i] != b'\n' {
        i += 1;
    }
    i
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
                None => BindingResult {
                    has_operator: true,
                    bindings: Vec::new(),
                },
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
    // #1391 follow-on: blank-line carrier for substrate-symmetric
    // round-trip. When the source has a blank line between two
    // statements, emit a concept:blank-line marker so the lower side
    // (java) can carry it through and rust realize can re-emit the
    // blank line. Detection: stmt end line vs next stmt start line
    // is > 1 (i.e. at least one blank line between them).
    let mut prev_end_line: Option<usize> = None;
    for stmt in &block.stmts {
        let span = stmt.span();
        let start_line = span.start().line;
        if let Some(prev) = prev_end_line {
            if start_line > prev + 1 {
                // One concept:blank-line marker per gap, regardless of
                // how many blank lines (rustfmt normalizes multi-blank
                // to single).
                shapes.push(gamma_operation("concept:blank-line", Vec::new()));
            }
        }
        prev_end_line = Some(span.end().line);
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
            gamma_operation(
                "concept:item-decl",
                vec![CValue::object([
                    ("kind", CValue::string("symbol")),
                    ("text", CValue::string(source)),
                ])],
            )
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
                let mut assign_args = vec![target_leaf, shape_of_expr(&init.expr, ctx)];
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
                return gamma_operation("concept:while-let", vec![pattern_leaf, value, body]);
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
            // catalog #1391: nullary path-call → kit-op → concept (reverse lookup).
            if e.args.is_empty() {
                let kit_op = match callee_text.as_str() {
                    "Vec::new" => Some("rust:vec-new"),
                    "HashMap::new" => Some("rust:hashmap-new"),
                    _ => None,
                };
                if let Some(kit_op) = kit_op {
                    if let Some(concept) = provekit_realize_rust_core::operation_realization_catalog::concept_for_rust_op(kit_op) {
                        return gamma_operation(&concept, vec![]);
                    }
                }
            }
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
            // catalog-driven abstraction recognition (#1391): when the
            // method+arity matches a catalog'd realization, emit the
            // abstraction operator directly instead of concept:call wrapping
            // a method:<name> leaf. Both sides do the same; the cycle
            // collapses to the abstraction at the substrate seam.
            let m_name = e.method.to_string();
            if e.args.is_empty() {
                // catalog #1391: reverse-lookup matcher AST shape → kit-op
                // name → concept-hub name. The catalog supplies the concept.
                let kit_op = match m_name.as_str() {
                    "as_bytes" => Some("rust:str-as-bytes"),
                    "as_str" => Some("rust:serde-value-as-str"),
                    "is_some" => Some("rust:option-is-some"),
                    _ => None,
                };
                if let Some(kit_op) = kit_op {
                    if let Some(concept) = provekit_realize_rust_core::operation_realization_catalog::concept_for_rust_op(kit_op) {
                        return gamma_operation(&concept, vec![shape_of_expr(&e.receiver, ctx)]);
                    }
                }
            }
            // args[0]: receiver shape, matching bindings_of_expr layout above.
            // args[1]: canonical method-concept leaf (kind:"method",
            // concept_name:"method:<name>", arity:<n>, op_cid:<derived>).
            // The CID is determined by structure — no minting required.
            // args[2..]: call arguments.
            let method_leaf = method_concept_leaf(&m_name, e.args.len());
            let mut args = vec![shape_of_expr(&e.receiver, ctx), method_leaf];
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
            gamma_operation("concept:ref", vec![shape_of_expr(&e.expr, ctx), mut_leaf])
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
                args.push(gamma_operation(
                    "concept:match-arm",
                    vec![pattern_leaf, body],
                ));
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
            // #1075 federation: operand NAMES are sugar — the term_shape is
            // structural identity, so `f(x)=x*2` and `f(y)=y*2` MUST share a
            // term_shape. A scoped param/local reference therefore emits an
            // EMPTY structural leaf `{}`; its symbol travels on the CID-invisible
            // operand_bindings sidecar (same position, see bindings_of_expr's
            // matching scoped_names check). This makes the rust term_shape
            // byte-identical to the python lifter's empty-leaf + operand_bindings
            // form (seam-4 federation). Discharge is unaffected: the realize
            // binary's term_shape_leaf_expression checks operand_bindings.get(
            // position) BEFORE kind=symbol, so it never read these leaves for
            // scoped names. FREE identifiers (None, Some, Vec::new, enum paths)
            // are NOT operands — they keep their symbol leaf so deeper consumers
            // can lower them without operand_bindings position threading.
            if ctx.scoped_names.contains(&text) {
                non_operation_shape()
            } else {
                CValue::object([
                    ("kind", CValue::string("symbol")),
                    ("text", CValue::string(text)),
                ])
            }
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

/// #1396 PR-0: count abstraction-REALIZATION catalog entries per concept name.
///
/// A realization that counts toward a concept's cross-language admissibility is
/// a catalog entry with `kind == "realization"` whose underlying memento has
/// `role == "abstraction-realization"` — i.e. an N-edge answering "language L
/// realizes concept X".  These are exactly the FORWARD-direction entries whose
/// `name` starts with `concept:X` (e.g. `concept:closure->rust:closure-expression`
/// or the colon form `concept:bool-cell:c:pointer-indirection`).
///
/// We deliberately EXCLUDE reverse-direction entries (`lang:form->concept:X`).
/// Those are `role == "abstraction-lift"` M-edges minted by the lift scripts;
/// they answer a different question ("how does language L's surface form lift
/// INTO the hub", not "how many languages realize this concept") and must not
/// count toward the >=2 singleton threshold. See #1397 (PR-1 absorbed): the
/// seven reverse-direction entries are intentional M-edges, not mis-filed
/// N-edges.
///
/// The embedded `index.json` does not carry the `role` field, but role
/// correlates exactly with direction in the catalog: every `concept:X->...`
/// entry is `abstraction-realization` and every `...->concept:X` entry is
/// `abstraction-lift`. Filtering by direction is therefore equivalent to
/// filtering by `role == "abstraction-realization"`.
///
/// Returns a map keyed by the bare concept name (e.g. `"concept:closure"`),
/// used by the singleton-concept lift-gap validator at @sugar attribute
/// extraction time.
fn concept_realization_counts() -> &'static BTreeMap<String, usize> {
    static COUNTS: OnceLock<BTreeMap<String, usize>> = OnceLock::new();
    COUNTS.get_or_init(|| {
        let index: Value = serde_json::from_str(CONCEPT_SHAPES_CATALOG_INDEX_JSON)
            .expect("embedded concept-shapes catalog index is valid JSON");
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        let Some(entries) = index.get("entries").and_then(Value::as_object) else {
            return counts;
        };
        for meta in entries.values() {
            let kind = meta.get("kind").and_then(Value::as_str).unwrap_or("");
            if kind != "realization" {
                continue;
            }
            let Some(name) = meta.get("name").and_then(Value::as_str) else {
                continue;
            };
            // Reverse-direction (`lang:form->concept:X`) = abstraction-lift
            // M-edge. Skip entirely — does not count toward the threshold.
            if name.contains("->concept:") {
                continue;
            }
            // Forward-direction (`concept:X->lang:form` arrow form, or the
            // colon form `concept:X:lang:form`) = abstraction-realization.
            let Some(rest) = name.strip_prefix("concept:") else {
                continue;
            };
            // Bare concept name is the segment up to the first `->` (arrow
            // form) or, absent an arrow, up to the next `:` (colon form).
            let bare = if let Some(idx) = rest.find("->") {
                &rest[..idx]
            } else if let Some(idx) = rest.find(':') {
                &rest[..idx]
            } else {
                rest
            };
            let concept = format!("concept:{bare}");
            *counts.entry(concept).or_insert(0) += 1;
        }
        counts
    })
}

/// #1396 PR-0: build a singleton-concept lift-gap diagnostic.
///
/// Emitted (non-fatally) into the lift `diagnostics` channel whenever a
/// `#[provekit::sugar(concept = "concept:X", ...)]` annotation names a
/// `concept:X` whose catalog has fewer than 2 realizations — i.e. it is a
/// singleton or absent.  The gap is informational: the caller (substrate /
/// audit tooling) can choose to refuse, rewrite (`concept:X` → `library:X`),
/// or mint a second realization in another language.
///
/// `library:X` prefixed concepts are SKIPPED at the call site — they're
/// already-honest kit-specific identifiers and carry no cross-language claim.
fn singleton_concept_gap(
    fn_name: &str,
    rel_path: &str,
    line: usize,
    concept: &str,
    realization_count: usize,
) -> Value {
    // Field name is `function_name` (not `fn_name`) deliberately: the bind
    // payload's substrate-hygiene assertion forbids `fn_name` outside the
    // `gapRecords` subtree (see `assert_no_fn_name_outside_gap_records` in
    // this file's tests).  The lift `diagnostics` channel rides alongside
    // `ir` and is NOT stripped by `strip_realize_sidecar_from_lift_term`,
    // so any field name we choose here lands verbatim in downstream JSON.
    // `function_name` carries the same signal without colliding with the
    // existing `fn_name` invariant.
    json!({
        "kind": "lift-gap",
        "category": "singleton-concept",
        "function_name": fn_name,
        "path": format!("{rel_path}:{line}"),
        "concept": concept,
        "realization_count": realization_count,
        "suggestion": format!(
            "`{concept}` has {realization_count} realization(s) in the catalog; \
             a portable cross-language `concept:` claim requires >=2. Either \
             rename to `library:{}` (kit-specific identifier; no cross-language \
             claim) or mint a second realization in another language.",
            concept.strip_prefix("concept:").unwrap_or(concept),
        ),
    })
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
        // Federation invariant (#1075/A9): the bind-lift-entry is CID-bearing
        // (embedded as arg[0] of the federated concept:bind-result payload and
        // feeding NamedTerm), so declared signature types must NOT appear on it
        // under the bare keys — typed-Rust would otherwise bind to a different
        // CID than untyped-Python and seam-4 byte-identity would break.
        assert!(
            entry.get("param_types").is_none(),
            "bare param_types must not ride the CID-bearing bind-lift-entry"
        );
        assert!(
            entry.get("return_type").is_none(),
            "bare return_type must not ride the CID-bearing bind-lift-entry"
        );
        assert!(
            entry.get("original_param_types").is_none(),
            "bare original_param_types must not ride the CID-bearing bind-lift-entry"
        );
        // The types are not LOST: they travel on the CID-invisible realize
        // sidecar (stripped before hashing by strip_realize_sidecar_from_lift_term)
        // so the Java boundary emitter can still realize the interface signature.
        assert_eq!(
            entry["realize_param_types"],
            json!(["i64", "i64"]),
            "signature types must be carried on the CID-invisible realize sidecar"
        );
        assert_eq!(
            entry["realize_return_type"],
            json!("i64"),
            "return type must be carried on the CID-invisible realize sidecar"
        );
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

    #[test]
    fn sugar_body_text_uses_rust_block_span_for_braces_inside_tokens() {
        let src = r####"
#[provekit::sugar(concept = "concept:http-request", library = "reqwest")]
async fn render(url: String) -> String {
    let normal = "}";
    let raw = r###"raw } braces { stay"###;
    // comment with } before the real block end
    let macro_value = format!("literal }} and {}", url);
    let nested = {
        let inner = "{";
        inner.len()
    };
    let future = async {
        let block = unsafe { normal.len() + raw.len() };
        block + nested
    };
    format!("{normal}:{raw}:{macro_value}:{}", future.await)
}
"####;
        let entry = single_sugar_entry_for_source("sugar_body_span_braces", src);
        let body_text = entry["body_source"]["body_text"]
            .as_str()
            .expect("body_text string");
        let expected = r####"let normal = "}";
    let raw = r###"raw } braces { stay"###;
    // comment with } before the real block end
    let macro_value = format!("literal }} and {}", url);
    let nested = {
        let inner = "{";
        inner.len()
    };
    let future = async {
        let block = unsafe { normal.len() + raw.len() };
        block + nested
    };
    format!("{normal}:{raw}:{macro_value}:{}", future.await)"####;

        assert_eq!(body_text, expected);
        assert_eq!(
            entry["body_source"]["source_cid"],
            blake3_512_of(expected.as_bytes())
        );
        assert!(
            !body_text.contains("async fn render"),
            "body_text must not include the function signature: {body_text}"
        );
        assert!(
            !body_text.contains("#[provekit::sugar"),
            "body_text must not include the sugar attribute: {body_text}"
        );
    }

    #[test]
    fn sugar_body_text_uses_byte_offsets_for_unicode_same_line_body() {
        let src = r#"
#[provekit::sugar(concept = "concept:unicode", library = "unicode-lib")]
pub fn snowman() -> &'static str { "☃ } still body" }
"#;
        let entry = single_sugar_entry_for_source("sugar_body_unicode_byte_offsets", src);
        let body_text = entry["body_source"]["body_text"]
            .as_str()
            .expect("body_text string");
        let expected = r#""☃ } still body""#;

        assert_eq!(
            body_text, expected,
            "body_text must slice parser span byte offsets exactly"
        );
        assert_eq!(
            entry["body_source"]["source_cid"],
            blake3_512_of(expected.as_bytes())
        );
    }

    #[test]
    fn sugar_body_source_declares_trimmed_body_canonicalization_for_cid() {
        let src_a = r#"
#[provekit::sugar(concept = "concept:canonical-body", library = "test-lib")]
pub fn canonical_body() -> i64 {

    41 + 1

}
"#;
        let src_b = r#"
#[provekit::sugar(concept = "concept:canonical-body", library = "test-lib")]
pub fn canonical_body() -> i64 {    41 + 1    }
"#;

        let entry_a = single_sugar_entry_for_source("sugar_body_canonical_a", src_a);
        let entry_b = single_sugar_entry_for_source("sugar_body_canonical_b", src_b);
        let body_source_a = &entry_a["body_source"];
        let body_source_b = &entry_b["body_source"];

        assert_eq!(
            body_source_a["body_text_canonicalization"],
            "trim-outer-whitespace-v1"
        );
        assert_eq!(
            body_source_b["body_text_canonicalization"],
            "trim-outer-whitespace-v1"
        );
        assert_eq!(body_source_a["body_text"], body_source_b["body_text"]);
        assert_eq!(body_source_a["source_cid"], body_source_b["source_cid"]);
        assert_eq!(
            body_source_a["source_cid"],
            blake3_512_of("41 + 1".as_bytes())
        );
    }

    // ---------------------------------------------------------------------
    // Recognizer foundation (#81 / #82): identifier-canonical AST template
    // alongside body_text. Same source-pass extracts both; the .proof
    // envelope carries body_text (for materialize) AND ast_template (for
    // recognize). Param names canonicalize to $1, $2 positional markers so
    // user-code variants alpha-equivalent to the sugar match the same
    // template. T's directive: keep both in the lifter's output.
    // ---------------------------------------------------------------------

    #[test]
    fn sugar_body_emits_ast_template_alongside_body_text() {
        let src = r##"
#[provekit::sugar(concept = "concept:json-parse", library = "serde_json")]
pub fn json_parse(s: &str) -> i64 {
    serde_json::from_str(s)
}
"##;
        let entry = single_sugar_entry_for_source("sugar_ast_template_basic", src);
        let template = &entry["body_source"]["ast_template"];
        assert_eq!(template["kind"], "block");
        let stmts = template["stmts"].as_array().expect("stmts array");
        assert_eq!(stmts.len(), 1, "one statement in body");
        let stmt = &stmts[0];
        assert_eq!(stmt["kind"], "expr_stmt");
        let call = &stmt["expr"];
        assert_eq!(call["kind"], "call");
        assert_eq!(call["func"]["kind"], "path");
        assert_eq!(call["func"]["segments"], json!(["serde_json", "from_str"]));
        // Param `s` canonicalized to $1.
        let args = call["args"].as_array().expect("args array");
        assert_eq!(args.len(), 1);
        assert_eq!(args[0]["kind"], "param_ref");
        assert_eq!(args[0]["index"], 1);
    }

    #[test]
    fn sugar_body_template_canonicalizes_multiple_params_positionally() {
        let src = r##"
#[provekit::sugar(concept = "concept:sql-execute", library = "rusqlite")]
pub fn execute(conn: &i64, sql: &str, args: &i64) -> i64 {
    conn.execute(sql, args)
}
"##;
        let entry = single_sugar_entry_for_source("sugar_ast_template_params", src);
        let template = &entry["body_source"]["ast_template"];
        let stmt = &template["stmts"][0];
        let mc = &stmt["expr"];
        assert_eq!(mc["kind"], "method_call");
        // Receiver is param #1 (conn).
        assert_eq!(mc["receiver"]["kind"], "param_ref");
        assert_eq!(mc["receiver"]["index"], 1);
        assert_eq!(mc["method"], "execute");
        // Args are $2 (sql) and $3 (args).
        let mc_args = mc["args"].as_array().expect("mc args");
        assert_eq!(mc_args.len(), 2);
        assert_eq!(mc_args[0]["kind"], "param_ref");
        assert_eq!(mc_args[0]["index"], 2);
        assert_eq!(mc_args[1]["kind"], "param_ref");
        assert_eq!(mc_args[1]["index"], 3);
    }

    #[test]
    fn sugar_body_template_cid_is_stable_under_param_renaming() {
        // Canonical templates with $1/$2 must be byte-identical for two
        // sugar functions that differ only in their parameter names.
        let src_a = r##"
#[provekit::sugar(concept = "concept:noop", library = "ka")]
pub fn op(x: &i64, y: &i64) -> i64 {
    x.add(y)
}
"##;
        let src_b = r##"
#[provekit::sugar(concept = "concept:noop", library = "kb")]
pub fn op(alpha: &i64, beta: &i64) -> i64 {
    alpha.add(beta)
}
"##;
        let entry_a = single_sugar_entry_for_source("sugar_ast_template_alpha_a", src_a);
        let entry_b = single_sugar_entry_for_source("sugar_ast_template_alpha_b", src_b);
        let tpl_a = entry_a["body_source"]["ast_template"].to_string();
        let tpl_b = entry_b["body_source"]["ast_template"].to_string();
        assert_eq!(
            tpl_a, tpl_b,
            "alpha-equivalent sugars must produce byte-identical templates\nA: {tpl_a}\nB: {tpl_b}"
        );
        assert_eq!(
            entry_a["body_source"]["template_cid"], entry_b["body_source"]["template_cid"],
            "template_cid must match across alpha-equivalent sugars"
        );
        // But body_text and source_cid DIFFER (they include the original
        // parameter spellings). That asymmetry is intentional: body_text
        // drives materialize (verbatim splice), ast_template drives
        // recognize (canonical structural match).
        assert_ne!(
            entry_a["body_source"]["body_text"],
            entry_b["body_source"]["body_text"]
        );
        assert_ne!(
            entry_a["body_source"]["source_cid"],
            entry_b["body_source"]["source_cid"]
        );
    }

    // ---------------------------------------------------------------------
    // Recognizer foundation Phase C (#81, #82, #86): the provekit.plugin.recognize
    // RPC handler. Walks user source, matches function bodies' identifier-
    // canonical templates against supplied binding_templates by template_cid,
    // emits tier-`exact` tags for matches. Tier-1 = exact-cid match.
    // ---------------------------------------------------------------------

    #[test]
    fn recognize_emits_exact_tag_for_alpha_equivalent_user_function() {
        // The shim's sugar (what would land in the .proof envelope):
        let sugar_src = r##"
#[provekit::sugar(concept = "concept:json-parse", library = "provekit-shim-serde-json-rust")]
pub fn json_parse(s: &str) -> i64 {
    serde_json::from_str(s)
}
"##;
        let sugar_entry = single_sugar_entry_for_source("recognize_sugar_src", sugar_src);
        let binding_template = json!({
            "concept_name": sugar_entry["concept_name"],
            "library_tag": sugar_entry["target_library_tag"],
            "family": sugar_entry.get("family").cloned().unwrap_or(Value::Null),
            "ast_template": sugar_entry["body_source"]["ast_template"],
            "template_cid": sugar_entry["body_source"]["template_cid"],
            "param_names": sugar_entry["body_source"]["param_names"],
            "contract_cid": "blake3-512:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
        });

        // The user's function — alpha-equivalent (different param name).
        let user_src = r##"
pub fn json_parse(input: &str) -> Result<serde_json::Value, String> {
    serde_json::from_str(input)
}
"##;
        let root = temp_workspace("recognize_user_src");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        let user_rel = "src/lib.rs";
        fs::write(root.join(user_rel), user_src).expect("write user source");

        let resp = recognize(&json!({
            "project_root": root.to_string_lossy(),
            "source_paths": [user_rel],
            "binding_templates": [binding_template],
        }))
        .expect("recognize should succeed");

        let tags = resp["tags"].as_array().expect("tags array");
        assert_eq!(tags.len(), 1, "alpha-equivalent body must match: {tags:?}");
        let tag = &tags[0];
        assert_eq!(tag["concept_name"], "concept:json-parse");
        assert_eq!(tag["library_tag"], "provekit-shim-serde-json-rust");
        assert_eq!(tag["match_tier"], "exact");
        assert_eq!(tag["file"], user_rel);
        // param_bindings reflects the USER's spelling (input), not the sugar's (s).
        let bindings = tag["param_bindings"].as_array().expect("param_bindings");
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0]["index"], 1);
        assert_eq!(bindings[0]["source_text"], "input");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn recognize_returns_empty_tags_for_non_matching_source() {
        let sugar_src = r##"
#[provekit::sugar(concept = "concept:json-parse", library = "provekit-shim-serde-json-rust")]
pub fn json_parse(s: &str) -> i64 {
    serde_json::from_str(s)
}
"##;
        let sugar_entry = single_sugar_entry_for_source("recognize_neg_sugar", sugar_src);
        let binding_template = json!({
            "concept_name": sugar_entry["concept_name"],
            "library_tag": sugar_entry["target_library_tag"],
            "ast_template": sugar_entry["body_source"]["ast_template"],
            "template_cid": sugar_entry["body_source"]["template_cid"],
            "contract_cid": "blake3-512:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
        });

        // User's function is structurally DIFFERENT — calls a different function.
        let user_src = r##"
pub fn json_parse(s: &str) -> i64 {
    completely_different_function(s)
}
"##;
        let root = temp_workspace("recognize_neg_user");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        let user_rel = "src/lib.rs";
        fs::write(root.join(user_rel), user_src).expect("write user source");

        let resp = recognize(&json!({
            "project_root": root.to_string_lossy(),
            "source_paths": [user_rel],
            "binding_templates": [binding_template],
        }))
        .expect("recognize should succeed");

        let tags = resp["tags"].as_array().expect("tags array");
        assert!(
            tags.is_empty(),
            "non-matching source must produce no tags: {tags:?}"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn recognize_routes_multiple_bindings_per_call_site_pool() {
        // Two binding templates (json + sql shapes). User source contains
        // one match for each. Recognize emits two tags.
        let json_sugar = r##"
#[provekit::sugar(concept = "concept:json-parse", library = "json-lib")]
pub fn json_parse(s: &str) -> i64 {
    serde_json::from_str(s)
}
"##;
        let sql_sugar = r##"
#[provekit::sugar(concept = "concept:sql-execute", library = "sql-lib")]
pub fn sql_execute(conn: &i64, sql: &str, args: &i64) -> i64 {
    conn.execute(sql, args)
}
"##;
        let json_entry = single_sugar_entry_for_source("recognize_multi_json", json_sugar);
        let sql_entry = single_sugar_entry_for_source("recognize_multi_sql", sql_sugar);
        let bindings = json!([
            {
                "concept_name": json_entry["concept_name"],
                "library_tag": json_entry["target_library_tag"],
                "ast_template": json_entry["body_source"]["ast_template"],
                "template_cid": json_entry["body_source"]["template_cid"],
                "contract_cid": "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            },
            {
                "concept_name": sql_entry["concept_name"],
                "library_tag": sql_entry["target_library_tag"],
                "ast_template": sql_entry["body_source"]["ast_template"],
                "template_cid": sql_entry["body_source"]["template_cid"],
                "contract_cid": "blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            }
        ]);

        let user_src = r##"
pub fn json_parse(input: &str) -> i64 {
    serde_json::from_str(input)
}

pub fn sql_execute(c: &i64, q: &str, p: &i64) -> i64 {
    c.execute(q, p)
}
"##;
        let root = temp_workspace("recognize_multi_user");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        let user_rel = "src/lib.rs";
        fs::write(root.join(user_rel), user_src).expect("write user source");

        let resp = recognize(&json!({
            "project_root": root.to_string_lossy(),
            "source_paths": [user_rel],
            "binding_templates": bindings,
        }))
        .expect("recognize");

        let tags = resp["tags"].as_array().expect("tags array");
        assert_eq!(tags.len(), 2, "expected 2 tags (json + sql): {tags:?}");
        let concepts: Vec<&str> = tags
            .iter()
            .filter_map(|t| t["concept_name"].as_str())
            .collect();
        assert!(concepts.contains(&"concept:json-parse"));
        assert!(concepts.contains(&"concept:sql-execute"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn sugar_body_emits_param_names_list_for_recognize_binding() {
        // The recognize side needs the original param names to bind the
        // template's $N markers back to the user's actual variables at
        // tag emission time. The lifter exposes them as a separate field.
        let src = r##"
#[provekit::sugar(concept = "concept:sql-query-row", library = "rusqlite")]
pub fn query_row(conn: &i64, sql: &str, params: &i64, mapper: &i64) -> i64 {
    conn.query_row(sql, params, mapper)
}
"##;
        let entry = single_sugar_entry_for_source("sugar_ast_template_paramnames", src);
        let names = entry["body_source"]["param_names"]
            .as_array()
            .expect("param_names array");
        assert_eq!(names.len(), 4);
        assert_eq!(names[0], "conn");
        assert_eq!(names[1], "sql");
        assert_eq!(names[2], "params");
        assert_eq!(names[3], "mapper");
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
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        fs::write(
            src_dir.join("lib.rs"),
            r#"
pub fn fetch_status(url: &str) -> i64 {
    reqwest::get(url)
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

        // Substrate-canonical shape (#1075 federation): operand NAMES are
        // sugar, so scoped param/local references lift as EMPTY structural
        // leaves {}. The names travel on the CID-invisible operand_bindings
        // sidecar (verified separately below), making the rust term_shape
        // byte-identical to the python lifter's empty-leaf form so the same
        // algebra federates cross-language. f(x)=x+y and f(a)=a+b share this
        // shape. (Earlier the names rode the leaf, which made the shape
        // name-dependent and broke seam-4 byte-identity.)
        assert_eq!(
            shape,
            json!({
                "args": [
                    {},
                    {},
                ],
                "concept_name": "concept:add",
                "op_cid": "blake3-512:95fc70e63a5550fd2e25142f13932919c59d085654ab387789c798886b0111c61d28fe533fc98b50df70eea9428a9af8aa75372c8b1c1deb3acc1a4094790468"
            })
        );

        // Names-are-sugar: the operand symbols are recovered from the
        // operand_bindings sidecar (CID-invisible), not the leaves.
        let out = bind_lift(&{
            let root = temp_workspace("term_shape_add_bindings");
            let src_dir = root.join("src");
            fs::create_dir_all(&src_dir).expect("create src dir");
            fs::write(
                src_dir.join("lib.rs"),
                "pub fn add(x: i64, y: i64) -> i64 {\n    x + y\n}\n",
            )
            .expect("write source");
            let params = json!({
                "workspace_root": root.to_string_lossy(),
                "source_paths": ["."]
            });
            // leak the temp dir path via params; cleaned by OS temp reaper
            params
        })
        .expect("bind lift should succeed");
        let entry = out["ir"].as_array().expect("ir").first().expect("entry");
        let bindings = entry["operand_bindings"]
            .as_array()
            .expect("operand_bindings array");
        let symbols: Vec<&str> = bindings
            .iter()
            .filter_map(|b| b["symbol"].as_str())
            .collect();
        assert!(
            symbols.contains(&"x") && symbols.contains(&"y"),
            "operand symbols x,y must live on the operand_bindings sidecar, not the leaves: {bindings:#?}"
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
        let assign_cid = concept_op_cid("concept:assign").expect("assign cid");
        let add_cid = concept_op_cid("concept:add").expect("add cid");

        // Substrate-canonical shape (#1075 federation):
        // - assign TARGET is kind:symbol "q" — a let-binding NAME is structural
        //   data (it declares a new name), so it stays in the shape.
        // - SCOPED operand references (a, b) are EMPTY leaves {} — operand NAMES
        //   are sugar, recovered from operand_bindings so the shape is
        //   name-independent and federates cross-language.
        // - The tail expression `q` is a bare scoped-variable return: it is now
        //   an empty {} non-operation leaf and is dropped by
        //   collapse_operation_shapes, so the body collapses from
        //   seq([assign, q]) to just the assign. This is a CONSEQUENCE of
        //   names-are-sugar: `{ let q = a+b; q }` and `{ let z = a+b; z }` carry
        //   the same computation and now share a term_shape. The value-return
        //   flows via operand_bindings; discharge (#1441 5/5) and the rust/go
        //   production bridges round-trip unaffected.
        assert_eq!(
            shape,
            json!({
                "args": [
                    {"kind": "symbol", "text": "q"},
                    {
                        "args": [
                            {},
                            {},
                        ],
                        "concept_name": "concept:add",
                        "op_cid": add_cid
                    }
                ],
                "concept_name": "concept:assign",
                "op_cid": assign_cid
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
        // #1075 federation: `{ let q = a+b; q }` lifts as concept:assign(q, add).
        // The trailing bare scoped-variable return `q` is now an empty {}
        // non-operation leaf (operand NAMES are sugar), so collapse_operation_shapes
        // drops it and the seq([assign, q]) collapses to the assign alone. The
        // assignment boundary is still structurally distinct from a top-level
        // operator (assign != add), which is what this test guards.
        assert_eq!(
            let_rhs["concept_name"],
            json!("concept:assign"),
            "let+bare-tail-return body collapses to the concept:assign for the let-binding"
        );
        assert_eq!(
            let_rhs["args"][0],
            json!({"kind": "symbol", "text": "q"}),
            "assign's first child is the let-binding NAME leaf (structural, not sugar)"
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
        for expected in [
            "concept:conditional",
            "concept:eq",
            "concept:div",
            "concept:lt",
        ] {
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
        let target_names = collect_bind_lift_targets_with_source(&file, "")
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

    fn single_sugar_entry_for_source(name: &str, source: &str) -> Value {
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
            .iter()
            .find(|entry| entry["kind"] == "library-sugar-binding-entry")
            .expect("single sugar entry")
            .clone();
        let _ = fs::remove_dir_all(root);
        entry
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

    fn rust_lifter_parse_refusal_workspace(label: &str) -> PathBuf {
        let unique = format!(
            "provekit-walk-parse-refusal-{}-{}",
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

    // ========================================================================
    // #1396 PR-0: singleton-concept lift-gap validator tests
    //
    // Discrimination triplet covering the three input shapes:
    //   * positive  — `concept:X` singleton in catalog → gap emitted
    //   * negative  — `concept:X` with ≥2 realizations → no gap
    //   * structural — `library:X` prefix skipped entirely (no lookup)
    //
    // Plus a sanity check on the realization-count helper itself.
    // ========================================================================

    fn singleton_concept_gap_diagnostics(out: &Value) -> Vec<&Value> {
        out["diagnostics"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter(|d| d["kind"] == "lift-gap" && d["category"] == "singleton-concept")
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    }

    fn bind_lift_for_source(name: &str, source: &str) -> (PathBuf, Value) {
        let root = temp_workspace(name);
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        fs::write(src_dir.join("lib.rs"), source).expect("write source");
        let out = bind_lift(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": ["."]
        }))
        .expect("bind lift should succeed");
        (root, out)
    }

    #[test]
    fn singleton_concept_validator_positive_emits_gap_for_unknown_concept() {
        // A `concept:X` that the catalog has never heard of (0 realizations)
        // is the strictest singleton case.  The gap must be emitted.
        let (root, out) = bind_lift_for_source(
            "singleton_validator_positive_unknown",
            r#"
#[provekit::sugar(
    concept = "concept:totally-unknown-singleton-from-pr0-test",
    library = "test-library",
    loss = [],
)]
pub fn lonely_singleton(_x: i64) -> i64 {
    _x
}
"#,
        );

        let gaps = singleton_concept_gap_diagnostics(&out);
        assert_eq!(
            gaps.len(),
            1,
            "expected exactly one singleton-concept gap, got: {:#?}",
            out["diagnostics"]
        );
        let gap = gaps[0];
        assert_eq!(gap["kind"], "lift-gap");
        assert_eq!(gap["category"], "singleton-concept");
        assert_eq!(
            gap["concept"],
            "concept:totally-unknown-singleton-from-pr0-test"
        );
        assert_eq!(gap["realization_count"], 0);
        assert_eq!(gap["function_name"], "lonely_singleton");
        // Path is "src/lib.rs:<line>" with a colon-separated line number.
        let path = gap["path"].as_str().expect("path string");
        assert!(
            path.starts_with("src/lib.rs:"),
            "expected path to begin with src/lib.rs:, got {path}"
        );
        assert!(
            gap["suggestion"]
                .as_str()
                .expect("suggestion string")
                .contains("library:totally-unknown-singleton-from-pr0-test"),
            "suggestion should propose prefix-flip with bare concept name"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn singleton_concept_validator_negative_no_gap_for_well_realized_concept() {
        // `concept:closure` has 4 realizations in the on-disk catalog
        // (jvm:lambda-invokedynamic, rust:closure-expression,
        // python:native-closure, c11:defunctionalized-env-struct).
        // The validator must NOT emit a gap for it.
        let (root, out) = bind_lift_for_source(
            "singleton_validator_negative_well_realized",
            r#"
#[provekit::sugar(
    concept = "concept:closure",
    library = "well-realized-test",
    loss = [],
)]
pub fn closure_carrier(_x: i64) -> i64 {
    _x
}
"#,
        );

        let gaps = singleton_concept_gap_diagnostics(&out);
        assert!(
            gaps.is_empty(),
            "concept:closure has >=2 realizations; no singleton-concept gap \
             expected, got: {:#?}",
            gaps
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn singleton_concept_validator_structural_skips_library_prefix() {
        // `library:X` prefixed concepts are kit-specific identifiers and
        // make no cross-language claim.  The validator MUST NOT perform a
        // catalog lookup or emit a gap, even though `library:foo` is
        // certainly absent from the catalog.
        let (root, out) = bind_lift_for_source(
            "singleton_validator_structural_library_prefix",
            r#"
#[provekit::sugar(
    concept = "library:kit-private-helper",
    library = "structural-test",
    loss = [],
)]
pub fn kit_private_helper(_x: i64) -> i64 {
    _x
}
"#,
        );

        let gaps = singleton_concept_gap_diagnostics(&out);
        assert!(
            gaps.is_empty(),
            "library:X prefix must be skipped entirely; no gap expected, got: {:#?}",
            gaps
        );

        // Sanity: the @sugar entry itself IS still emitted (validator is
        // GAP-EMITTING, not refusing — existing behavior preserved).
        let ir = out["ir"].as_array().expect("ir array");
        let sugar_entries: Vec<_> = ir
            .iter()
            .filter(|e| e["kind"] == "library-sugar-binding-entry")
            .collect();
        assert_eq!(
            sugar_entries.len(),
            1,
            "library-sugar-binding-entry must still be emitted regardless of \
             singleton-concept validator outcome"
        );
        assert_eq!(
            sugar_entries[0]["concept_name"],
            "library:kit-private-helper"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn concept_realization_counts_matches_catalog_shape() {
        // Sanity test on the helper: known multi-realized concepts surface
        // their expected counts, and pure singletons or absent names map to
        // 0 (the `.get().copied().unwrap_or(0)` fallback at the call site).
        let counts = concept_realization_counts();
        let closure = counts.get("concept:closure").copied().unwrap_or(0);
        assert!(
            closure >= 2,
            "concept:closure has at least two realizations in the bundled \
             catalog (got {closure})"
        );
        let absent = counts
            .get("concept:totally-unknown-singleton-from-pr0-test")
            .copied()
            .unwrap_or(0);
        assert_eq!(
            absent, 0,
            "an absent concept name must map to zero realizations"
        );
    }

    #[test]
    fn concept_realization_counts_excludes_abstraction_lift_m_edges() {
        // #1397 (PR-1 absorbed): the seven reverse-direction `lang->concept`
        // entries are abstraction-lift M-edges, NOT abstraction-realization
        // N-edges, and must not count toward the >=2 threshold.
        //
        // `concept:option` and `concept:pair` each have exactly ONE forward
        // realization plus ONE reverse abstraction-lift M-edge.  Counting the
        // M-edge would inflate them to 2 (no gap); excluding it leaves them at
        // 1 (singleton -> gap).  This test is the regression guard for the
        // counting-rule fix.
        let counts = concept_realization_counts();
        assert_eq!(
            counts.get("concept:option").copied().unwrap_or(0),
            1,
            "concept:option has one abstraction-realization; its reverse \
             abstraction-lift M-edge must not be counted"
        );
        assert_eq!(
            counts.get("concept:pair").copied().unwrap_or(0),
            1,
            "concept:pair has one abstraction-realization; its reverse \
             abstraction-lift M-edge must not be counted"
        );
        // Sanity: a concept with multiple forward realizations PLUS a reverse
        // M-edge still counts only the forward ones.
        assert_eq!(
            counts.get("concept:option-bind").copied().unwrap_or(0),
            3,
            "concept:option-bind has three forward realizations; its reverse \
             abstraction-lift M-edge must not inflate the count to 4"
        );
    }

    #[test]
    fn singleton_concept_validator_flags_concept_with_only_an_m_edge_companion() {
        // End-to-end: a @sugar function claiming `concept:option` (1 forward
        // realization + 1 reverse M-edge) must surface a singleton-concept
        // gap, because the M-edge does not count toward the >=2 threshold.
        let (root, out) = bind_lift_for_source(
            "singleton_validator_m_edge_companion",
            r#"
#[provekit::sugar(
    concept = "concept:option",
    library = "m-edge-test",
    loss = [],
)]
pub fn option_carrier(_x: i64) -> i64 {
    _x
}
"#,
        );

        let gaps = singleton_concept_gap_diagnostics(&out);
        assert_eq!(
            gaps.len(),
            1,
            "concept:option is a singleton once abstraction-lift M-edges are \
             excluded; expected exactly one gap, got: {:#?}",
            out["diagnostics"]
        );
        assert_eq!(gaps[0]["concept"], "concept:option");
        assert_eq!(gaps[0]["realization_count"], 1);

        let _ = fs::remove_dir_all(root);
    }

    // -----------------------------------------------------------------
    // Implication lifter (#97). Walks production-code function bodies
    // and emits one kind:bridge memento per Expr::Call / Expr::MethodCall
    // call site, looked up against the supplied contract_bindings by
    // ctor-name index over the contract name's "<callee>@..." prefix.
    // -----------------------------------------------------------------

    #[test]
    fn lift_implications_emits_bridge_per_call_expression_matched_by_callee_name() {
        let src = r##"
pub fn caller(input: &str) -> i64 {
    let parsed = parse_input(input);
    parsed.normalize_value()
}
"##;
        let root = temp_workspace("lift_implications_basic");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        let rel = "src/lib.rs";
        fs::write(root.join(rel), src).expect("write source");

        let contract_bindings = json!([
            { "name": "parse_input@src/lib.rs:5:8",
              "contract_cid": "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" },
            { "name": "normalize_value@src/lib.rs:12:8",
              "contract_cid": "blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb" },
        ]);

        let resp = lift_implications(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": [rel],
            "contract_bindings": contract_bindings,
        }))
        .expect("lift_implications");

        assert_eq!(resp["kind"], "ir-document");
        let ir = resp["ir"].as_array().expect("ir array");
        assert_eq!(
            ir.len(),
            2,
            "expected one bridge per call expression (parse_input + normalize_value), got: {ir:?}"
        );

        // Free function call -> kind:bridge with sourceSymbol = parse_input.
        let parse = ir
            .iter()
            .find(|e| e["sourceSymbol"] == "parse_input")
            .expect("parse_input bridge");
        assert_eq!(parse["kind"], "bridge");
        assert_eq!(parse["sourceLayer"], "rust");
        assert_eq!(parse["targetLayer"], "rust-tests");
        assert_eq!(
            parse["targetContractCid"],
            "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        assert_eq!(parse["target"]["cid"], parse["targetContractCid"]);
        assert!(parse["name"]
            .as_str()
            .unwrap()
            .starts_with("intra-body:rust:parse_input@"));

        // Method call -> sourceSymbol = the method ident, NOT the receiver.
        let norm = ir
            .iter()
            .find(|e| e["sourceSymbol"] == "normalize_value")
            .expect("normalize_value bridge");
        assert_eq!(norm["kind"], "bridge");
        assert_eq!(
            norm["targetContractCid"],
            "blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn function_contract_lift_stamps_library_from_cargo_package_name() {
        let root = temp_workspace("function_contract_library_stamp");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        fs::write(
            root.join("Cargo.toml"),
            r#"
[package]
name = "provekit-cli"
version = "0.1.0"
edition = "2021"
"#,
        )
        .expect("write Cargo.toml");
        fs::write(
            src_dir.join("lib.rs"),
            r#"
pub fn identity(value: i64) -> i64 {
    value
}
"#,
        )
        .expect("write source");

        let resp = function_contract_lift(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": ["."]
        }))
        .expect("function contract lift");

        let entries = resp["ir"].as_array().expect("ir array");
        let entry = entries
            .iter()
            .find(|entry| entry["name"] == "identity")
            .expect("identity function contract");
        assert_eq!(entry["library"], "provekit_cli");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn lift_implications_resolves_annotated_method_receiver_to_type_crate() {
        let src = r##"
use dep_crate::Widget;

pub fn make_widget() -> Widget {
    panic!()
}

pub fn caller() {
    let widget: Widget = make_widget();
    widget.run();
}
"##;
        let root = temp_workspace("lift_implications_typed_receiver");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        fs::write(
            root.join("Cargo.toml"),
            r#"
[package]
name = "consumer-crate"
version = "0.1.0"
edition = "2021"
"#,
        )
        .expect("write Cargo.toml");
        let rel = "src/lib.rs";
        fs::write(root.join(rel), src).expect("write source");

        let current_run = "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let dep_run = "blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let contract_bindings = json!([
            { "name": "make_widget@src/lib.rs:4:1",
              "library": "consumer_crate",
              "contract_cid": "blake3-512:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc" },
            { "name": "run@src/lib.rs:10:5",
              "library": "consumer_crate",
              "contract_cid": current_run },
            { "name": "run@dep/src/lib.rs:10:5",
              "library": "dep_crate",
              "contract_cid": dep_run },
        ]);

        let resp = lift_implications(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": [rel],
            "contract_bindings": contract_bindings,
        }))
        .expect("lift_implications");

        let ir = resp["ir"].as_array().expect("ir array");
        let run = ir
            .iter()
            .find(|entry| entry["sourceSymbol"] == "run")
            .expect("run bridge");
        assert_eq!(run["targetContractCid"], dep_run);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn lift_implications_resolves_inferred_method_receivers_to_type_crate() {
        let src = r##"
use dep_crate::Widget;

pub fn make_widget() -> Widget {
    panic!()
}

pub fn caller() {
    let from_return = make_widget();
    from_return.run();

    let from_ctor = Widget::new();
    from_ctor.run();
}
"##;
        let root = temp_workspace("lift_implications_inferred_receiver");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        fs::write(
            root.join("Cargo.toml"),
            r#"
[package]
name = "consumer-crate"
version = "0.1.0"
edition = "2021"
"#,
        )
        .expect("write Cargo.toml");
        let rel = "src/lib.rs";
        fs::write(root.join(rel), src).expect("write source");

        let current_run = "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let dep_run = "blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let contract_bindings = json!([
            { "name": "make_widget@src/lib.rs:4:1",
              "library": "consumer_crate",
              "contract_cid": "blake3-512:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc" },
            { "name": "new@dep/src/lib.rs:3:1",
              "library": "dep_crate",
              "contract_cid": "blake3-512:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd" },
            { "name": "run@src/lib.rs:10:5",
              "library": "consumer_crate",
              "contract_cid": current_run },
            { "name": "run@dep/src/lib.rs:10:5",
              "library": "dep_crate",
              "contract_cid": dep_run },
        ]);

        let resp = lift_implications(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": [rel],
            "contract_bindings": contract_bindings,
        }))
        .expect("lift_implications");

        let ir = resp["ir"].as_array().expect("ir array");
        let run_targets: Vec<&str> = ir
            .iter()
            .filter(|entry| entry["sourceSymbol"] == "run")
            .filter_map(|entry| entry["targetContractCid"].as_str())
            .collect();
        assert_eq!(run_targets, vec![dep_run, dep_run]);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn lift_implications_scans_src_when_source_path_is_project_root() {
        let src = r##"
pub fn caller(input: &str) -> i64 {
    parse_input(input)
}
"##;
        let root = temp_workspace("lift_implications_project_root");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        fs::write(src_dir.join("lib.rs"), src).expect("write source");

        let contract_bindings = json!([
            { "name": "parse_input@src/lib.rs:5:8",
              "contract_cid": "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" },
        ]);

        let resp = lift_implications(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": ["."],
            "contract_bindings": contract_bindings,
        }))
        .expect("lift_implications");

        let ir = resp["ir"].as_array().expect("ir array");
        assert!(
            ir.iter()
                .any(|entry| entry["sourceSymbol"] == "parse_input"),
            "mint sends source_paths=[\".\"], so implication lift must scan src/: {ir:?}"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rpc_dispatches_provekit_plugin_lift_implications_method() {
        let src = r##"
pub fn caller(input: &str) -> i64 {
    parse_input(input)
}
"##;
        let root = temp_workspace("lift_implications_rpc_method");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        fs::write(src_dir.join("lib.rs"), src).expect("write source");

        let response = handle_line(&json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "provekit.plugin.lift_implications",
            "params": {
                "workspace_root": root.to_string_lossy(),
                "source_paths": ["."],
                "contract_bindings": [{
                    "name": "parse_input@src/lib.rs:5:8",
                    "contract_cid": "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                }],
            }
        }).to_string());

        assert!(
            response.get("error").is_none(),
            "RPC method table must expose provekit.plugin.lift_implications: {response}"
        );
        let ir = response["result"]["ir"].as_array().expect("ir array");
        assert!(
            ir.iter()
                .any(|entry| entry["sourceSymbol"] == "parse_input"),
            "RPC implication route should return bridge IR: {response}"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn lift_implications_emits_lift_gap_for_unmatched_callee() {
        let src = r##"
pub fn caller() -> i64 {
    completely_unknown_function(0)
}
"##;
        let root = temp_workspace("lift_implications_gap");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        let rel = "src/lib.rs";
        fs::write(root.join(rel), src).expect("write source");

        let resp = lift_implications(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": [rel],
            "contract_bindings": [],
        }))
        .expect("lift_implications");

        let ir = resp["ir"].as_array().expect("ir array");
        assert!(ir.is_empty(), "no bridges should be emitted: {ir:?}");
        let diags = resp["diagnostics"].as_array().expect("diagnostics array");
        assert_eq!(
            diags.len(),
            1,
            "one lift-gap for the unmatched call: {diags:?}"
        );
        assert_eq!(diags[0]["kind"], "lift-gap");
        assert_eq!(diags[0]["reason"], "no-contract-for-callee");
        assert_eq!(diags[0]["callee"], "completely_unknown_function");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn lift_implications_uses_last_path_segment_as_callee_name() {
        // `serde_json::from_str(s)` lowers to a callsite with
        // sourceSymbol = "from_str" (the LAST segment), which matches
        // how the test lifter names contracts: `from_str@...`.
        let src = r##"
pub fn caller(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap()
}
"##;
        let root = temp_workspace("lift_implications_path");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        let rel = "src/lib.rs";
        fs::write(root.join(rel), src).expect("write source");

        let contract_bindings = json!([
            { "name": "from_str@src/foreign.rs:42:8",
              "library": "serde_json",
              "contract_cid": "blake3-512:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc" },
            { "name": "unwrap@somewhere.rs:1:1",
              "contract_cid": "blake3-512:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd" },
        ]);

        let resp = lift_implications(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": [rel],
            "contract_bindings": contract_bindings,
        }))
        .expect("lift_implications");

        let ir = resp["ir"].as_array().expect("ir array");
        let symbols: Vec<&str> = ir
            .iter()
            .filter_map(|e| e["sourceSymbol"].as_str())
            .collect();
        assert!(
            symbols.contains(&"from_str"),
            "expected from_str (last path segment) in: {symbols:?}"
        );
        assert!(
            symbols.contains(&"unwrap"),
            "expected unwrap (method call) in: {symbols:?}"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn function_contract_lift_marks_only_body_discharge_supported_contracts_eligible() {
        let src = r##"
pub struct ExitReport {
    pub code: i64,
}

pub fn double(x: i64) -> i64 {
    x * 2
}

pub fn report_exit_code(report: ExitReport) -> i64 {
    report.code
}

pub fn wrap_ok(x: i64) -> Result<i64, ()> {
    Ok(x)
}
"##;
        let root = temp_workspace("function_contract_body_discharge_eligibility");
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        fs::write(src_dir.join("lib.rs"), src).expect("write source");

        let resp = function_contract_lift(&json!({
            "workspace_root": root.to_string_lossy(),
            "source_paths": ["."],
        }))
        .expect("function contract lift");

        let ir = resp["ir"].as_array().expect("ir array");
        let by_name = |name: &str| -> &Value {
            ir.iter()
                .find(|entry| entry["name"] == name)
                .unwrap_or_else(|| panic!("missing lifted function contract `{name}`: {ir:?}"))
        };

        assert_eq!(
            by_name("double")["bodyDischargeEligible"],
            true,
            "plain arithmetic body should be eligible for current body discharge"
        );
        assert_eq!(
            by_name("report_exit_code")["bodyDischargeEligible"],
            false,
            "field projection is Rust-kit-owned sugar that is not yet solver-backed"
        );
        assert_eq!(
            by_name("wrap_ok")["bodyDischargeEligible"],
            false,
            "Result::Ok construction needs Rust stdlib algebra before it is body-discharge eligible"
        );
        let diagnostics = resp["diagnostics"].as_array().expect("diagnostics array");
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag["kind"] == "body-discharge-gap"
                    && diag["function"] == "report_exit_code"),
            "ineligible contracts must surface a precise kit diagnostic: {diagnostics:?}"
        );
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag["kind"] == "body-discharge-gap" && diag["function"] == "wrap_ok"),
            "ineligible stdlib constructor contract must surface a precise kit diagnostic: {diagnostics:?}"
        );

        let _ = fs::remove_dir_all(root);
    }
}
