// SPDX-License-Identifier: Apache-2.0
//
// Body-discharge: reduce a harvested-assertion obligation through the
// CALLEE FUNCTION BODY, so the solver sees the body's value-semantics
// (`double(x) = x*2`) instead of an uninterpreted symbol `double`.
//
// This is the verification spine the rest of the pipeline was missing
// (#1440). The pieces it wires together already existed:
//
//   - `libprovekit::wp` — the weakest-precondition evaluator that inlines
//     a callee's body value-expression into the postcondition. It is a
//     REDUCTION (push the postcondition back through the body, extract the
//     obligation), not a re-encoding of the body as ProofIR.
//
//   - the body-derived `post` carried on a contract memento: for
//     `fn double(x) -> i64 { x * 2 }`, walk's `lift_function_postcondition`
//     derives `post = (result == *(x, 2))` — the actual definition, lifted.
//     (This is walk's lift-half, the verification substrate; NOT the
//     lower/cycle/carrier machinery.)
//
//   - the bridge `sourceSymbol -> targetContractCid`, which names the
//     body-derived op-contract for a harvested call.
//
// What this module adds:
//
//   1. `CatalogResolver` — the first non-test `OpContractResolver`. Given
//      an op name it finds the bridge by symbol, follows
//      `targetContractCid` to the contract memento, and projects its
//      `formals` + `post` into an `OpContractInfo` wp can consume.
//
//   2. `extract_body_obligation` — for a body-bearing callsite whose
//      harvested assertion has the shape `=(<call>, <expected>)`,
//      reconstructs the call as a `core::Term::Op` (Ctor->Op, Delta 1),
//      derives the postcondition `Q = (result == <expected>)`, and runs
//      `wp(call, Q, resolver)`. The result is a body-reduced obligation
//      (e.g. `*(3, 2) == 6`) with no uninterpreted callee symbol.
//
// Anything outside the recognized `=(<call>, <expected>)` shape returns
// `None`: the caller falls through to the existing instantiate path. The
// spine discharges a real body-obligation; it does not try to be the full
// gauntlet.
//
// Delta: `=(<call_a>, <call_b>)` (eq-both-calls tier, #eq-both-calls)
//
// When BOTH sides of an `=` assertion are the SAME callee, reduce each
// independently through the body and emit `=(body_a, body_b)`. This
// handles `assert_eq!(f(args), f(args))` determinism tests:
//
//   POSITIVE: `=(double(3), double(3))` -> `=(*(3,2), *(3,2))` -> z3 UNSAT
//             (reflexive; labeled Reflexive in the discharge method).
//
//   NEGATIVE: `=(double(3), double(4))` -> `=(6, 8)` -> z3 SAT on the
//             negation -> NOT discharged. Real z3 runs on the concrete
//             reduced formula; this is not a SKIPPED-as-pass.
//
// SOUNDNESS: wp inlines the body definition (the ground truth); it does
// NOT assume the callee's postcondition as an axiom. Reduction of both
// sides is identical to the existing `=(call, expected)` path. No self-
// circular assumption is introduced.

use serde_json::Value as Json;

use libprovekit::core::types::Term;
use libprovekit::wp::{self, OpContractInfo, OpContractResolver, SlotInfo, WpError};
use provekit_ir_types::{IrFormula, IrTerm};

use crate::types::{memento_body, memento_kind, CallSite, MementoPool};

/// Does the callsite's RESOLVED TARGET CONTRACT carry a non-trivial `pre`
/// (a real precondition), as opposed to None or the literal-true tautology?
///
/// This is the ROUTING PREDICATE for the call-site obligation. It is generic
/// and LANGUAGE-BLIND: it inspects the contract body's `pre` field as opaque
/// JSON and recognizes no Rust predicate name (`is_some`/`is_ok`/...), no
/// callee name (`option_unwrap`/...), and no kit. The contract carrying a real
/// `pre` is the structural signal that this call-site obligation is to
/// DISCHARGE THAT `pre` UNDER THE GUARD CONTEXT (`cs.guard_facts`), and that
/// the discharge MUST take precedence over the reflexive self-post path that
/// `extract_body_obligation` would otherwise take.
///
/// The pre is read from the EXACT contract `resolve_target` resolves
/// (`cs.bridge_target_cid`), so the routing decision and the subsequent guard
/// discharge are about the same contract.
///
/// "Non-trivial" means: the contract body has a `pre` field AND that pre is
/// not the literal-true atomic (`{kind:atomic, name:"true"}`). A post-only
/// sugar contract carries no `pre` -> `false` (stays on the body-discharge
/// path). A genuinely-total contract carrying `pre = true` (e.g. an
/// `unwrap_or`) is also `false`: a vacuously-true pre is nothing to discharge,
/// so rerouting it would only mis-bucket a body-derived discharge. Any missing
/// or unresolvable contract returns `false`, keeping the no-pre path
/// byte-identical to its prior behavior.
///
/// SOUNDNESS: this predicate only ever DIVERTS a call site away from the
/// reflexive self-post path TOWARD the guard-discharge path. Diverting toward
/// the guard path can only UNDER-prove (an unguarded site stays undecidable);
/// it can never mint a false "cannot panic." The reflexive path is the one
/// that over-claimed on an unguarded pre-bearing site (the bug this fixes).
pub fn target_has_nontrivial_pre(cs: &CallSite, pool: &MementoPool) -> bool {
    let Some(env) = pool.mementos.get(&cs.bridge_target_cid) else {
        return false;
    };
    if memento_kind(env) != Some("contract") {
        return false;
    }
    let Some(body) = memento_body(env).filter(|v| v.is_object()) else {
        return false;
    };
    match body.get("pre") {
        None => false,
        Some(pre) => !pre_is_trivial(pre),
    }
}

/// True iff `pre` is the trivially-valid precondition: the literal-true
/// atomic (`{kind:atomic, name:"true"}`) or JSON `null`. A trivial pre is
/// nothing to discharge and must NOT trigger the guard-discharge route.
fn pre_is_trivial(pre: &Json) -> bool {
    if pre.is_null() {
        return true;
    }
    pre.get("kind").and_then(|v| v.as_str()) == Some("atomic")
        && pre.get("name").and_then(|v| v.as_str()) == Some("true")
}

/// The variable name the derived postcondition equates the call's result
/// with. Matches `libprovekit::wp::DEFAULT_RESULT_VAR` and the
/// body-derived contract's `result = ...` shape.
const RESULT_VAR: &str = "result";

/// A catalog/pool-backed [`OpContractResolver`]. Resolves an op name to
/// its body-derived op-contract by walking the bridge index:
///
///   op_name -> bridges_by_symbol[op_name].targetContractCid
///           -> mementos[targetContractCid].body
///           -> OpContractInfo { slots = formals, post }
///
/// This is the production resolver wp needs; the only other impl is the
/// in-memory `MapResolver` used by wp's unit tests.
pub struct CatalogResolver<'a> {
    pool: &'a MementoPool,
}

impl<'a> CatalogResolver<'a> {
    /// Build a resolver over a loaded memento pool.
    pub fn new(pool: &'a MementoPool) -> Self {
        Self { pool }
    }

    /// Find the contract memento body that the bridge for `op_name`
    /// targets. Returns the contract body JSON object, or `None` if there
    /// is no bridge for that symbol, the target CID is not in the pool, or
    /// the target memento is not a contract.
    fn target_contract_body(&self, op_name: &str) -> Option<&'a Json> {
        let bridge = self.pool.bridges_by_symbol.get(op_name)?;
        let bbody = bridge.get("evidence").and_then(|e| e.get("body"));
        // v1.2-layered bridges carry the fields on `header`; v1.1-flat on
        // `evidence.body`. Try flat first, then the header form.
        let target_cid = bbody
            .and_then(|b| b.get("targetContractCid"))
            .or_else(|| bridge.pointer("/header/targetContractCid"))
            .and_then(|v| v.as_str())?;
        let env = self.pool.mementos.get(target_cid)?;
        if memento_kind(env) != Some("contract") {
            return None;
        }
        memento_body(env).filter(|v| v.is_object())
    }
}

impl OpContractResolver for CatalogResolver<'_> {
    fn lookup(&self, op_name: &str) -> Option<OpContractInfo> {
        let body = self.target_contract_body(op_name)?;

        // The body-derived op-contract carries the function's formals as a
        // `formals` array (written by `core::bind`'s bridge writer). The
        // slot names MUST match the free variables of `post` so wp can
        // substitute the call's arg into the right formal. A contract with
        // no `formals` is NOT a body-derived op-contract (e.g. the older
        // cross-language refinement-target contracts that carry only a
        // `pre` forall); return `None` so the caller falls through to the
        // existing refinement path rather than mis-firing wp on it.
        let formals: Vec<String> = body
            .get("formals")
            .and_then(|v| v.as_array())?
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();

        // The body-derived postcondition (`result == <value_expr>`). A
        // body-derived op-contract MUST carry one; without it there is
        // nothing for wp to inline, so this is not a body-bearing target.
        let post: IrFormula = serde_json::from_value(body.get("post")?.clone()).ok()?;

        // A value-op has one value slot per formal; no Stmt slots (the body
        // value-expression is a pure value, not a sub-statement).
        let slots: Vec<SlotInfo> = formals.iter().map(SlotInfo::value).collect();

        let mut info = OpContractInfo::new(slots);
        info.post = Some(post);
        // Require a recognizable `result == <value_expr>` shape; otherwise
        // wp has no value expression to inline and would error. Falling
        // through is the honest posture for a non-body-derived contract.
        info.value_expr()?;
        Some(info)
    }
}

/// The body-discharge route that produced an obligation. This is separate from
/// [`DischargeMethod`]: the route says WHICH reducer path produced the formula;
/// the method says HOW a discharged formula was proven.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyDischargeTier {
    /// Standard harvested assertion shape: `=(<call>, <expected>)`.
    CallExpected,
    /// Both sides are the same callee: `=(<call_a>, <call_b>)`.
    EqBothCallsSameCallee,
}

impl BodyDischargeTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CallExpected => "body-call-expected",
            Self::EqBothCallsSameCallee => "body-eq-same-callee",
        }
    }
}

/// The kind of obligation `extract_body_obligation` produced, for the caller's
/// receipt/report row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BodyObligation {
    /// A body-reduced obligation formula, ready for the SMT emitter. The
    /// callee symbol has been inlined; no uninterpreted constant remains.
    Reduced {
        formula: Json,
        tier: BodyDischargeTier,
    },
}

/// Try to build a body-reduced obligation for a body-bearing callsite.
///
/// Returns `Ok(Some(..))` when:
///   - the callsite's harvested assertion is `=(<call>, <expected>)` with
///     `<call>` the ctor whose name == the bridge's sourceSymbol, AND
///   - the resolver knows the callee's body-derived op-contract.
///
/// Returns `Ok(None)` ONLY when the callee has **no** body-derived
/// op-contract: the claim is genuinely not body-bearing and the caller
/// falls through to the existing (instantiate-based) refinement path.
///
/// Returns `Err` (a REFUSAL) when the callee **does** have a body-derived
/// op-contract but the obligation cannot be reduced through it: the
/// assertion shape is not the recognized `=(<call>, <expected>)`, an
/// operand is unconvertible, or wp itself refuses/errors. This is the
/// load-bearing honesty boundary: once a callee is body-bearing, the spine
/// MUST either reduce the obligation or refuse it. It must NOT fall through
/// to the refinement path, because that path treats a body-derived
/// op-contract (which carries `post`/`formals` but no `pre`) as VACUOUS and
/// reports a false `discharged`/`pass` for a claim it never checked.
pub fn extract_body_obligation(
    cs: &CallSite,
    pool: &MementoPool,
) -> Result<Option<BodyObligation>, String> {
    let resolver = CatalogResolver::new(pool);

    // The callee must have a body-derived contract; otherwise this is not
    // a body-bearing claim (fall through honestly).
    if resolver.lookup(&cs.bridge_ir_name).is_none() {
        return Ok(None);
    }

    // From here on the callee IS body-bearing. Every failure to build the
    // obligation is a REFUSAL (`Err`), never an `Ok(None)` fall-through —
    // see the doc comment's honesty boundary.

    // Recognize the `=(<call>, <expected>)` assertion shape. The callsite
    // carries the containing atomic (the `=` predicate the call sits in),
    // captured by `enumerate_callsites`.
    let Some((call_json, expected_json)) = recognize_eq_call(cs) else {
        // Delta: eq-both-calls tier. When BOTH sides of the `=` assertion are
        // the same callee `=(<call_a>, <call_b>)`, reduce each independently
        // through the body and emit `=(body_a, body_b)` for the solver.
        // This handles `assert_eq!(f(args), f(args))` determinism tests.
        // This branch is tried BEFORE the refusal so we can discharge the
        // obligation soundly rather than leaving it undecidable.
        if let Some((call_a_json, call_b_json)) = recognize_eq_both_calls(cs) {
            return extract_eq_both_calls_obligation(cs, call_a_json, call_b_json, pool);
        }

        let shape = cs
            .containing_atomic
            .as_ref()
            .and_then(|a| a.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("<none>");
        return Err(format!(
            "refuse: callee `{}` has a body-derived contract but the harvested \
             assertion is not the reducible `=(<call>, <expected>)` shape \
             (containing predicate: `{shape}`); the body-discharge spine \
             refuses rather than reporting a vacuous pass",
            cs.bridge_ir_name
        ));
    };

    // Ctor->Op (Delta 1): deserialize the harvested call as an `IrTerm`
    // (a `ctor`), then convert into a `core::Term::Op` so wp's `wp_op`
    // dispatches on it. The op CID is the deterministic name-derived CID.
    let call_ir: IrTerm = serde_json::from_value(call_json.clone()).map_err(|e| {
        format!(
            "refuse: callee `{}` is body-bearing but its call term did not \
             deserialize as an IR term: {e}",
            cs.bridge_ir_name
        )
    })?;
    let call_term: Term = call_ir.into();
    // Guard: the recognized call must actually be an op (a ctor with args).
    if !matches!(call_term, Term::Op { .. }) {
        return Err(format!(
            "refuse: callee `{}` is body-bearing but its call term is not an \
             op (no args to reduce through the body)",
            cs.bridge_ir_name
        ));
    }

    // The expected value (RHS of the assertion) becomes the postcondition:
    //   Q = (result == <expected>)
    let expected_ir: IrTerm = serde_json::from_value(expected_json.clone()).map_err(|e| {
        format!(
            "refuse: callee `{}` is body-bearing but the assertion's expected \
             value did not deserialize as an IR term: {e}",
            cs.bridge_ir_name
        )
    })?;
    let q = IrFormula::Atomic {
        name: "=".to_string(),
        args: vec![
            IrTerm::Var {
                name: RESULT_VAR.to_string(),
            },
            expected_ir,
        ],
    };

    // Reduce: push Q back through the body. wp inlines `double`'s body
    // value-expression, leaving e.g. `*(3, 2) == 6` (no `double` symbol).
    match wp::wp(&call_term, &q, &resolver) {
        Ok(reduced) => {
            let reduced_json = serde_json::to_value(&reduced)
                .map_err(|e| format!("wp obligation serialize: {e}"))?;
            Ok(Some(BodyObligation::Reduced {
                formula: reduced_json,
                tier: BodyDischargeTier::CallExpected,
            }))
        }
        Err(WpError::Refused(r)) => {
            // The top-level callee resolved (we are past the lookup gate),
            // but wp could not complete the reduction — e.g. a nested call
            // whose contract has not landed. This is a body-bearing claim we
            // cannot discharge, so we REFUSE: falling through to the
            // refinement path would mis-read the body-derived op-contract as
            // vacuous and report a false pass.
            Err(format!(
                "refuse: callee `{}` is body-bearing but wp could not reduce \
                 the obligation: {r}",
                cs.bridge_ir_name
            ))
        }
        Err(e) => Err(format!("wp body-reduction failed: {e}")),
    }
}

/// Build a body-reduced obligation for the `=(<call_a>, <call_b>)` shape
/// where BOTH sides are the same callee.
///
/// Reduces EACH call independently through its body-derived contract via
/// wp, then emits `=(body_a, body_b)` as the concrete obligation. When the
/// calls have identical arguments, the result is a reflexive equality
/// (`=(E, E)`) that z3 discharges in UNSAT; when they differ, z3 checks
/// the concrete reduced formula.
///
/// SOUNDNESS: wp inlines the body (the function definition); it does NOT
/// assume the callee's postcondition as an axiom. Both reductions are the
/// same mechanism as the existing `=(call, expected)` path. No circular
/// self-assumption is introduced. falsePass=0 is preserved: only a
/// genuinely-valid body-equality discharges; a false one refutes via z3.
///
/// This function is called from `extract_body_obligation` after
/// `recognize_eq_call` returns `None` and `recognize_eq_both_calls`
/// matches. It is an internal path, not a public entry point.
fn extract_eq_both_calls_obligation(
    cs: &CallSite,
    call_a_json: &Json,
    call_b_json: &Json,
    pool: &MementoPool,
) -> Result<Option<BodyObligation>, String> {
    let resolver = CatalogResolver::new(pool);

    // Reduce call_a to its body value expression.
    let value_a: IrTerm = reduce_to_value_expr(call_a_json, &cs.bridge_ir_name, &resolver)
        .map_err(|e| format!("body-discharge: eq-both-calls: call_a: {e}"))?;

    // Reduce call_b to its body value expression.
    let value_b: IrTerm = reduce_to_value_expr(call_b_json, &cs.bridge_ir_name, &resolver)
        .map_err(|e| format!("body-discharge: eq-both-calls: call_b: {e}"))?;

    // Build the final obligation: =(body_a, body_b).
    // When args are identical this is reflexive (body_a == body_b structurally);
    // when they differ, z3 checks the concrete arithmetic/algebraic equality.
    let obligation = IrFormula::Atomic {
        name: "=".to_string(),
        args: vec![value_a, value_b],
    };
    let obligation_json = serde_json::to_value(&obligation)
        .map_err(|e| format!("body-discharge: eq-both-calls: serialize: {e}"))?;

    Ok(Some(BodyObligation::Reduced {
        formula: obligation_json,
        tier: BodyDischargeTier::EqBothCallsSameCallee,
    }))
}

/// How a DISCHARGED obligation was proven. This is the honesty axis the
/// reflexive-discharge work introduces: a `result == <body term>`
/// obligation (the function's self-derived post) reduces, after wp
/// inlines the body, to `<term> == <term>`, which any solver proves by
/// reflexivity/congruence WITHOUT understanding the term. Such a discharge
/// is SOUND but SHALLOW: it proves "the function returns what it returns,"
/// not anything about behavior. It MUST be counted apart from a discharge
/// where the solver did substantive work (real arithmetic, a non-trivial
/// implication). Never conflate the two in a report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DischargeMethod {
    /// Proven by reflexivity/congruence alone: the obligation is a
    /// conjunction of structurally-identical equalities (`T == T`) over
    /// uninterpreted terms. Sound but shallow.
    Reflexive,
    /// The solver did substantive work: the obligation contains an
    /// equality whose two sides differ, or an arithmetic/relational atom
    /// that is not a trivial reflexive identity.
    Substantive,
}

impl DischargeMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Reflexive => "reflexive",
            Self::Substantive => "solver-substantive",
        }
    }
}

/// Classify a DISCHARGED obligation as [`DischargeMethod::Reflexive`] or
/// [`DischargeMethod::Substantive`]. Call ONLY on obligations the solver
/// returned `Discharged` for; the classification is about HOW it was
/// proven, not WHETHER.
///
/// Reflexive iff every leaf the validity of the obligation rests on is a
/// structurally-identical equality `=(a, a)` (or a literal `true`). The
/// moment a leaf is an equality with distinct sides, or any other
/// relational/arithmetic atom, the proof needed congruence over a real
/// computation or an SMT theory: that is `Substantive`. Negation and
/// conjunction/disjunction recurse. This is deliberately conservative:
/// when in doubt it returns `Substantive`, so the reflexive bucket never
/// over-claims.
pub fn classify_discharge_method(obligation: &Json) -> DischargeMethod {
    if formula_is_reflexive(obligation) {
        DischargeMethod::Reflexive
    } else {
        DischargeMethod::Substantive
    }
}

/// True iff this formula's validity rests purely on reflexive equalities
/// and literal truths. Any equality with structurally-distinct sides, or
/// any non-`=` relational atom, makes it non-reflexive (the solver did
/// real work).
fn formula_is_reflexive(f: &Json) -> bool {
    let Some(kind) = f.get("kind").and_then(|v| v.as_str()) else {
        return false;
    };
    match kind {
        "atomic" => {
            let name = f.get("name").and_then(|v| v.as_str()).unwrap_or("");
            match name {
                // The nullary truth literal is reflexively valid.
                "true" => true,
                // An equality is reflexive iff its two operands are the
                // SAME IR term. `Ok(x) == Ok(x)` -> reflexive; `Ok(x) ==
                // Err(x)` or `*(3,2) == 6` -> not (distinct sides / real
                // computation).
                "=" | "eq" => match f.get("args").and_then(|v| v.as_array()) {
                    Some(args) if args.len() == 2 => args[0] == args[1],
                    _ => false,
                },
                // Any other relational/arithmetic predicate (`<`, `≤`,
                // `distinct`, an uninterpreted boolean predicate, ...)
                // needed substantive reasoning.
                _ => false,
            }
        }
        "and" | "or" => f
            .get("operands")
            .and_then(|v| v.as_array())
            .map(|ops| ops.iter().all(formula_is_reflexive))
            .unwrap_or(false),
        "not" => f
            .get("operands")
            .and_then(|v| v.as_array())
            .and_then(|ops| ops.first())
            .map(formula_is_reflexive)
            .unwrap_or(false),
        // Quantifiers / implications / wp-schema nodes: not a reflexive
        // shape (conservatively substantive).
        _ => false,
    }
}

/// Extract a guard fact from the callee's postcondition.
///
/// When a callsite's `arg_term` is a `ctor` (i.e. the argument to the
/// partial is the RETURN VALUE of another function call), look up that
/// callee's contract in the pool and check whether its `post` is the
/// `is_ok(result)` postcondition. If so, return `is_ok(arg_term)` as an
/// established fact -- the callee's totality contract supplies the fact
/// that its result is always `Ok`, so the receiving `unwrap()` cannot
/// panic.
///
/// This is the CROSS-FUNCTION-POSTCONDITION-AS-ASSUMABLE-FACT mechanism
/// (Phase 2 Tier D-lib). It is LANGUAGE-BLIND: it reads the post field as
/// opaque JSON, recognizes no callee name, no library name, and no
/// type name. The only structural signal is:
///
///   1. `cs.arg_term` is a `ctor` (the arg is a call expression, not a bare var)
///   2. The ctor name has a bridge in the pool
///   3. The bridge's target contract has `post = is_ok(result)`
///      (the strengthened singleton totality post, NOT the generic
///      `is_ok || is_err` disjunction)
///
/// SOUNDNESS: the postcondition is read from a contract in the pool whose
/// soundness is the contract author's responsibility. This function only
/// checks structural shape. A false `is_ok` post is a false contract, not
/// a verifier bug.
///
/// DISCRIMINATION: when the bridge target's post is NOT exactly
/// `is_ok(result)` (e.g. it is the generic `is_ok(result) || is_err(result)`
/// or any other predicate), this returns `None`. Only the STRENGTHENED
/// singleton triggers the fact supply.
///
/// Returns `Some(is_ok(arg_term))` when all three conditions hold, else `None`.
pub fn callee_post_guard_fact(cs: &CallSite, pool: &MementoPool) -> Option<Json> {
    // Condition 1: the arg_term must be a `ctor` (a call expression, not a var).
    let arg = cs.arg_term.as_ref()?;
    if arg.get("kind").and_then(|v| v.as_str()) != Some("ctor") {
        return None;
    }
    let ctor_name = arg.get("name").and_then(|v| v.as_str())?;

    // Condition 2: find a bridge for this ctor name and follow it to the target contract.
    let bridge = pool.bridges_by_symbol.get(ctor_name)?;
    let target_cid = bridge
        .get("evidence")
        .and_then(|e| e.get("body"))
        .and_then(|b| b.get("targetContractCid"))
        .or_else(|| bridge.pointer("/header/targetContractCid"))
        .and_then(|v| v.as_str())?;

    let env = pool.mementos.get(target_cid)?;
    if memento_kind(env) != Some("contract") {
        return None;
    }
    let body = memento_body(env).filter(|v| v.is_object())?;

    // Condition 3: the contract's `post` is exactly `is_ok(result)`.
    let post = body.get("post")?;
    if !post_is_is_ok_of_result(post) {
        return None;
    }

    // Supply the fact: `is_ok(arg_term)`. The arg_term (the ctor expression)
    // is the value whose is_ok status the contract guarantees. This is
    // structurally identical to what a syntactic `if result.is_ok()` guard
    // supplies; the discharge engine cannot tell them apart (language-blind).
    Some(serde_json::json!({
        "kind": "atomic",
        "name": "is_ok",
        "args": [arg.clone()]
    }))
}

/// True iff `post` is the singleton totality postcondition `is_ok(result)`.
///
/// Shape: `{"kind": "atomic", "name": "is_ok", "args": [{"kind": "var", "name": "result"}]}`.
///
/// Recognizes ONLY the strengthened singleton, not the generic
/// `is_ok(result) || is_err(result)`. This is the soundness boundary for
/// D-lib: only a contract that explicitly strengthens to ALWAYS-OK carries
/// this post.
fn post_is_is_ok_of_result(post: &Json) -> bool {
    if post.get("kind").and_then(|v| v.as_str()) != Some("atomic") {
        return false;
    }
    if post.get("name").and_then(|v| v.as_str()) != Some("is_ok") {
        return false;
    }
    let args = match post.get("args").and_then(|v| v.as_array()) {
        Some(a) => a,
        None => return false,
    };
    if args.len() != 1 {
        return false;
    }
    args[0].get("kind").and_then(|v| v.as_str()) == Some("var")
        && args[0].get("name").and_then(|v| v.as_str()) == Some("result")
}

/// Recognize the `=(<call>, <expected>)` harvested-assertion shape on a
/// callsite. `<call>` is the ctor whose name == `cs.bridge_ir_name`.
/// Returns `(call_json, expected_json)` — borrowing from the callsite's
/// captured containing atomic — or `None` for any other shape.
fn recognize_eq_call(cs: &CallSite) -> Option<(&Json, &Json)> {
    let atomic = cs.containing_atomic.as_ref()?;
    if atomic.get("kind").and_then(|v| v.as_str()) != Some("atomic") {
        return None;
    }
    if atomic.get("name").and_then(|v| v.as_str()) != Some("=") {
        return None;
    }
    let args = atomic.get("args").and_then(|v| v.as_array())?;
    if args.len() != 2 {
        return None;
    }
    // Find which side is the bridged call ctor; the other is the expected
    // value. The harvester writes `=(call, expected)`, but accept either
    // order defensively.
    let is_call = |t: &Json| -> bool {
        t.get("kind").and_then(|v| v.as_str()) == Some("ctor")
            && t.get("name").and_then(|v| v.as_str()) == Some(cs.bridge_ir_name.as_str())
    };
    if is_call(&args[0]) && !is_call(&args[1]) {
        Some((&args[0], &args[1]))
    } else if is_call(&args[1]) && !is_call(&args[0]) {
        Some((&args[1], &args[0]))
    } else {
        None
    }
}

/// Recognize `=(<call_a>, <call_b>)` where BOTH sides are ctors whose name
/// matches `cs.bridge_ir_name`. This is the "eq-both-calls" variant arising
/// from `assert_eq!(f(x), f(x))` determinism/idempotence tests.
///
/// Returns `(call_a_json, call_b_json)` borrowing from the callsite's
/// containing atomic, or `None` for any other shape.
fn recognize_eq_both_calls(cs: &CallSite) -> Option<(&Json, &Json)> {
    let atomic = cs.containing_atomic.as_ref()?;
    if atomic.get("kind").and_then(|v| v.as_str()) != Some("atomic") {
        return None;
    }
    if atomic.get("name").and_then(|v| v.as_str()) != Some("=") {
        return None;
    }
    let args = atomic.get("args").and_then(|v| v.as_array())?;
    if args.len() != 2 {
        return None;
    }
    let is_call = |t: &Json| -> bool {
        t.get("kind").and_then(|v| v.as_str()) == Some("ctor")
            && t.get("name").and_then(|v| v.as_str()) == Some(cs.bridge_ir_name.as_str())
    };
    // BOTH sides must be the same callee for the eq-both-calls shape.
    // If only one side is the call, that is the standard `=(call, expected)` shape
    // which `recognize_eq_call` handles. This function is called only when
    // `recognize_eq_call` has already returned `None`.
    if is_call(&args[0]) && is_call(&args[1]) {
        Some((&args[0], &args[1]))
    } else {
        None
    }
}

/// Reduce a call term through its body-derived contract to extract the pure
/// value expression. Runs `wp(call, result == sentinel, resolver)` and
/// extracts the LHS of the resulting `body_expr == sentinel` equality.
///
/// This is an internal helper for `extract_eq_both_calls_obligation` only.
/// It is NOT a public API: the callee must already be confirmed body-bearing
/// before calling this (checked by the caller via `resolver.lookup`).
///
/// Returns `Err` when the call cannot be reduced (slot mismatch, wp refusal,
/// unexpected formula shape from wp).
fn reduce_to_value_expr(
    call_json: &Json,
    callee_name: &str,
    resolver: &CatalogResolver<'_>,
) -> Result<IrTerm, String> {
    let call_ir: IrTerm = serde_json::from_value(call_json.clone())
        .map_err(|e| format!("refuse: `{callee_name}` call term deser: {e}"))?;
    let call_term: Term = call_ir.into();
    if !matches!(call_term, Term::Op { .. }) {
        return Err(format!(
            "refuse: callee `{callee_name}` call term is not an op"
        ));
    }

    // Use a sentinel variable that cannot appear in any user formula.
    // wp(call, result == __sentinel) -> body_expr(args) == __sentinel
    const SENTINEL_VAR: &str = "__eq_both_sentinel";
    let q = IrFormula::Atomic {
        name: "=".to_string(),
        args: vec![
            IrTerm::Var {
                name: RESULT_VAR.to_string(),
            },
            IrTerm::Var {
                name: SENTINEL_VAR.to_string(),
            },
        ],
    };

    let reduced = match wp::wp(&call_term, &q, resolver) {
        Ok(f) => f,
        Err(WpError::Refused(r)) => {
            return Err(format!(
                "refuse: callee `{callee_name}` is body-bearing but wp could not \
                 reduce through body for eq-both-calls: {r}"
            ))
        }
        Err(e) => {
            return Err(format!(
                "body-discharge: wp body-reduction failed for eq-both-calls: {e}"
            ))
        }
    };

    // Extract the LHS of `=(body_expr, sentinel)`.
    // wp(call, result==sentinel) -> `=(body_expr, sentinel)`.
    // The LHS (args[0]) IS the body value expression.
    let shape_str = serde_json::to_string(&reduced).unwrap_or_else(|_| "<unserializable>".into());
    match reduced {
        IrFormula::Atomic { name, mut args } if name == "=" && args.len() == 2 => {
            // args[0] = body_expr(call.args), args[1] = __eq_both_sentinel
            Ok(args.swap_remove(0))
        }
        _ => Err(format!(
            "refuse: callee `{callee_name}` wp produced unexpected formula shape \
             for eq-both-calls (expected `=(body_expr, sentinel)`): {shape_str}"
        )),
    }
}

#[cfg(test)]
mod discharge_method_tests {
    use super::*;
    use serde_json::json;

    fn ok_ctor(arg: &str) -> Json {
        json!({"kind": "ctor", "name": "Ok", "args": [{"kind": "var", "name": arg}]})
    }
    fn err_ctor(arg: &str) -> Json {
        json!({"kind": "ctor", "name": "Err", "args": [{"kind": "var", "name": arg}]})
    }

    #[test]
    fn identical_sides_classify_reflexive() {
        // `Ok(x) == Ok(x)`: the self-derived post against its own body. The
        // two sides are structurally identical; provable by reflexivity.
        let ob = json!({"kind": "atomic", "name": "=", "args": [ok_ctor("x"), ok_ctor("x")]});
        assert_eq!(
            classify_discharge_method(&ob),
            DischargeMethod::Reflexive,
            "T == T must classify reflexive"
        );
    }

    #[test]
    fn distinct_sides_classify_substantive_not_reflexive() {
        // SOUNDNESS GUARD. `Ok(x) == Err(x)`: a lifter bug emitting a post
        // that does NOT match the body. The sides differ, so this is NOT
        // reflexive. (z3 would refute it; here we assert the classifier
        // refuses to call it reflexive, so it can never be reported as a
        // shallow-but-fine reflexive discharge.)
        let ob = json!({"kind": "atomic", "name": "=", "args": [ok_ctor("x"), err_ctor("x")]});
        assert_eq!(
            classify_discharge_method(&ob),
            DischargeMethod::Substantive,
            "Ok(x) == Err(x) must NOT classify reflexive"
        );
    }

    #[test]
    fn arithmetic_equality_classifies_substantive() {
        // `*(3, 2) == 6`: a real body-reduced arithmetic obligation. Sides
        // differ structurally; the solver did substantive work.
        let ob = json!({"kind": "atomic", "name": "=", "args": [
            {"kind": "ctor", "name": "*", "args": [
                {"kind": "const", "value": 3, "sort": {"kind": "primitive", "name": "Int"}},
                {"kind": "const", "value": 2, "sort": {"kind": "primitive", "name": "Int"}}
            ]},
            {"kind": "const", "value": 6, "sort": {"kind": "primitive", "name": "Int"}}
        ]});
        assert_eq!(classify_discharge_method(&ob), DischargeMethod::Substantive);
    }

    #[test]
    fn conjunction_of_reflexive_equalities_is_reflexive() {
        let ob = json!({"kind": "and", "operands": [
            {"kind": "atomic", "name": "=", "args": [ok_ctor("x"), ok_ctor("x")]},
            {"kind": "atomic", "name": "true", "args": []}
        ]});
        assert_eq!(classify_discharge_method(&ob), DischargeMethod::Reflexive);
    }

    #[test]
    fn conjunction_with_one_substantive_leaf_is_substantive() {
        let ob = json!({"kind": "and", "operands": [
            {"kind": "atomic", "name": "=", "args": [ok_ctor("x"), ok_ctor("x")]},
            {"kind": "atomic", "name": "<", "args": [
                {"kind": "var", "name": "a"}, {"kind": "var", "name": "b"}
            ]}
        ]});
        assert_eq!(classify_discharge_method(&ob), DischargeMethod::Substantive);
    }
}

#[cfg(test)]
mod routing_predicate_tests {
    //! `target_has_nontrivial_pre`: the call-site-obligation routing rule.
    //! Three tests per behavior (positive, discrimination, structural), so a
    //! false positive (rerouting a no-pre target) and a false negative (failing
    //! to reroute a real pre) are both caught.
    use super::*;
    use crate::types::{CallSite, MementoPool};
    use serde_json::json;

    fn contract_env(body: Json) -> Json {
        json!({"evidence": {"kind": "contract", "body": body}})
    }

    fn pool_with(cid: &str, env: Json) -> MementoPool {
        let mut pool = MementoPool::default();
        pool.mementos.insert(cid.into(), env);
        pool
    }

    fn cs_targeting(cid: &str) -> CallSite {
        CallSite {
            bridge_target_cid: cid.into(),
            ..Default::default()
        }
    }

    #[test]
    fn positive_real_pre_routes_to_guard_discharge() {
        // A real precondition (`is_some(opt)`) is the structural signal to
        // discharge the pre under guards.
        let cid = "blake3-512:has-pre";
        let env = contract_env(json!({
            "pre": {"kind": "atomic", "name": "is_some",
                "args": [{"kind": "var", "name": "opt"}]},
            "post": {"kind": "atomic", "name": "=", "args": []},
            "formals": ["opt"]
        }));
        assert!(target_has_nontrivial_pre(
            &cs_targeting(cid),
            &pool_with(cid, env)
        ));
    }

    #[test]
    fn discrimination_post_only_and_true_pre_do_not_route() {
        // No `pre`: a post-only sugar/body contract stays on body-discharge.
        let cid = "blake3-512:post-only";
        let post_only = contract_env(json!({
            "post": {"kind": "atomic", "name": "=", "args": []},
            "formals": ["x"]
        }));
        assert!(
            !target_has_nontrivial_pre(&cs_targeting(cid), &pool_with(cid, post_only)),
            "a post-only contract must NOT reroute"
        );

        // `pre = true`: trivially valid, nothing to discharge under guards.
        let true_pre = contract_env(json!({
            "pre": {"kind": "atomic", "name": "true", "args": []},
            "formals": ["x"]
        }));
        assert!(
            !target_has_nontrivial_pre(&cs_targeting(cid), &pool_with(cid, true_pre)),
            "a pre=true contract is trivial and must NOT reroute"
        );

        // `pre = null`: absent precondition.
        let null_pre = contract_env(json!({"pre": null, "formals": ["x"]}));
        assert!(
            !target_has_nontrivial_pre(&cs_targeting(cid), &pool_with(cid, null_pre)),
            "a null pre must NOT reroute"
        );
    }

    #[test]
    fn structural_missing_or_non_contract_target_is_false() {
        // Target CID not in the pool -> false (keeps the no-pre path
        // byte-identical; never panics looking up a stale bridge).
        let empty = MementoPool::default();
        assert!(!target_has_nontrivial_pre(
            &cs_targeting("blake3-512:absent"),
            &empty
        ));

        // Target memento is not a contract (e.g. a bridge) -> false.
        let cid = "blake3-512:not-a-contract";
        let bridge_env = json!({"evidence": {"kind": "bridge", "body": {
            "pre": {"kind": "atomic", "name": "is_some",
                "args": [{"kind": "var", "name": "opt"}]}}}});
        assert!(
            !target_has_nontrivial_pre(&cs_targeting(cid), &pool_with(cid, bridge_env)),
            "a non-contract memento must never route, even if it carries a `pre`-named field"
        );
    }
}

#[cfg(test)]
mod callee_post_guard_fact_tests {
    //! `callee_post_guard_fact`: the D-lib cross-function-postcondition supply.
    //!
    //! Three tests per behavior (positive, discrimination, structural), ensuring:
    //!   - a `ctor` arg_term whose bridge target has `post = is_ok(result)` yields
    //!     the `is_ok(arg_term)` fact (positive),
    //!   - a `ctor` whose target has the generic `is_ok || is_err` post does NOT
    //!     yield a fact (discrimination: only the strengthened singleton fires),
    //!   - a `var` arg_term (not a call) does NOT yield a fact (structural).
    use super::*;
    use crate::types::{CallSite, MementoPool};
    use serde_json::json;

    // CIDs for hand-built contracts in these tests.
    const TOTAL_CONTRACT_CID: &str = "blake3-512:serde-value-totality";
    const GENERIC_CONTRACT_CID: &str = "blake3-512:generic-result-contract";
    const BRIDGE_SYMBOL: &str = "serde_json_to_string_value";
    const GENERIC_BRIDGE_SYMBOL: &str = "to_string_generic";

    /// A memento pool with:
    ///   - a contract with `post = is_ok(result)` (the Value-totality contract)
    ///   - a bridge from BRIDGE_SYMBOL to that contract
    fn totality_pool() -> MementoPool {
        let contract_env = json!({
            "evidence": {
                "kind": "contract",
                "body": {
                    "post": {
                        "kind": "atomic",
                        "name": "is_ok",
                        "args": [{"kind": "var", "name": "result"}]
                    }
                }
            }
        });
        let bridge_env = json!({
            "evidence": {
                "kind": "bridge",
                "body": {
                    "sourceSymbol": BRIDGE_SYMBOL,
                    "targetContractCid": TOTAL_CONTRACT_CID
                }
            }
        });
        let mut pool = MementoPool::default();
        pool.mementos
            .insert(TOTAL_CONTRACT_CID.into(), contract_env);
        pool.bridges_by_symbol
            .insert(BRIDGE_SYMBOL.into(), bridge_env);
        pool
    }

    /// A pool with a GENERIC (non-total) Result contract: post = is_ok || is_err.
    fn generic_pool() -> MementoPool {
        let contract_env = json!({
            "evidence": {
                "kind": "contract",
                "body": {
                    "post": {
                        "kind": "or",
                        "operands": [
                            {"kind": "atomic", "name": "is_ok",
                             "args": [{"kind": "var", "name": "result"}]},
                            {"kind": "atomic", "name": "is_err",
                             "args": [{"kind": "var", "name": "result"}]}
                        ]
                    }
                }
            }
        });
        let bridge_env = json!({
            "evidence": {
                "kind": "bridge",
                "body": {
                    "sourceSymbol": GENERIC_BRIDGE_SYMBOL,
                    "targetContractCid": GENERIC_CONTRACT_CID
                }
            }
        });
        let mut pool = MementoPool::default();
        pool.mementos
            .insert(GENERIC_CONTRACT_CID.into(), contract_env);
        pool.bridges_by_symbol
            .insert(GENERIC_BRIDGE_SYMBOL.into(), bridge_env);
        pool
    }

    /// A callsite whose arg_term is a `ctor` (a function-call expression).
    fn cs_with_ctor_arg(ctor_name: &str) -> CallSite {
        CallSite {
            arg_term: Some(json!({
                "kind": "ctor",
                "name": ctor_name,
                "args": [{"kind": "var", "name": "v"}]
            })),
            ..Default::default()
        }
    }

    /// A callsite whose arg_term is a `var` (a bare variable, not a call).
    fn cs_with_var_arg(var_name: &str) -> CallSite {
        CallSite {
            arg_term: Some(json!({"kind": "var", "name": var_name})),
            ..Default::default()
        }
    }

    // -----------------------------------------------------------------------
    // POSITIVE: ctor arg + is_ok post -> supplies is_ok(arg_term) fact
    // -----------------------------------------------------------------------

    #[test]
    fn positive_ctor_with_is_ok_post_supplies_fact() {
        // SOUNDNESS: ctor arg_term + bridge + contract with post = is_ok(result)
        // -> the function returns `Some(is_ok(arg_term))`.
        let cs = cs_with_ctor_arg(BRIDGE_SYMBOL);
        let pool = totality_pool();
        let fact = callee_post_guard_fact(&cs, &pool);
        assert!(
            fact.is_some(),
            "a ctor arg whose contract has post=is_ok(result) must yield a guard fact"
        );
        let fact = fact.unwrap();
        assert_eq!(
            fact.get("kind").and_then(|v| v.as_str()),
            Some("atomic"),
            "the supplied fact must be an atomic"
        );
        assert_eq!(
            fact.get("name").and_then(|v| v.as_str()),
            Some("is_ok"),
            "the supplied fact must be is_ok"
        );
        // The arg of is_ok(.) is the whole ctor expression.
        let fact_args = fact.get("args").and_then(|v| v.as_array()).unwrap();
        assert_eq!(fact_args.len(), 1);
        assert_eq!(
            fact_args[0].get("kind").and_then(|v| v.as_str()),
            Some("ctor"),
            "the is_ok fact's arg must be the ctor expression (the whole call)"
        );
        assert_eq!(
            fact_args[0].get("name").and_then(|v| v.as_str()),
            Some(BRIDGE_SYMBOL),
            "the ctor name in the fact must match the bridge symbol"
        );
    }

    // -----------------------------------------------------------------------
    // DISCRIMINATION: generic is_ok||is_err post must NOT supply the fact
    // -----------------------------------------------------------------------

    #[test]
    fn discrimination_generic_result_post_does_not_supply_fact() {
        // SOUNDNESS GUARD. A function with the generic `is_ok(result) || is_err(result)`
        // post is NOT declared total -- the contract says "it returns SOME Result",
        // not "it always returns Ok". This must NOT supply the is_ok guard fact.
        //
        // This is the discrimination test for D-lib's specialization: only the
        // STRENGTHENED `is_ok(result)` singleton (not the disjunction) fires.
        // If the generic post leaked, any Result-returning function would falsely
        // discharge its callee's unwrap as panic-safe.
        let cs = cs_with_ctor_arg(GENERIC_BRIDGE_SYMBOL);
        let pool = generic_pool();
        assert!(
            callee_post_guard_fact(&cs, &pool).is_none(),
            "a generic is_ok||is_err post must NOT supply the is_ok guard fact"
        );
    }

    // -----------------------------------------------------------------------
    // STRUCTURAL: non-ctor arg_term (bare var) must not supply a fact
    // -----------------------------------------------------------------------

    #[test]
    fn structural_var_arg_does_not_supply_fact() {
        // A bare variable arg_term (not a call expression) -- the standard case
        // for a syntactically-guarded unwrap like `if x.is_ok() { x.unwrap() }`.
        // In that pattern the guard fact comes from the cf_ite guard, not from
        // the callee. This path must not interfere.
        let cs = cs_with_var_arg("result");
        let pool = totality_pool(); // pool has the totality contract, but arg is var
        assert!(
            callee_post_guard_fact(&cs, &pool).is_none(),
            "a var arg_term (not a ctor) must not supply a callee-post guard fact"
        );
    }

    #[test]
    fn structural_no_bridge_for_ctor_returns_none() {
        // Ctor arg_term but no bridge in the pool for that ctor name.
        let cs = cs_with_ctor_arg("unknown_callee");
        let pool = totality_pool(); // bridge is for BRIDGE_SYMBOL, not "unknown_callee"
        assert!(
            callee_post_guard_fact(&cs, &pool).is_none(),
            "a ctor with no bridge in the pool must not supply a fact"
        );
    }

    #[test]
    fn structural_none_arg_term_returns_none() {
        // No arg_term at all (degenerate callsite).
        let cs = CallSite {
            arg_term: None,
            ..Default::default()
        };
        let pool = totality_pool();
        assert!(
            callee_post_guard_fact(&cs, &pool).is_none(),
            "a callsite with no arg_term must not supply a fact"
        );
    }
}

#[cfg(test)]
mod eq_both_calls_discharge_tests {
    //! `extract_body_obligation` / `extract_eq_both_calls_obligation`:
    //! the eq-both-calls tier.
    //!
    //! Three tests per the discrimination protocol:
    //!
    //!   POSITIVE  -- `=(double(3), double(3))` must reduce to `=(6, 6)` (same
    //!                args -> reflexive obligation; classified Reflexive).
    //!
    //!   NEGATIVE  -- `=(double(3), double(4))` must reduce to `=(6, 8)` and
    //!                the NEGATIVE control is confirmed: the obligation is NOT
    //!                reflexive (sides differ: 6 vs 8), so `formula_is_reflexive`
    //!                returns false and the obligation would be Substantive, with
    //!                z3 returning SAT on the negation (not discharged).
    //!
    //!   STRUCTURAL -- `=(double(3), other(3))` (different callee names) does NOT
    //!                match the eq-both-calls recognizer; the call_a side still
    //!                routes to the standard `recognize_eq_call` or refusal path.
    //!
    //! These tests run in-process (no binary, no z3 fork). The positive test
    //! confirms the REDUCED formula is reflexive; the negative test confirms it
    //! is NOT reflexive (the reduced sides differ). Actual z3 discharge of the
    //! corpus rows is confirmed by the integration run (scripts/self-apply.sh).
    use super::*;
    use crate::types::{CallSite, MementoPool};
    use serde_json::json;

    /// CID for the hand-built "double" body-derived contract in these tests.
    const DOUBLE_CID: &str = "blake3-512:double-body-contract";
    const DOUBLE_SYMBOL: &str = "double";

    /// A memento pool containing the body-derived contract for `double(x) = x*2`
    /// and a bridge from `double` to that contract.
    ///
    /// Contract shape: `formals: ["x"]`, `post: result == *(x, 2)`.
    /// This is the same fixture the `cmd_verify_body_discharge.rs` integration
    /// tests use; here it's in-process for fast unit-test feedback.
    fn double_pool() -> MementoPool {
        let contract_env = json!({
            "evidence": {
                "kind": "contract",
                "body": {
                    "formals": ["x"],
                    "post": {
                        "kind": "atomic",
                        "name": "=",
                        "args": [
                            {"kind": "var", "name": "result"},
                            {"kind": "ctor", "name": "*", "args": [
                                {"kind": "var", "name": "x"},
                                {"kind": "const", "value": 2,
                                 "sort": {"kind": "primitive", "name": "Int"}}
                            ]}
                        ]
                    }
                }
            }
        });
        let bridge_env = json!({
            "evidence": {
                "kind": "bridge",
                "body": {
                    "sourceSymbol": DOUBLE_SYMBOL,
                    "targetContractCid": DOUBLE_CID
                }
            }
        });
        let mut pool = MementoPool::default();
        pool.mementos.insert(DOUBLE_CID.into(), contract_env);
        pool.bridges_by_symbol
            .insert(DOUBLE_SYMBOL.into(), bridge_env);
        pool
    }

    fn int_const(n: i64) -> serde_json::Value {
        json!({"kind": "const", "value": n, "sort": {"kind": "primitive", "name": "Int"}})
    }

    fn double_call(arg: i64) -> serde_json::Value {
        json!({"kind": "ctor", "name": DOUBLE_SYMBOL, "args": [int_const(arg)]})
    }

    fn cs_with_eq_both_calls(call_a: serde_json::Value, call_b: serde_json::Value) -> CallSite {
        CallSite {
            bridge_ir_name: DOUBLE_SYMBOL.into(),
            containing_atomic: Some(json!({
                "kind": "atomic",
                "name": "=",
                "args": [call_a, call_b]
            })),
            ..Default::default()
        }
    }

    // -----------------------------------------------------------------------
    // POSITIVE: same-args both-calls -> reduced to reflexive obligation
    // -----------------------------------------------------------------------

    #[test]
    fn positive_same_args_reduces_to_reflexive_obligation() {
        // `=(double(3), double(3))` -> `=(*(3,2), *(3,2))` -> reflexive.
        //
        // This is the `build_signed_attestation_is_deterministic` pattern:
        // `assert_eq!(f(args), f(args))` where the function is deterministic.
        // The obligation reduces to a structural tautology. z3 (in the full
        // pipeline) returns UNSAT; here we confirm the SHAPE is reflexive.
        let cs = cs_with_eq_both_calls(double_call(3), double_call(3));
        let pool = double_pool();

        let result = extract_body_obligation(&cs, &pool);
        assert!(
            result.is_ok(),
            "same-args both-calls must not return Err: {result:?}"
        );
        let obligation_opt = result.unwrap();
        assert!(
            obligation_opt.is_some(),
            "same-args both-calls must produce an obligation (not None)"
        );
        let BodyObligation::Reduced {
            formula: obligation_json,
            tier,
        } = obligation_opt.unwrap();
        assert_eq!(tier, BodyDischargeTier::EqBothCallsSameCallee);

        // The obligation must be reflexive: =(body_expr(3), body_expr(3))
        assert!(
            formula_is_reflexive(&obligation_json),
            "same-args both-calls obligation must be reflexive (both sides are \
             the same body expression); got: {obligation_json}"
        );
        assert_eq!(
            classify_discharge_method(&obligation_json),
            DischargeMethod::Reflexive,
            "same-args both-calls must classify as Reflexive"
        );
    }

    // -----------------------------------------------------------------------
    // NEGATIVE: different-args both-calls -> reduced obligation is NOT reflexive
    // -----------------------------------------------------------------------

    #[test]
    fn negative_different_args_reduces_to_non_reflexive_obligation() {
        // `=(double(3), double(4))` -> `=(*(3,2), *(4,2))` = `=(6, 8)`.
        //
        // THE DISCRIMINATION TEST. The reduced formula has DISTINCT sides
        // (6 != 8). `formula_is_reflexive` must return false, and in the full
        // pipeline z3 finds the negation SAT (9 != 8 is a model for ~(6==8))
        // -> NOT discharged. This is NOT a SKIPPED-as-pass: the body WAS
        // reduced (the callee symbol is gone), and the solver does real work.
        let cs = cs_with_eq_both_calls(double_call(3), double_call(4));
        let pool = double_pool();

        let result = extract_body_obligation(&cs, &pool);
        assert!(
            result.is_ok(),
            "different-args both-calls must produce an obligation (not Err): {result:?}"
        );
        let obligation_opt = result.unwrap();
        assert!(
            obligation_opt.is_some(),
            "different-args both-calls must produce an obligation (not None)"
        );
        let BodyObligation::Reduced {
            formula: obligation_json,
            tier,
        } = obligation_opt.unwrap();
        assert_eq!(tier, BodyDischargeTier::EqBothCallsSameCallee);

        // THE DECISIVE ASSERTION: sides differ -> NOT reflexive.
        // `formula_is_reflexive` must return false for `=(6, 8)`.
        assert!(
            !formula_is_reflexive(&obligation_json),
            "different-args both-calls obligation must NOT be reflexive \
             (sides differ: double(3) reduces to 6, double(4) to 8); \
             got: {obligation_json}"
        );
        assert_eq!(
            classify_discharge_method(&obligation_json),
            DischargeMethod::Substantive,
            "different-args both-calls must classify Substantive (sides differ)"
        );

        // Confirm the reduced formula is `=(*(3,2), *(4,2))` concretely.
        let args = obligation_json
            .get("args")
            .and_then(|v| v.as_array())
            .expect("obligation must be an equality with args");
        assert_eq!(args.len(), 2, "equality must have exactly 2 args");
        // Both sides must be the `*` ctor (body of double), not `double`.
        assert_ne!(args[0], args[1], "sides of reduced =(6,8) must differ");
        assert!(
            !obligation_json.to_string().contains(DOUBLE_SYMBOL),
            "no uninterpreted `double` symbol must remain in the reduced obligation; \
             got: {obligation_json}"
        );
    }

    // -----------------------------------------------------------------------
    // STRUCTURAL: asymmetric shape (only ONE side is the call) -> NOT this path
    // -----------------------------------------------------------------------

    #[test]
    fn structural_one_sided_call_uses_standard_path_not_both_calls() {
        // `=(double(3), 6)` -- only one side is the call; this is the STANDARD
        // `=(call, expected)` shape handled by `recognize_eq_call`, NOT the
        // eq-both-calls path. Confirm `recognize_eq_both_calls` returns None.
        let atomic = json!({
            "kind": "atomic",
            "name": "=",
            "args": [double_call(3), int_const(6)]
        });
        let cs = CallSite {
            bridge_ir_name: DOUBLE_SYMBOL.into(),
            containing_atomic: Some(atomic),
            ..Default::default()
        };
        // `recognize_eq_both_calls` must return None for this shape.
        assert!(
            recognize_eq_both_calls(&cs).is_none(),
            "one-sided `=(call, constant)` must NOT match recognize_eq_both_calls"
        );
        // `recognize_eq_call` MUST match it.
        assert!(
            recognize_eq_call(&cs).is_some(),
            "one-sided `=(call, constant)` must match the standard recognize_eq_call"
        );
        // The full path: extract_body_obligation must ALSO succeed (standard path).
        let pool = double_pool();
        let result = extract_body_obligation(&cs, &pool);
        assert!(
            result.is_ok(),
            "one-sided call must still discharge via the standard path: {result:?}"
        );
        assert!(
            result.unwrap().is_some(),
            "one-sided call must produce an obligation"
        );
    }
}
