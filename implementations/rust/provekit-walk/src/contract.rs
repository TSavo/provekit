// SPDX-License-Identifier: Apache-2.0
//
// FunctionContractMemento: every function's externally-visible
// behavior collapsed to a content-addressed memento. Per #376.
//
// Each function in the lifted source emits ONE FunctionContractMemento.
// Pure functions' contracts compose by hash combination, collapsing
// arbitrarily-deep call subtrees into single CIDs (paper 07 §6's
// "compose for free, compress to nothing"). Impure functions also
// emit contract mementos for cross-program lookup but cannot compose.
//
// Per Sir's design constraints:
//   1. Singular formal substitution: each formal arrival is its own
//      composition. f(a, g(b), c) yields three arrivals; only the
//      g-substitution composes f's contract with g's contract.
//   2. CID-namespaced result variable: each contract's post uses
//      `result` locally, but composition renames inner.result to
//      result_<inner.cid> before substituting into outer.pre, so
//      different functions' results never collide.
//   3. Effect-set from the start, not a one-bit pure marker. Pure is
//      the empty set. Adding effect variants over time is content-
//      addressing-safe (existing CIDs unchanged).
//   4. Every function emits; composition refuses non-pure.
//
// Schema (canonical bytes, JCS-encoded):
//
//   FunctionContractMemento:
//   {
//     "schemaVersion": "1",
//     "kind": "function-contract",
//     "fnName": <string>,
//     "formals": [<string>, ...],          // parameter names
//     "formalSorts": [<Sort>, ...],         // per-formal sort
//     "returnSort": <Sort>,
//     "pre": <IrFormula>,
//     "post": <IrFormula>,                  // references `result`
//     "bodyCid": <CID or null>,             // shadow-source CID of body
//     "effects": [<Effect>, ...]            // empty = pure
//   }
//
//   Effect:
//   { "kind": "reads", "target": <string> }
//   { "kind": "writes", "target": <string> }
//   { "kind": "io" }
//   { "kind": "unsafe" }
//   { "kind": "panics" }
//   { "kind": "unresolved_call", "name": <string> }

use std::sync::Arc;

use provekit_canonicalizer::Value;
use provekit_ir_types::{IrFormula, IrTerm, Sort};
use syn::{Expr, ExprUnsafe, FnArg, ItemFn, Pat, Stmt};

use crate::canonical::{cid_of_value, formula_to_canonical, jcs_bytes_of_value};
use crate::lift::{lift_function_postcondition, lift_function_precondition};
use crate::wp::{substitute_in_formula, Wp};

// ---- Effect set ----

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Reads a named state cell (global, capture, mut binding).
    Reads { target: String },
    /// Writes a named state cell.
    Writes { target: String },
    /// Performs IO (println!, file I/O, network, syscall).
    Io,
    /// Contains an unsafe block.
    Unsafe,
    /// May panic on inputs satisfying the precondition.
    Panics,
    /// Calls a function whose effect-set is unknown to the lifter.
    UnresolvedCall { name: String },
}

impl Effect {
    fn to_value(&self) -> Arc<Value> {
        match self {
            Effect::Reads { target } => Value::object([
                ("kind", Value::string("reads")),
                ("target", Value::string(target.clone())),
            ]),
            Effect::Writes { target } => Value::object([
                ("kind", Value::string("writes")),
                ("target", Value::string(target.clone())),
            ]),
            Effect::Io => Value::object([("kind", Value::string("io"))]),
            Effect::Unsafe => Value::object([("kind", Value::string("unsafe"))]),
            Effect::Panics => Value::object([("kind", Value::string("panics"))]),
            Effect::UnresolvedCall { name } => Value::object([
                ("kind", Value::string("unresolved_call")),
                ("name", Value::string(name.clone())),
            ]),
        }
    }

    fn sort_key(&self) -> String {
        // Stable string for sorting effects in the canonical encoding.
        match self {
            Effect::Reads { target } => format!("0:reads:{}", target),
            Effect::Writes { target } => format!("1:writes:{}", target),
            Effect::Io => "2:io".to_string(),
            Effect::Unsafe => "3:unsafe".to_string(),
            Effect::Panics => "4:panics".to_string(),
            Effect::UnresolvedCall { name } => format!("5:unresolved:{}", name),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EffectSet {
    pub effects: Vec<Effect>,
}

impl EffectSet {
    pub fn empty() -> Self {
        Self { effects: vec![] }
    }
    pub fn is_pure(&self) -> bool {
        self.effects.is_empty()
    }
    pub fn add(&mut self, e: Effect) {
        if !self.effects.iter().any(|x| x == &e) {
            self.effects.push(e);
        }
    }
    fn to_value(&self) -> Arc<Value> {
        let mut sorted = self.effects.clone();
        sorted.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));
        let items: Vec<Arc<Value>> = sorted.iter().map(|e| e.to_value()).collect();
        Value::array(items)
    }
}

// ---- Contract memento ----

#[derive(Debug, Clone)]
pub struct FunctionContractMemento {
    pub fn_name: String,
    pub formals: Vec<String>,
    pub formal_sorts: Vec<Sort>,
    pub return_sort: Sort,
    pub pre: IrFormula,
    pub post: IrFormula,
    pub body_cid: Option<String>,
    pub effects: EffectSet,
    pub canonical_bytes: Vec<u8>,
    pub cid: String,
}

impl FunctionContractMemento {
    pub fn is_pure(&self) -> bool {
        self.effects.is_pure()
    }

    /// Extract the term-side of `result = <expr>` from the post, used
    /// when composing this contract's result into another contract's
    /// pre/post. Returns None if the post doesn't have a recognizable
    /// `result = ...` equation.
    pub fn result_value(&self) -> Option<IrTerm> {
        find_result_equation(&self.post, "result")
    }

    /// Short tag used to namespace this contract's `result` variable
    /// when composing. Uses the CID's hex tail.
    pub fn result_var_name(&self) -> String {
        let tail: String = self
            .cid
            .chars()
            .rev()
            .take(12)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        format!("result__{}", tail)
    }
}

/// Build a FunctionContractMemento for an `ItemFn`. The body_cid is
/// optional — pass None when the body's shadow source isn't computed
/// (e.g., during a lift-only pass).
pub fn build_function_contract(
    item_fn: &ItemFn,
    body_cid: Option<String>,
) -> FunctionContractMemento {
    let fn_name = item_fn.sig.ident.to_string();
    let (formals, formal_sorts) = extract_formals(item_fn);
    let return_sort = extract_return_sort(item_fn);
    let pre = lift_function_precondition(item_fn).into_formula();
    let post = lift_function_postcondition(item_fn).into_formula();
    let effects = detect_effects(item_fn);

    let value = build_value(
        &fn_name,
        &formals,
        &formal_sorts,
        &return_sort,
        &pre,
        &post,
        body_cid.as_deref(),
        &effects,
    );
    let canonical_bytes = jcs_bytes_of_value(&value);
    let cid = cid_of_value(&value);

    FunctionContractMemento {
        fn_name,
        formals,
        formal_sorts,
        return_sort,
        pre,
        post,
        body_cid,
        effects,
        canonical_bytes,
        cid,
    }
}

fn build_value(
    fn_name: &str,
    formals: &[String],
    formal_sorts: &[Sort],
    return_sort: &Sort,
    pre: &IrFormula,
    post: &IrFormula,
    body_cid: Option<&str>,
    effects: &EffectSet,
) -> Arc<Value> {
    let formals_arr: Vec<Arc<Value>> = formals.iter().map(|n| Value::string(n.clone())).collect();
    let formal_sorts_arr: Vec<Arc<Value>> =
        formal_sorts.iter().map(|s| sort_to_value(s)).collect();
    let body_cid_val: Arc<Value> = match body_cid {
        Some(c) => Value::string(c.to_string()),
        None => Value::null(),
    };
    Value::object([
        ("schemaVersion", Value::string("1")),
        ("kind", Value::string("function-contract")),
        ("fnName", Value::string(fn_name.to_string())),
        ("formals", Value::array(formals_arr)),
        ("formalSorts", Value::array(formal_sorts_arr)),
        ("returnSort", sort_to_value(return_sort)),
        ("pre", formula_to_canonical(pre)),
        ("post", formula_to_canonical(post)),
        ("bodyCid", body_cid_val),
        ("effects", effects.to_value()),
    ])
}

fn sort_to_value(s: &Sort) -> Arc<Value> {
    match s {
        Sort::Primitive { name } => Value::object([
            ("kind", Value::string("primitive")),
            ("name", Value::string(name.clone())),
        ]),
    }
}

// ---- Composition ----

#[derive(Debug, Clone)]
pub struct ComposedFunctionContract {
    pub component_cids: Vec<String>,
    pub formal_idx: usize,
    pub pre: IrFormula,
    pub post: IrFormula,
    pub canonical_bytes: Vec<u8>,
    pub cid: String,
}

/// Compose two function contracts: the inner contract's result feeds
/// the outer contract's `formal_idx`-th formal.
///
/// Refuses (returns None) if either contract is impure.
pub fn compose_function_contracts(
    outer: &FunctionContractMemento,
    inner: &FunctionContractMemento,
    formal_idx: usize,
) -> Option<ComposedFunctionContract> {
    if !outer.is_pure() || !inner.is_pure() {
        return None;
    }
    if formal_idx >= outer.formals.len() {
        return None;
    }

    // Step 1: rename inner's `result` to namespaced form (CID-prefixed).
    let inner_result_name = inner.result_var_name();
    let inner_post_renamed = substitute_in_formula(
        inner.post.clone(),
        "result",
        &IrTerm::Var {
            name: inner_result_name.clone(),
        },
    );

    // Step 2: extract the term equated with the renamed result.
    let inner_value = match find_result_equation(&inner_post_renamed, &inner_result_name) {
        Some(t) => t,
        None => return None,
    };

    // Step 3: substitute outer's formal with the inner's result-value.
    let outer_formal = &outer.formals[formal_idx];
    let outer_pre_substituted =
        substitute_in_formula(outer.pre.clone(), outer_formal, &inner_value);
    let outer_post_substituted =
        substitute_in_formula(outer.post.clone(), outer_formal, &inner_value);

    // Step 4: compose.
    //   pre  = inner.pre ∧ (inner.post → outer.pre[inner_value/formal])
    //   post = outer.post[inner_value/formal]
    let pre = IrFormula::And {
        operands: vec![
            inner.pre.clone(),
            IrFormula::Implies {
                operands: vec![inner_post_renamed.clone(), outer_pre_substituted],
            },
        ],
    };
    let post = outer_post_substituted;

    // Step 5: content-address.
    let value = Value::object([
        ("schemaVersion", Value::string("1")),
        ("kind", Value::string("composed-function-contract")),
        (
            "components",
            Value::array(vec![
                Value::string(outer.cid.clone()),
                Value::string(inner.cid.clone()),
            ]),
        ),
        ("formalIdx", Value::integer(formal_idx as i64)),
        ("pre", formula_to_canonical(&pre)),
        ("post", formula_to_canonical(&post)),
    ]);
    let canonical_bytes = jcs_bytes_of_value(&value);
    let cid = cid_of_value(&value);

    Some(ComposedFunctionContract {
        component_cids: vec![outer.cid.clone(), inner.cid.clone()],
        formal_idx,
        pre,
        post,
        canonical_bytes,
        cid,
    })
}

/// Compose with the inner being an already-composed contract. Used
/// during chain folding so each step keeps composing without losing
/// the previous composition's CID.
pub fn compose_with_composed(
    outer: &FunctionContractMemento,
    inner: &ComposedFunctionContract,
    formal_idx: usize,
) -> Option<ComposedFunctionContract> {
    if !outer.is_pure() {
        return None;
    }
    if formal_idx >= outer.formals.len() {
        return None;
    }

    // The inner ComposedFunctionContract's post still has the result-
    // equation form (because outer's post was substituted into it
    // during the prior compose step). Extract it directly without
    // renaming — the inner's CID-namespaced result name is preserved.
    let inner_value = find_result_equation(&inner.post, "result").or_else(|| {
        // Fallback: scan for any result__-prefixed equation.
        find_namespaced_result(&inner.post)
    })?;

    let outer_formal = &outer.formals[formal_idx];
    let outer_pre_substituted =
        substitute_in_formula(outer.pre.clone(), outer_formal, &inner_value);
    let outer_post_substituted =
        substitute_in_formula(outer.post.clone(), outer_formal, &inner_value);

    let pre = IrFormula::And {
        operands: vec![
            inner.pre.clone(),
            IrFormula::Implies {
                operands: vec![inner.post.clone(), outer_pre_substituted],
            },
        ],
    };
    let post = outer_post_substituted;

    let mut components = vec![outer.cid.clone()];
    components.extend(inner.component_cids.iter().cloned());
    let component_values: Vec<Arc<Value>> =
        components.iter().map(|c| Value::string(c.clone())).collect();
    let value = Value::object([
        ("schemaVersion", Value::string("1")),
        ("kind", Value::string("composed-function-contract")),
        ("components", Value::array(component_values)),
        ("formalIdx", Value::integer(formal_idx as i64)),
        ("pre", formula_to_canonical(&pre)),
        ("post", formula_to_canonical(&post)),
    ]);
    let canonical_bytes = jcs_bytes_of_value(&value);
    let cid = cid_of_value(&value);

    Some(ComposedFunctionContract {
        component_cids: components,
        formal_idx,
        pre,
        post,
        canonical_bytes,
        cid,
    })
}

/// One step in an N-deep chain composition. The contract receives the
/// previous step's result at `formal_idx`. The first step's formal_idx
/// is unused (it's the chain's source).
#[derive(Debug, Clone, Copy)]
pub struct ChainStep<'a> {
    pub contract: &'a FunctionContractMemento,
    pub formal_idx: usize,
}

/// Compose a chain of pure function contracts left-to-right. Each
/// step's contract receives the previous step's result at its
/// `formal_idx`-th formal. Returns None if any contract is impure or
/// the chain is shorter than 2 steps.
///
/// The chain's overall CID is derivable from its component CIDs in
/// order — re-composing the same chain produces the same CID
/// byte-for-byte.
pub fn compose_chain_contracts(steps: &[ChainStep<'_>]) -> Option<ComposedFunctionContract> {
    if steps.len() < 2 {
        return None;
    }
    let mut acc = compose_function_contracts(
        steps[1].contract,
        steps[0].contract,
        steps[1].formal_idx,
    )?;
    for step in &steps[2..] {
        acc = compose_with_composed(step.contract, &acc, step.formal_idx)?;
    }
    Some(acc)
}

fn find_namespaced_result(formula: &IrFormula) -> Option<IrTerm> {
    match formula {
        IrFormula::Atomic { name, args } if name == "=" && args.len() == 2 => {
            for (var_arg, value_arg) in [(&args[0], &args[1]), (&args[1], &args[0])] {
                if let IrTerm::Var { name: n } = var_arg {
                    if n.starts_with("result__") {
                        return Some(value_arg.clone());
                    }
                }
            }
            None
        }
        IrFormula::And { operands } => operands.iter().find_map(find_namespaced_result),
        _ => None,
    }
}

// ---- Effect detection ----

fn detect_effects(item_fn: &ItemFn) -> EffectSet {
    let mut set = EffectSet::empty();
    // 1. &mut params
    for input in &item_fn.sig.inputs {
        if let FnArg::Typed(pt) = input {
            if let syn::Type::Reference(r) = &*pt.ty {
                if r.mutability.is_some() {
                    if let Pat::Ident(ident) = &*pt.pat {
                        set.add(Effect::Writes {
                            target: ident.ident.to_string(),
                        });
                    } else {
                        set.add(Effect::Writes {
                            target: "<param>".to_string(),
                        });
                    }
                }
            }
        }
    }
    // 2. Walk the body for unsafe, IO, panics, unknown calls.
    for stmt in &item_fn.block.stmts {
        scan_stmt_for_effects(stmt, &mut set);
    }
    set
}

fn scan_stmt_for_effects(stmt: &Stmt, set: &mut EffectSet) {
    match stmt {
        Stmt::Local(local) => {
            if let Some(init) = &local.init {
                scan_expr_for_effects(&init.expr, set);
            }
        }
        Stmt::Expr(e, _) => scan_expr_for_effects(e, set),
        Stmt::Macro(m) => scan_macro_for_effects(&m.mac, set),
        Stmt::Item(_) => {}
    }
}

fn scan_expr_for_effects(expr: &Expr, set: &mut EffectSet) {
    match expr {
        Expr::Unsafe(ExprUnsafe { block, .. }) => {
            set.add(Effect::Unsafe);
            for s in &block.stmts {
                scan_stmt_for_effects(s, set);
            }
        }
        Expr::Macro(m) => scan_macro_for_effects(&m.mac, set),
        Expr::Call(c) => {
            // Direct callsite — we don't know the callee's effects without
            // a substrate lookup, so mark unresolved.
            if let Expr::Path(p) = c.func.as_ref() {
                if let Some(seg) = p.path.segments.last() {
                    let name = seg.ident.to_string();
                    if !is_known_pure_call(&name) {
                        set.add(Effect::UnresolvedCall { name });
                    }
                }
            }
            for a in &c.args {
                scan_expr_for_effects(a, set);
            }
        }
        Expr::MethodCall(m) => {
            let method = m.method.to_string();
            if is_io_method(&method) {
                set.add(Effect::Io);
            } else if !is_known_pure_method(&method) {
                set.add(Effect::UnresolvedCall {
                    name: format!(".{}", method),
                });
            }
            scan_expr_for_effects(&m.receiver, set);
            for a in &m.args {
                scan_expr_for_effects(a, set);
            }
        }
        Expr::If(i) => {
            scan_expr_for_effects(&i.cond, set);
            for s in &i.then_branch.stmts {
                scan_stmt_for_effects(s, set);
            }
            if let Some((_, e)) = &i.else_branch {
                scan_expr_for_effects(e, set);
            }
        }
        Expr::Block(b) => {
            for s in &b.block.stmts {
                scan_stmt_for_effects(s, set);
            }
        }
        Expr::While(w) => {
            scan_expr_for_effects(&w.cond, set);
            for s in &w.body.stmts {
                scan_stmt_for_effects(s, set);
            }
        }
        Expr::ForLoop(f) => {
            scan_expr_for_effects(&f.expr, set);
            for s in &f.body.stmts {
                scan_stmt_for_effects(s, set);
            }
        }
        Expr::Loop(l) => {
            for s in &l.body.stmts {
                scan_stmt_for_effects(s, set);
            }
        }
        Expr::Match(m) => {
            scan_expr_for_effects(&m.expr, set);
            for arm in &m.arms {
                scan_expr_for_effects(&arm.body, set);
            }
        }
        Expr::Assign(a) => {
            // x = expr → writes x.
            if let Expr::Path(p) = a.left.as_ref() {
                if let Some(seg) = p.path.segments.last() {
                    set.add(Effect::Writes {
                        target: seg.ident.to_string(),
                    });
                }
            }
            scan_expr_for_effects(&a.right, set);
        }
        Expr::Return(r) => {
            if let Some(inner) = &r.expr {
                scan_expr_for_effects(inner, set);
            }
        }
        Expr::Try(t) => scan_expr_for_effects(&t.expr, set),
        Expr::Binary(b) => {
            scan_expr_for_effects(&b.left, set);
            scan_expr_for_effects(&b.right, set);
        }
        Expr::Unary(u) => scan_expr_for_effects(&u.expr, set),
        Expr::Paren(p) => scan_expr_for_effects(&p.expr, set),
        Expr::Reference(r) => scan_expr_for_effects(&r.expr, set),
        Expr::Field(f) => scan_expr_for_effects(&f.base, set),
        Expr::Index(i) => {
            scan_expr_for_effects(&i.expr, set);
            scan_expr_for_effects(&i.index, set);
        }
        Expr::Tuple(t) => {
            for e in &t.elems {
                scan_expr_for_effects(e, set);
            }
        }
        Expr::Array(a) => {
            for e in &a.elems {
                scan_expr_for_effects(e, set);
            }
        }
        // Pure expression shapes — no effects to add.
        Expr::Lit(_) | Expr::Path(_) | Expr::Closure(_) => {}
        _ => {}
    }
}

fn scan_macro_for_effects(mac: &syn::Macro, set: &mut EffectSet) {
    let name = mac
        .path
        .segments
        .last()
        .map(|s| s.ident.to_string())
        .unwrap_or_default();
    match name.as_str() {
        // Panic-shaped macros.
        "panic" | "unreachable" | "todo" | "unimplemented" => set.add(Effect::Panics),
        // IO macros.
        "println" | "print" | "eprintln" | "eprint" | "dbg" => set.add(Effect::Io),
        // Pure (compile-time) macros.
        "assert" | "debug_assert" | "assert_eq" | "assert_ne" => {}
        "vec" | "format" | "concat" | "stringify" => {}
        // Unknown — conservative mark unresolved.
        _ => set.add(Effect::UnresolvedCall {
            name: format!("{}!", name),
        }),
    }
}

fn is_io_method(name: &str) -> bool {
    matches!(
        name,
        "write"
            | "write_all"
            | "read"
            | "read_to_string"
            | "read_to_end"
            | "send"
            | "recv"
            | "lock"
            | "unlock"
            | "flush"
            | "open"
            | "close"
    )
}

fn is_known_pure_method(name: &str) -> bool {
    matches!(
        name,
        "len"
            | "is_empty"
            | "iter"
            | "into_iter"
            | "map"
            | "filter"
            | "fold"
            | "sum"
            | "product"
            | "count"
            | "min"
            | "max"
            | "clone"
            | "as_str"
            | "as_ref"
            | "as_slice"
            | "to_string"
            | "to_owned"
            | "abs"
            | "saturating_add"
            | "saturating_sub"
            | "checked_add"
            | "checked_sub"
            | "wrapping_add"
            | "wrapping_sub"
            | "trailing_zeros"
            | "leading_zeros"
    )
}

fn is_known_pure_call(name: &str) -> bool {
    // Free functions known pure. Conservative; grow over time.
    matches!(name, "min" | "max" | "abs")
}

// ---- helpers ----

fn extract_formals(item_fn: &ItemFn) -> (Vec<String>, Vec<Sort>) {
    let mut names = Vec::new();
    let mut sorts = Vec::new();
    for input in &item_fn.sig.inputs {
        if let FnArg::Typed(pt) = input {
            let name = match &*pt.pat {
                Pat::Ident(p) => p.ident.to_string(),
                _ => "<arg>".to_string(),
            };
            names.push(name);
            sorts.push(infer_sort(&pt.ty));
        }
    }
    (names, sorts)
}

fn extract_return_sort(item_fn: &ItemFn) -> Sort {
    match &item_fn.sig.output {
        syn::ReturnType::Default => Sort::Primitive {
            name: "Unit".to_string(),
        },
        syn::ReturnType::Type(_, ty) => infer_sort(ty),
    }
}

fn infer_sort(ty: &syn::Type) -> Sort {
    use quote::ToTokens;
    let s = ty.to_token_stream().to_string();
    let name = match s.trim() {
        "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64"
        | "u128" | "usize" => "Int",
        "bool" => "Bool",
        "f32" | "f64" => "Real",
        "String" | "& str" | "&str" => "String",
        _ => "Int", // Conservative default; richer Sort inference is its own issue.
    };
    Sort::Primitive {
        name: name.to_string(),
    }
}

/// Find a top-level `<var> = <expr>` equation in a formula and return
/// the expr term. Used to extract a function's result-value from its
/// post for composition.
fn find_result_equation(formula: &IrFormula, var_name: &str) -> Option<IrTerm> {
    match formula {
        IrFormula::Atomic { name, args } if name == "=" && args.len() == 2 => {
            // Recognize either `var = expr` or `expr = var`.
            if let IrTerm::Var { name: n } = &args[0] {
                if n == var_name {
                    return Some(args[1].clone());
                }
            }
            if let IrTerm::Var { name: n } = &args[1] {
                if n == var_name {
                    return Some(args[0].clone());
                }
            }
            None
        }
        IrFormula::And { operands } => operands
            .iter()
            .find_map(|f| find_result_equation(f, var_name)),
        _ => None,
    }
}

// Suppress unused-import warning in dev builds.
#[allow(dead_code)]
fn _unused_wp_path(_w: &Wp) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wp::{atomic_ge, const_int, var};

    fn parse_fn(src: &str) -> ItemFn {
        let file: syn::File = syn::parse_str(src).unwrap();
        file.items
            .into_iter()
            .find_map(|item| match item {
                syn::Item::Fn(f) => Some(f),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn pure_function_has_empty_effects() {
        let item_fn = parse_fn(
            r#"
            fn double(x: u32) -> u32 {
                x * 2
            }
        "#,
        );
        let contract = build_function_contract(&item_fn, None);
        assert!(contract.is_pure(), "double should be pure: {:?}", contract.effects);
        assert_eq!(contract.fn_name, "double");
        assert_eq!(contract.formals, vec!["x".to_string()]);
    }

    #[test]
    fn function_with_panic_marks_panics_effect() {
        let item_fn = parse_fn(
            r#"
            fn must_be_ten(x: u32) -> u32 {
                if x != 10 { panic!(); }
                x
            }
        "#,
        );
        let contract = build_function_contract(&item_fn, None);
        assert!(contract.effects.effects.contains(&Effect::Panics));
    }

    #[test]
    fn function_with_println_marks_io_effect() {
        let item_fn = parse_fn(
            r#"
            fn loud(x: u32) -> u32 {
                println!("got {}", x);
                x + 1
            }
        "#,
        );
        let contract = build_function_contract(&item_fn, None);
        assert!(contract.effects.effects.contains(&Effect::Io));
        assert!(!contract.is_pure());
    }

    #[test]
    fn function_with_unsafe_marks_unsafe_effect() {
        let item_fn = parse_fn(
            r#"
            fn raw_add(x: *const u32, y: u32) -> u32 {
                unsafe { *x + y }
            }
        "#,
        );
        let contract = build_function_contract(&item_fn, None);
        assert!(contract.effects.effects.contains(&Effect::Unsafe));
    }

    #[test]
    fn function_with_mut_ref_param_marks_writes_effect() {
        let item_fn = parse_fn(
            r#"
            fn increment(buf: &mut u32) {
                *buf = *buf + 1;
            }
        "#,
        );
        let contract = build_function_contract(&item_fn, None);
        let writes = contract
            .effects
            .effects
            .iter()
            .any(|e| matches!(e, Effect::Writes { .. }));
        assert!(writes);
    }

    #[test]
    fn contract_cid_is_deterministic_across_runs() {
        let item_fn = parse_fn(
            r#"
            fn double(x: u32) -> u32 {
                x * 2
            }
        "#,
        );
        let a = build_function_contract(&item_fn, None);
        let b = build_function_contract(&item_fn, None);
        assert_eq!(a.cid, b.cid);
        assert_eq!(a.canonical_bytes, b.canonical_bytes);
        assert!(a.cid.starts_with("blake3-512:"));
    }

    #[test]
    fn distinct_functions_have_distinct_cids() {
        let f1 = parse_fn(r#"fn double(x: u32) -> u32 { x * 2 }"#);
        let f2 = parse_fn(r#"fn triple(x: u32) -> u32 { x * 3 }"#);
        let c1 = build_function_contract(&f1, None);
        let c2 = build_function_contract(&f2, None);
        assert_ne!(c1.cid, c2.cid);
    }

    #[test]
    fn compose_two_pure_contracts_succeeds() {
        // outer: f(x) = x * 2, post says result = x * 2
        // inner: g(y) = y + 1, post says result = y + 1
        // composition f(g(y)): pre = ∅, post = result = (y + 1) * 2
        let f = build_function_contract(&parse_fn(r#"fn f(x: u32) -> u32 { x * 2 }"#), None);
        let g = build_function_contract(&parse_fn(r#"fn g(y: u32) -> u32 { y + 1 }"#), None);
        let composed = compose_function_contracts(&f, &g, 0).expect("compose succeeds");
        assert!(composed.cid.starts_with("blake3-512:"));
        // Re-composing yields same CID.
        let composed2 = compose_function_contracts(&f, &g, 0).unwrap();
        assert_eq!(composed.cid, composed2.cid);
    }

    #[test]
    fn compose_refuses_impure_contract() {
        let pure_f = build_function_contract(&parse_fn(r#"fn f(x: u32) -> u32 { x * 2 }"#), None);
        let impure_g = build_function_contract(
            &parse_fn(
                r#"
                fn g(y: u32) -> u32 {
                    println!("{}", y);
                    y + 1
                }
            "#,
            ),
            None,
        );
        assert!(compose_function_contracts(&pure_f, &impure_g, 0).is_none());
        assert!(compose_function_contracts(&impure_g, &pure_f, 0).is_none());
    }

    #[test]
    fn compose_refuses_out_of_bounds_formal_idx() {
        let f = build_function_contract(&parse_fn(r#"fn f(x: u32) -> u32 { x * 2 }"#), None);
        let g = build_function_contract(&parse_fn(r#"fn g(y: u32) -> u32 { y + 1 }"#), None);
        assert!(compose_function_contracts(&f, &g, 5).is_none());
    }

    #[test]
    fn result_var_name_is_cid_namespaced() {
        let f = build_function_contract(&parse_fn(r#"fn f(x: u32) -> u32 { x * 2 }"#), None);
        let name = f.result_var_name();
        assert!(name.starts_with("result__"));
        // Different functions get different namespaces.
        let g = build_function_contract(&parse_fn(r#"fn g(y: u32) -> u32 { y * 3 }"#), None);
        assert_ne!(f.result_var_name(), g.result_var_name());
    }

    #[test]
    fn effects_stable_across_runs() {
        let f = parse_fn(
            r#"
            fn loud(x: u32) -> u32 {
                println!("hi");
                if x < 10 { panic!(); }
                x + 1
            }
        "#,
        );
        let a = build_function_contract(&f, None);
        let b = build_function_contract(&f, None);
        assert_eq!(a.effects, b.effects);
        assert_eq!(a.cid, b.cid);
    }

    // Suppress unused-import warnings.
    #[test]
    fn _unused_helpers() {
        let _ = atomic_ge(var("x"), const_int(1));
    }
}
