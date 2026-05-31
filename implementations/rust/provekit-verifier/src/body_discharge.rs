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

/// The kind of obligation `extract_body_obligation` produced, for the
/// caller's receipt row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BodyObligation {
    /// A body-reduced obligation formula, ready for the SMT emitter. The
    /// callee symbol has been inlined; no uninterpreted constant remains.
    Reduced(Json),
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
            Ok(Some(BodyObligation::Reduced(reduced_json)))
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
        assert!(target_has_nontrivial_pre(&cs_targeting(cid), &pool_with(cid, env)));
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
        assert!(!target_has_nontrivial_pre(&cs_targeting("blake3-512:absent"), &empty));

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
        pool.mementos.insert(TOTAL_CONTRACT_CID.into(), contract_env);
        pool.bridges_by_symbol.insert(BRIDGE_SYMBOL.into(), bridge_env);
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
        pool.mementos.insert(GENERIC_CONTRACT_CID.into(), contract_env);
        pool.bridges_by_symbol.insert(GENERIC_BRIDGE_SYMBOL.into(), bridge_env);
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
