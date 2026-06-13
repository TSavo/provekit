// SPDX-License-Identifier: Apache-2.0
//
// RPC entrypoint for the Rust test-assertion consistency lifter.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use sugar_ir_symbolic::serialize::marshal_declarations;
use sugar_lift_rust_tests::source_oracle;
use sugar_lift_rust_tests::{lift_file_with_options, LiftOptions, TargetCfg};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const SURFACE: &str = "rust-test-assertions";
const KIT_DECLARATION_RPC_METHOD: &str = "sugar.plugin.kit_declaration";

fn initialize_result() -> Value {
    json!({
        "name": "sugar-lift-rust-tests-rpc",
        "version": VERSION,
        "protocol_version": "pep/1.7.0",
        "capabilities": {
            "authoring_surfaces": [SURFACE],
            "ir_version": "v1.1.0",
            "emits_signed_mementos": false,
        },
    })
}

fn kit_declaration_result() -> Value {
    json!({
        "kit": {
            "id": SURFACE,
            "language": "rust",
            "version": VERSION,
        },
        "rpc": {
            "methods": [
                {"name": "initialize", "required": true},
                {"name": KIT_DECLARATION_RPC_METHOD, "required": true},
                {"name": "lift", "required": true},
                {"name": "shutdown", "required": false},
            ]
        },
        "proofResolution": {"strategy": "cargo"},
        "effectKinds": [],
        "effectLeaves": [],
        "guardPredicates": [],
        "controlCarriers": [],
        "residueCategories": [],
    })
}

fn lift(params: &Value) -> Value {
    let workspace_root = params
        .get("workspace_root")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let requested: Vec<String> = match params.get("source_paths").and_then(Value::as_array) {
        Some(arr) if !arr.is_empty() => arr
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        _ => vec![".".to_string()],
    };

    let mut rel_paths = Vec::new();
    for entry in &requested {
        let abs = workspace_root.join(entry);
        if abs.is_dir() {
            for rel in enumerate_rs_files(&abs) {
                let joined = if entry == "." {
                    rel
                } else {
                    format!("{}/{}", entry.trim_end_matches('/'), rel)
                };
                rel_paths.push(joined);
            }
        } else {
            rel_paths.push(entry.clone());
        }
    }
    rel_paths.sort();
    rel_paths.dedup();

    let mut entries = Vec::new();
    let mut diagnostics = Vec::new();
    // Source-audit accounting (parity with Java's SourceWarrant/sourceLedger,
    // #2134-#2136). The DENOMINATOR is the SourceOracle's enumeration of every
    // function in the file -- not just the things the kit classified. A `#[test]`
    // fn is in this kit's universe (warranted, or refused if the lift warned);
    // a non-test fn the kit does not speak is `unclassified` -- the dark this
    // metric exists to surface. `unclassified` is therefore MEASURED against the
    // whole source, not 0 by construction.
    let mut source_loci: Vec<Value> = Vec::new();
    let mut source_mementos: Vec<Value> = Vec::new();
    let options = match lift_options_from_config(&workspace_root, params) {
        Ok(options) => options,
        Err(reason) => {
            diagnostics.push(json!({
                "kind": "lift-gap",
                "path": params
                    .get("config_path")
                    .and_then(Value::as_str)
                    .unwrap_or(".sugar/config.toml"),
                "item": "rust-test-assertions.target_cfg",
                "reason": reason,
            }));
            LiftOptions::default()
        }
    };
    for rel in &rel_paths {
        let abs = workspace_root.join(rel);
        let bytes = match std::fs::read(&abs) {
            Ok(bytes) => bytes,
            Err(e) => {
                diagnostics.push(json!({
                    "kind": "lift-gap",
                    "path": rel,
                    "reason": format!("read: {e}"),
                }));
                continue;
            }
        };
        let src = match std::str::from_utf8(&bytes) {
            Ok(src) => src,
            Err(_) => {
                diagnostics.push(json!({
                    "kind": "lift-gap",
                    "path": rel,
                    "reason": "non-utf8 source",
                }));
                continue;
            }
        };
        let file = match syn::parse_file(src) {
            Ok(file) => file,
            Err(e) => {
                diagnostics.push(json!({
                    "kind": "lift-gap",
                    "path": rel,
                    "reason": format!("parse: {e}"),
                }));
                continue;
            }
        };
        let out = lift_file_with_options(&file, rel, &options);
        let marshalled = marshal_declarations(&out.decls);
        let parsed: Value = serde_json::from_str(&marshalled).unwrap_or_else(|_| json!([]));
        if let Some(arr) = parsed.as_array() {
            entries.extend(arr.iter().cloned());
        }
        for w in &out.warnings {
            diagnostics.push(json!({
                "kind": "lift-gap",
                "path": w.source_path,
                "item": w.item_name,
                "reason": w.reason,
            }));
        }
        // DENOMINATOR: the oracle enumerates every function in the file. Each one
        // gets a content-addressed memento and a classified locus, so the dark
        // (functions the kit does not speak) is COUNTED, not skipped.
        let mut fns: Vec<FnRef> = Vec::new();
        collect_fns(&file.items, &mut fns);
        for fr in fns {
            let memento =
                source_oracle::source_memento_of(rel, src, fr.span, &fr.name, fr.sig, fr.block);
            let name = fr.name.clone();
            let is_test = fn_has_test_attr(fr.attrs);
            let warning = out.warnings.iter().find(|w| {
                w.item_name == name || w.item_name.ends_with(&format!("::{name}"))
            });
            // TOTAL classifier (Phase 1 -- close the gate): every function exits
            // into warranted / support / refused-by-name. There is NO
            // `unclassified` fall-through: a function the kit cannot warrant is
            // REFUSED with a named reason (loudly-bounded-lossy), never dark.
            // `unclassified` can now only mean a classifier BUG, not a tuning
            // number; the real meter is `refused` (= bin-1 to drain).
            let (status, reason): (&str, Option<String>) = if is_test {
                match warning {
                    Some(w) => ("refused", Some(w.reason.clone())),
                    None => ("warranted", None),
                }
            } else if is_generalizable_value_fn(fr.block) {
                // body is a pure formula over params: contract is `result = <body>`,
                // constructible + recompute-verifiable from the memento.
                ("warranted", None)
            } else if out.reduced_helpers.contains(&name) {
                // a non-test fn the reducer inlined to discharge a test: it backs
                // a warrant, so it is `support`.
                ("support", None)
            } else {
                // Refused BY NAME with a categorized reason (conjunct 4): the IO
                // membrane (bin-2, the floor) split from drainable bin-1.
                ("refused", Some(refusal_reason(fr.block)))
            };
            let mut locus = json!({
                "file": rel,
                "role": "rust-test-assertions",
                "ast_kind": if is_test { "test-fn" } else { "fn" },
                "ast_path": name,
                "line": memento.span.start_line,
                "status": status,
            });
            if let Some(r) = reason {
                locus["reason"] = json!(r);
            }
            source_loci.push(locus);
            source_mementos.push(memento.to_json());
        }
    }

    let ledger = source_ledger(&source_loci);
    json!({
        "kind": "ir-document",
        "ir": entries,
        "diagnostics": diagnostics,
        "refusals": [],
        "sourceLedger": ledger,
        "sourceAudits": [json!({
            "role": "rust-test-assertions",
            "universe_kind": "test-assertion",
            "loci": source_loci,
        })],
        // Content-addressed mementos (file + span + BLAKE3-512 of body/template,
        // never source text) -- one per enumerated function, recompute-verifiable.
        "sourceMementos": source_mementos,
    })
}

/// Collect every `fn` item in the file -- top-level and nested in inline
/// modules -- as the source-audit denominator. (Impl methods are
/// `ImplItemFn`, not `ItemFn`; handled below.)
struct FnRef<'a> {
    span: proc_macro2::Span,
    name: String,
    sig: &'a syn::Signature,
    block: &'a syn::Block,
    attrs: &'a [syn::Attribute],
}

/// Enumerate EVERY function body in the file as the source-audit denominator:
/// free `fn` items, methods in `impl` blocks, and trait methods with default
/// bodies, recursing into inline modules. A trait method without a body declares
/// no constructor here, so it is not a locus. Impl methods are the bulk of real
/// code -- excluding them would make `unclassified=0` hollow.
fn collect_fns<'a>(items: &'a [syn::Item], out: &mut Vec<FnRef<'a>>) {
    use syn::spanned::Spanned;
    for item in items {
        match item {
            syn::Item::Fn(f) => out.push(FnRef {
                span: f.span(),
                name: f.sig.ident.to_string(),
                sig: &f.sig,
                block: &f.block,
                attrs: &f.attrs,
            }),
            syn::Item::Mod(m) => {
                if let Some((_, inner)) = &m.content {
                    collect_fns(inner, out);
                }
            }
            syn::Item::Impl(im) => {
                for ii in &im.items {
                    if let syn::ImplItem::Fn(m) = ii {
                        out.push(FnRef {
                            span: m.span(),
                            name: m.sig.ident.to_string(),
                            sig: &m.sig,
                            block: &m.block,
                            attrs: &m.attrs,
                        });
                    }
                }
            }
            syn::Item::Trait(tr) => {
                for ti in &tr.items {
                    if let syn::TraitItem::Fn(m) = ti {
                        if let Some(block) = &m.default {
                            out.push(FnRef {
                                span: m.span(),
                                name: m.sig.ident.to_string(),
                                sig: &m.sig,
                                block,
                                attrs: &m.attrs,
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// True iff the function body is a single non-diverging tail expression that is
/// a PURE FORMULA over params/literals -- its contract is `result = <body>`,
/// warrantable by construction. (The narrow, sound shape; widens slice by slice.)
fn is_generalizable_value_fn(block: &syn::Block) -> bool {
    block_reduces_to_pure_formula(block)
}

/// A block whose VALUE is a pure formula: zero or more immutable
/// `let <ident> = <pure formula>;` bindings, then a single pure-formula tail
/// expression. The contract is `result = <tail, lets substituted>` -- still a
/// pure formula, so still sound.
fn block_reduces_to_pure_formula(block: &syn::Block) -> bool {
    let Some((syn::Stmt::Expr(tail, None), prefix)) = block.stmts.split_last() else {
        return false;
    };
    expr_is_pure_formula(tail) && prefix.iter().all(is_immutable_pure_let)
}

/// `let <immutable ident> = <pure formula>;` with no `else` arm.
fn is_immutable_pure_let(stmt: &syn::Stmt) -> bool {
    let syn::Stmt::Local(local) = stmt else {
        return false;
    };
    let ident_immutable = match &local.pat {
        syn::Pat::Ident(p) => p.mutability.is_none() && p.subpat.is_none(),
        syn::Pat::Type(pt) => {
            matches!(&*pt.pat, syn::Pat::Ident(p) if p.mutability.is_none() && p.subpat.is_none())
        }
        _ => false,
    };
    let init_pure = local
        .init
        .as_ref()
        .is_some_and(|init| init.diverge.is_none() && expr_is_pure_formula(&init.expr));
    ident_immutable && init_pure
}

/// A pure formula over params and literals: literals, paths, arithmetic/bit ops,
/// casts, references, field/index, tuples/arrays, and pure named calls. A method
/// call, macro, `?`, await, or closure is opaque/effectful -> not a pure formula.
fn expr_is_pure_formula(expr: &syn::Expr) -> bool {
    use syn::Expr;
    match expr {
        Expr::Lit(_) | Expr::Path(_) => true,
        Expr::Paren(p) => expr_is_pure_formula(&p.expr),
        Expr::Group(g) => expr_is_pure_formula(&g.expr),
        Expr::Reference(r) => expr_is_pure_formula(&r.expr),
        Expr::Cast(c) => expr_is_pure_formula(&c.expr),
        Expr::Unary(u) => expr_is_pure_formula(&u.expr),
        Expr::Binary(b) => expr_is_pure_formula(&b.left) && expr_is_pure_formula(&b.right),
        Expr::Field(f) => expr_is_pure_formula(&f.base),
        Expr::Index(i) => expr_is_pure_formula(&i.expr) && expr_is_pure_formula(&i.index),
        Expr::Tuple(t) => t.elems.iter().all(expr_is_pure_formula),
        Expr::Array(a) => a.elems.iter().all(expr_is_pure_formula),
        Expr::Call(call) => {
            matches!(&*call.func, Expr::Path(_)) && call.args.iter().all(expr_is_pure_formula)
        }
        // Pure value constructors over pure fields/elements.
        Expr::Struct(s) => {
            s.fields.iter().all(|f| expr_is_pure_formula(&f.expr))
                && s.rest.as_ref().is_none_or(|r| expr_is_pure_formula(r))
        }
        Expr::Repeat(r) => expr_is_pure_formula(&r.expr) && expr_is_pure_formula(&r.len),
        Expr::Range(r) => {
            r.start.as_ref().is_none_or(|s| expr_is_pure_formula(s))
                && r.end.as_ref().is_none_or(|e| expr_is_pure_formula(e))
        }
        // Control-flow as a VALUE: an ite / match-tree over pure formulas is
        // itself a pure formula (result = ite(cond, then, else) / match-tree).
        Expr::If(e) => {
            expr_is_pure_formula(&e.cond)
                && block_reduces_to_pure_formula(&e.then_branch)
                && e.else_branch
                    .as_ref()
                    .is_some_and(|(_, els)| expr_is_pure_formula(els))
        }
        Expr::Match(m) => {
            expr_is_pure_formula(&m.expr)
                && m.arms
                    .iter()
                    .all(|a| a.guard.is_none() && expr_is_pure_formula(&a.body))
        }
        Expr::Block(b) => block_reduces_to_pure_formula(&b.block),
        _ => false,
    }
}

/// True iff the function carries a `#[test]` (or `#[…::test]`) attribute.
fn fn_has_test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path()
            .segments
            .last()
            .is_some_and(|seg| seg.ident == "test")
    })
}

/// Scan a refused body for the dominant blocker, so every `refused` carries a
/// NAMED, CATEGORIZED reason -- and the IO membrane (bin-2, the floor) is split
/// from drainable bin-1 (loop / `?` / method-call / macro).
#[derive(Default)]
struct BlockerScan {
    io: Option<String>,
    has_loop: bool,
    has_try: bool,
    has_method: bool,
    macro_name: Option<String>,
    has_assign: bool,
}

impl<'ast> syn::visit::Visit<'ast> for BlockerScan {
    fn visit_expr_for_loop(&mut self, n: &'ast syn::ExprForLoop) {
        self.has_loop = true;
        syn::visit::visit_expr_for_loop(self, n);
    }
    fn visit_expr_while(&mut self, n: &'ast syn::ExprWhile) {
        self.has_loop = true;
        syn::visit::visit_expr_while(self, n);
    }
    fn visit_expr_loop(&mut self, n: &'ast syn::ExprLoop) {
        self.has_loop = true;
        syn::visit::visit_expr_loop(self, n);
    }
    fn visit_expr_try(&mut self, n: &'ast syn::ExprTry) {
        self.has_try = true;
        syn::visit::visit_expr_try(self, n);
    }
    fn visit_expr_assign(&mut self, n: &'ast syn::ExprAssign) {
        self.has_assign = true;
        syn::visit::visit_expr_assign(self, n);
    }
    fn visit_expr_method_call(&mut self, n: &'ast syn::ExprMethodCall) {
        self.has_method = true;
        syn::visit::visit_expr_method_call(self, n);
    }
    fn visit_expr_call(&mut self, n: &'ast syn::ExprCall) {
        if self.io.is_none() {
            if let syn::Expr::Path(p) = &*n.func {
                let path = p
                    .path
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::");
                if is_io_path(&path) {
                    self.io = Some(format!("call `{path}`"));
                }
            }
        }
        syn::visit::visit_expr_call(self, n);
    }
    fn visit_macro(&mut self, m: &'ast syn::Macro) {
        let name = m
            .path
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_default();
        if self.io.is_none()
            && matches!(
                name.as_str(),
                "println" | "print" | "eprintln" | "eprint" | "write" | "writeln"
            )
        {
            self.io = Some(format!("macro `{name}!`"));
        }
        if self.macro_name.is_none() {
            self.macro_name = Some(name);
        }
        syn::visit::visit_macro(self, m);
    }
}

fn is_io_path(path: &str) -> bool {
    [
        "fs::",
        "io::",
        "net::",
        "process::",
        "time::Instant",
        "time::SystemTime",
        "File",
        "stdin",
        "stdout",
        "stderr",
        "thread::spawn",
        "thread::sleep",
    ]
    .iter()
    .any(|m| path.contains(m))
}

/// The named, categorized refusal reason for a body the kit can't warrant.
/// IO markers => bin-2 (the named floor); everything else => a bin-1 category
/// (a future walker slice migrates it to `warranted`).
fn refusal_reason(block: &syn::Block) -> String {
    let mut s = BlockerScan::default();
    syn::visit::Visit::visit_block(&mut s, block);
    if let Some(io) = s.io {
        return format!("IO membrane (bin-2): {io}");
    }
    if s.has_loop {
        return "loop: walker does not fold loops yet (bin-1)".to_string();
    }
    if s.has_try {
        return "`?` operator: short-circuit not modeled yet (bin-1)".to_string();
    }
    if s.has_method {
        return "method call: opaque receiver, needs oracle resolution (bin-1)".to_string();
    }
    if let Some(m) = s.macro_name {
        return format!("opaque macro `{m}!` (bin-1)");
    }
    if s.has_assign {
        return "mutation / reassignment not modeled yet (bin-1)".to_string();
    }
    "multi-statement / unmodeled body shape (bin-1)".to_string()
}

/// Roll the per-locus statuses into the `sourceLedger` the CLI source-audit gate
/// requires. The CLI RECOMPUTES this from the loci, so it must be exactly the
/// per-status counts. `unclassified_source` is the silent-drop measure: every
/// locus the kit emits carries a classified status, so a non-zero count here
/// means a source locus escaped accounting -- the thing `silent = 0` forbids.
fn source_ledger(loci: &[Value]) -> Value {
    let count = |status: &str| {
        loci.iter()
            .filter(|l| l.get("status").and_then(Value::as_str) == Some(status))
            .count()
    };
    json!({
        "source_loci": loci.len(),
        "source_warranted": count("warranted"),
        "source_support": count("support"),
        "source_refused": count("refused"),
        "source_inactive": count("inactive"),
        "source_refuted": count("refuted"),
        "unclassified_source": count("unclassified"),
    })
}

fn lift_options_from_config(workspace_root: &Path, params: &Value) -> Result<LiftOptions, String> {
    let config_rel = params
        .get("config_path")
        .and_then(Value::as_str)
        .unwrap_or(".sugar/config.toml");
    let config_path = workspace_root.join(config_rel);
    match std::fs::read_to_string(&config_path) {
        Ok(text) => target_cfg_from_config_text(&text).map(|cfg| match cfg {
            Some(cfg) => LiftOptions::for_target_cfg(cfg),
            None => LiftOptions::default(),
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(LiftOptions::default()),
        Err(e) => Err(format!("cannot read {}: {e}", config_path.display())),
    }
}

fn target_cfg_from_config_text(text: &str) -> Result<Option<TargetCfg>, String> {
    let doc: toml::Value =
        toml::from_str(text).map_err(|e| format!("invalid TOML in cfg config: {e}"))?;
    let Some(surface) = doc.get("rust-test-assertions") else {
        return Ok(None);
    };
    let Some(section) = surface.get("target_cfg") else {
        return Ok(None);
    };
    let target = section
        .get("target")
        .and_then(toml::Value::as_str)
        .unwrap_or("")
        .trim();
    if target.is_empty() {
        return Err(
            "[rust-test-assertions.target_cfg] requires target = \"<pinned target>\"".to_string(),
        );
    }
    let Some(facts) = section.get("facts").and_then(toml::Value::as_array) else {
        return Err(
            "[rust-test-assertions.target_cfg] requires facts = [rustc --print cfg lines]"
                .to_string(),
        );
    };
    if facts.is_empty() {
        return Err("[rust-test-assertions.target_cfg].facts must not be empty".to_string());
    }
    let mut parsed = Vec::with_capacity(facts.len());
    for fact in facts {
        let Some(fact) = fact.as_str() else {
            return Err(
                "[rust-test-assertions.target_cfg].facts entries must be strings".to_string(),
            );
        };
        parsed.push(fact);
    }
    TargetCfg::from_rustc_cfg_facts(parsed)
        .map(Some)
        .map_err(|e| format!("invalid rust-test-assertions target cfg facts: {e}"))
}

const IGNORED_DIRS: &[&str] = &[
    "target",
    ".git",
    "node_modules",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    ".venv",
    "venv",
];

fn enumerate_rs_files(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            !(entry.file_type().is_dir() && IGNORED_DIRS.contains(&name.as_ref()))
        })
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
            if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    out.sort();
    out
}

fn send(obj: &Value) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{}", serde_json::to_string(obj).unwrap_or_default());
    let _ = out.flush();
}

fn err_reply(id: &Value, msg: String) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32603, "message": msg}})
}

fn handle(id: &Value, method: &str, params: &Value) -> Value {
    match method {
        "initialize" => json!({"jsonrpc": "2.0", "id": id, "result": initialize_result()}),
        KIT_DECLARATION_RPC_METHOD => {
            json!({"jsonrpc": "2.0", "id": id, "result": kit_declaration_result()})
        }
        "lift" => json!({"jsonrpc": "2.0", "id": id, "result": lift(params)}),
        "shutdown" => json!({"jsonrpc": "2.0", "id": id, "result": Value::Null}),
        other => err_reply(id, format!("unknown method: {other}")),
    }
}

fn main() {
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let req: Value = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                send(
                    &json!({"jsonrpc": "2.0", "id": Value::Null, "error": {"code": -32700, "message": format!("parse error: {e}")}}),
                );
                continue;
            }
        };
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        let method = req.get("method").and_then(Value::as_str).unwrap_or("");
        let params = req.get("params").cloned().unwrap_or(Value::Null);
        let reply = handle(&id, method, &params);
        send(&reply);
        if method == "shutdown" {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fns_of(src: &str) -> Vec<FnRef<'_>> {
        // leak so the FnRefs can borrow for the test's lifetime
        let file: &'static syn::File = Box::leak(Box::new(syn::parse_file(src).expect("parses")));
        let mut out = Vec::new();
        collect_fns(&file.items, &mut out);
        out
    }

    #[test]
    fn collect_fns_enumerates_impl_and_trait_default_methods() {
        // free fn + inherent impl method + trait default method must all appear:
        // excluding impl/trait methods is what made unclassified=0 hollow.
        let src = r#"
            fn free_fn() -> i32 { 1 }
            struct S;
            impl S { fn method(&self) -> i32 { 2 } }
            trait T { fn defaulted(&self) -> i32 { 3 } fn decl_only(&self) -> i32; }
        "#;
        let names: Vec<String> = fns_of(src).iter().map(|f| f.name.clone()).collect();
        assert!(names.contains(&"free_fn".to_string()));
        assert!(names.contains(&"method".to_string()), "impl method must be enumerated: {names:?}");
        assert!(names.contains(&"defaulted".to_string()), "trait default method must be enumerated: {names:?}");
        // a trait method WITHOUT a body declares no constructor -> not a locus.
        assert!(!names.contains(&"decl_only".to_string()), "bodyless trait decl is not a locus: {names:?}");
    }

    fn block_of(src: &str) -> syn::Block {
        let f: syn::ItemFn = syn::parse_str(src).expect("fn parses");
        *f.block
    }

    #[test]
    fn generalizable_warrants_pure_value_bodies() {
        for src in [
            "fn f() -> i32 { 6 }",
            "fn f(x: i32) -> i32 { let y = x * 2; y + 1 }",
            "fn f(c: bool, a: i32, b: i32) -> i32 { if c { a } else { b } }",
            "fn f(x: i32) -> i32 { match x { 0 => 1, _ => 2 } }",
            "fn f(a: i32, b: i32) -> (i32, i32) { (a, b) }",
        ] {
            assert!(
                is_generalizable_value_fn(&block_of(src)),
                "should warrant pure-value body: {src}"
            );
        }
    }

    #[test]
    fn generalizable_refuses_effectful_bodies() {
        for src in [
            "fn f(v: Vec<i32>) -> usize { v.len() }",      // method call
            "fn f() { for _ in 0..3 {} }",                  // loop
            "fn f(r: Result<i32, ()>) -> i32 { r.unwrap() }", // method call
            "fn f() -> i32 { let mut x = 0; x = 1; x }",    // mutation (non-immutable let / assign)
        ] {
            assert!(
                !is_generalizable_value_fn(&block_of(src)),
                "should NOT warrant effectful/opaque body: {src}"
            );
        }
    }

    #[test]
    fn refusal_reason_categorizes_and_splits_bin2() {
        let io = refusal_reason(&block_of("fn f() { println!(\"x\"); }"));
        assert!(io.contains("IO membrane (bin-2)"), "print is the IO floor: {io}");
        let lp = refusal_reason(&block_of("fn f() { for _ in 0..3 {} }"));
        assert!(lp.contains("loop") && lp.contains("bin-1"), "loop is drainable bin-1: {lp}");
        let mc = refusal_reason(&block_of("fn f(v: Vec<i32>) -> usize { v.len() }"));
        assert!(mc.contains("method call") && mc.contains("bin-1"), "method call is bin-1: {mc}");
    }

    #[test]
    fn target_cfg_config_is_optional() {
        let cfg = target_cfg_from_config_text(
            r#"
[[plugins]]
name = "rust-test-assertions-lift"
"#,
        )
        .expect("config parses");

        assert!(cfg.is_none());
    }

    #[test]
    fn target_cfg_config_requires_pinned_target() {
        let err = target_cfg_from_config_text(
            r#"
[rust-test-assertions.target_cfg]
facts = ["unix"]
"#,
        )
        .expect_err("target is required");

        assert!(err.contains("requires target"));
    }

    #[test]
    fn target_cfg_config_parses_explicit_rustc_facts() {
        let cfg = target_cfg_from_config_text(
            r#"
[rust-test-assertions.target_cfg]
target = "x86_64-apple-darwin"
facts = [
  "target_pointer_width=\"64\"",
  "unix",
]
"#,
        )
        .expect("config parses");

        assert!(cfg.is_some());
    }
}
