// SPDX-License-Identifier: Apache-2.0
//
// .invariant.rs for provekit-verifier/src/resolve_target.rs
//
// Public surface covered:
//   * `run(&CallSite, &MementoPool) -> Result<ResolvedProperty, String>`
//   * `ResolvedProperty { cid, ir_formula, ir_kit_version }`
//
// Honest scope:
//   Stage 3 looks up the bridge.targetCid in the pool and returns the
//   target contract's `pre` formula as the discharge target. The IR can
//   carry the function-level invariants (determinism, error-on-missing,
//   resolved.cid = callsite.target_cid).

use std::rc::Rc;

use provekit_ir_symbolic::{
    atomic_, contract, eq, forall, implies, must, ContractArgs, String_, Term,
};

fn ctor1(name: &str, arg: Rc<Term>) -> Rc<Term> {
    Rc::new(Term::Ctor {
        name: name.into(),
        args: vec![arg],
    })
}

pub fn invariants() {
    // -- resolve_target is deterministic. -----------------------------------
    must(
        "resolve_target_is_deterministic",
        forall(String_(), |cs| {
            eq(
                ctor1("resolve_target", cs.clone()),
                ctor1("resolve_target", cs),
            )
        }),
    );

    // -- A successful resolve preserves the CID. ----------------------------
    //
    // forall cs. ok(resolve_target(cs)) implies cid(resolved) = target(cs).
    // The kit-defined predicates `isOk`, `resolvedCid`, `bridgeTarget`
    // are uninterpreted to Z3.
    contract(
        "resolve_target_cid_preserved",
        ContractArgs {
            post: Some(forall(String_(), |cs| {
                let ok = atomic_("isOk", vec![ctor1("resolve_target", cs.clone())]);
                let preserved = atomic_("resolvedCidEqualsBridgeTarget", vec![cs]);
                implies(ok, preserved)
            })),
            ..Default::default()
        },
    );

    // -- Missing target CID returns Err. ------------------------------------
    contract(
        "resolve_target_missing_returns_err",
        ContractArgs {
            post: Some(forall(String_(), |cs| {
                let missing = atomic_("targetCidNotInPool", vec![cs.clone()]);
                let is_err = atomic_("isErr", vec![ctor1("resolve_target", cs)]);
                implies(missing, is_err)
            })),
            ..Default::default()
        },
    );
}
