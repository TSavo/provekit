// SPDX-License-Identifier: Apache-2.0
//
// Contract Composition Protocol (CCP) reference implementation.
//
// Per the spec at protocol/specs/2026-05-09-contract-composition-protocol.md
// (sections 2, 5, 9), this module is the single canonical home for the
// compose primitive. Every per-language lifter (Rust walk, C kernel-doc,
// future Java / Go / TypeScript / Python lifters) calls into THIS function
// either directly (Rust linkage), via the C ABI wrapper (planned in a
// follow-up), or via the JSON-RPC subprocess transport (also planned).
//
// CCP §5 mandates:
//   1. Pure. No state, no I/O, no clock, no random.
//   2. Deterministic. Identical inputs produce identical outputs byte-for-byte.
//   3. Schema-versioned. The CCP version tag is part of the canonical bytes.
//   4. Reference-only. No mutation of inputs.
//
// CCP §9 codifies the eight algebra rules. The implementation here MUST
// follow them byte-for-byte. Any change to the algebra requires a CCP
// version bump.
//
// This module was extracted from `provekit-walk/src/contract.rs` so the
// algebra becomes a workspace-level primitive callable from any consumer.
// Walk continues to host the AST-walking helpers that BUILD a
// FunctionContractMemento from a syn::ItemFn; the algebra that COMPOSES
// mementos lives here.
//
// Internal helpers (substitute_in_formula and the JCS / CID glue) are
// duplicated from walk's wp.rs and canonical.rs so this module is
// self-contained and walk can keep its existing module layout untouched.
// Both copies are pure formula manipulations over identical types from
// provekit-ir-types; byte-equivalent output is guaranteed by construction.

use std::collections::HashSet;
use std::sync::Arc;

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value};
use provekit_ir_types::{IrFormula, IrTerm, Sort};

/// CCP version tag carried inside the composed memento body. Bumped only
/// when the algebra changes in a CID-affecting way.
pub const CCP_VERSION: &str = "1.0.0";

// ============================================================
// Effect set
// ============================================================

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
    /// LLBC body block; it identifies the loop independent of the
    /// containing function. A separate `LoopInvariantMemento`
    /// keyed by this loop_cid supplies the invariant + decreasing
    /// function; the substrate refuses to compose this contract
    /// downstream until that memento is present and verified.
    OpaqueLoop { loop_cid: String },
    /// Contains a `?` operator (Try-branch with early return). The
    /// `try_cid` is the content hash of the Switch::Match block that
    /// implements the Try-branch shape. A separate
    /// `TryBranchMemento` (or specific Result/Option-shape spec)
    /// supplies the success-path/failure-path contract pair.
    EarlyReturn { try_cid: String },
    /// Constructs a closure value. The closure body is itself a
    /// regular fun_decl. This effect records the link between the
    /// CAPTURING site (this function) and the body fn.
    ClosureCapture {
        body_fn_cid: String,
        n_captures: usize,
    },
    /// Accepts a `Pin<P>` formal parameter. Opaque until a
    /// PinProjectionMemento supplies the projection safety proof.
    PinnedReference { target: String },
    /// Accepts or dereferences a raw pointer in a formal parameter
    /// position. Always co-present with `Effect::Unsafe`.
    RawPointerProvenance { target: String, mutable: bool },
    /// Calls an atomic intrinsic (core::sync::atomic::Atomic*).
    AtomicAccess {
        target: String,
        kind: AtomicKind,
        ordering: Option<String>,
    },
    /// Emitted when a function has formal parameters that are shared
    /// references (&T) to types with interior mutability.
    PossibleAliasing {
        /// Sorted lexicographically for JCS byte-determinism.
        formals: Vec<String>,
    },
    /// Calls `drop_in_place`. Drops can run user-defined `Drop::drop`,
    /// allocate, panic, or perform arbitrary side effects.
    Drop { name: String },
}

/// Operation class for `Effect::AtomicAccess`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AtomicKind {
    Load,
    Store,
    Rmw,
    Cas,
}

impl AtomicKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            AtomicKind::Load => "load",
            AtomicKind::Store => "store",
            AtomicKind::Rmw => "rmw",
            AtomicKind::Cas => "cas",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AliasingMemento {
    pub formal_a: String,
    pub formal_b: String,
    pub status: AliasingStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AliasingStatus {
    Disjoint,
    MaybeAlias,
}

impl AliasingStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            AliasingStatus::Disjoint => "Disjoint",
            AliasingStatus::MaybeAlias => "MaybeAlias",
        }
    }
}

impl AliasingMemento {
    pub fn to_jcs_value(&self) -> Arc<Value> {
        debug_assert!(
            self.formal_a <= self.formal_b,
            "AliasingMemento invariants violated: formal_a ({}) must be <= formal_b ({})",
            self.formal_a,
            self.formal_b
        );
        Value::object([
            ("kind", Value::string("aliasing-memento")),
            ("formal_a", Value::string(self.formal_a.clone())),
            ("formal_b", Value::string(self.formal_b.clone())),
            ("status", Value::string(self.status.as_str())),
        ])
    }
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
            Effect::PinnedReference { target } => Value::object([
                ("kind", Value::string("pinned_reference")),
                ("target", Value::string(target.clone())),
            ]),
            Effect::RawPointerProvenance { target, mutable } => Value::object([
                ("kind", Value::string("raw_ptr_provenance")),
                ("mutable", Value::boolean(*mutable)),
                ("target", Value::string(target.clone())),
            ]),
            Effect::AtomicAccess {
                target,
                kind,
                ordering,
            } => {
                let mut fields: Vec<(&str, Arc<Value>)> = vec![
                    ("kind", Value::string("atomic_access")),
                    ("atomicKind", Value::string(kind.as_str())),
                    ("target", Value::string(target.clone())),
                ];
                if let Some(ord) = ordering {
                    fields.push(("ordering", Value::string(ord.clone())));
                }
                Value::object(fields)
            }
            Effect::PossibleAliasing { formals } => {
                let formals_arr: Vec<Arc<Value>> =
                    formals.iter().map(|f| Value::string(f.clone())).collect();
                Value::object([
                    ("kind", Value::string("possible_aliasing")),
                    ("formals", Value::array(formals_arr)),
                ])
            }
            Effect::Drop { name } => Value::object([
                ("kind", Value::string("drop")),
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
            Effect::OpaqueLoop { loop_cid } => format!("6:opaque_loop:{}", loop_cid),
            Effect::EarlyReturn { try_cid } => format!("7:early_return:{}", try_cid),
            Effect::ClosureCapture {
                body_fn_cid,
                n_captures,
            } => format!("8:closure_capture:{}:{}", body_fn_cid, n_captures),
            Effect::PinnedReference { target } => format!("9:pinned_reference:{}", target),
            Effect::RawPointerProvenance { target, mutable } => {
                format!("10:raw_ptr_provenance:{}:{}", target, mutable)
            }
            Effect::AtomicAccess {
                target,
                kind,
                ordering,
            } => format!(
                "11:atomic_access:{}:{}:{}",
                target,
                kind.as_str(),
                ordering.as_deref().unwrap_or("")
            ),
            Effect::PossibleAliasing { formals } => {
                format!("13:possible_aliasing:{}", formals.join(","))
            }
            Effect::Drop { name } => format!("14:drop:{}", name),
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
        sorted.sort_by_key(|a| a.sort_key());
        let items: Vec<Arc<Value>> = sorted.iter().map(|e| e.to_value()).collect();
        Value::array(items)
    }
}

// ============================================================
// Locus (data type only; the syn-driven `from_span` constructor lives
// in walk's locus.rs and produces a libprovekit Locus).
// ============================================================

/// One source location. `file` is whatever the caller passed in; if
/// None, the locus is from in-memory source or untracked input.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Locus {
    pub file: Option<String>,
    pub line: usize,
    pub col: usize,
}

impl Locus {
    /// Empty/unknown locus.
    pub fn unknown() -> Self {
        Self::default()
    }

    pub fn is_unknown(&self) -> bool {
        self.file.is_none() && self.line == 0 && self.col == 0
    }

    pub fn to_value(&self) -> Arc<Value> {
        Value::object([
            (
                "file",
                match &self.file {
                    Some(p) => Value::string(p.clone()),
                    None => Value::null(),
                },
            ),
            ("line", Value::integer(self.line as i64)),
            ("col", Value::integer(self.col as i64)),
        ])
    }
}

// ============================================================
// FunctionContractMemento
// ============================================================

#[derive(Debug, Clone)]
pub struct FunctionContractMemento {
    pub fn_name: String,
    pub formals: Vec<String>,
    pub formal_sorts: Vec<Sort>,
    pub formal_regions: Vec<Option<String>>,
    pub return_sort: Sort,
    pub return_region: Option<String>,
    pub pre: IrFormula,
    pub post: IrFormula,
    pub body_cid: Option<String>,
    pub effects: EffectSet,
    pub locus: Locus,
    pub canonical_bytes: Vec<u8>,
    pub cid: String,
    pub auto_minted_mementos: Vec<AliasingMemento>,
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

    /// Check that every `Effect::PossibleAliasing` pair of formals has a
    /// matching AliasingMemento in `auto_minted_mementos`. Returns
    /// `Ok(())` if aliasing is fully discharged or no PossibleAliasing
    /// effects are present. Returns the first undischarged pair as an error.
    pub fn check_aliasing_discharged(&self) -> Result<(), OpacityError> {
        for effect in &self.effects.effects {
            if let Effect::PossibleAliasing { formals } = effect {
                if formals.len() < 2 {
                    continue;
                }
                for i in 0..formals.len() {
                    for j in (i + 1)..formals.len() {
                        let a = &formals[i];
                        let b = &formals[j];
                        let covered = self.auto_minted_mementos.iter().any(|m| {
                            (m.formal_a == *a && m.formal_b == *b)
                                || (m.formal_a == *b && m.formal_b == *a)
                        });
                        if !covered {
                            return Err(OpacityError::AliasingNotDischarged {
                                formal_a: a.clone(),
                                formal_b: b.clone(),
                            });
                        }
                    }
                }
            }
        }
        Ok(())
    }
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
        &c.auto_minted_mementos,
    )
}

/// Construct the canonical Value tree for a function-contract memento.
/// Public so walk's AST-side builders can compute their own CIDs without
/// having to round-trip through a memento struct first.
#[allow(clippy::too_many_arguments)]
pub fn build_value(
    fn_name: &str,
    formals: &[String],
    formal_sorts: &[Sort],
    return_sort: &Sort,
    pre: &IrFormula,
    post: &IrFormula,
    body_cid: Option<&str>,
    effects: &EffectSet,
    locus: &Locus,
    auto_minted_mementos: &[AliasingMemento],
) -> Arc<Value> {
    let formals_arr: Vec<Arc<Value>> = formals.iter().map(|n| Value::string(n.clone())).collect();
    let formal_sorts_arr: Vec<Arc<Value>> = formal_sorts.iter().map(sort_to_value).collect();
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
        (
            "autoMintedMementos",
            aliasing_mementos_to_value(auto_minted_mementos),
        ),
    ])
}

/// Canonical JCS array for a set of AliasingMementos. Entries are sorted
/// lexicographically by (formal_a, formal_b) for byte-determinism. Empty
/// list is always included as `[]` (never omitted) so the CID is
/// consistent across deployments.
fn aliasing_mementos_to_value(mementos: &[AliasingMemento]) -> Arc<Value> {
    let mut sorted: Vec<&AliasingMemento> = mementos.iter().collect();
    sorted.sort_by(|a, b| {
        a.formal_a
            .cmp(&b.formal_a)
            .then_with(|| a.formal_b.cmp(&b.formal_b))
    });
    let items: Vec<Arc<Value>> = sorted.iter().map(|m| m.to_jcs_value()).collect();
    Value::array(items)
}

/// Canonical encoding of a `Sort`. Public so external lifters that
/// build their own canonical bytes can match libprovekit's encoding
/// byte-for-byte.
pub fn sort_to_value(s: &Sort) -> Arc<Value> {
    match s {
        Sort::Primitive { name } => Value::object([
            ("kind", Value::string("primitive")),
            ("name", Value::string(name.clone())),
        ]),
        Sort::Function { .. } | Sort::Dependent { .. } => Value::object([
            ("kind", Value::string("opaque")),
            (
                "reason",
                Value::string("function-or-dependent-sort-not-yet-modeled"),
            ),
        ]),
        Sort::Float { width } => Value::object([
            ("kind", Value::string("float")),
            ("width", Value::integer(i64::from(*width))),
        ]),
        Sort::Region { name } => Value::object([
            ("kind", Value::string("region")),
            ("name", Value::string(name.clone())),
        ]),
    }
}

// ============================================================
// Opacity discharge
// ============================================================

/// Trait for querying whether a discharge memento is present for a
/// given opacity effect. Implemented by `MementoPool` in
/// `provekit-verifier`; in-crate tests use a mock.
pub trait OpacityMementoLookup {
    fn has_loop_invariant(&self, loop_cid: &str) -> bool;
    fn has_try_branch(&self, try_cid: &str) -> bool;
    fn has_closure_binding(&self, body_fn_cid: &str) -> bool;
    fn has_drop_contract(&self, type_name: &str) -> bool;
    fn has_aliasing_memento(&self, formal_a: &str, formal_b: &str) -> bool;
    fn lookup_pin_invariant(
        &self,
        function_cid: &str,
        target: &str,
    ) -> Option<PinInvariantMementoView>;
}

/// A no-op pool that never has any discharge mementos.
pub struct EmptyOpacityPool;
impl OpacityMementoLookup for EmptyOpacityPool {
    fn has_loop_invariant(&self, _: &str) -> bool {
        false
    }
    fn has_try_branch(&self, _: &str) -> bool {
        false
    }
    fn has_closure_binding(&self, _: &str) -> bool {
        false
    }
    fn has_drop_contract(&self, _: &str) -> bool {
        false
    }
    fn has_aliasing_memento(&self, _: &str, _: &str) -> bool {
        false
    }
    fn lookup_pin_invariant(
        &self,
        _function_cid: &str,
        _target: &str,
    ) -> Option<PinInvariantMementoView> {
        None
    }
}

/// Lightweight pool lookup type for PinInvariantMemento.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinInvariantMementoView {
    pub function_cid: String,
    pub pinned_target: String,
    pub invariant: String,
}

/// Error returned when composition is refused because an opacity effect
/// is not discharged by a memento in the pool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpacityError {
    LoopNotDischarged { loop_cid: String },
    EarlyReturnNotDischarged { try_cid: String },
    ClosureCaptureNotDischarged { body_fn_cid: String },
    UnresolvedCallNotDischarged { name: String },
    AliasingNotDischarged { formal_a: String, formal_b: String },
    DropNotDischarged { name: String },
    PinInvariantNotDischarged { target: String },
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
            Self::AliasingNotDischarged { formal_a, formal_b } =>
                write!(f, "aliasing: PossibleAliasing pair ({formal_a}, {formal_b}) has no AliasingMemento in auto_minted_mementos"),
            Self::DropNotDischarged { name } =>
                write!(f, "opacity: Drop({name}) has no lifted drop function in pool"),
            Self::PinInvariantNotDischarged { target } =>
                write!(f, "opacity: PinnedReference(target={target}) has no PinInvariantMemento in pool"),
        }
    }
}

impl EffectSet {
    /// Check whether all opacity effects in this set are discharged by
    /// the given pool. Returns `Ok(())` iff:
    ///   - there are no opacity effects at all, OR
    ///   - every opacity effect has a corresponding memento in `pool`.
    pub fn check_opacity_effects(
        &self,
        pool: &dyn OpacityMementoLookup,
        function_cid: Option<&str>,
    ) -> Result<(), OpacityError> {
        for effect in &self.effects {
            match effect {
                Effect::OpaqueLoop { loop_cid } => {
                    if !pool.has_loop_invariant(loop_cid) {
                        return Err(OpacityError::LoopNotDischarged {
                            loop_cid: loop_cid.clone(),
                        });
                    }
                }
                Effect::EarlyReturn { try_cid } => {
                    if !pool.has_try_branch(try_cid) {
                        return Err(OpacityError::EarlyReturnNotDischarged {
                            try_cid: try_cid.clone(),
                        });
                    }
                }
                Effect::ClosureCapture { body_fn_cid, .. } => {
                    if !pool.has_closure_binding(body_fn_cid) {
                        return Err(OpacityError::ClosureCaptureNotDischarged {
                            body_fn_cid: body_fn_cid.clone(),
                        });
                    }
                }
                Effect::UnresolvedCall { name } => {
                    return Err(OpacityError::UnresolvedCallNotDischarged { name: name.clone() });
                }
                Effect::Drop { name } => {
                    if !pool.has_drop_contract(name) {
                        return Err(OpacityError::DropNotDischarged { name: name.clone() });
                    }
                }
                Effect::PossibleAliasing { .. } => {}
                Effect::PinnedReference { target } => {
                    let fc = match function_cid {
                        Some(cid) if !cid.is_empty() => cid,
                        _ => {
                            return Err(OpacityError::PinInvariantNotDischarged {
                                target: target.clone(),
                            })
                        }
                    };
                    match pool.lookup_pin_invariant(fc, target) {
                        Some(view) if !view.invariant.is_empty() => { /* discharged */ }
                        _ => {
                            return Err(OpacityError::PinInvariantNotDischarged {
                                target: target.clone(),
                            })
                        }
                    }
                }
                Effect::Reads { .. }
                | Effect::Writes { .. }
                | Effect::Io
                | Effect::Unsafe
                | Effect::Panics
                | Effect::RawPointerProvenance { .. }
                | Effect::AtomicAccess { .. } => {}
            }
        }
        Ok(())
    }
}

// ============================================================
// Composition
// ============================================================

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

    let inner_result_name = inner.result_var_name();
    let inner_post_renamed = substitute_in_formula(
        inner.post.clone(),
        "result",
        &IrTerm::Var {
            name: inner_result_name.clone(),
        },
    );

    let inner_value = find_result_equation(&inner_post_renamed, &inner_result_name)?;

    let outer_formal = &outer.formals[formal_idx];
    let outer_pre_substituted =
        substitute_in_formula(outer.pre.clone(), outer_formal, &inner_value);
    let outer_post_substituted =
        substitute_in_formula(outer.post.clone(), outer_formal, &inner_value);

    let pre = IrFormula::And {
        operands: vec![
            inner.pre.clone(),
            IrFormula::Implies {
                operands: vec![inner_post_renamed.clone(), outer_pre_substituted],
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
pub fn compose_function_contracts_checked(
    outer: &FunctionContractMemento,
    inner: &FunctionContractMemento,
    formal_idx: usize,
    pool: &dyn OpacityMementoLookup,
) -> Result<Option<ComposedFunctionContract>, OpacityError> {
    outer
        .effects
        .check_opacity_effects(pool, Some(&outer.cid))?;
    inner
        .effects
        .check_opacity_effects(pool, Some(&inner.cid))?;

    outer.check_aliasing_discharged()?;
    inner.check_aliasing_discharged()?;

    let outer_non_opacity_pure = outer.effects.effects.iter().all(|e| {
        matches!(
            e,
            Effect::OpaqueLoop { .. }
                | Effect::EarlyReturn { .. }
                | Effect::ClosureCapture { .. }
                | Effect::UnresolvedCall { .. }
                | Effect::PossibleAliasing { .. }
                | Effect::Drop { .. }
                | Effect::PinnedReference { .. }
        )
    });
    let inner_non_opacity_pure = inner.effects.effects.iter().all(|e| {
        matches!(
            e,
            Effect::OpaqueLoop { .. }
                | Effect::EarlyReturn { .. }
                | Effect::ClosureCapture { .. }
                | Effect::UnresolvedCall { .. }
                | Effect::PossibleAliasing { .. }
                | Effect::Drop { .. }
                | Effect::PinnedReference { .. }
        )
    });
    if !outer_non_opacity_pure || !inner_non_opacity_pure {
        return Ok(None);
    }

    if formal_idx >= outer.formals.len() {
        return Ok(None);
    }

    let inner_result_name = inner.result_var_name();
    let inner_post_renamed = substitute_in_formula(
        inner.post.clone(),
        "result",
        &IrTerm::Var {
            name: inner_result_name.clone(),
        },
    );
    let inner_value = match find_result_equation(&inner_post_renamed, &inner_result_name) {
        Some(t) => t,
        None => return Ok(None),
    };
    let outer_formal = &outer.formals[formal_idx];
    let outer_pre_substituted =
        substitute_in_formula(outer.pre.clone(), outer_formal, &inner_value);
    let outer_post_substituted =
        substitute_in_formula(outer.post.clone(), outer_formal, &inner_value);
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

/// Compose with the inner being an already-composed contract.
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
    let component_values: Vec<Arc<Value>> = components
        .iter()
        .map(|c| Value::string(c.clone()))
        .collect();
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
/// Per CCP §2 + §9: this is the canonical primitive named in the spec.
/// The chain's overall CID is derivable from its component CIDs in
/// order; re-composing the same chain produces the same CID
/// byte-for-byte. Cross-language consumers (C lifter via FFI, future
/// JSON-RPC subprocess) call into this function.
pub fn compose_chain_contracts(steps: &[ChainStep<'_>]) -> Option<ComposedFunctionContract> {
    if steps.len() < 2 {
        return None;
    }
    let mut acc =
        compose_function_contracts(steps[1].contract, steps[0].contract, steps[1].formal_idx)?;
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

/// Find a top-level `<var> = <expr>` equation in a formula and return
/// the expr term. Used to extract a function's result-value from its
/// post for composition.
fn find_result_equation(formula: &IrFormula, var_name: &str) -> Option<IrTerm> {
    match formula {
        IrFormula::Atomic { name, args } if name == "=" && args.len() == 2 => {
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

// ============================================================
// JCS / CID helpers (canonical encoding glue)
//
// Duplicated from walk's canonical.rs so this module is self-contained.
// Both impls operate over identical types from provekit-ir-types and
// produce byte-equivalent output by construction. CCP §5 requires the
// compose primitive be the single canonical home; the wp / canonical
// modules in walk persist for the rest of walk's pipeline.
// ============================================================

use serde_json::Value as JsonValue;

fn serde_to_canonical(j: JsonValue) -> Arc<Value> {
    match j {
        JsonValue::Null => Value::null(),
        JsonValue::Bool(b) => Value::boolean(b),
        JsonValue::Number(n) => match n.as_i64() {
            Some(i) => Value::integer(i),
            None => Value::object(vec![(
                "__provekit_non_i64_number__".to_string(),
                Value::string(n.to_string()),
            )]),
        },
        JsonValue::String(s) => Value::string(s),
        JsonValue::Array(items) => {
            let mapped: Vec<Arc<Value>> = items.into_iter().map(serde_to_canonical).collect();
            Value::array(mapped)
        }
        JsonValue::Object(map) => {
            let entries: Vec<(String, Arc<Value>)> = map
                .into_iter()
                .map(|(k, v)| (k, serde_to_canonical(v)))
                .collect();
            Value::object(entries)
        }
    }
}

/// Canonicalize an `IrFormula` into a JCS-canonicalizer Value tree.
pub fn formula_to_canonical(f: &IrFormula) -> Arc<Value> {
    let serde =
        serde_json::to_value(f).expect("IrFormula serializes (provekit-ir-types is generated)");
    serde_to_canonical(serde)
}

/// Compute the BLAKE3-512 CID of a canonicalizer Value, JCS-encoded.
/// Returns the spec's `"blake3-512:<hex>"` self-identifying string.
pub fn cid_of_value(v: &Value) -> String {
    blake3_512_of(encode_jcs(v).as_bytes())
}

/// Encode a canonicalizer Value to JCS bytes.
pub fn jcs_bytes_of_value(v: &Value) -> Vec<u8> {
    encode_jcs(v).into_bytes()
}

// ============================================================
// Substitution (capture-avoiding) over IrFormula / IrTerm.
//
// Duplicated from walk's wp.rs for the same reason as the canonical
// helpers: this module is the canonical home for the algebra and must
// be self-contained.
// ============================================================

/// Substitute `replacement` for every occurrence of `Var { name == var_name }`
/// in `formula`. Capture-avoiding via alpha-rename of binders whose
/// bound name appears free in `replacement`.
pub fn substitute_in_formula(
    formula: IrFormula,
    var_name: &str,
    replacement: &IrTerm,
) -> IrFormula {
    match formula {
        IrFormula::Atomic { name, args } => IrFormula::Atomic {
            name,
            args: args
                .into_iter()
                .map(|t| substitute_in_term(t, var_name, replacement))
                .collect(),
        },
        IrFormula::And { operands } => IrFormula::And {
            operands: operands
                .into_iter()
                .map(|f| substitute_in_formula(f, var_name, replacement))
                .collect(),
        },
        IrFormula::Or { operands } => IrFormula::Or {
            operands: operands
                .into_iter()
                .map(|f| substitute_in_formula(f, var_name, replacement))
                .collect(),
        },
        IrFormula::Not { operands } => IrFormula::Not {
            operands: operands
                .into_iter()
                .map(|f| substitute_in_formula(f, var_name, replacement))
                .collect(),
        },
        IrFormula::Implies { operands } => IrFormula::Implies {
            operands: operands
                .into_iter()
                .map(|f| substitute_in_formula(f, var_name, replacement))
                .collect(),
        },
        IrFormula::Forall { name, sort, body } => {
            let (name, body) = handle_formula_binder(name, body, var_name, replacement);
            IrFormula::Forall { name, sort, body }
        }
        IrFormula::Exists { name, sort, body } => {
            let (name, body) = handle_formula_binder(name, body, var_name, replacement);
            IrFormula::Exists { name, sort, body }
        }
        IrFormula::Choice {
            var_name: bound,
            sort,
            body,
        } => {
            let (bound, body) = handle_formula_binder(bound, body, var_name, replacement);
            IrFormula::Choice {
                var_name: bound,
                sort,
                body,
            }
        }
    }
}

fn handle_formula_binder(
    bound: String,
    body: Box<IrFormula>,
    var_name: &str,
    replacement: &IrTerm,
) -> (String, Box<IrFormula>) {
    if bound == var_name {
        return (bound, body);
    }
    let replacement_free = free_vars_term(replacement);
    if replacement_free.contains(&bound) {
        let mut taken = replacement_free;
        taken.extend(free_vars_formula(&body));
        taken.insert(var_name.to_string());
        taken.insert(bound.clone());
        let fresh = fresh_name(&taken, &bound);
        let renamed = substitute_in_formula(
            *body,
            &bound,
            &IrTerm::Var {
                name: fresh.clone(),
            },
        );
        let substituted = substitute_in_formula(renamed, var_name, replacement);
        (fresh, Box::new(substituted))
    } else {
        let body_subst = substitute_in_formula(*body, var_name, replacement);
        (bound, Box::new(body_subst))
    }
}

fn substitute_in_term(term: IrTerm, var_name: &str, replacement: &IrTerm) -> IrTerm {
    match term {
        IrTerm::Var { name } => {
            if name == var_name {
                replacement.clone()
            } else {
                IrTerm::Var { name }
            }
        }
        IrTerm::Const { value, sort } => IrTerm::Const { value, sort },
        IrTerm::Ctor { name, args } => IrTerm::Ctor {
            name,
            args: args
                .into_iter()
                .map(|t| substitute_in_term(t, var_name, replacement))
                .collect(),
        },
        IrTerm::Lambda {
            param_name,
            param_sort,
            body,
        } => {
            if param_name == var_name {
                IrTerm::Lambda {
                    param_name,
                    param_sort,
                    body,
                }
            } else {
                let replacement_free = free_vars_term(replacement);
                if replacement_free.contains(&param_name) {
                    let mut taken = replacement_free;
                    taken.extend(free_vars_term(&body));
                    taken.insert(var_name.to_string());
                    taken.insert(param_name.clone());
                    let fresh = fresh_name(&taken, &param_name);
                    let renamed = substitute_in_term(
                        *body,
                        &param_name,
                        &IrTerm::Var {
                            name: fresh.clone(),
                        },
                    );
                    let substituted = substitute_in_term(renamed, var_name, replacement);
                    IrTerm::Lambda {
                        param_name: fresh,
                        param_sort,
                        body: Box::new(substituted),
                    }
                } else {
                    let body = Box::new(substitute_in_term(*body, var_name, replacement));
                    IrTerm::Lambda {
                        param_name,
                        param_sort,
                        body,
                    }
                }
            }
        }
        IrTerm::Let { bindings, body } => {
            let replacement_free = free_vars_term(replacement);
            let mut new_bindings: Vec<provekit_ir_types::LetBinding> =
                Vec::with_capacity(bindings.len());
            let mut shadowed = false;
            let mut prior_renames: Vec<(String, IrTerm)> = Vec::new();

            for b in bindings.into_iter() {
                let mut bound_term = b.bound_term;
                for (old, new) in &prior_renames {
                    bound_term = substitute_in_term(bound_term, old, new);
                }
                if !shadowed {
                    bound_term = substitute_in_term(bound_term, var_name, replacement);
                }
                let mut name = b.name;
                if !shadowed && replacement_free.contains(&name) {
                    let mut taken = replacement_free.clone();
                    taken.insert(name.clone());
                    taken.insert(var_name.to_string());
                    let fresh = fresh_name(&taken, &name);
                    prior_renames.push((
                        name.clone(),
                        IrTerm::Var {
                            name: fresh.clone(),
                        },
                    ));
                    name = fresh;
                }
                if name == var_name {
                    shadowed = true;
                }
                new_bindings.push(provekit_ir_types::LetBinding { name, bound_term });
            }

            let mut body_term = *body;
            for (old, new) in &prior_renames {
                body_term = substitute_in_term(body_term, old, new);
            }
            if !shadowed {
                body_term = substitute_in_term(body_term, var_name, replacement);
            }
            IrTerm::Let {
                bindings: new_bindings,
                body: Box::new(body_term),
            }
        }
    }
}

fn free_vars_term(t: &IrTerm) -> HashSet<String> {
    let mut acc = HashSet::new();
    free_vars_term_into(t, &mut acc);
    acc
}

fn free_vars_term_into(t: &IrTerm, acc: &mut HashSet<String>) {
    match t {
        IrTerm::Var { name } => {
            acc.insert(name.clone());
        }
        IrTerm::Const { .. } => {}
        IrTerm::Ctor { args, .. } => {
            for a in args {
                free_vars_term_into(a, acc);
            }
        }
        IrTerm::Lambda {
            param_name, body, ..
        } => {
            let mut inner = HashSet::new();
            free_vars_term_into(body, &mut inner);
            inner.remove(param_name);
            acc.extend(inner);
        }
        IrTerm::Let { bindings, body } => {
            let mut bound_so_far: HashSet<String> = HashSet::new();
            for b in bindings {
                let mut bf = HashSet::new();
                free_vars_term_into(&b.bound_term, &mut bf);
                for name in &bound_so_far {
                    bf.remove(name);
                }
                acc.extend(bf);
                bound_so_far.insert(b.name.clone());
            }
            let mut bf = HashSet::new();
            free_vars_term_into(body, &mut bf);
            for name in &bound_so_far {
                bf.remove(name);
            }
            acc.extend(bf);
        }
    }
}

fn free_vars_formula(f: &IrFormula) -> HashSet<String> {
    let mut acc = HashSet::new();
    free_vars_formula_into(f, &mut acc);
    acc
}

fn free_vars_formula_into(f: &IrFormula, acc: &mut HashSet<String>) {
    match f {
        IrFormula::Atomic { args, .. } => {
            for a in args {
                free_vars_term_into(a, acc);
            }
        }
        IrFormula::And { operands }
        | IrFormula::Or { operands }
        | IrFormula::Not { operands }
        | IrFormula::Implies { operands } => {
            for o in operands {
                free_vars_formula_into(o, acc);
            }
        }
        IrFormula::Forall { name, body, .. } | IrFormula::Exists { name, body, .. } => {
            let mut inner = HashSet::new();
            free_vars_formula_into(body, &mut inner);
            inner.remove(name);
            acc.extend(inner);
        }
        IrFormula::Choice { var_name, body, .. } => {
            let mut inner = HashSet::new();
            free_vars_formula_into(body, &mut inner);
            inner.remove(var_name);
            acc.extend(inner);
        }
    }
}

fn fresh_name(taken: &HashSet<String>, base: &str) -> String {
    if !taken.contains(base) {
        return base.to_string();
    }
    let mut n: u32 = 1;
    loop {
        let candidate = format!("{}_{}", base, n);
        if !taken.contains(&candidate) {
            return candidate;
        }
        n += 1;
    }
}
