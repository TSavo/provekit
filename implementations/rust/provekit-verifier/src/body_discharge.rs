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
