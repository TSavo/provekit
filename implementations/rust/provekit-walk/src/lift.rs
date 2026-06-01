// SPDX-License-Identifier: Apache-2.0
//
// lift.rs — build predicates from Rust source.
//
// Recognizes the patterns that paper 07 reads as "every if-statement is
// a contract": `if cond { panic!() }`, `assert!(cond)`, `debug_assert!(cond)`.
// Each such pattern contributes a leaf precondition the function's body
// is implicitly demanding from its caller.
//
// MVP scope:
//   - if-then-panic: `if cond { panic!(...) }` → ¬cond holds for the
//     non-panic continuation, so the function's effective precondition
//     accumulates ¬cond.
//   - assert! family: `assert!(cond)` → cond holds afterward (and the
//     caller must have established cond up to here).
//   - Binary comparisons `<`, `≤`, `>`, `≥`, `==`, `!=` lift to the
//     corresponding `AtomicPredicateName`.
//   - Compound `&&` lifts to `IrFormula::And`; `||` to `IrFormula::Or`;
//     `!cond` to `IrFormula::Not`.
//
// Out of scope (later commits on #368):
//   - if-then-else with non-panic else (introduces conditional
//     strengthening, not a flat precondition).
//   - match arms.
//   - early-return patterns beyond `panic!`.
//   - postcondition lifting from `return` expressions.

use std::collections::{BTreeMap, HashSet};

use libprovekit::concept::panic_freedom;
use proc_macro2::{Delimiter, TokenStream, TokenTree};
use provekit_ir_types::{IrFormula, IrTerm};
use syn::{
    BinOp, Expr, ExprBinary, ExprIf, ExprMacro, ExprUnary, FnArg, GenericArgument, ItemFn, Lit,
    Local, Macro, Pat, PathArguments, ReturnType, Stmt, StmtMacro, Type, UnOp,
};

use crate::wp::{free_vars_formula, Wp};

// ---- LiftCtx: scope-tracked name resolution ----
//
// The shadow AST is the structural witness of the source. Per Sir's
// "shadow AST pays rent" directive (#368), the lifter consults a
// scope walker mirroring the shadow tree's structure when emitting IR
// variable references. Each closure binder receives a globally-unique
// id within the formula; references inside the closure body resolve
// to that unique id; references outside (free variables) keep their
// surface name. The result is that lifted IR is in barendregt form
// for closure binders by construction — capture is impossible at the
// lift layer for those binders. The capture-avoiding substitution in
// `wp.rs` is the belt-and-suspenders second line.
//
// The binder counter is per-formula. Two structurally identical
// inputs produce structurally identical IR (deterministic for content
// addressing).
struct LiftCtx {
    next_binder_id: u32,
    /// Stack of frames; each frame holds (surface_name, unique_name) pairs
    /// in declaration order. Innermost frame shadows outer frames.
    scope: Vec<Vec<(String, String)>>,
    local_value_kinds: BTreeMap<String, ValueKind>,
    return_facts: FunctionReturnFacts,
    assertion_guard_facts: Vec<TrackedGuardFact>,
    len_eq_one_facts: Vec<LenEqOneFact>,
    mutable_roots: HashSet<String>,
}

impl LiftCtx {
    fn new() -> Self {
        Self::with_return_facts(FunctionReturnFacts::default())
    }

    fn with_return_facts(return_facts: FunctionReturnFacts) -> Self {
        Self {
            next_binder_id: 0,
            scope: Vec::new(),
            local_value_kinds: BTreeMap::new(),
            return_facts,
            assertion_guard_facts: Vec::new(),
            len_eq_one_facts: Vec::new(),
            mutable_roots: HashSet::new(),
        }
    }

    fn push_frame(&mut self) {
        self.scope.push(Vec::new());
    }

    fn pop_frame(&mut self) {
        self.scope.pop();
    }

    /// Bind `base` in the innermost frame to a fresh unique name; return
    /// the unique name. Caller must have pushed at least one frame.
    fn bind(&mut self, base: &str) -> String {
        let id = self.next_binder_id;
        self.next_binder_id += 1;
        let unique = format!("{}#{}", base, id);
        self.scope
            .last_mut()
            .expect("LiftCtx::bind without push_frame")
            .push((base.to_string(), unique.clone()));
        unique
    }

    /// Resolve a surface name to its unique form. If not bound in any
    /// frame, the name is free in this formula and returned unchanged.
    fn resolve(&self, base: &str) -> String {
        for frame in self.scope.iter().rev() {
            for (b, u) in frame.iter().rev() {
                if b == base {
                    return u.clone();
                }
            }
        }
        base.to_string()
    }

    fn invalidate_root(&mut self, root: &str) {
        self.local_value_kinds.remove(root);
        self.assertion_guard_facts.retain(|fact| fact.root != root);
        self.len_eq_one_facts.retain(|fact| fact.root != root);
    }
}

#[derive(Clone, Debug, Default)]
pub struct FunctionReturnFacts {
    direct_string: HashSet<String>,
    result_string: HashSet<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ValueKind {
    String,
    Number,
    Bool,
    JsonObject(BTreeMap<String, ValueKind>),
    Unknown,
}

#[derive(Clone, Debug)]
struct TrackedGuardFact {
    root: String,
    receiver_key: String,
    guard_head: String,
    guard: IrTerm,
}

#[derive(Clone, Debug)]
struct LenEqOneFact {
    root: String,
    receiver_key: String,
}

/// Collect only EXPLICIT signature facts from a Rust file. This is a
/// refuse-floor data source for json! construction tracking: the kit trusts
/// `fn f(...) -> String` and `fn g(...) -> Result<String, _>` declarations,
/// but never infers return kinds from function bodies in this slice.
pub fn collect_explicit_function_return_facts(file: &syn::File) -> FunctionReturnFacts {
    let mut facts = FunctionReturnFacts::default();
    for item in &file.items {
        let syn::Item::Fn(item_fn) = item else {
            continue;
        };
        let name = item_fn.sig.ident.to_string();
        match explicit_return_kind(&item_fn.sig.output) {
            Some(ExplicitReturnKind::String) => {
                facts.direct_string.insert(name);
            }
            Some(ExplicitReturnKind::ResultString) => {
                facts.result_string.insert(name);
            }
            None => {}
        }
    }
    facts
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum ExplicitReturnKind {
    String,
    ResultString,
}

fn explicit_return_kind(output: &ReturnType) -> Option<ExplicitReturnKind> {
    let ReturnType::Type(_, ty) = output else {
        return None;
    };
    if type_is_string(ty) {
        return Some(ExplicitReturnKind::String);
    }
    if type_is_result_string(ty) {
        return Some(ExplicitReturnKind::ResultString);
    }
    None
}

fn type_is_string(ty: &Type) -> bool {
    let Type::Path(type_path) = ty else {
        return false;
    };
    type_path
        .path
        .segments
        .last()
        .map(|seg| seg.ident == "String")
        .unwrap_or(false)
}

fn type_is_result_string(ty: &Type) -> bool {
    let Type::Path(type_path) = ty else {
        return false;
    };
    let Some(seg) = type_path.path.segments.last() else {
        return false;
    };
    if seg.ident != "Result" {
        return false;
    }
    let PathArguments::AngleBracketed(args) = &seg.arguments else {
        return false;
    };
    let Some(GenericArgument::Type(first)) = args.args.first() else {
        return false;
    };
    type_is_string(first)
}

/// Lift the implicit precondition from a function body. Walks every
/// statement and conjoins the contribution of each pattern recognized.
///
/// Returns `Wp(true)` if no patterns are recognized — this means the
/// function makes no demands on its caller (a vacuous precondition).
pub fn lift_function_precondition(item_fn: &ItemFn) -> Wp {
    let mut ctx = LiftCtx::new();
    let mut accum: Vec<IrFormula> = Vec::new();
    for stmt in &item_fn.block.stmts {
        if let Some(predicate) = lift_stmt_contribution(stmt, &mut ctx) {
            accum.push(predicate);
        }
    }
    Wp(simplify_conjunction(accum))
}

/// Lift the implicit postcondition from a function body. Returns the
/// conjunction of predicates DERIVED from the body's structure that
/// hold at every reachable return point.
///
/// Derivation sources:
///   - if-then-panic: ¬cond holds for the non-panic continuation
///     (Sir's "every if is a free post; every else is the contraposition").
///   - assert!(c): c holds afterward.
///   - Trailing return expression: derives `result = <expr>` where
///     <expr> is lifted to an IrTerm and named-result equates with it.
///
/// This is real derivation: facts the substrate produces from the
/// body's structure that did not appear as explicit annotations. The
/// postcondition is the conjunction of every such derived fact.
pub fn lift_function_postcondition(item_fn: &ItemFn) -> Wp {
    lift_function_postcondition_with_return_facts(item_fn, &FunctionReturnFacts::default())
}

pub fn lift_function_postcondition_with_return_facts(
    item_fn: &ItemFn,
    return_facts: &FunctionReturnFacts,
) -> Wp {
    let mut ctx = LiftCtx::with_return_facts(return_facts.clone());

    // 1. Collect entry-assertion contributions, but track which names are
    //    subsequently shadowed by `let` bindings. An entry assertion
    //    `assert!(x >= 5)` that is followed LATER by `let x = 0` is UNSOUND to
    //    copy into the postcondition: after `let x = 0` the name `x` means
    //    something else. Drop any entry assertion whose free variables are
    //    shadowed by a `let` at a LATER position in the body.
    //
    //    Algorithm: walk statements in order, collecting (formula, position)
    //    pairs for assertions. Then for each assertion, collect names bound by
    //    `let` statements that come AFTER the assertion's index, and filter out
    //    assertions whose free variables overlap those later-bound names.
    let stmts = &item_fn.block.stmts;
    let mut entry_assertions: Vec<(IrFormula, usize)> = Vec::new();
    for (i, stmt) in stmts.iter().enumerate() {
        if let Some(predicate) = lift_stmt_contribution(stmt, &mut ctx) {
            entry_assertions.push((predicate, i));
        }
    }

    // Keep only assertions whose free variables are NOT shadowed by a LATER
    // `let` binding. A `let` that precedes the assertion is fine (it introduces
    // the name the assertion references); only later rebindings are unsound.
    let mut accum: Vec<IrFormula> = entry_assertions
        .into_iter()
        .filter(|(formula, assert_idx)| {
            let free = free_vars_formula(formula);
            // Collect names bound by `let` statements at positions AFTER this assertion.
            let mut later_bound: HashSet<String> = HashSet::new();
            for stmt in stmts.iter().skip(assert_idx + 1) {
                collect_let_bound_names(stmt, &mut later_bound);
            }
            // Keep this assertion only if none of its free vars are rebound later.
            free.is_disjoint(&later_bound)
        })
        .map(|(formula, _)| formula)
        .collect();

    seed_param_value_kinds(item_fn, &mut ctx);
    seed_mutable_param_roots(item_fn, &mut ctx);
    collect_assertion_guard_facts(stmts, &mut ctx);
    collect_local_value_facts(stmts, &mut ctx);

    // 2. Trailing-expression derivation: if the function body ends with
    //    an expression statement (no trailing semicolon), that
    //    expression is the function's return value. Derive
    //    `result = <lifted expression>` and add to the postcondition.
    if let Some(Stmt::Expr(e, None)) = stmts.last() {
        if let Some(term) = lift_tail_expr_to_result_term(e, &mut ctx) {
            let result_var = IrTerm::Var {
                name: "result".to_string(),
            };
            accum.push(IrFormula::Atomic {
                name: "=".to_string(),
                args: vec![result_var, term],
            });
        }
    }

    // 3. Explicit `return expr;` tails. If the body has an explicit
    //    `return <expr>;` statement, derive `result = <lifted expr>`.
    for stmt in stmts {
        if let Some(formula) = lift_return_stmt_postcondition(stmt, &mut ctx) {
            accum.push(formula);
        }
    }

    Wp(simplify_conjunction(accum))
}

fn lift_tail_expr_to_result_term(expr: &Expr, ctx: &mut LiftCtx) -> Option<IrTerm> {
    match expr {
        Expr::If(if_expr) => lift_tail_if_to_ite_term(if_expr, ctx),
        _ => lift_expr_to_term_inner(expr, ctx),
    }
}

fn lift_tail_if_to_ite_term(if_expr: &ExprIf, ctx: &mut LiftCtx) -> Option<IrTerm> {
    // The condition lifts as a boolean TERM. Prefer the structured
    // predicate lift (it normalizes comparisons / De Morgan); fall back to
    // lifting the condition directly as a term (`path.is_absolute()` ->
    // `method:is_absolute(path)`), so a method-call or other non-whitelist
    // boolean condition no longer collapses the whole `ite`. The cond term
    // is uninterpreted; the `ite` still discharges reflexively.
    let cond_term = match lift_predicate_inner(&if_expr.cond, ctx).and_then(formula_to_term) {
        Some(t) => t,
        None => lift_expr_to_term_inner(&if_expr.cond, ctx)?,
    };
    // The then-branch value is the block's TAIL expression (last
    // expr-statement), not necessarily a single-statement block: a
    // branch may run `let x = ...; x`. Leading statements do not change
    // the returned value term.
    let then_expr = block_tail_expr(&if_expr.then_branch)?;
    let then_term = lift_expr_to_term_inner(then_expr, ctx)?;
    let else_term = match if_expr.else_branch.as_ref() {
        Some((_, else_expr)) => {
            // `else if ...` nests another `if`; `else { ... }` is a block.
            // Both reduce through `lift_expr_to_term_inner` (the `If`/`Block`
            // arms), so stmt-bearing else blocks and else-if chains work.
            lift_expr_to_term_inner(else_expr, ctx)?
        }
        None => {
            // if-without-else in value position: the implicit else value is
            // `()`. Model it as an opaque nullary ctor; encoded as an
            // uninterpreted constant by the verifier, so the synthesized
            // `ite` still discharges reflexively against the body's own.
            IrTerm::Ctor {
                name: "unit".to_string(),
                args: vec![],
            }
        }
    };
    // PANIC-FREEDOM guard resolution lives HERE, in the Rust kit. The kit is
    // the only layer ALLOWED to know that `is_some`'s complement is `is_none`,
    // that `is_ok`'s is `is_err`, etc. -- this is Rust-std semantics, exactly
    // as JUnit lives in the Java emitter. The then-branch is dominated by the
    // POSITIVE guard predicate; the else-branch by its COMPLEMENT. We wrap each
    // branch value in `cf_guarded(<resolved-predicate-term>, <value>)` so the
    // language-blind verifier can thread the already-resolved atom into its
    // path condition without recognizing a single Rust predicate name.
    let then_term = wrap_branch_guard(&cond_term, false, then_term);
    let else_term = wrap_branch_guard(&cond_term, true, else_term);
    Some(IrTerm::Ctor {
        // `cf_ite`, not the SMT builtin `ite`: a synthesized control-flow
        // value over uninterpreted Int-sorted operands. Using the builtin
        // `ite` makes z3 demand a Bool guard and Bool/typed branches, which
        // an uninterpreted guard term (`match_guard(..)` : Int) does not
        // satisfy -- a sort-mismatch error that fails the reflexive
        // discharge. As a fresh uninterpreted symbol, congruence closes
        // `cf_ite(g,a,b) == cf_ite(g,a,b)` regardless of operand sorts.
        name: panic_freedom::CF_ITE.to_string(),
        args: vec![cond_term, then_term, else_term],
    })
}

/// The closed set of boolean-predicate guards whose POSITIVE form is a
/// panic-freedom obligation a Rust-std partial demands, plus each one's
/// COMPLEMENT. This table is Rust-std semantics and is ALLOWED to live in the
/// Rust kit (the lifter) -- it is the kit's job to know that the else-branch of
/// `if opt.is_some()` is governed by `is_none`. The verifier never sees these
/// names; it only threads whatever resolved atom the kit emits on a branch.
///
/// Returns the guard-predicate HEAD that governs `branch` (then = positive,
/// else = complement), or `None` when the condition head is not a recognized
/// unary boolean predicate (a comparison `cf_lt`, a non-predicate method,
/// `cf_and`, ...). `None` means: emit no guard wrapper -> the branch carries no
/// usable fact -> a partial inside it stays honestly undecidable. `!is_empty`
/// also returns `None`: its complement establishes no partial's pre.
///
/// NAME NORMALIZATION (load-bearing for discharge). A caller's condition
/// `opt.is_some()` lifts to the method-call ctor `method:is_some` (see
/// `Expr::MethodCall` -> `format!("method:{}", ..)`), but the PARTIAL's own
/// precondition was lifted from `assert!(opt.is_some())` to the BARE predicate
/// `is_some` (the `assert!` lifter produces bare heads). For the syntactic
/// discharge `guard => pre` to hold, the guard atom this kit emits must use the
/// SAME bare head as the partial's pre. So we strip a `method:` prefix on the
/// condition head and emit the bare predicate name. The verifier never sees
/// this normalization -- it only threads the resolved bare atom.
fn branch_guard_head(cond_head: &str, else_branch: bool) -> Option<&'static str> {
    let head = cond_head.strip_prefix("method:").unwrap_or(cond_head);
    match (head, else_branch) {
        (panic_freedom::IS_SOME, false) | (panic_freedom::IS_NONE, true) => {
            Some(panic_freedom::IS_SOME)
        }
        (panic_freedom::IS_NONE, false) | (panic_freedom::IS_SOME, true) => {
            Some(panic_freedom::IS_NONE)
        }
        (panic_freedom::IS_OK, false) | (panic_freedom::IS_ERR, true) => Some(panic_freedom::IS_OK),
        (panic_freedom::IS_ERR, false) | (panic_freedom::IS_OK, true) => {
            Some(panic_freedom::IS_ERR)
        }
        ("is_empty", false) => Some("is_empty"),
        // `!is_empty` (else of `if c.is_empty()`) establishes no partial pre.
        ("is_empty", true) => None,
        _ => None,
    }
}

/// Wrap a branch value in `cf_guarded(<resolved-guard-term>, <value>)` when the
/// dominating condition is a recognized unary boolean predicate. The guard term
/// reuses the condition's argument terms (the receiver the predicate is about),
/// so the carried atom (`is_some(x)`) names the SAME term the partial's `pre`
/// is instantiated over -- the only way the syntactic discharge can match.
///
/// SOUNDNESS: only a recognized head is wrapped. An unrecognized condition
/// (comparison, method, conjunction) leaves the branch value UNCHANGED, so it
/// carries no fact and a partial inside it stays undecidable. The else-branch
/// receives the COMPLEMENT predicate, which never establishes the positive
/// `pre`. No path wraps a branch with a guard that would over-prove. Leaving
/// the value unchanged when no guard applies also keeps every non-guarded
/// `cf_ite` byte-identical to before this change (CID stability / reflexive
/// discharge unperturbed).
fn wrap_branch_guard(cond_term: &IrTerm, else_branch: bool, value: IrTerm) -> IrTerm {
    if !else_branch {
        if let Some(guard) = len_eq_one_branch_guard(cond_term, &value) {
            return wrap_cf_guarded(guard, value);
        }
    }
    let (head, args) = match &cond_term {
        IrTerm::Ctor { name, args } => (name.as_str(), args),
        _ => return value,
    };
    let Some(resolved_head) = branch_guard_head(head, else_branch) else {
        return value;
    };
    let guard = IrTerm::Ctor {
        name: resolved_head.to_string(),
        args: args.clone(),
    };
    IrTerm::Ctor {
        name: panic_freedom::CF_GUARDED.to_string(),
        args: vec![guard, value],
    }
}

fn len_eq_one_branch_guard(cond_term: &IrTerm, value: &IrTerm) -> Option<IrTerm> {
    let receiver_key = len_eq_one_receiver_key(cond_term)?;
    let next_receiver = find_next_partial_receiver(value, &receiver_key)?;
    Some(IrTerm::Ctor {
        name: panic_freedom::IS_SOME.to_string(),
        args: vec![next_receiver],
    })
}

fn len_eq_one_receiver_key(cond_term: &IrTerm) -> Option<String> {
    let IrTerm::Ctor { name, args } = cond_term else {
        return None;
    };
    if name != "cf_eq" || args.len() != 2 {
        return None;
    }
    let receiver = if is_const_one(&args[1]) {
        len_receiver_term(&args[0])?
    } else if is_const_one(&args[0]) {
        len_receiver_term(&args[1])?
    } else {
        return None;
    };
    term_key(&receiver)
}

fn find_next_partial_receiver(term: &IrTerm, collection_receiver_key: &str) -> Option<IrTerm> {
    match term {
        IrTerm::Ctor { name, args }
            if matches!(
                name.as_str(),
                panic_freedom::METHOD_UNWRAP | panic_freedom::METHOD_EXPECT
            ) && !args.is_empty()
                && next_into_iter_receiver_key(&args[0]).as_deref()
                    == Some(collection_receiver_key) =>
        {
            Some(args[0].clone())
        }
        IrTerm::Ctor { args, .. } => args
            .iter()
            .find_map(|arg| find_next_partial_receiver(arg, collection_receiver_key)),
        _ => None,
    }
}

/// Fold a value-position `match` into a right-nested `ite` chain keyed
/// by each arm's recognized guard predicate. Arm `i` with pattern-guard
/// `g_i` and value `v_i` becomes `ite(g_i, v_i, <rest>)`; the final arm
/// is the chain's tail (no guard needed). A pattern we cannot turn into
/// a boolean guard is modeled with an opaque `match_arm` guard term so
/// the chain still forms; the resulting term is uninterpreted but
/// discharges reflexively against the body's own identical match.
fn lift_match_to_ite_term(match_expr: &syn::ExprMatch, ctx: &mut LiftCtx) -> Option<IrTerm> {
    let scrutinee = lift_expr_to_term_inner(&match_expr.expr, ctx)?;
    let arms = &match_expr.arms;
    if arms.is_empty() {
        return None;
    }
    // Build from the last arm backwards.
    let mut acc: Option<IrTerm> = None;
    for (idx, arm) in arms.iter().enumerate().rev() {
        let value = lift_expr_to_term_inner(&arm.body, ctx)?;
        let is_last = idx == arms.len() - 1;
        acc = Some(if is_last && acc.is_none() {
            // Final arm: the fall-through value of the chain.
            value
        } else {
            // A guard predicate keyed by this arm's pattern against the
            // scrutinee. We do not interpret the pattern; we name an
            // opaque `match_guard(scrutinee, <pattern-hash>)` boolean. The
            // verifier encodes it as an uninterpreted predicate symbol.
            let pat_hash = opaque_token_hash(&arm.pat);
            let guard = IrTerm::Ctor {
                name: "match_guard".to_string(),
                args: vec![
                    scrutinee.clone(),
                    IrTerm::Var {
                        name: format!("#pat:{pat_hash}"),
                    },
                ],
            };
            IrTerm::Ctor {
                // `cf_ite` (uninterpreted), not the builtin `ite`: see the
                // note in `lift_tail_if_to_ite_term`.
                name: panic_freedom::CF_ITE.to_string(),
                args: vec![guard, value, acc.expect("non-final arm has an accumulator")],
            }
        });
    }
    acc
}

/// Lift a macro invocation (`json!{...}`, `format!(...)`, `vec![...]`,
/// ...) to an OPAQUE uninterpreted term keyed by the macro's name plus a
/// deterministic hash of its token stream. Two identical macro calls
/// (same name, same tokens) produce the SAME term, so a body returning
/// `Ok(json!({...}))` and a post derived from that same body yield
/// `Ok(macro:json!#<h>) == Ok(macro:json!#<h>)`, which discharges by
/// reflexivity. A DIFFERENT macro call hashes differently and does not
/// spuriously unify. The argument tokens are not lifted (a macro body is
/// not Rust expression grammar); the hash is the whole content.
fn lift_macro_to_opaque_term(mac: &Macro) -> IrTerm {
    let name = mac
        .path
        .segments
        .last()
        .map(|s| s.ident.to_string())
        .unwrap_or_else(|| "macro".to_string());
    let tok_hash = opaque_token_hash(&mac.tokens);
    IrTerm::Ctor {
        name: format!("macro:{name}#{tok_hash}"),
        args: vec![],
    }
}

/// Deterministic short hash of any `ToTokens` node (a macro's token
/// stream, a match pattern, ...). Stable across runs: the token stream
/// renders to a canonical string and is blake3-hashed; the first 16 hex
/// chars key the opaque term. Determinism matters because the SAME
/// surface node must encode to the SAME symbol on both sides of the
/// reflexive equality.
fn opaque_token_hash<T: quote::ToTokens>(node: &T) -> String {
    let rendered = node.to_token_stream().to_string();
    let full = provekit_canonicalizer::blake3_512_hex(rendered.as_bytes());
    // `blake3_512_hex` returns a `blake3-512:<hex>` prefixed string; take
    // the hex tail and keep it short for readable terms.
    let hex = full.rsplit(':').next().unwrap_or(&full);
    hex.chars().take(16).collect()
}

/// Map an SMT builtin predicate/connective head to a `cf_`-prefixed
/// UNINTERPRETED head. `formula_to_term` is used to fold a control-flow
/// CONDITION into a value term (the guard arg of `cf_ite`). A builtin
/// like `<` or `and` there would be applied as a Bool-typed term inside
/// an uninterpreted Int-sorted context, raising an SMT sort mismatch. As
/// `cf_lt`/`cf_and` it is uninterpreted and discharges by congruence. A
/// non-builtin head (`is_some`, a method) is already uninterpreted and
/// passes through unchanged.
fn cf_head(name: &str) -> String {
    match name {
        "=" | "eq" => "cf_eq",
        "≠" | "ne" | "neq" => "cf_ne",
        "<" | "lt" => "cf_lt",
        "≤" | "le" | "lte" => "cf_le",
        ">" | "gt" => "cf_gt",
        "≥" | "ge" | "gte" => "cf_ge",
        "and" => "cf_and",
        "or" => "cf_or",
        "not" => "cf_not",
        "implies" => "cf_implies",
        other => return other.to_string(),
    }
    .to_string()
}

fn formula_to_term(formula: IrFormula) -> Option<IrTerm> {
    match formula {
        IrFormula::Atomic { name, args } => Some(IrTerm::Ctor {
            name: cf_head(&name),
            args,
        }),
        IrFormula::And { operands } => formula_operands_to_term("cf_and", operands),
        IrFormula::Or { operands } => formula_operands_to_term("cf_or", operands),
        IrFormula::Not { operands } => formula_operands_to_term("cf_not", operands),
        IrFormula::Implies { operands } => formula_operands_to_term("cf_implies", operands),
        IrFormula::Forall { .. } | IrFormula::Exists { .. } | IrFormula::Choice { .. } => None,
        // Substitute and Apply are meta-level; not reducible to a term here.
        IrFormula::Substitute { .. }
        | IrFormula::Apply { .. }
        | IrFormula::DivergenceBetween { .. } => None,
    }
}

fn formula_operands_to_term(name: &str, operands: Vec<IrFormula>) -> Option<IrTerm> {
    let args = operands
        .into_iter()
        .map(formula_to_term)
        .collect::<Option<Vec<_>>>()?;
    Some(IrTerm::Ctor {
        name: name.to_string(),
        args,
    })
}

/// The block's value expression: its trailing expr-statement (no
/// semicolon). Unlike `block_single_tail_expr` this tolerates LEADING
/// statements (`let x = ...; x`), because they do not change the value
/// the block evaluates to. Returns `None` for a block whose last
/// statement is not a tail expression (e.g. ends in a `;` -> unit value).
fn block_tail_expr(block: &syn::Block) -> Option<&Expr> {
    match block.stmts.last() {
        Some(Stmt::Expr(expr, None)) => Some(expr),
        _ => None,
    }
}

/// Collect all names bound by `let` patterns at the top level of a statement.
/// Used for the shadowing check in `lift_function_postcondition`.
fn collect_let_bound_names(stmt: &Stmt, out: &mut HashSet<String>) {
    if let Stmt::Local(Local { pat, .. }) = stmt {
        collect_pat_names(pat, out);
    }
}

/// Recursively collect all bound names from a pattern.
fn collect_pat_names(pat: &Pat, out: &mut HashSet<String>) {
    match pat {
        Pat::Ident(p) => {
            out.insert(p.ident.to_string());
        }
        Pat::Type(pt) => collect_pat_names(&pt.pat, out),
        Pat::Reference(r) => collect_pat_names(&r.pat, out),
        Pat::Paren(p) => collect_pat_names(&p.pat, out),
        Pat::Tuple(t) => {
            for sub in &t.elems {
                collect_pat_names(sub, out);
            }
        }
        Pat::TupleStruct(ts) => {
            for sub in &ts.elems {
                collect_pat_names(sub, out);
            }
        }
        Pat::Struct(s) => {
            for field in &s.fields {
                collect_pat_names(&field.pat, out);
            }
        }
        Pat::Slice(s) => {
            for sub in &s.elems {
                collect_pat_names(sub, out);
            }
        }
        _ => {}
    }
}

fn seed_mutable_param_roots(item_fn: &ItemFn, ctx: &mut LiftCtx) {
    for input in &item_fn.sig.inputs {
        let FnArg::Typed(arg) = input else {
            continue;
        };
        let Pat::Ident(ident) = &*arg.pat else {
            continue;
        };
        if ident.mutability.is_some() {
            ctx.mutable_roots.insert(ident.ident.to_string());
        }
    }
}

fn seed_param_value_kinds(item_fn: &ItemFn, ctx: &mut LiftCtx) {
    for input in &item_fn.sig.inputs {
        let FnArg::Typed(arg) = input else {
            continue;
        };
        let Pat::Ident(ident) = &*arg.pat else {
            continue;
        };
        if ident.mutability.is_some() {
            continue;
        }
        if type_is_string(&arg.ty) {
            ctx.local_value_kinds
                .insert(ident.ident.to_string(), ValueKind::String);
        }
    }
}

/// Track top-level facts established by `assert!` for later panic partials.
///
/// SOUNDNESS / refuse-floor: the propagation is same-function, top-level, and
/// same-receiver only. Branch-local asserts are not scanned; mutations or
/// shadowing invalidate a fact immediately. Mutable roots are refused because
/// this slice does not model aliasing or mutation.
fn collect_assertion_guard_facts(stmts: &[Stmt], ctx: &mut LiftCtx) {
    for stmt in stmts {
        match stmt {
            Stmt::Local(local) => {
                let mut names = HashSet::new();
                collect_pat_names(&local.pat, &mut names);
                let local_mutable = local_binding_ident(local)
                    .map(|(_, mutable)| mutable)
                    .unwrap_or(false);
                for name in names {
                    ctx.invalidate_root(&name);
                    if local_mutable {
                        ctx.mutable_roots.insert(name);
                    }
                }
            }
            Stmt::Macro(StmtMacro { mac, .. }) => {
                collect_assert_macro_guard_fact(mac, ctx);
            }
            Stmt::Expr(Expr::Macro(ExprMacro { mac, .. }), _) => {
                collect_assert_macro_guard_fact(mac, ctx);
            }
            Stmt::Expr(expr, _) => invalidate_assignment_targets(expr, ctx),
            Stmt::Item(_) => {}
        }
    }
}

fn collect_assert_macro_guard_fact(mac: &Macro, ctx: &mut LiftCtx) {
    let Some(first) = assert_macro_condition(mac) else {
        return;
    };
    if let Some(fact) = tracked_direct_guard_fact(&first, ctx) {
        ctx.assertion_guard_facts.push(fact);
        return;
    }
    if let Some(fact) = tracked_len_eq_one_fact(&first, ctx) {
        ctx.len_eq_one_facts.push(fact);
    }
}

fn assert_macro_condition(mac: &Macro) -> Option<Expr> {
    let seg = mac.path.segments.last()?;
    if seg.ident != "assert" {
        return None;
    }
    let parsed_cond = syn::parse2::<Expr>(mac.tokens.clone()).ok()?;
    // assert!(c) parses to just c. assert!(c, "msg") parses as a tuple-expr;
    // take the first elem.
    match parsed_cond {
        Expr::Tuple(t) => t.elems.first().cloned(),
        other => Some(other),
    }
}

fn tracked_direct_guard_fact(expr: &Expr, ctx: &mut LiftCtx) -> Option<TrackedGuardFact> {
    let Expr::MethodCall(method_call) = expr else {
        return None;
    };
    if !method_call.args.is_empty() {
        return None;
    }
    let guard_head = method_call.method.to_string();
    if !matches!(
        guard_head.as_str(),
        panic_freedom::IS_SOME | panic_freedom::IS_OK | panic_freedom::IS_ERR
    ) {
        return None;
    }
    let root = expr_root_ident(&method_call.receiver)?;
    if ctx.mutable_roots.contains(&root) {
        return None;
    }
    let receiver = lift_expr_to_term_inner(&method_call.receiver, ctx)?;
    let receiver_key = term_key(&receiver)?;
    let guard = IrTerm::Ctor {
        name: guard_head.clone(),
        args: vec![receiver],
    };
    Some(TrackedGuardFact {
        root,
        receiver_key,
        guard_head,
        guard,
    })
}

fn tracked_len_eq_one_fact(expr: &Expr, ctx: &mut LiftCtx) -> Option<LenEqOneFact> {
    let Expr::Binary(binary) = expr else {
        return None;
    };
    if !matches!(binary.op, BinOp::Eq(_)) {
        return None;
    }
    let left = lift_expr_to_term_inner(&binary.left, ctx)?;
    let right = lift_expr_to_term_inner(&binary.right, ctx)?;
    let receiver = if is_const_one(&right) {
        len_receiver_term(&left)?
    } else if is_const_one(&left) {
        len_receiver_term(&right)?
    } else {
        return None;
    };
    let root =
        len_receiver_root_expr(&binary.left).or_else(|| len_receiver_root_expr(&binary.right))?;
    if ctx.mutable_roots.contains(&root) {
        return None;
    }
    Some(LenEqOneFact {
        root,
        receiver_key: term_key(&receiver)?,
    })
}

fn len_receiver_term(term: &IrTerm) -> Option<IrTerm> {
    match term {
        IrTerm::Ctor { name, args } if name == "method:len" && args.len() == 1 => {
            Some(args[0].clone())
        }
        _ => None,
    }
}

fn len_receiver_root_expr(expr: &Expr) -> Option<String> {
    match expr {
        Expr::MethodCall(method_call)
            if method_call.method == "len" && method_call.args.is_empty() =>
        {
            expr_root_ident(&method_call.receiver)
        }
        Expr::Paren(paren) => len_receiver_root_expr(&paren.expr),
        _ => None,
    }
}

fn is_const_one(term: &IrTerm) -> bool {
    matches!(term, IrTerm::Const { value, .. } if value.as_i64() == Some(1))
}

fn expr_root_ident(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Path(path) => path.path.segments.last().map(|seg| seg.ident.to_string()),
        Expr::Reference(reference) => expr_root_ident(&reference.expr),
        Expr::Paren(paren) => expr_root_ident(&paren.expr),
        Expr::Cast(cast) => expr_root_ident(&cast.expr),
        Expr::Field(field) => expr_root_ident(&field.base),
        Expr::Index(index) => expr_root_ident(&index.expr),
        _ => None,
    }
}

fn term_key(term: &IrTerm) -> Option<String> {
    serde_json::to_string(term).ok()
}

/// Track local json! construction facts for this function body.
///
/// SOUNDNESS / refuse-floor: this is a Rust-kit-only analysis and must bless
/// only facts proven by explicit local construction. Any uncertainty (mutable
/// binding, assignment to a tracked value, non-literal json! macro, opaque call,
/// or divergent shape we do not model) becomes `Unknown`; `Unknown` never emits
/// `cf_guarded`, so the panic site stays honestly undecidable. In particular,
/// `let mut x = json!(...); x["k"] = ...` is refused in this slice rather than
/// tracked.
fn collect_local_value_facts(stmts: &[Stmt], ctx: &mut LiftCtx) {
    for stmt in stmts {
        match stmt {
            Stmt::Local(local) => collect_local_binding_value_fact(local, ctx),
            Stmt::Expr(expr, _) => invalidate_assignment_targets(expr, ctx),
            Stmt::Macro(_) | Stmt::Item(_) => {}
        }
    }
}

fn collect_local_binding_value_fact(local: &Local, ctx: &mut LiftCtx) {
    let Some((name, mutable)) = local_binding_ident(local) else {
        let mut names = HashSet::new();
        collect_pat_names(&local.pat, &mut names);
        for name in names {
            ctx.local_value_kinds.remove(&name);
        }
        return;
    };
    if mutable {
        ctx.local_value_kinds.remove(&name);
        return;
    }
    let kind = local
        .init
        .as_ref()
        .map(|init| infer_value_kind(&init.expr, ctx))
        .unwrap_or(ValueKind::Unknown);
    ctx.local_value_kinds.insert(name, kind);
}

fn local_binding_ident(local: &Local) -> Option<(String, bool)> {
    match &local.pat {
        Pat::Ident(ident) => Some((ident.ident.to_string(), ident.mutability.is_some())),
        Pat::Type(typed) => match &*typed.pat {
            Pat::Ident(ident) => Some((ident.ident.to_string(), ident.mutability.is_some())),
            _ => None,
        },
        _ => None,
    }
}

fn invalidate_assignment_targets(expr: &Expr, ctx: &mut LiftCtx) {
    match expr {
        Expr::Assign(assign) => {
            if let Some(root) = assignment_root_ident(&assign.left) {
                ctx.invalidate_root(&root);
            }
            invalidate_assignment_targets(&assign.right, ctx);
        }
        Expr::Block(block) => {
            for stmt in &block.block.stmts {
                if let Stmt::Expr(expr, _) = stmt {
                    invalidate_assignment_targets(expr, ctx);
                }
            }
        }
        Expr::If(if_expr) => {
            invalidate_assignment_targets(&if_expr.cond, ctx);
            for stmt in &if_expr.then_branch.stmts {
                if let Stmt::Expr(expr, _) = stmt {
                    invalidate_assignment_targets(expr, ctx);
                }
            }
            if let Some((_, else_expr)) = &if_expr.else_branch {
                invalidate_assignment_targets(else_expr, ctx);
            }
        }
        _ => {}
    }
}

fn assignment_root_ident(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Path(path) => path.path.segments.last().map(|seg| seg.ident.to_string()),
        Expr::Index(index) => assignment_root_ident(&index.expr),
        Expr::Field(field) => assignment_root_ident(&field.base),
        Expr::Paren(paren) => assignment_root_ident(&paren.expr),
        _ => None,
    }
}

fn infer_value_kind(expr: &Expr, ctx: &LiftCtx) -> ValueKind {
    match expr {
        Expr::Lit(lit) => match &lit.lit {
            Lit::Str(_) => ValueKind::String,
            Lit::Int(_) | Lit::Float(_) => ValueKind::Number,
            Lit::Bool(_) => ValueKind::Bool,
            _ => ValueKind::Unknown,
        },
        Expr::Path(path) => path
            .path
            .segments
            .last()
            .and_then(|seg| ctx.local_value_kinds.get(&seg.ident.to_string()))
            .cloned()
            .unwrap_or(ValueKind::Unknown),
        Expr::Paren(paren) => infer_value_kind(&paren.expr, ctx),
        Expr::Reference(reference) => infer_value_kind(&reference.expr, ctx),
        Expr::Group(group) => infer_value_kind(&group.expr, ctx),
        Expr::MethodCall(method) => {
            let method_name = method.method.to_string();
            match method_name.as_str() {
                "to_string" => ValueKind::String,
                "clone" if method.args.is_empty() => infer_value_kind(&method.receiver, ctx),
                _ => ValueKind::Unknown,
            }
        }
        Expr::Call(call) => {
            let Some(callee) = call_callee_name(call) else {
                return ValueKind::Unknown;
            };
            if ctx.return_facts.direct_string.contains(&callee) {
                ValueKind::String
            } else {
                // A bare call to `fn f() -> Result<String, _>` is not a string;
                // only `f()?` produces the inner String in this slice.
                ValueKind::Unknown
            }
        }
        Expr::Try(try_expr) => {
            if let Expr::Call(call) = &*try_expr.expr {
                if let Some(callee) = call_callee_name(call) {
                    if ctx.return_facts.result_string.contains(&callee) {
                        return ValueKind::String;
                    }
                }
            }
            ValueKind::Unknown
        }
        Expr::Index(index) => infer_indexed_json_value_kind(index, ctx),
        Expr::Macro(expr_macro) => infer_macro_value_kind(&expr_macro.mac, ctx),
        _ => ValueKind::Unknown,
    }
}

fn call_callee_name(call: &syn::ExprCall) -> Option<String> {
    let Expr::Path(path) = &*call.func else {
        return None;
    };
    path.path.segments.last().map(|seg| seg.ident.to_string())
}

fn infer_indexed_json_value_kind(index: &syn::ExprIndex, ctx: &LiftCtx) -> ValueKind {
    let ValueKind::JsonObject(fields) = infer_value_kind(&index.expr, ctx) else {
        return ValueKind::Unknown;
    };
    let Some(key) = expr_string_literal(&index.index) else {
        return ValueKind::Unknown;
    };
    fields.get(&key).cloned().unwrap_or(ValueKind::Unknown)
}

fn expr_string_literal(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Lit(lit) => match &lit.lit {
            Lit::Str(s) => Some(s.value()),
            _ => None,
        },
        Expr::Paren(paren) => expr_string_literal(&paren.expr),
        Expr::Group(group) => expr_string_literal(&group.expr),
        _ => None,
    }
}

fn infer_macro_value_kind(mac: &Macro, ctx: &LiftCtx) -> ValueKind {
    match macro_leaf_name(mac).as_deref() {
        Some("format") => ValueKind::String,
        Some("json") => parse_json_object_macro(mac, ctx)
            .map(ValueKind::JsonObject)
            .unwrap_or(ValueKind::Unknown),
        _ => ValueKind::Unknown,
    }
}

fn macro_leaf_name(mac: &Macro) -> Option<String> {
    mac.path.segments.last().map(|seg| seg.ident.to_string())
}

fn parse_json_object_macro(mac: &Macro, ctx: &LiftCtx) -> Option<BTreeMap<String, ValueKind>> {
    let mut iter = mac.tokens.clone().into_iter();
    let Some(TokenTree::Group(group)) = iter.next() else {
        return None;
    };
    if group.delimiter() != Delimiter::Brace || iter.next().is_some() {
        return None;
    }
    parse_json_object_tokens(group.stream(), ctx)
}

fn parse_json_object_tokens(
    tokens: TokenStream,
    ctx: &LiftCtx,
) -> Option<BTreeMap<String, ValueKind>> {
    let mut fields = BTreeMap::new();
    let mut iter = tokens.into_iter().peekable();
    while let Some(token) = iter.next() {
        if is_comma(&token) {
            continue;
        }
        let key = token_string_literal(&token)?;
        let Some(colon) = iter.next() else {
            return None;
        };
        if !is_colon(&colon) {
            return None;
        }
        let mut value_tokens = TokenStream::new();
        while let Some(next) = iter.peek() {
            if is_comma(next) {
                iter.next();
                break;
            }
            let next = iter.next().expect("peeked token exists");
            value_tokens.extend(std::iter::once(next));
        }
        if value_tokens.is_empty() {
            return None;
        }
        fields.insert(key, infer_json_value_tokens(value_tokens, ctx));
    }
    Some(fields)
}

fn infer_json_value_tokens(tokens: TokenStream, ctx: &LiftCtx) -> ValueKind {
    let mut iter = tokens.clone().into_iter();
    if let Some(TokenTree::Group(group)) = iter.next() {
        if group.delimiter() == Delimiter::Brace && iter.next().is_none() {
            return parse_json_object_tokens(group.stream(), ctx)
                .map(ValueKind::JsonObject)
                .unwrap_or(ValueKind::Unknown);
        }
    }
    syn::parse2::<Expr>(tokens)
        .ok()
        .map(|expr| infer_value_kind(&expr, ctx))
        .unwrap_or(ValueKind::Unknown)
}

fn token_string_literal(token: &TokenTree) -> Option<String> {
    let TokenTree::Literal(lit) = token else {
        return None;
    };
    let parsed = syn::parse_str::<Lit>(&lit.to_string()).ok()?;
    match parsed {
        Lit::Str(s) => Some(s.value()),
        _ => None,
    }
}

fn is_comma(token: &TokenTree) -> bool {
    matches!(token, TokenTree::Punct(p) if p.as_char() == ',')
}

fn is_colon(token: &TokenTree) -> bool {
    matches!(token, TokenTree::Punct(p) if p.as_char() == ':')
}

fn receiver_as_str_is_known_json_string(receiver: &Expr, ctx: &LiftCtx) -> bool {
    let Expr::MethodCall(method) = receiver else {
        return false;
    };
    method.method == "as_str"
        && method.args.is_empty()
        && matches!(infer_value_kind(&method.receiver, ctx), ValueKind::String)
}

fn wrap_known_option_unwrap_guard(receiver: IrTerm, value: IrTerm) -> IrTerm {
    wrap_cf_guarded(
        IrTerm::Ctor {
            name: panic_freedom::IS_SOME.to_string(),
            args: vec![receiver],
        },
        value,
    )
}

fn wrap_cf_guarded(guard: IrTerm, value: IrTerm) -> IrTerm {
    IrTerm::Ctor {
        name: panic_freedom::CF_GUARDED.to_string(),
        args: vec![guard, value],
    }
}

fn assertion_guard_for_partial(
    method: &syn::Ident,
    receiver_term: &IrTerm,
    ctx: &LiftCtx,
) -> Option<IrTerm> {
    let method = method.to_string();
    let receiver_key = term_key(receiver_term)?;
    for fact in &ctx.assertion_guard_facts {
        if fact.receiver_key != receiver_key {
            continue;
        }
        match (method.as_str(), fact.guard_head.as_str()) {
            ("unwrap" | "expect", panic_freedom::IS_SOME | panic_freedom::IS_OK) => {
                return Some(fact.guard.clone())
            }
            ("unwrap_err", panic_freedom::IS_ERR) => return Some(fact.guard.clone()),
            _ => {}
        }
    }
    if matches!(method.as_str(), "unwrap" | "expect")
        && ctx.len_eq_one_facts.iter().any(|fact| {
            next_into_iter_receiver_key(receiver_term).as_ref() == Some(&fact.receiver_key)
        })
    {
        return Some(IrTerm::Ctor {
            name: panic_freedom::IS_SOME.to_string(),
            args: vec![receiver_term.clone()],
        });
    }
    None
}

fn next_into_iter_receiver_key(term: &IrTerm) -> Option<String> {
    match term {
        IrTerm::Ctor { name, args } if name == "method:next" && args.len() == 1 => match &args[0] {
            IrTerm::Ctor { name, args } if name == "method:into_iter" && args.len() == 1 => {
                term_key(&args[0])
            }
            _ => None,
        },
        _ => None,
    }
}

/// If a statement is an explicit `return <expr>;`, derive
/// `result = <lifted expr>`. Returns None for other statement kinds.
fn lift_return_stmt_postcondition(stmt: &Stmt, ctx: &mut LiftCtx) -> Option<IrFormula> {
    let expr = match stmt {
        Stmt::Expr(e, Some(_)) => e, // Expr with trailing semicolon
        _ => return None,
    };
    if let Expr::Return(ret) = expr {
        if let Some(inner) = &ret.expr {
            if let Some(term) = lift_expr_to_term_inner(inner, ctx) {
                let result_var = IrTerm::Var {
                    name: "result".to_string(),
                };
                return Some(IrFormula::Atomic {
                    name: "=".to_string(),
                    args: vec![result_var, term],
                });
            }
        }
    }
    None
}

/// What does this single statement contribute to the function's
/// implicit precondition? Returns None for statements that don't lift
/// (let-bindings, plain expressions, etc.).
fn lift_stmt_contribution(stmt: &Stmt, ctx: &mut LiftCtx) -> Option<IrFormula> {
    match stmt {
        Stmt::Expr(e, _) => lift_expr_contribution(e, ctx),
        // `assert!(c);` at statement position parses to Stmt::Macro
        // (with optional trailing semicolon), not Stmt::Expr(Expr::Macro).
        Stmt::Macro(StmtMacro { mac, .. }) => lift_macro_contribution(mac, ctx),
        _ => None,
    }
}

/// Recognize and lift macro contributions at statement or expression
/// position. Used by both `Stmt::Macro` and `Expr::Macro` paths.
fn lift_macro_contribution(mac: &Macro, ctx: &mut LiftCtx) -> Option<IrFormula> {
    let seg = mac.path.segments.last()?;
    let name = seg.ident.to_string();
    match name.as_str() {
        "assert" => {
            let first = assert_macro_condition(mac)?;
            lift_predicate_inner(&first, ctx)
        }
        // debug_assert! is compiled out in release builds. Lifting its
        // predicate as a real contract would misrepresent what holds in
        // release mode. Skip it entirely.
        _ => None,
    }
}

fn lift_expr_contribution(expr: &Expr, ctx: &mut LiftCtx) -> Option<IrFormula> {
    // if-then-panic pattern: `if cond { panic!() }` lifts to ¬cond.
    if let Expr::If(ExprIf {
        cond,
        then_branch,
        else_branch,
        ..
    }) = expr
    {
        if else_branch.is_none() && block_only_panics(then_branch) {
            let cond_formula = lift_predicate_inner(cond, ctx)?;
            return Some(negate(cond_formula));
        }
    }
    // assert!()-shaped macros sometimes parse as Expr::Macro (e.g. when
    // they're the trailing tail expression of a block).
    if let Expr::Macro(ExprMacro { mac, .. }) = expr {
        if let Some(formula) = lift_macro_contribution(mac, ctx) {
            return Some(formula);
        }
    }
    None
}

/// Lift an arbitrary Rust predicate-shaped expression to `IrFormula`.
/// Returns None for shapes the MVP does not yet handle.
pub fn lift_predicate(expr: &Expr) -> Option<IrFormula> {
    let mut ctx = LiftCtx::new();
    lift_predicate_inner(expr, &mut ctx)
}

fn lift_predicate_inner(expr: &Expr, ctx: &mut LiftCtx) -> Option<IrFormula> {
    match expr {
        Expr::Binary(ExprBinary {
            left, op, right, ..
        }) => match op {
            BinOp::And(_) => {
                let l = lift_predicate_inner(left, ctx)?;
                let r = lift_predicate_inner(right, ctx)?;
                Some(IrFormula::And {
                    operands: vec![l, r],
                })
            }
            BinOp::Or(_) => {
                let l = lift_predicate_inner(left, ctx)?;
                let r = lift_predicate_inner(right, ctx)?;
                Some(IrFormula::Or {
                    operands: vec![l, r],
                })
            }
            _ => {
                // Comparison: lift both sides as terms, pick the IR predicate name.
                let name = bin_op_to_predicate_name(op)?;
                let l_term = lift_expr_to_term_inner(left, ctx)?;
                let r_term = lift_expr_to_term_inner(right, ctx)?;
                Some(IrFormula::Atomic {
                    name: name.to_string(),
                    args: vec![l_term, r_term],
                })
            }
        },
        Expr::Unary(ExprUnary {
            op: UnOp::Not(_),
            expr,
            ..
        }) => {
            let inner = lift_predicate_inner(expr, ctx)?;
            // Apply De Morgan / double-negation via the negate helper,
            // so `!(x >= 10)` lifts to `x < 10`, not `¬(x ≥ 10)`.
            Some(negate(inner))
        }
        Expr::Paren(p) => lift_predicate_inner(&p.expr, ctx),
        // Zero-argument method calls that return bool: `.is_some()`, `.is_none()`,
        // `.is_empty()`, `.is_err()`, `.is_ok()`. These are common predicate shapes
        // in Rust and appear naturally in the dropper's emitted guard code.
        // Each lifts to `IrFormula::Atomic { name: "is_some" (or similar), args: [recv] }`.
        Expr::MethodCall(syn::ExprMethodCall {
            receiver,
            method,
            args,
            ..
        }) if args.is_empty() => {
            let method_name = method.to_string();
            let is_bool_predicate = matches!(
                method_name.as_str(),
                panic_freedom::IS_SOME
                    | panic_freedom::IS_NONE
                    | "is_empty"
                    | panic_freedom::IS_ERR
                    | panic_freedom::IS_OK
            );
            if is_bool_predicate {
                let recv_term = lift_expr_to_term_inner(receiver, ctx)?;
                Some(IrFormula::Atomic {
                    name: method_name,
                    args: vec![recv_term],
                })
            } else {
                None
            }
        }
        // Anything else is unrecognized in the MVP.
        _ => None,
    }
}

/// Lift a Rust expression to a canonical `IrTerm`. Supported shapes:
///   - Integer literal: `IrTerm::Const { value: <num>, sort: Int }`.
///   - Bool literal: `IrTerm::Const { value: <bool>, sort: Bool }`.
///   - Bare identifier: `IrTerm::Var { name: <ident> }`.
///   - Parenthesized expression: recurses on the inner expression.
///   - Reference (`&x`, `&mut x`): unwraps; for substrate purposes
///     a borrow is the value's identity (substitution-equivalent).
///   - Cast (`x as u32`): unwraps to the inner term (the IR's Sort
///     captures type changes; the term-level lift ignores casts).
///   - Field access (`s.f`): `Ctor("field", [s_term, "f"])`.
///   - Index (`a[i]`): `Ctor("index", [a_term, i_term])`.
///   - Method call (`x.foo(args)`): `Ctor("method:foo", [x, ...args])`.
///   - Range (`a..b`, `a..=b`): `Ctor("range", [a, b])` /
///     `Ctor("range_incl", [a, b])`.
///   - Tuple (`(a, b, c)`): `Ctor("tuple", [a, b, c])`.
///   - Unary negation (`-x`), bitwise not (`!x` on integers):
///     `Ctor("neg" / "bit-not", [...])`.
///   - Binary arithmetic (`+`, `-`, `*`, `/`, `%`, `&`, `|`, `^`,
///     `<<`, `>>`): lifts to `Ctor(<op>, [lhs, rhs])`.
///
/// Anything else returns None.
pub fn lift_expr_to_term(expr: &Expr) -> Option<IrTerm> {
    let mut ctx = LiftCtx::new();
    lift_expr_to_term_inner(expr, &mut ctx)
}

fn lift_expr_to_term_inner(expr: &Expr, ctx: &mut LiftCtx) -> Option<IrTerm> {
    match expr {
        Expr::Lit(lit) => match &lit.lit {
            Lit::Int(n) => match n.base10_parse::<i64>() {
                Ok(v) => Some(crate::wp::const_int(v)),
                Err(_) => None,
            },
            Lit::Bool(b) => Some(IrTerm::Const {
                value: serde_json::Value::Bool(b.value),
                sort: provekit_ir_types::Sort::Primitive {
                    name: "Bool".to_string(),
                },
            }),
            // A byte literal `b'0'` is its ASCII codepoint; lift as an int
            // const so byte-arithmetic tails (`byte - b'0'`) and byte match
            // arms lift instead of collapsing the whole term to None.
            Lit::Byte(b) => Some(crate::wp::const_int(b.value() as i64)),
            // A char literal `'a'` is its Unicode scalar value.
            Lit::Char(c) => Some(crate::wp::const_int(c.value() as i64)),
            // A string literal lifts to a String const so string-returning
            // tails (`"const"`) carry a real term.
            Lit::Str(s) => Some(IrTerm::Const {
                value: serde_json::Value::String(s.value()),
                sort: provekit_ir_types::Sort::Primitive {
                    name: "String".to_string(),
                },
            }),
            _ => None,
        },
        Expr::Path(syn::ExprPath { path, .. }) => {
            let seg = path.segments.last()?;
            // Resolve through the scope stack: bound names map to their
            // unique forms; free variables keep their surface name.
            Some(crate::wp::var(ctx.resolve(&seg.ident.to_string())))
        }
        Expr::Paren(p) => lift_expr_to_term_inner(&p.expr, ctx),
        Expr::Reference(r) => lift_expr_to_term_inner(&r.expr, ctx),
        Expr::Cast(c) => lift_expr_to_term_inner(&c.expr, ctx),
        Expr::Field(f) => {
            let base = lift_expr_to_term_inner(&f.base, ctx)?;
            let name = match &f.member {
                syn::Member::Named(id) => id.to_string(),
                syn::Member::Unnamed(idx) => idx.index.to_string(),
            };
            Some(IrTerm::Ctor {
                name: "field".to_string(),
                args: vec![
                    base,
                    IrTerm::Var {
                        name: format!(".{}", name),
                    },
                ],
            })
        }
        Expr::Index(i) => {
            let arr = lift_expr_to_term_inner(&i.expr, ctx)?;
            let idx = lift_expr_to_term_inner(&i.index, ctx)?;
            Some(IrTerm::Ctor {
                name: "index".to_string(),
                args: vec![arr, idx],
            })
        }
        Expr::MethodCall(m) => {
            let receiver = lift_expr_to_term_inner(&m.receiver, ctx)?;
            let mut args = vec![receiver.clone()];
            for a in &m.args {
                let lifted = lift_expr_to_term_inner(a, ctx)?;
                args.push(lifted);
            }
            let value = IrTerm::Ctor {
                name: format!("method:{}", m.method),
                args,
            };
            if let Some(guard) = assertion_guard_for_partial(&m.method, &receiver, ctx) {
                return Some(wrap_cf_guarded(guard, value));
            }
            if m.method == "unwrap"
                && m.args.is_empty()
                && receiver_as_str_is_known_json_string(&m.receiver, ctx)
            {
                if let IrTerm::Ctor { args, .. } = &value {
                    return Some(wrap_known_option_unwrap_guard(args[0].clone(), value));
                }
            }
            Some(value)
        }
        Expr::Range(r) => {
            let start = match &r.start {
                Some(e) => lift_expr_to_term_inner(e, ctx)?,
                None => crate::wp::var("_"),
            };
            let end = match &r.end {
                Some(e) => lift_expr_to_term_inner(e, ctx)?,
                None => crate::wp::var("_"),
            };
            let name = match r.limits {
                syn::RangeLimits::HalfOpen(_) => "range",
                syn::RangeLimits::Closed(_) => "range_incl",
            };
            Some(IrTerm::Ctor {
                name: name.to_string(),
                args: vec![start, end],
            })
        }
        Expr::Tuple(t) => {
            let mut args = Vec::with_capacity(t.elems.len());
            for e in &t.elems {
                args.push(lift_expr_to_term_inner(e, ctx)?);
            }
            Some(IrTerm::Ctor {
                name: "tuple".to_string(),
                args,
            })
        }
        Expr::Array(a) => {
            let mut args = Vec::with_capacity(a.elems.len());
            for e in &a.elems {
                args.push(lift_expr_to_term_inner(e, ctx)?);
            }
            Some(IrTerm::Ctor {
                name: "array".to_string(),
                args,
            })
        }
        Expr::Repeat(r) => {
            let elem = lift_expr_to_term_inner(&r.expr, ctx)?;
            let count = lift_expr_to_term_inner(&r.len, ctx)?;
            Some(IrTerm::Ctor {
                name: "array_repeat".to_string(),
                args: vec![elem, count],
            })
        }
        Expr::Closure(c) => {
            // `|x| body` lifts to IrTerm::Lambda. Multi-arg closures
            // collapse into nested lambdas (right-associative). Each
            // closure parameter is bound in a fresh scope frame and
            // assigned a globally-unique id by the LiftCtx; references
            // to that name inside the closure body resolve to the
            // unique form. The shadow AST's structural traversal owns
            // this name resolution.
            ctx.push_frame();
            let mut unique_names: Vec<String> = Vec::with_capacity(c.inputs.len());
            for (idx, input) in c.inputs.iter().enumerate() {
                // A closure param's binding NAME. Ident/typed-ident keep
                // their name. A wildcard `_` or a destructuring pattern
                // (`|(a, _)| ..`, common in `.map`/`.unwrap_or_else`) binds
                // a synthetic positional placeholder so the closure lifts
                // to a lambda instead of collapsing the whole tail to None.
                // The body references to a destructured name resolve as
                // free vars (not the placeholder), which is fine under the
                // reflexive encoding: the lambda term is uninterpreted and
                // discharges against the body's own identical lambda.
                let base = match input {
                    syn::Pat::Ident(p) => p.ident.to_string(),
                    syn::Pat::Type(pt) => match &*pt.pat {
                        syn::Pat::Ident(p) => p.ident.to_string(),
                        _ => format!("_closure_arg{idx}"),
                    },
                    _ => format!("_closure_arg{idx}"),
                };
                unique_names.push(ctx.bind(&base));
            }
            let body_lifted = lift_expr_to_term_inner(&c.body, ctx);
            ctx.pop_frame();
            let body = body_lifted?;
            let mut term = body;
            for unique in unique_names.into_iter().rev() {
                term = IrTerm::Lambda {
                    param_name: unique,
                    param_sort: provekit_ir_types::Sort::Primitive {
                        name: "Int".to_string(),
                    },
                    body: Box::new(term),
                };
            }
            Some(term)
        }
        Expr::Await(a) => {
            // `expr.await` desugars to a state machine that yields and
            // resumes; for substrate purposes it produces the awaited
            // value, so we lift as the inner expr.
            lift_expr_to_term_inner(&a.base, ctx)
        }
        Expr::Async(a) => {
            // `async { body }` produces a Future. The substrate sees
            // through to the body's eventual value: lift the trailing
            // expression of the block.
            if let Some(syn::Stmt::Expr(e, None)) = a.block.stmts.last() {
                lift_expr_to_term_inner(e, ctx)
            } else {
                None
            }
        }
        Expr::Call(call) => {
            // Plain function call `f(args)`. Lift to a ctor named by the
            // callee's bare symbol (the last path segment) so the call tree
            // SURVIVES into the contract formula. This is the keystone: the
            // ctor name matches the callee's auto-minted bridge `sourceSymbol`,
            // so `enumerate_callsites` finds the seam, `resolve_target` pulls
            // the callee's precondition, and the runner discharges
            // `producer_post -> callee_pre`. Without this arm the call tree
            // vanished and the postcondition collapsed to a vacuous `true` --
            // the missing edge was invisible to the solver. Mirrors the
            // `Expr::MethodCall` arm. Language-blind once emitted: the catch
            // lives in the verifier, below the source language.
            let Expr::Path(syn::ExprPath { path, .. }) = &*call.func else {
                // Calls through a non-path callee (closure value, fn pointer
                // in a local, etc.) have no stable bridge symbol to name.
                return None;
            };
            let callee = path.segments.last()?.ident.to_string();
            let mut args = Vec::with_capacity(call.args.len());
            for a in &call.args {
                args.push(lift_expr_to_term_inner(a, ctx)?);
            }
            Some(IrTerm::Ctor { name: callee, args })
        }
        Expr::If(if_expr) => {
            // A value-position `if`. Reuse the tail-if synthesis (it folds
            // `if c { a } else { b }` into `ite(c, a, b)`, and now also
            // handles if-without-else and stmt-bearing branch blocks).
            lift_tail_if_to_ite_term(if_expr, ctx)
        }
        Expr::Match(match_expr) => {
            // A value-position `match`. Fold it into a right-nested `ite`
            // chain keyed by each arm's recognized guard predicate. Every
            // arm value lifts (often to an uninterpreted ctor term), so the
            // whole match becomes one term that discharges reflexively when
            // it equals the body's own match. See `lift_match_to_ite_term`.
            lift_match_to_ite_term(match_expr, ctx)
        }
        Expr::Block(block_expr) => {
            // A block expression `{ ...; tail }` lifts to the lift of its
            // trailing expression (the block's value). Leading statements
            // do not contribute to the returned value term.
            let tail = block_expr.block.stmts.last()?;
            match tail {
                Stmt::Expr(e, None) => lift_expr_to_term_inner(e, ctx),
                _ => None,
            }
        }
        Expr::Try(t) => {
            // The `?` operator: `e?` evaluates `e` and unwraps its `Ok`
            // (or short-circuits on `Err`). For the returned-value term we
            // model it as an opaque unary `?` applied to the lifted inner
            // expression. Encoded as an uninterpreted function symbol by the
            // verifier, so `?(e) == ?(e)` discharges reflexively. Without
            // this arm any tail containing `?` collapsed the whole term to
            // None (mechanism (ii) in the body-discharge diagnostic).
            let inner = lift_expr_to_term_inner(&t.expr, ctx)?;
            Some(IrTerm::Ctor {
                name: "?".to_string(),
                args: vec![inner],
            })
        }
        Expr::Macro(m) => Some(lift_macro_to_opaque_term(&m.mac)),
        Expr::Struct(s) => {
            // A struct literal `Name { f: v, g: w, .. }`. Lift to a ctor
            // named by the struct path with the field VALUES as args. To
            // make the term canonical (independent of source field order)
            // the fields are sorted by name; the field names ride along as
            // opaque `#field:<name>` markers so two literals with the same
            // names+values produce the same term (reflexive) and differing
            // ones do not. A `..rest` base, if present, is appended as its
            // lifted term. Without this arm `Ok(Report { .. })` collapsed
            // the whole tail to None.
            let name = s
                .path
                .segments
                .last()
                .map(|seg| seg.ident.to_string())
                .unwrap_or_else(|| "struct".to_string());
            let mut fields: Vec<(&syn::Member, &Expr)> =
                s.fields.iter().map(|f| (&f.member, &f.expr)).collect();
            fields.sort_by_key(|(m, _)| match m {
                syn::Member::Named(id) => id.to_string(),
                syn::Member::Unnamed(idx) => format!("{}", idx.index),
            });
            let mut args = Vec::with_capacity(fields.len() * 2 + 1);
            for (member, value) in fields {
                let fname = match member {
                    syn::Member::Named(id) => id.to_string(),
                    syn::Member::Unnamed(idx) => idx.index.to_string(),
                };
                args.push(IrTerm::Var {
                    name: format!("#field:{fname}"),
                });
                args.push(lift_expr_to_term_inner(value, ctx)?);
            }
            if let Some(rest) = &s.rest {
                args.push(lift_expr_to_term_inner(rest, ctx)?);
            }
            Some(IrTerm::Ctor { name, args })
        }
        Expr::Binary(ExprBinary {
            left, op, right, ..
        }) => {
            let op_name = match op {
                BinOp::Add(_) => "+",
                BinOp::Sub(_) => "-",
                BinOp::Mul(_) => "*",
                BinOp::Div(_) => "/",
                BinOp::Rem(_) => "%",
                BinOp::BitAnd(_) => "&",
                BinOp::BitOr(_) => "|",
                BinOp::BitXor(_) => "^",
                BinOp::Shl(_) => "<<",
                BinOp::Shr(_) => ">>",
                // Boolean / relational operators in VALUE position (a
                // function returning `bool`, e.g. `!x.is_absolute() && ..`,
                // or `a == b`). Lift to a `cf_`-prefixed UNINTERPRETED head
                // (NOT the SMT builtins `and`/`or`/`=`/`<`): a builtin
                // demands Bool operands and a Bool result, but these sit
                // over uninterpreted Int-sorted value terms, so the builtin
                // raises a sort mismatch and the reflexive discharge fails.
                // As fresh uninterpreted symbols they discharge by
                // congruence: `cf_and(a,b) == cf_and(a,b)`. (Builtin
                // arithmetic comes from a different, substantive path and is
                // untouched.)
                BinOp::And(_) => "cf_and",
                BinOp::Or(_) => "cf_or",
                BinOp::Eq(_) => "cf_eq",
                BinOp::Ne(_) => "cf_ne",
                BinOp::Lt(_) => "cf_lt",
                BinOp::Le(_) => "cf_le",
                BinOp::Gt(_) => "cf_gt",
                BinOp::Ge(_) => "cf_ge",
                _ => return None,
            };
            let l = lift_expr_to_term_inner(left, ctx)?;
            let r = lift_expr_to_term_inner(right, ctx)?;
            Some(IrTerm::Ctor {
                name: op_name.to_string(),
                args: vec![l, r],
            })
        }
        Expr::Unary(ExprUnary { op, expr, .. }) => {
            let inner = lift_expr_to_term_inner(expr, ctx)?;
            let name = match op {
                UnOp::Neg(_) => {
                    if let IrTerm::Const { value, sort } = &inner {
                        if let Some(n) = value.as_i64() {
                            return Some(IrTerm::Const {
                                value: serde_json::json!(-n),
                                sort: sort.clone(),
                            });
                        }
                    }
                    "neg"
                }
                UnOp::Not(_) => "bit-not",
                UnOp::Deref(_) => return Some(inner), // *x is x for substitution
                _ => return None,
            };
            Some(IrTerm::Ctor {
                name: name.to_string(),
                args: vec![inner],
            })
        }
        _ => None,
    }
}

fn bin_op_to_predicate_name(op: &BinOp) -> Option<&'static str> {
    match op {
        BinOp::Eq(_) => Some("="),
        BinOp::Ne(_) => Some("≠"),
        BinOp::Lt(_) => Some("<"),
        BinOp::Le(_) => Some("≤"),
        BinOp::Gt(_) => Some(">"),
        BinOp::Ge(_) => Some("≥"),
        _ => None,
    }
}

fn block_only_panics(block: &syn::Block) -> bool {
    if block.stmts.len() != 1 {
        return false;
    }
    let stmt = &block.stmts[0];
    let mac: &Macro = match stmt {
        Stmt::Expr(Expr::Macro(ExprMacro { mac, .. }), _) => mac,
        Stmt::Macro(StmtMacro { mac, .. }) => mac,
        _ => return false,
    };
    mac.path
        .segments
        .last()
        .map(|s| s.ident == "panic")
        .unwrap_or(false)
}

fn negate(f: IrFormula) -> IrFormula {
    // Comparison flips: `if x < 10 panic` lifts to `x ≥ 10`, not `¬(x < 10)`.
    if let IrFormula::Atomic { name, args } = &f {
        let flipped = match name.as_str() {
            "<" => Some("≥"),
            "≤" => Some(">"),
            ">" => Some("≤"),
            "≥" => Some("<"),
            "=" => Some("≠"),
            "≠" => Some("="),
            _ => None,
        };
        if let Some(new_name) = flipped {
            return IrFormula::Atomic {
                name: new_name.to_string(),
                args: args.clone(),
            };
        }
    }
    // De Morgan's laws: push negation inward.
    //   ¬(a ∧ b) → ¬a ∨ ¬b
    //   ¬(a ∨ b) → ¬a ∧ ¬b
    // Sir's "every else is the contraposition" — when the lifter
    // produces the contraposition of `a && b` for an if-then-panic,
    // the result is `¬a ∨ ¬b`, not the harder-to-discharge `¬(a ∧ b)`.
    match f {
        IrFormula::And { operands } => IrFormula::Or {
            operands: operands.into_iter().map(negate).collect(),
        },
        IrFormula::Or { operands } => IrFormula::And {
            operands: operands.into_iter().map(negate).collect(),
        },
        IrFormula::Not { mut operands } if operands.len() == 1 => {
            // Double-negation elimination: ¬¬a → a.
            operands.pop().unwrap()
        }
        other => IrFormula::Not {
            operands: vec![other],
        },
    }
}

fn simplify_conjunction(parts: Vec<IrFormula>) -> IrFormula {
    if parts.is_empty() {
        IrFormula::Atomic {
            name: "true".to_string(),
            args: vec![],
        }
    } else if parts.len() == 1 {
        parts.into_iter().next().unwrap()
    } else {
        IrFormula::And { operands: parts }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wp::{atomic_ge, atomic_true, const_int, var};

    fn parse_fn(src: &str) -> ItemFn {
        let file: syn::File = syn::parse_str(src).expect("parses");
        file.items
            .into_iter()
            .find_map(|item| match item {
                syn::Item::Fn(f) => Some(f),
                _ => None,
            })
            .expect("function present")
    }

    #[test]
    fn lifts_if_then_panic_as_negated_condition() {
        let item_fn = parse_fn(
            r#"
            fn f(x: u32) -> u32 {
                if x < 10 {
                    panic!("x must be >= 10");
                }
                x * 2
            }
        "#,
        );
        let pre = lift_function_precondition(&item_fn);
        // ¬(x < 10) simplifies to x ≥ 10 via negate's comparison flip.
        assert_eq!(
            pre.as_formula(),
            atomic_ge(var("x"), const_int(10)).as_formula()
        );
    }

    #[test]
    fn lifts_assert_macro_as_predicate() {
        let item_fn = parse_fn(
            r#"
            fn g(x: u32) -> u32 {
                assert!(x >= 5);
                x * 3
            }
        "#,
        );
        let pre = lift_function_precondition(&item_fn);
        assert_eq!(
            pre.as_formula(),
            atomic_ge(var("x"), const_int(5)).as_formula()
        );
    }

    #[test]
    fn empty_body_lifts_to_vacuous_true() {
        let item_fn = parse_fn(r#"fn h() {}"#);
        let pre = lift_function_precondition(&item_fn);
        assert_eq!(pre.as_formula(), atomic_true().as_formula());
    }

    #[test]
    fn function_without_preconditions_lifts_to_true() {
        let item_fn = parse_fn(
            r#"
            fn k(x: u32) -> u32 {
                x + 1
            }
        "#,
        );
        let pre = lift_function_precondition(&item_fn);
        assert_eq!(pre.as_formula(), atomic_true().as_formula());
    }

    #[test]
    fn multiple_assertions_conjoin() {
        let item_fn = parse_fn(
            r#"
            fn m(x: u32, y: u32) -> u32 {
                assert!(x >= 1);
                assert!(y >= 2);
                x + y
            }
        "#,
        );
        let pre = lift_function_precondition(&item_fn);
        let expected = IrFormula::And {
            operands: vec![
                atomic_ge(var("x"), const_int(1)).into_formula(),
                atomic_ge(var("y"), const_int(2)).into_formula(),
            ],
        };
        assert_eq!(pre.as_formula(), &expected);
    }

    #[test]
    fn postcondition_derives_return_value_relation() {
        // f's body: `if x < 10 panic; x * 2`.
        // Derived post: `(x ≥ 10) ∧ (result = x * 2)`.
        // The first conjunct is the contraposition lifted from the
        // if-then-panic. The second is derived from the trailing
        // return expression `x * 2`.
        let item_fn = parse_fn(
            r#"
            fn f(x: u32) -> u32 {
                if x < 10 { panic!(); }
                x * 2
            }
        "#,
        );
        let post = lift_function_postcondition(&item_fn);
        let json = serde_json::to_string(post.as_formula()).unwrap();
        assert!(
            json.contains("\"≥\""),
            "post should include x ≥ 10: {}",
            json
        );
        // The return-value derivation: result = x * 2.
        assert!(
            json.contains("\"result\""),
            "post should include `result` variable: {}",
            json
        );
        assert!(
            json.contains("\"*\""),
            "post should include the multiplication ctor: {}",
            json
        );
    }

    #[test]
    fn binary_ops_lift_to_ctor_terms() {
        // `x + 5` lifts to Ctor("+", [Var("x"), Const(5)]).
        let item_fn = parse_fn(
            r#"
            fn k(x: u32) -> u32 {
                x + 5
            }
        "#,
        );
        let post = lift_function_postcondition(&item_fn);
        let json = serde_json::to_string(post.as_formula()).unwrap();
        assert!(
            json.contains("\"+\""),
            "post should encode the + ctor: {}",
            json
        );
        assert!(json.contains("\"x\""));
    }

    #[test]
    fn postcondition_for_tail_if_expression_is_branch_sensitive() {
        let item_fn = parse_fn(
            r#"
            fn foo(x: i32) -> i32 {
                if x == 0 { -22 } else { x }
            }
        "#,
        );
        let post = lift_function_postcondition(&item_fn).into_formula();
        let expected = IrFormula::Atomic {
            name: "=".to_string(),
            args: vec![
                IrTerm::Var {
                    name: "result".to_string(),
                },
                IrTerm::Ctor {
                    // Synthesized control-flow heads are `cf_`-prefixed
                    // (uninterpreted), not the SMT builtins `ite`/`=`, so
                    // they encode over uninterpreted operands without a
                    // sort mismatch and discharge reflexively.
                    name: panic_freedom::CF_ITE.to_string(),
                    args: vec![
                        IrTerm::Ctor {
                            name: "cf_eq".to_string(),
                            args: vec![var("x"), const_int(0)],
                        },
                        const_int(-22),
                        var("x"),
                    ],
                },
            ],
        };
        assert_eq!(post, expected);
    }

    #[test]
    fn lifts_or_condition_with_de_morgan() {
        // `if x < 10 || y < 5 panic` lifts to ¬(x<10 ∨ y<5)
        // which simplifies via De Morgan to (x≥10 ∧ y≥5).
        let item_fn = parse_fn(
            r#"
            fn h(x: u32, y: u32) -> u32 {
                if x < 10 || y < 5 {
                    panic!();
                }
                x * y
            }
        "#,
        );
        let pre = lift_function_precondition(&item_fn);
        let json = serde_json::to_string(pre.as_formula()).unwrap();
        // Expect an `and` of two `≥` atoms (De Morgan applied + comparison flips).
        assert!(
            json.contains("\"and\""),
            "pre should be a conjunction: {}",
            json
        );
        assert!(
            json.contains("\"≥\""),
            "pre should contain ≥ atoms: {}",
            json
        );
        assert!(json.contains("\"x\"") && json.contains("\"y\""));
    }

    #[test]
    fn lifts_and_condition_with_de_morgan() {
        // `if x < 10 && y < 5 panic` lifts to ¬(x<10 ∧ y<5)
        // which is (x≥10 ∨ y≥5).
        let item_fn = parse_fn(
            r#"
            fn h(x: u32, y: u32) -> u32 {
                if x < 10 && y < 5 {
                    panic!();
                }
                x + y
            }
        "#,
        );
        let pre = lift_function_precondition(&item_fn);
        let json = serde_json::to_string(pre.as_formula()).unwrap();
        // Expect an `or` of two `≥` atoms.
        assert!(
            json.contains("\"or\""),
            "pre should be a disjunction: {}",
            json
        );
        assert!(
            json.contains("\"≥\""),
            "pre should contain ≥ atoms: {}",
            json
        );
    }

    #[test]
    fn double_negation_eliminated() {
        // `if !(x >= 10) panic` is equivalent to `if x < 10 panic`.
        // The lifter should produce `x ≥ 10` (not `¬¬(x ≥ 10)`).
        let item_fn = parse_fn(
            r#"
            fn n(x: u32) -> u32 {
                if !(x >= 10) {
                    panic!();
                }
                x * 2
            }
        "#,
        );
        let pre = lift_function_precondition(&item_fn);
        let json = serde_json::to_string(pre.as_formula()).unwrap();
        // The atomic ≥ should appear directly, with no `not` wrapper.
        assert!(
            json.contains("\"≥\""),
            "pre should contain ≥ atom: {}",
            json
        );
        assert!(
            !json.contains("\"not\""),
            "pre should NOT contain a `not` wrapper (double negation eliminated): {}",
            json
        );
    }

    // ---- shadow-AST scope tracking ----

    #[test]
    fn lift_closure_assigns_unique_param_id() {
        // |x| x  ->  Lambda { x#0, body: Var x#0 }
        let expr: Expr = syn::parse_str("|x| x").unwrap();
        let term = lift_expr_to_term(&expr).unwrap();
        match term {
            IrTerm::Lambda {
                param_name, body, ..
            } => {
                assert!(
                    param_name.starts_with("x#"),
                    "expected x#N, got {}",
                    param_name
                );
                match *body {
                    IrTerm::Var { name } => assert_eq!(
                        name, param_name,
                        "body's `x` must resolve to the closure's unique id"
                    ),
                    other => panic!("expected Var, got {:?}", other),
                }
            }
            other => panic!("expected Lambda, got {:?}", other),
        }
    }

    #[test]
    fn lift_nested_closures_get_distinct_ids() {
        // |x| |x| x  ->  the inner `x` shadows the outer; the inner's
        // unique id is what the body resolves to.
        let expr: Expr = syn::parse_str("|x| |x| x").unwrap();
        let term = lift_expr_to_term(&expr).unwrap();
        match term {
            IrTerm::Lambda {
                param_name: outer,
                body,
                ..
            } => match *body {
                IrTerm::Lambda {
                    param_name: inner,
                    body: inner_body,
                    ..
                } => {
                    assert_ne!(outer, inner, "outer and inner ids must differ");
                    assert!(outer.starts_with("x#"));
                    assert!(inner.starts_with("x#"));
                    match *inner_body {
                        IrTerm::Var { name } => {
                            assert_eq!(name, inner, "innermost binding wins");
                        }
                        other => panic!("expected Var, got {:?}", other),
                    }
                }
                other => panic!("expected nested Lambda, got {:?}", other),
            },
            other => panic!("expected Lambda, got {:?}", other),
        }
    }

    #[test]
    fn lift_free_variable_keeps_original_name() {
        // Bare `y` lifts to Var("y") — y is free in this context.
        let expr: Expr = syn::parse_str("y").unwrap();
        let term = lift_expr_to_term(&expr).unwrap();
        match term {
            IrTerm::Var { name } => {
                assert_eq!(name, "y", "free variable keeps surface name");
            }
            other => panic!("expected Var, got {:?}", other),
        }
    }

    #[test]
    fn lift_closure_does_not_capture_outer_reference() {
        // |x| (x + y)  -- inside the closure, x is bound, y is free.
        let expr: Expr = syn::parse_str("|x| x + y").unwrap();
        let term = lift_expr_to_term(&expr).unwrap();
        match term {
            IrTerm::Lambda {
                param_name, body, ..
            } => {
                assert!(param_name.starts_with("x#"));
                let json = serde_json::to_string(&body).unwrap();
                // Body should reference the unique x#N for x and bare "y" for y.
                assert!(
                    json.contains(&format!("\"{}\"", param_name)),
                    "body should reference the unique x: {}",
                    json
                );
                assert!(
                    json.contains("\"y\""),
                    "body should reference free y unchanged: {}",
                    json
                );
            }
            other => panic!("expected Lambda, got {:?}", other),
        }
    }

    #[test]
    fn lift_function_with_closure_keeps_formal_unscoped() {
        // fn f(x) { let _ = |x| x; x }
        // The trailing `x` is the function's formal — should be plain "x".
        // The closure's body `x` is the closure's param — should be "x#N".
        let item_fn = parse_fn(
            r#"
            fn f(x: u32) -> u32 {
                let _ = |x: u32| x;
                x
            }
        "#,
        );
        let post = lift_function_postcondition(&item_fn);
        let json = serde_json::to_string(post.as_formula()).unwrap();
        // The trailing-expression derivation gives `result = x` (plain x).
        assert!(
            json.contains("\"result\""),
            "post should derive result = x: {}",
            json
        );
        // The plain "x" formal (not "x#N") should appear.
        assert!(
            json.contains("\"x\""),
            "post should reference the formal x: {}",
            json
        );
    }

    // ---- bug-fix regression tests ----

    #[test]
    fn call_expr_body_lifts_call_tree_into_postcondition() {
        // A function whose body is a nested call must derive a post that
        // CONTAINS the call tree as ctor terms. Otherwise the callees are
        // invisible to `enumerate_callsites` and the missing-edge seam can
        // never be discharged (it was the false-green hole: this collapsed to
        // a vacuous `true` because `Expr::Call` had no lift arm).
        let item_fn = parse_fn(
            r#"
            fn address_of(value: i64) -> i64 {
                content_address(serialize(value))
            }
        "#,
        );
        let post = lift_function_postcondition(&item_fn);
        let json = serde_json::to_string(post.as_formula()).unwrap();
        assert!(
            json.contains("\"result\""),
            "post must derive result = <body>: {json}"
        );
        assert!(
            json.contains("content_address"),
            "post must contain the outer call ctor `content_address`: {json}"
        );
        assert!(
            json.contains("serialize"),
            "post must contain the nested call ctor `serialize`: {json}"
        );
    }

    #[test]
    fn debug_assert_not_lifted_to_postcondition() {
        // Bug #5: `debug_assert!` is compiled out in release builds.
        // It must NOT contribute to the postcondition.
        let item_fn = parse_fn(
            r#"
            fn f(x: u32) -> u32 {
                debug_assert!(x >= 5);
                x * 2
            }
        "#,
        );
        let post = lift_function_postcondition(&item_fn);
        let json = serde_json::to_string(post.as_formula()).unwrap();
        // The postcondition should NOT include the debug_assert predicate.
        // It should only have the trailing-expression derivation.
        assert!(
            !json.contains("\"≥\""),
            "debug_assert! must NOT appear in postcondition: {}",
            json
        );
        // The trailing `x * 2` should still derive a result postcondition.
        assert!(
            json.contains("\"result\""),
            "postcondition should still include result = x * 2: {}",
            json
        );
    }

    #[test]
    fn assert_shadowed_by_later_let_dropped_from_postcondition() {
        // Bug #6: `assert!(x >= 5); let x = 0; x` — the assert refers to
        // the original `x`, but `let x = 0` rebinds `x` afterward.
        // The assert is UNSOUND in the postcondition and must be dropped.
        let item_fn = parse_fn(
            r#"
            fn f(x: u32) -> u32 {
                assert!(x >= 5);
                let x = 0u32;
                x
            }
        "#,
        );
        let post = lift_function_postcondition(&item_fn);
        let json = serde_json::to_string(post.as_formula()).unwrap();
        // The original assert!(x >= 5) is unsound after let x = 0.
        // It must NOT appear in the postcondition.
        assert!(
            !json.contains("\"≥\""),
            "shadowed assert! must NOT appear in postcondition: {}",
            json
        );
    }

    #[test]
    fn assert_not_shadowed_stays_in_postcondition() {
        // Bug #6 complementary: when no later `let` shadows the assert's
        // free variables, the assert correctly stays in the postcondition.
        let item_fn = parse_fn(
            r#"
            fn f(x: u32, y: u32) -> u32 {
                assert!(x >= 5);
                let z = 0u32;   // shadows `z`, not `x`
                x + y
            }
        "#,
        );
        let post = lift_function_postcondition(&item_fn);
        let json = serde_json::to_string(post.as_formula()).unwrap();
        // `x` is NOT rebound; the assert should remain.
        assert!(
            json.contains("\"≥\""),
            "non-shadowed assert! should remain in postcondition: {}",
            json
        );
    }

    #[test]
    fn explicit_return_derives_result_postcondition() {
        // Bug #7: `fn f() -> i32 { return x + 1; }` must derive
        // `result = x + 1` in the postcondition.
        let item_fn = parse_fn(
            r#"
            fn f(x: i32) -> i32 {
                return x + 1;
            }
        "#,
        );
        let post = lift_function_postcondition(&item_fn);
        let json = serde_json::to_string(post.as_formula()).unwrap();
        assert!(
            json.contains("\"result\""),
            "explicit return expr must derive result = ...: {}",
            json
        );
        assert!(
            json.contains("\"+\""),
            "explicit return expr must include the + ctor: {}",
            json
        );
        assert!(
            json.contains("\"x\""),
            "explicit return expr must reference x: {}",
            json
        );
    }

    #[test]
    fn lifts_is_none_method_call_as_atomic_predicate() {
        // `if x.is_none() { panic!() }` lifts to is_none(x) at the
        // precondition (via the if-then-panic path: ¬panic_cond = ¬is_none(x)).
        // This is the shape the dropper emits for the Defensive template.
        let item_fn = parse_fn(
            r#"
            fn caller(x: Option<i32>) {
                if x.is_none() { panic!("not_null: x must be Some"); }
            }
        "#,
        );
        let pre = lift_function_precondition(&item_fn);
        let json = serde_json::to_string(pre.as_formula()).unwrap();
        // The if-then-panic lifts ¬is_none(x) as the precondition.
        // The JSON should contain "is_none" to confirm the method-call lift fired.
        assert!(
            json.contains("is_none"),
            "if x.is_none() panic must lift is_none to precondition: {}",
            json
        );
    }

    #[test]
    fn lifts_is_some_method_call_as_atomic_predicate() {
        // `if !x.is_some() { panic!() }` (double-negation via De Morgan)
        // should also lift correctly.
        let item_fn = parse_fn(
            r#"
            fn caller(x: Option<i32>) {
                if !x.is_some() { panic!("not_null: x must be Some"); }
            }
        "#,
        );
        let pre = lift_function_precondition(&item_fn);
        let json = serde_json::to_string(pre.as_formula()).unwrap();
        assert!(
            json.contains("is_some"),
            "if !x.is_some() panic must lift is_some to precondition: {}",
            json
        );
    }

    /// The post's `result == <value>` equation, or None if the body has
    /// no result term (genuinely unit-returning).
    fn result_equation_of(item_fn: &ItemFn) -> Option<IrTerm> {
        let post = lift_function_postcondition(item_fn);
        libprovekit::wp::find_result_equation(post.as_formula(), "result")
    }

    #[test]
    fn ok_struct_literal_tail_lifts_a_result_equation() {
        // `Ok(Report { code: 0 })` formerly collapsed to None (the struct
        // literal child had no lift arm). The new `Expr::Struct` arm lifts
        // it; the whole tail becomes `Ok(Report(...))`, a `result ==` post
        // that discharges reflexively.
        let item_fn = parse_fn(
            r#"
            fn make() -> Result<Report, ()> {
                Ok(Report { code: 0 })
            }
        "#,
        );
        let eq = result_equation_of(&item_fn);
        assert!(
            eq.is_some(),
            "Ok(StructLiteral{{..}}) tail must synthesize a result equation"
        );
        let json = serde_json::to_string(&eq.unwrap()).unwrap();
        assert!(
            json.contains("Ok"),
            "result term must carry the Ok ctor: {json}"
        );
        assert!(
            json.contains("Report"),
            "result term must carry the struct ctor: {json}"
        );
    }

    #[test]
    fn pathbuf_from_format_macro_tail_lifts_a_result_equation() {
        // `PathBuf::from(format!("{}", x))`: a call whose argument is a
        // `format!` macro. The macro arm encodes it as an opaque
        // `macro:format#<hash>` ctor instead of collapsing the whole tail
        // to None (diagnostic mechanism (ii)).
        let item_fn = parse_fn(
            r#"
            fn p(x: i64) -> String {
                from(format!("{}", x))
            }
        "#,
        );
        let eq = result_equation_of(&item_fn);
        assert!(
            eq.is_some(),
            "call wrapping a format! macro must synthesize a result equation"
        );
        let json = serde_json::to_string(&eq.unwrap()).unwrap();
        assert!(
            json.contains("macro:format#"),
            "the format! macro must lift to an opaque keyed ctor: {json}"
        );
    }

    #[test]
    fn match_tail_lifts_to_ite_chain_result_equation() {
        // A value-position `match` now folds to an `ite` chain rather than
        // collapsing to None (diagnostic mechanism (i)).
        let item_fn = parse_fn(
            r#"
            fn classify(x: i64) -> i64 {
                match x {
                    0 => zero(),
                    _ => other(),
                }
            }
        "#,
        );
        let eq = result_equation_of(&item_fn);
        assert!(eq.is_some(), "match tail must synthesize a result equation");
        let json = serde_json::to_string(&eq.unwrap()).unwrap();
        assert!(
            json.contains("cf_ite"),
            "match must lift to a cf_ite chain: {json}"
        );
    }

    #[test]
    fn try_operator_tail_lifts_a_result_equation() {
        // `serialize(x)?` (the `?` operator) formerly collapsed the tail.
        let item_fn = parse_fn(
            r#"
            fn t(x: i64) -> Result<i64, ()> {
                Ok(serialize(x)?)
            }
        "#,
        );
        let eq = result_equation_of(&item_fn);
        assert!(
            eq.is_some(),
            "tail containing `?` must synthesize a result equation"
        );
        let json = serde_json::to_string(&eq.unwrap()).unwrap();
        assert!(
            json.contains("\"?\""),
            "`?` must lift to an opaque `?` ctor: {json}"
        );
    }

    #[test]
    fn macro_token_hash_is_deterministic_and_distinguishing() {
        // The SAME macro call (same name + tokens) lifts to the SAME term
        // (so reflexive equality holds); a DIFFERENT call lifts to a
        // different term (so distinct calls do not spuriously unify).
        let a1: syn::Macro = syn::parse_quote!(format!("{}", x));
        let a2: syn::Macro = syn::parse_quote!(format!("{}", x));
        let b: syn::Macro = syn::parse_quote!(format!("{}", y));
        let ta = lift_macro_to_opaque_term(&a1);
        let ta2 = lift_macro_to_opaque_term(&a2);
        let tb = lift_macro_to_opaque_term(&b);
        assert_eq!(
            ta, ta2,
            "identical macro calls must lift to the same opaque term"
        );
        assert_ne!(
            ta, tb,
            "different macro calls must lift to different opaque terms"
        );
    }

    // ---- PANIC-FREEDOM guard resolution lives in the Rust kit ----
    //
    // The complement table (`is_some` <-> `is_none`, `is_ok` <-> `is_err`,
    // `is_empty`) was RELOCATED here from the verifier so the verifier stays
    // language-blind. These tests pin the Rust-std semantics: the then-branch
    // carries the POSITIVE predicate, the else-branch the COMPLEMENT, and an
    // unrecognized guard wraps NOTHING (fail-safe).

    #[test]
    fn branch_guard_head_then_is_positive_else_is_complement() {
        // The full complement table, both branches.
        assert_eq!(branch_guard_head("is_some", false), Some("is_some"));
        assert_eq!(branch_guard_head("is_some", true), Some("is_none"));
        assert_eq!(branch_guard_head("is_none", false), Some("is_none"));
        assert_eq!(branch_guard_head("is_none", true), Some("is_some"));
        assert_eq!(branch_guard_head("is_ok", false), Some("is_ok"));
        assert_eq!(branch_guard_head("is_ok", true), Some("is_err"));
        assert_eq!(branch_guard_head("is_err", false), Some("is_err"));
        assert_eq!(branch_guard_head("is_err", true), Some("is_ok"));
        assert_eq!(branch_guard_head("is_empty", false), Some("is_empty"));
        // The method-call form `opt.is_some()` lifts to `method:is_some`; it must
        // normalize to the bare `is_some` that the partial's pre uses, or the
        // syntactic discharge `guard => pre` never matches.
        assert_eq!(branch_guard_head("method:is_some", false), Some("is_some"));
        assert_eq!(branch_guard_head("method:is_some", true), Some("is_none"));
        assert_eq!(branch_guard_head("method:is_ok", false), Some("is_ok"));
        assert_eq!(branch_guard_head("method:is_err", true), Some("is_ok"));
    }

    #[test]
    fn branch_guard_head_refuses_unrecognized_and_negated_is_empty() {
        // `!is_empty` establishes no partial's pre -> no guard.
        assert_eq!(branch_guard_head("is_empty", true), None);
        // Comparisons / conjunctions / method guards are not partial-pre
        // predicates -> no guard, so a partial inside stays undecidable.
        assert_eq!(branch_guard_head("cf_lt", false), None);
        assert_eq!(branch_guard_head("cf_and", false), None);
        assert_eq!(branch_guard_head("match_guard", false), None);
        assert_eq!(branch_guard_head("method:is_absolute", false), None);
    }

    #[test]
    fn wrap_branch_guard_then_carries_positive_else_carries_complement() {
        // Condition `is_some(x)`. The then-branch must wrap to
        // cf_guarded(is_some(x), value); the else-branch to
        // cf_guarded(is_none(x), value) -- the kit-computed complement.
        let recv = IrTerm::Var { name: "x".into() };
        let cond = IrTerm::Ctor {
            name: "is_some".into(),
            args: vec![recv.clone()],
        };
        let val = || IrTerm::Var { name: "v".into() };

        let then_t = wrap_branch_guard(&cond, false, val());
        let else_t = wrap_branch_guard(&cond, true, val());

        match &then_t {
            IrTerm::Ctor { name, args } => {
                assert_eq!(name, "cf_guarded");
                match &args[0] {
                    IrTerm::Ctor { name, args } => {
                        assert_eq!(
                            name, "is_some",
                            "then-branch carries the POSITIVE predicate"
                        );
                        assert_eq!(args, &vec![recv.clone()], "guard names the receiver term");
                    }
                    other => panic!("then guard not a ctor: {other:?}"),
                }
            }
            other => panic!("then not cf_guarded: {other:?}"),
        }
        match &else_t {
            IrTerm::Ctor { name, args } => {
                assert_eq!(name, "cf_guarded");
                match &args[0] {
                    IrTerm::Ctor { name, .. } => assert_eq!(
                        name, "is_none",
                        "else-branch carries the COMPLEMENT (the trap: never is_some)"
                    ),
                    other => panic!("else guard not a ctor: {other:?}"),
                }
            }
            other => panic!("else not cf_guarded: {other:?}"),
        }
    }

    #[test]
    fn wrap_branch_guard_unrecognized_condition_wraps_nothing() {
        // A comparison condition (`cf_lt(...)`) is not a partial-pre predicate:
        // the branch value passes through UNCHANGED (no cf_guarded), so a
        // partial inside it stays undecidable and the cf_ite is byte-stable.
        let cond = IrTerm::Ctor {
            name: "cf_lt".into(),
            args: vec![IrTerm::Var { name: "x".into() }, const_int(10)],
        };
        let val = IrTerm::Var { name: "v".into() };
        let wrapped = wrap_branch_guard(&cond, false, val.clone());
        assert_eq!(wrapped, val, "unrecognized guard must wrap nothing");
        // A method-call condition likewise.
        let mcond = IrTerm::Ctor {
            name: "method:is_absolute".into(),
            args: vec![IrTerm::Var { name: "p".into() }],
        };
        assert_eq!(
            wrap_branch_guard(&mcond, true, val.clone()),
            val,
            "a method guard must wrap nothing"
        );
    }

    #[test]
    fn if_is_some_lifts_then_to_cf_guarded_is_some_else_to_is_none() {
        // End-to-end through the tail-if lifter: an `if opt.is_some()` produces
        // a cf_ite whose then-branch is cf_guarded(is_some(opt), ..) and whose
        // else-branch is cf_guarded(is_none(opt), ..).
        let item_fn = parse_fn(
            r#"
            fn f(opt: Option<i64>) -> i64 {
                if opt.is_some() {
                    opt.unwrap()
                } else {
                    0
                }
            }
        "#,
        );
        let eq = result_equation_of(&item_fn).expect("if-tail must synthesize a result equation");
        let json = serde_json::to_string(&eq).unwrap();
        assert!(json.contains("cf_ite"), "must lift to a cf_ite: {json}");
        assert!(
            json.contains("cf_guarded"),
            "guarded branches must carry cf_guarded wrappers: {json}"
        );
        assert!(
            json.contains("is_some"),
            "then-branch guard is is_some: {json}"
        );
        assert!(
            json.contains("is_none"),
            "else-branch guard is the complement is_none: {json}"
        );
    }

    #[test]
    fn assert_is_some_guards_later_option_unwrap() {
        let item_fn = parse_fn(
            r#"
            fn f(x: Option<i64>) -> i64 {
                assert!(x.is_some());
                x.unwrap()
            }
        "#,
        );
        let eq = result_equation_of(&item_fn).expect("unwrap must remain in result term");
        let json = serde_json::to_string(&eq).unwrap();
        assert!(
            json.contains("cf_guarded"),
            "assert!(x.is_some()) must guard the later unwrap: {json}"
        );
        assert!(
            json.contains("is_some"),
            "guard must carry the option precondition: {json}"
        );
    }

    #[test]
    fn assert_is_ok_guards_later_result_unwrap() {
        let item_fn = parse_fn(
            r#"
            fn f(r: Result<i64, String>) -> i64 {
                assert!(r.is_ok());
                r.unwrap()
            }
        "#,
        );
        let eq = result_equation_of(&item_fn).expect("unwrap must remain in result term");
        let json = serde_json::to_string(&eq).unwrap();
        assert!(
            json.contains("cf_guarded"),
            "assert!(r.is_ok()) must guard the later unwrap: {json}"
        );
        assert!(
            json.contains("is_ok"),
            "guard must carry the result precondition: {json}"
        );
    }

    #[test]
    fn assert_is_ok_guards_later_result_expect() {
        let item_fn = parse_fn(
            r#"
            fn f(r: Result<i64, String>) -> i64 {
                assert!(r.is_ok());
                r.expect("present")
            }
        "#,
        );
        let eq = result_equation_of(&item_fn).expect("expect must remain in result term");
        let json = serde_json::to_string(&eq).unwrap();
        assert!(
            json.contains("cf_guarded"),
            "assert!(r.is_ok()) must guard the later expect: {json}"
        );
        assert!(
            json.contains("is_ok"),
            "guard must carry the result precondition: {json}"
        );
    }

    #[test]
    fn len_eq_one_guards_into_iter_next_unwrap() {
        let item_fn = parse_fn(
            r#"
            fn f(values: Vec<i64>) -> i64 {
                assert!(values.len() == 1);
                values.into_iter().next().unwrap()
            }
        "#,
        );
        let eq = result_equation_of(&item_fn).expect("unwrap must remain in result term");
        let json = serde_json::to_string(&eq).unwrap();
        assert!(
            json.contains("cf_guarded"),
            "len == 1 must guard into_iter().next().unwrap(): {json}"
        );
        assert!(
            json.contains("is_some"),
            "guard must prove next() returns Some: {json}"
        );
        assert!(
            json.contains("method:next"),
            "guard must name the same next() receiver term: {json}"
        );
    }

    #[test]
    fn if_len_eq_one_guards_then_branch_into_iter_next_unwrap() {
        let item_fn = parse_fn(
            r#"
            fn f(values: Vec<i64>) -> Option<i64> {
                if values.len() == 1 {
                    Some(values.into_iter().next().unwrap())
                } else {
                    None
                }
            }
        "#,
        );
        let eq = result_equation_of(&item_fn).expect("if-tail must synthesize a result equation");
        let json = serde_json::to_string(&eq).unwrap();
        assert!(json.contains("cf_ite"), "must lift to a cf_ite: {json}");
        assert!(
            json.contains("cf_guarded"),
            "positive len == 1 branch must guard next().unwrap(): {json}"
        );
        assert!(
            json.contains("is_some"),
            "guard must prove next() returns Some: {json}"
        );
        assert!(
            json.contains("method:next"),
            "guard must name the next() receiver term: {json}"
        );
    }

    #[test]
    fn if_len_eq_one_wrong_collection_does_not_guard_next_unwrap() {
        let item_fn = parse_fn(
            r#"
            fn f(a: Vec<i64>, b: Vec<i64>) -> Option<i64> {
                if a.len() == 1 {
                    Some(b.into_iter().next().unwrap())
                } else {
                    None
                }
            }
        "#,
        );
        let eq = result_equation_of(&item_fn).expect("if-tail must synthesize a result equation");
        let json = serde_json::to_string(&eq).unwrap();
        assert!(
            !json.contains("cf_guarded"),
            "len fact for a must not guard b.into_iter().next().unwrap(): {json}"
        );
    }

    #[test]
    fn if_compound_len_eq_one_condition_does_not_guard_next_unwrap() {
        let item_fn = parse_fn(
            r#"
            fn f(values: Vec<i64>, ready: bool) -> Option<i64> {
                if values.len() == 1 && ready {
                    Some(values.into_iter().next().unwrap())
                } else {
                    None
                }
            }
        "#,
        );
        let eq = result_equation_of(&item_fn).expect("if-tail must synthesize a result equation");
        let json = serde_json::to_string(&eq).unwrap();
        assert!(
            !json.contains("cf_guarded"),
            "compound conditions are outside the audited len==1 shape: {json}"
        );
    }

    #[test]
    fn if_len_not_one_does_not_guard_next_unwrap() {
        let item_fn = parse_fn(
            r#"
            fn f(values: Vec<i64>) -> Option<i64> {
                if values.len() != 0 {
                    Some(values.into_iter().next().unwrap())
                } else {
                    None
                }
            }
        "#,
        );
        let eq = result_equation_of(&item_fn).expect("if-tail must synthesize a result equation");
        let json = serde_json::to_string(&eq).unwrap();
        assert!(
            !json.contains("cf_guarded"),
            "len != 0 does not establish the exact len==1 iterator fact: {json}"
        );
    }

    #[test]
    fn no_assert_does_not_guard_option_unwrap() {
        let item_fn = parse_fn(
            r#"
            fn f(x: Option<i64>) -> i64 {
                x.unwrap()
            }
        "#,
        );
        let eq = result_equation_of(&item_fn).expect("unwrap must remain in result term");
        let json = serde_json::to_string(&eq).unwrap();
        assert!(
            !json.contains("cf_guarded"),
            "unguarded unwrap must remain honestly undecidable: {json}"
        );
    }

    #[test]
    fn unrelated_assert_does_not_guard_option_unwrap() {
        let item_fn = parse_fn(
            r#"
            fn f(x: Option<i64>, ready: bool) -> i64 {
                assert!(ready);
                x.unwrap()
            }
        "#,
        );
        let eq = result_equation_of(&item_fn).expect("unwrap must remain in result term");
        let json = serde_json::to_string(&eq).unwrap();
        assert!(
            !json.contains("cf_guarded"),
            "unrelated assert must not guard x.unwrap(): {json}"
        );
    }

    #[test]
    fn different_receiver_assert_does_not_guard_option_unwrap() {
        let item_fn = parse_fn(
            r#"
            fn f(x: Option<i64>, y: Option<i64>) -> i64 {
                assert!(y.is_some());
                x.unwrap()
            }
        "#,
        );
        let eq = result_equation_of(&item_fn).expect("unwrap must remain in result term");
        let json = serde_json::to_string(&eq).unwrap();
        assert!(
            !json.contains("cf_guarded"),
            "assert about y must not guard x.unwrap(): {json}"
        );
    }

    #[test]
    fn mutation_after_assert_does_not_guard_option_unwrap() {
        let item_fn = parse_fn(
            r#"
            fn f(mut x: Option<i64>) -> i64 {
                assert!(x.is_some());
                x = None;
                x.unwrap()
            }
        "#,
        );
        let eq = result_equation_of(&item_fn).expect("unwrap must remain in result term");
        let json = serde_json::to_string(&eq).unwrap();
        assert!(
            !json.contains("cf_guarded"),
            "mutation after assert must invalidate the guard fact: {json}"
        );
    }

    #[test]
    fn branch_local_assert_does_not_guard_outer_unwrap() {
        let item_fn = parse_fn(
            r#"
            fn f(x: Option<i64>, cond: bool) -> i64 {
                if cond {
                    assert!(x.is_some());
                }
                x.unwrap()
            }
        "#,
        );
        let eq = result_equation_of(&item_fn).expect("unwrap must remain in result term");
        let json = serde_json::to_string(&eq).unwrap();
        assert!(
            !json.contains("cf_guarded"),
            "branch-local assert must not guard an outer unwrap: {json}"
        );
    }

    #[test]
    fn json_string_field_unwrap_carries_is_some_guard_fact() {
        let item_fn = parse_fn(
            r#"
            fn f(from_catalog_cid: String) -> String {
                let body = json!({
                    "fromCatalogCid": from_catalog_cid
                });
                let payload = json!({
                    "fromCatalogCid": body["fromCatalogCid"].clone()
                });
                payload["fromCatalogCid"].as_str().unwrap().to_string()
            }
        "#,
        );
        let eq = result_equation_of(&item_fn).expect("json unwrap must remain in result term");
        let json = serde_json::to_string(&eq).unwrap();
        assert!(
            json.contains("cf_guarded"),
            "json string construction must carry an opaque guard fact: {json}"
        );
        assert!(
            json.contains("is_some"),
            "json string construction must prove as_str() is Some: {json}"
        );
        assert!(
            json.contains("fromCatalogCid"),
            "guard must name the same indexed JSON field: {json}"
        );
    }

    #[test]
    fn dynamic_json_construction_does_not_carry_guard_fact() {
        let item_fn = parse_fn(
            r#"
            fn f() -> String {
                let payload = make_payload();
                payload["value"].as_str().unwrap().to_string()
            }
        "#,
        );
        let eq = result_equation_of(&item_fn).expect("dynamic unwrap must remain in result term");
        let json = serde_json::to_string(&eq).unwrap();
        assert!(
            !json.contains("cf_guarded"),
            "opaque dynamic construction must stay honestly unguarded: {json}"
        );
    }

    #[test]
    fn numeric_json_field_does_not_carry_string_guard_fact() {
        let item_fn = parse_fn(
            r#"
            fn f() -> String {
                let payload = json!({
                    "value": 7
                });
                payload["value"].as_str().unwrap().to_string()
            }
        "#,
        );
        let eq =
            result_equation_of(&item_fn).expect("wrong-type unwrap must remain in result term");
        let json = serde_json::to_string(&eq).unwrap();
        assert!(
            !json.contains("cf_guarded"),
            "numeric JSON fields must not prove as_str() is Some: {json}"
        );
    }

    #[test]
    fn mutable_json_binding_does_not_carry_guard_fact() {
        let item_fn = parse_fn(
            r#"
            fn f() -> String {
                let mut payload = json!({
                    "value": "ok"
                });
                payload["value"].as_str().unwrap().to_string()
            }
        "#,
        );
        let eq =
            result_equation_of(&item_fn).expect("mutable JSON unwrap must remain in result term");
        let json = serde_json::to_string(&eq).unwrap();
        assert!(
            !json.contains("cf_guarded"),
            "mutable JSON construction must stay honestly unguarded: {json}"
        );
    }

    #[test]
    fn unknown_json_field_propagation_does_not_carry_guard_fact() {
        let item_fn = parse_fn(
            r#"
            fn f() -> String {
                let body = json!({
                    "value": opaque()
                });
                let payload = json!({
                    "value": body["value"].clone()
                });
                payload["value"].as_str().unwrap().to_string()
            }
        "#,
        );
        let eq =
            result_equation_of(&item_fn).expect("unknown propagated unwrap must remain in term");
        let json = serde_json::to_string(&eq).unwrap();
        assert!(
            !json.contains("cf_guarded"),
            "unknown field kind must remain unknown through clone propagation: {json}"
        );
    }
}
