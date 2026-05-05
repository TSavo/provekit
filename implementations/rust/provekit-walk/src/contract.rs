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
use crate::locus::Locus;
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
    /// Contains a loop whose invariant has not been supplied. The
    /// `loop_cid` is the JCS-byte BLAKE3-512 hash of the loop's
    /// LLBC body block — it identifies the loop independent of the
    /// containing function. A separate `LoopInvariantMemento`
    /// keyed by this loop_cid supplies the invariant + decreasing
    /// function; the substrate refuses to compose this contract
    /// downstream until that memento is present and verified.
    /// Per paper 07 §11 — loops are first-class deferred memento
    /// targets.
    OpaqueLoop { loop_cid: String },
    /// Contains a `?` operator (Try-branch with early return). The
    /// `try_cid` is the content hash of the Switch::Match block that
    /// implements the Try-branch shape. A separate
    /// `TryBranchMemento` (or specific Result/Option-shape spec)
    /// supplies the success-path/failure-path contract pair. The
    /// substrate refuses composition until that memento lands —
    /// honest opacity rather than silent assumption that all `?`
    /// callers handle the Err branch. Same shape of opacity as
    /// OpaqueLoop.
    EarlyReturn { try_cid: String },
    /// Constructs a closure value. The closure body is itself a
    /// regular fun_decl (Charon emits it as a Fn/FnMut/FnOnce trait
    /// impl method) — its contract is lifted normally through the
    /// usual pipeline. This effect records the link between the
    /// CAPTURING site (this function) and the body fn: when the
    /// closure is later called, the call composes against the body
    /// contract. The substrate uses this effect to recognize that
    /// composition through the call is mediated by the captured
    /// environment. `body_fn_cid` is the body fun_decl's content_cid
    /// once it's lifted; `n_captures` is the count of captured
    /// values bundled at this site.
    ClosureCapture {
        body_fn_cid: String,
        n_captures: usize,
    },
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
            Effect::OpaqueLoop { loop_cid } => Value::object([
                ("kind", Value::string("opaque_loop")),
                ("loopCid", Value::string(loop_cid.clone())),
            ]),
            Effect::EarlyReturn { try_cid } => Value::object([
                ("kind", Value::string("early_return")),
                ("tryCid", Value::string(try_cid.clone())),
            ]),
            Effect::ClosureCapture {
                body_fn_cid,
                n_captures,
            } => Value::object([
                ("kind", Value::string("closure_capture")),
                ("bodyFnCid", Value::string(body_fn_cid.clone())),
                ("nCaptures", Value::integer(*n_captures as i64)),
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
            Effect::OpaqueLoop { loop_cid } => format!("6:opaque_loop:{}", loop_cid),
            Effect::EarlyReturn { try_cid } => format!("7:early_return:{}", try_cid),
            Effect::ClosureCapture {
                body_fn_cid,
                n_captures,
            } => format!("8:closure_capture:{}:{}", body_fn_cid, n_captures),
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
    pub locus: Locus,
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
/// (e.g., during a lift-only pass). The locus carries source-position
/// metadata for downstream developer feedback; pass None to use an
/// unknown/empty locus.
pub fn build_function_contract(
    item_fn: &ItemFn,
    body_cid: Option<String>,
) -> FunctionContractMemento {
    build_function_contract_with_file(item_fn, body_cid, None)
}

/// Build a FunctionContractMemento with an explicit source file path
/// for locus annotation.
pub fn build_function_contract_with_file(
    item_fn: &ItemFn,
    body_cid: Option<String>,
    file_path: Option<&str>,
) -> FunctionContractMemento {
    let fn_name = item_fn.sig.ident.to_string();
    let (formals, formal_sorts) = extract_formals(item_fn);
    let return_sort = extract_return_sort(item_fn);
    let pre = lift_function_precondition(item_fn).into_formula();
    let post = lift_function_postcondition(item_fn).into_formula();
    let effects = detect_effects(item_fn);
    let locus = Locus::from_span(item_fn.sig.ident.span(), file_path);

    let value = build_value(
        &fn_name,
        &formals,
        &formal_sorts,
        &return_sort,
        &pre,
        &post,
        body_cid.as_deref(),
        &effects,
        &locus,
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
        locus,
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
    locus: &Locus,
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
        ("locus", locus.to_value()),
    ])
}

/// Build the canonical `Value` for a `FunctionContractMemento`. Used
/// by callers (LLBC lift, marriage) that need to recompute the
/// memento's canonical bytes / CID after replacing fields like the
/// pre/post formulas.
pub fn build_memento_value(c: &FunctionContractMemento) -> Arc<Value> {
    build_value(
        &c.fn_name,
        &c.formals,
        &c.formal_sorts,
        &c.return_sort,
        &c.pre,
        &c.post,
        c.body_cid.as_deref(),
        &c.effects,
        &c.locus,
    )
}

fn sort_to_value(s: &Sort) -> Arc<Value> {
    match s {
        Sort::Primitive { name } => Value::object([
            ("kind", Value::string("primitive")),
            ("name", Value::string(name.clone())),
        ]),
        // Function / Dependent sorts (added by #361 for the
        // dependent-type / first-class-function-sort grammar
        // expansion) aren't currently produced by the AST/LLBC
        // walk's `synth_item_fn` shell — we always emit a plain
        // primitive sort. Emit them as opaque so the contract
        // canonical bytes stay valid if a downstream caller passes
        // them through. Translating them faithfully into our IR
        // sort encoding is tracked as part of #384 Tier A.1
        // (type-aware predicate sorts).
        Sort::Function { .. } | Sort::Dependent { .. } => Value::object([
            ("kind", Value::string("opaque")),
            (
                "reason",
                Value::string("function-or-dependent-sort-not-yet-modeled"),
            ),
        ]),
    }
}

// ---- Opacity discharge ----

/// Trait for querying whether a discharge memento is present for a
/// given opacity effect. Implemented by `MementoPool` in
/// `provekit-verifier`; in-crate tests use a mock.
///
/// Each method returns `true` iff a conforming discharge memento for
/// that specific site is present in the pool and has passed all
/// validation rules defined in the corresponding spec:
///   - `LoopInvariantMemento`  (protocol/specs/2026-05-05-loop-invariant-memento.md)
///   - `TryBranchMemento`      (protocol/specs/2026-05-05-try-branch-memento.md)
///   - `ClosureBindingMemento` (protocol/specs/2026-05-05-closure-binding-memento.md)
///   - `UnresolvedCall`:       no memento kind yet; always undischarged.
pub trait OpacityMementoLookup {
    fn has_loop_invariant(&self, loop_cid: &str) -> bool;
    fn has_try_branch(&self, try_cid: &str) -> bool;
    fn has_closure_binding(&self, body_fn_cid: &str) -> bool;
}

/// A no-op pool that never has any discharge mementos. Used as the
/// default when callers don't yet track opacity discharge.
pub struct EmptyOpacityPool;
impl OpacityMementoLookup for EmptyOpacityPool {
    fn has_loop_invariant(&self, _: &str) -> bool { false }
    fn has_try_branch(&self, _: &str) -> bool { false }
    fn has_closure_binding(&self, _: &str) -> bool { false }
}

/// Error returned when composition is refused because an opacity effect
/// is not discharged by a memento in the pool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpacityError {
    /// An `Effect::OpaqueLoop { loop_cid }` is present but no
    /// `LoopInvariantMemento` with that `loopCid` is in the pool.
    LoopNotDischarged { loop_cid: String },
    /// An `Effect::EarlyReturn { try_cid }` is present but no
    /// `TryBranchMemento` with that `tryCid` is in the pool.
    EarlyReturnNotDischarged { try_cid: String },
    /// An `Effect::ClosureCapture { body_fn_cid, .. }` is present but
    /// no `ClosureBindingMemento` with that `bodyFnCid` is in the pool.
    ClosureCaptureNotDischarged { body_fn_cid: String },
    /// An `Effect::UnresolvedCall { name }` is present. No discharge
    /// memento kind exists yet; composition is always refused.
    UnresolvedCallNotDischarged { name: String },
}

impl std::fmt::Display for OpacityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LoopNotDischarged { loop_cid } =>
                write!(f, "opacity: OpaqueLoop(loopCid={loop_cid}) has no LoopInvariantMemento in pool"),
            Self::EarlyReturnNotDischarged { try_cid } =>
                write!(f, "opacity: EarlyReturn(tryCid={try_cid}) has no TryBranchMemento in pool"),
            Self::ClosureCaptureNotDischarged { body_fn_cid } =>
                write!(f, "opacity: ClosureCapture(bodyFnCid={body_fn_cid}) has no ClosureBindingMemento in pool"),
            Self::UnresolvedCallNotDischarged { name } =>
                write!(f, "opacity: UnresolvedCall({name}) has no discharge memento kind"),
        }
    }
}

impl EffectSet {
    /// Check whether all opacity effects in this set are discharged by
    /// the given pool. Returns `Ok(())` iff:
    ///   - there are no opacity effects at all, OR
    ///   - every opacity effect has a corresponding memento in `pool`.
    ///
    /// Non-opacity effects (Reads/Writes/Io/Unsafe/Panics) are NOT
    /// checked here; composition with non-opacity effects is refused
    /// by `compose_function_contracts` separately.
    ///
    /// Returns the FIRST undischarged opacity effect as an error.
    /// The caller can collect all errors by iterating effects directly.
    pub fn check_opacity_effects(&self, pool: &dyn OpacityMementoLookup) -> Result<(), OpacityError> {
        for effect in &self.effects {
            match effect {
                Effect::OpaqueLoop { loop_cid } => {
                    if !pool.has_loop_invariant(loop_cid) {
                        return Err(OpacityError::LoopNotDischarged { loop_cid: loop_cid.clone() });
                    }
                }
                Effect::EarlyReturn { try_cid } => {
                    if !pool.has_try_branch(try_cid) {
                        return Err(OpacityError::EarlyReturnNotDischarged { try_cid: try_cid.clone() });
                    }
                }
                Effect::ClosureCapture { body_fn_cid, .. } => {
                    if !pool.has_closure_binding(body_fn_cid) {
                        return Err(OpacityError::ClosureCaptureNotDischarged { body_fn_cid: body_fn_cid.clone() });
                    }
                }
                Effect::UnresolvedCall { name } => {
                    return Err(OpacityError::UnresolvedCallNotDischarged { name: name.clone() });
                }
                // Non-opacity effects: Reads/Writes/Io/Unsafe/Panics do not
                // participate in memento-based discharge.
                Effect::Reads { .. } | Effect::Writes { .. } | Effect::Io
                | Effect::Unsafe | Effect::Panics => {}
            }
        }
        Ok(())
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

/// Compose two function contracts with opacity discharge checking.
///
/// This is the substrate-verifier-facing API. It distinguishes three
/// refusal reasons:
///
/// 1. `Err(OpacityError::*)` — a contract carries an opacity effect
///    (`OpaqueLoop`, `EarlyReturn`, `ClosureCapture`, `UnresolvedCall`)
///    with no corresponding discharge memento in `pool`. The caller
///    knows exactly which effect is undischarged.
///
/// 2. `Ok(None)` — composition refused for a non-opacity reason:
///    an unconditionally-blocking effect (Reads/Writes/Io/Unsafe/Panics)
///    is present, `formal_idx` is out of bounds, or the inner post has
///    no result equation.
///
/// 3. `Ok(Some(composed))` — all opacity effects are discharged by
///    mementos in `pool`, no unconditionally-blocking effects are
///    present, and composition succeeded.
///
/// Design note: unlike `compose_function_contracts`, this function
/// allows contracts whose ONLY impure effects are opacity effects that
/// are fully discharged. The existing `compose_function_contracts`
/// (which refuses on ANY effect) remains unchanged for pre-discharge
/// callers.
pub fn compose_function_contracts_checked(
    outer: &FunctionContractMemento,
    inner: &FunctionContractMemento,
    formal_idx: usize,
    pool: &dyn OpacityMementoLookup,
) -> Result<Option<ComposedFunctionContract>, OpacityError> {
    // Phase 1: check opacity effects on both sides. Returns the first
    // undischarged opacity effect as a typed error.
    outer.effects.check_opacity_effects(pool)?;
    inner.effects.check_opacity_effects(pool)?;

    // Phase 2: check for unconditionally-blocking effects. After opacity
    // discharge, only Reads/Writes/Io/Unsafe/Panics can still block.
    // A contract that had ONLY opacity effects — all now discharged — has
    // no remaining blockers; one that still has non-opacity impure effects
    // is still refused.
    let outer_non_opacity_pure = outer.effects.effects.iter().all(|e| matches!(
        e,
        Effect::OpaqueLoop { .. } | Effect::EarlyReturn { .. }
        | Effect::ClosureCapture { .. } | Effect::UnresolvedCall { .. }
    ));
    let inner_non_opacity_pure = inner.effects.effects.iter().all(|e| matches!(
        e,
        Effect::OpaqueLoop { .. } | Effect::EarlyReturn { .. }
        | Effect::ClosureCapture { .. } | Effect::UnresolvedCall { .. }
    ));
    if !outer_non_opacity_pure || !inner_non_opacity_pure {
        return Ok(None);
    }

    if formal_idx >= outer.formals.len() {
        return Ok(None);
    }

    // Phase 3: perform the actual composition (same logic as
    // compose_function_contracts, duplicated here to avoid the
    // is_pure() gate in that function).
    let inner_result_name = inner.result_var_name();
    let inner_post_renamed = crate::wp::substitute_in_formula(
        inner.post.clone(),
        "result",
        &IrTerm::Var { name: inner_result_name.clone() },
    );
    let inner_value = match find_result_equation(&inner_post_renamed, &inner_result_name) {
        Some(t) => t,
        None => return Ok(None),
    };
    let outer_formal = &outer.formals[formal_idx];
    let outer_pre_substituted =
        crate::wp::substitute_in_formula(outer.pre.clone(), outer_formal, &inner_value);
    let outer_post_substituted =
        crate::wp::substitute_in_formula(outer.post.clone(), outer_formal, &inner_value);
    let pre = IrFormula::And {
        operands: vec![
            inner.pre.clone(),
            IrFormula::Implies {
                operands: vec![inner_post_renamed, outer_pre_substituted],
            },
        ],
    };
    let post = outer_post_substituted;
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
    Ok(Some(ComposedFunctionContract {
        component_cids: vec![outer.cid.clone(), inner.cid.clone()],
        formal_idx,
        pre,
        post,
        canonical_bytes,
        cid,
    }))
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
    crate::sort_translate::syn_type_to_sort(ty)
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

    // ---- Bug #384 A.1: sort-collapse regression tests ----

    /// fn f(x: u32) and fn f(x: bool) must produce DISTINCT contracts
    /// (distinct formal_sorts → distinct content_cid). This was the
    /// primary false-collision from the old token-string infer_sort.
    #[test]
    fn u32_and_bool_formals_produce_distinct_cids() {
        let f_u32 = build_function_contract(&parse_fn(r#"fn f(x: u32) {}"#), None);
        let f_bool = build_function_contract(&parse_fn(r#"fn f(x: bool) {}"#), None);
        assert_ne!(
            f_u32.cid, f_bool.cid,
            "u32 and bool formals must produce distinct contract CIDs"
        );
        assert_ne!(f_u32.formal_sorts, f_bool.formal_sorts);
    }

    /// Lifetime annotation on a reference must NOT change the formal sort
    /// or the contract CID. This was the false-split from whitespace
    /// tokenisation: &'a str tokenised as "& 'a str" (with spaces)
    /// fell to the catch-all, while &str matched the explicit arm.
    #[test]
    fn ref_lifetime_annotation_does_not_change_cid() {
        let with_lt = build_function_contract(
            &parse_fn(r#"fn f<'a>(s: &'a str) {}"#),
            None,
        );
        let without_lt = build_function_contract(
            &parse_fn(r#"fn f(s: &str) {}"#),
            None,
        );
        assert_eq!(
            with_lt.formal_sorts, without_lt.formal_sorts,
            "&'a str and &str must produce the same formal sort"
        );
        // CIDs will differ because fn names include the lifetime parameter
        // in the AST (`fn f<'a>` vs `fn f`), but formal_sorts must agree.
    }

    /// Vec<u32> and a user struct produce distinct formal sorts.
    #[test]
    fn vec_and_user_struct_formals_are_distinct() {
        let f_vec = build_function_contract(
            &parse_fn(r#"fn f(x: Vec<u32>) {}"#),
            None,
        );
        let f_struct = build_function_contract(
            &parse_fn(r#"fn f(x: SomeStruct) {}"#),
            None,
        );
        assert_ne!(
            f_vec.formal_sorts, f_struct.formal_sorts,
            "Vec<u32> and SomeStruct formals must produce distinct sorts"
        );
        assert_ne!(f_vec.cid, f_struct.cid);
    }

    // ---- Issue #384 B.5: opacity discharge tests ----

    /// Mock pool for testing: pre-seeded with specific loop_cid values.
    struct MockPool {
        loop_cids: Vec<String>,
        try_cids: Vec<String>,
        body_fn_cids: Vec<String>,
    }

    impl MockPool {
        fn empty() -> Self {
            Self { loop_cids: vec![], try_cids: vec![], body_fn_cids: vec![] }
        }
        fn with_loop(mut self, cid: &str) -> Self {
            self.loop_cids.push(cid.to_string());
            self
        }
        fn with_try(mut self, cid: &str) -> Self {
            self.try_cids.push(cid.to_string());
            self
        }
        fn with_closure(mut self, cid: &str) -> Self {
            self.body_fn_cids.push(cid.to_string());
            self
        }
    }

    impl OpacityMementoLookup for MockPool {
        fn has_loop_invariant(&self, loop_cid: &str) -> bool {
            self.loop_cids.iter().any(|c| c == loop_cid)
        }
        fn has_try_branch(&self, try_cid: &str) -> bool {
            self.try_cids.iter().any(|c| c == try_cid)
        }
        fn has_closure_binding(&self, body_fn_cid: &str) -> bool {
            self.body_fn_cids.iter().any(|c| c == body_fn_cid)
        }
    }

    /// Build a FunctionContractMemento from a `fn f(x: u32) -> u32` shell
    /// with a manually injected effect. Used by opacity tests below.
    fn contract_with_effects(name: &str, effects: Vec<Effect>) -> FunctionContractMemento {
        let mut c = build_function_contract(
            &parse_fn(&format!("fn {}(x: u32) -> u32 {{ x }}", name)),
            None,
        );
        for e in effects {
            c.effects.add(e);
        }
        // Recompute cid so it reflects the new effects set.
        let val = crate::contract::build_memento_value(&c);
        c.canonical_bytes = crate::canonical::jcs_bytes_of_value(&val);
        c.cid = crate::canonical::cid_of_value(&val);
        c
    }

    /// A contract with OpaqueLoop + no LoopInvariantMemento in pool →
    /// compose_function_contracts_checked returns Err.
    #[test]
    fn opaque_loop_without_memento_blocks_composition() {
        let loop_cid = "blake3-512:aabb".repeat(8); // fake stable cid
        let outer = contract_with_effects("outer", vec![Effect::OpaqueLoop { loop_cid: loop_cid.clone() }]);
        let inner = build_function_contract(&parse_fn(r#"fn inner(y: u32) -> u32 { y + 1 }"#), None);
        let pool = MockPool::empty(); // no loop invariant
        let result = compose_function_contracts_checked(&outer, &inner, 0, &pool);
        assert!(
            matches!(result, Err(OpacityError::LoopNotDischarged { .. })),
            "expected LoopNotDischarged, got {:?}", result
        );
    }

    /// Same contract with OpaqueLoop + matching LoopInvariantMemento in pool →
    /// compose_function_contracts_checked succeeds (returns Ok(Some(...))).
    #[test]
    fn opaque_loop_with_memento_allows_composition() {
        let loop_cid = "blake3-512:aabb".repeat(8);
        let outer = contract_with_effects("outer", vec![Effect::OpaqueLoop { loop_cid: loop_cid.clone() }]);
        let inner = build_function_contract(&parse_fn(r#"fn inner(y: u32) -> u32 { y + 1 }"#), None);
        let pool = MockPool::empty().with_loop(&loop_cid);
        let result = compose_function_contracts_checked(&outer, &inner, 0, &pool);
        // outer has only an OpaqueLoop (now discharged) and inner is pure →
        // composition should succeed.
        assert!(
            matches!(result, Ok(Some(_))),
            "expected Ok(Some(_)) after discharge, got {:?}", result
        );
    }

    /// EarlyReturn + no TryBranchMemento → Err(EarlyReturnNotDischarged).
    #[test]
    fn early_return_without_memento_blocks_composition() {
        let try_cid = "blake3-512:ccdd".repeat(8);
        let outer = contract_with_effects("outer", vec![Effect::EarlyReturn { try_cid: try_cid.clone() }]);
        let inner = build_function_contract(&parse_fn(r#"fn inner(y: u32) -> u32 { y }"#), None);
        let pool = MockPool::empty();
        let result = compose_function_contracts_checked(&outer, &inner, 0, &pool);
        assert!(
            matches!(result, Err(OpacityError::EarlyReturnNotDischarged { .. })),
            "expected EarlyReturnNotDischarged, got {:?}", result
        );
    }

    /// EarlyReturn + matching TryBranchMemento → Ok(Some(_)).
    #[test]
    fn early_return_with_memento_allows_composition() {
        let try_cid = "blake3-512:ccdd".repeat(8);
        let outer = contract_with_effects("outer", vec![Effect::EarlyReturn { try_cid: try_cid.clone() }]);
        let inner = build_function_contract(&parse_fn(r#"fn inner(y: u32) -> u32 { y }"#), None);
        let pool = MockPool::empty().with_try(&try_cid);
        let result = compose_function_contracts_checked(&outer, &inner, 0, &pool);
        assert!(
            matches!(result, Ok(Some(_))),
            "expected Ok(Some(_)) after discharge, got {:?}", result
        );
    }

    /// ClosureCapture + no ClosureBindingMemento → Err(ClosureCaptureNotDischarged).
    #[test]
    fn closure_capture_without_memento_blocks_composition() {
        let body_fn_cid = "blake3-512:eeff".repeat(8);
        let outer = contract_with_effects("outer", vec![Effect::ClosureCapture { body_fn_cid: body_fn_cid.clone(), n_captures: 1 }]);
        let inner = build_function_contract(&parse_fn(r#"fn inner(y: u32) -> u32 { y }"#), None);
        let pool = MockPool::empty();
        let result = compose_function_contracts_checked(&outer, &inner, 0, &pool);
        assert!(
            matches!(result, Err(OpacityError::ClosureCaptureNotDischarged { .. })),
            "expected ClosureCaptureNotDischarged, got {:?}", result
        );
    }

    /// ClosureCapture + matching ClosureBindingMemento → Ok(Some(_)).
    #[test]
    fn closure_capture_with_memento_allows_composition() {
        let body_fn_cid = "blake3-512:eeff".repeat(8);
        let outer = contract_with_effects("outer", vec![Effect::ClosureCapture { body_fn_cid: body_fn_cid.clone(), n_captures: 1 }]);
        let inner = build_function_contract(&parse_fn(r#"fn inner(y: u32) -> u32 { y }"#), None);
        let pool = MockPool::empty().with_closure(&body_fn_cid);
        let result = compose_function_contracts_checked(&outer, &inner, 0, &pool);
        assert!(
            matches!(result, Ok(Some(_))),
            "expected Ok(Some(_)) after discharge, got {:?}", result
        );
    }

    /// UnresolvedCall always blocks composition regardless of pool contents.
    #[test]
    fn unresolved_call_always_blocks_composition() {
        let outer = contract_with_effects("outer", vec![Effect::UnresolvedCall { name: "some_fn".to_string() }]);
        let inner = build_function_contract(&parse_fn(r#"fn inner(y: u32) -> u32 { y }"#), None);
        let pool = MockPool::empty();
        let result = compose_function_contracts_checked(&outer, &inner, 0, &pool);
        assert!(
            matches!(result, Err(OpacityError::UnresolvedCallNotDischarged { .. })),
            "expected UnresolvedCallNotDischarged, got {:?}", result
        );
    }

    /// Non-opacity impure effects (Io) still block composition even
    /// after all opacity effects are discharged: returns Ok(None).
    #[test]
    fn non_opacity_effects_still_block_after_opacity_discharge() {
        let loop_cid = "blake3-512:1122".repeat(8);
        // Contract has both OpaqueLoop (dischargeable) and Io (not dischargeable).
        let outer = contract_with_effects("outer", vec![
            Effect::OpaqueLoop { loop_cid: loop_cid.clone() },
            Effect::Io,
        ]);
        let inner = build_function_contract(&parse_fn(r#"fn inner(y: u32) -> u32 { y }"#), None);
        let pool = MockPool::empty().with_loop(&loop_cid);
        let result = compose_function_contracts_checked(&outer, &inner, 0, &pool);
        // OpaqueLoop is discharged → no OpacityError. But Io blocks → Ok(None).
        assert!(
            matches!(result, Ok(None)),
            "expected Ok(None) when non-opacity effect blocks, got {:?}", result
        );
    }

    /// check_opacity_effects returns Ok(()) for a pure contract.
    #[test]
    fn check_opacity_pure_contract_is_ok() {
        let f = build_function_contract(&parse_fn(r#"fn f(x: u32) -> u32 { x * 2 }"#), None);
        let pool = MockPool::empty();
        assert!(f.effects.check_opacity_effects(&pool).is_ok());
    }
}
