// SPDX-License-Identifier: Apache-2.0
//
// .invariant.rs for provekit-verifier/src/instantiate.rs
//
// Public surface covered:
//   * `run(&ResolvedProperty, &Option<Json>) -> Result<Obligation, String>`
//   * `Obligation { property_cid, ir_kit_version, ir_formula }`
//
// Honest scope:
//   Stage 4 substitutes the call-site arg term for the resolved
//   forall's bound variable. The IR can carry function-level claims
//   (determinism, error-on-missing-arg, error-when-pre-is-not-forall).
//   Capture-avoidance and structural correctness of substitution are
//   integration-tested in provekit-verifier/tests/instantiate.rs.

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
    // -- instantiate is deterministic. --------------------------------------
    must(
        "instantiate_is_deterministic",
        forall(String_(), |args| {
            eq(
                ctor1("instantiate", args.clone()),
                ctor1("instantiate", args),
            )
        }),
    );

    // -- Missing arg term yields Err. ---------------------------------------
    contract(
        "instantiate_missing_arg_returns_err",
        ContractArgs {
            post: Some(forall(String_(), |args| {
                let missing = atomic_("argTermIsNone", vec![args.clone()]);
                let is_err = atomic_("isErr", vec![ctor1("instantiate", args)]);
                implies(missing, is_err)
            })),
            ..Default::default()
        },
    );

    // -- Pre formula must be a forall (else Err). ---------------------------
    contract(
        "instantiate_non_forall_pre_returns_err",
        ContractArgs {
            post: Some(forall(String_(), |args| {
                let not_forall = atomic_("preIsNotForall", vec![args.clone()]);
                let is_err = atomic_("isErr", vec![ctor1("instantiate", args)]);
                implies(not_forall, is_err)
            })),
            ..Default::default()
        },
    );

    // -- Substituted obligation's CID equals resolved.cid. ------------------
    //
    // The instantiate stage doesn't change the property identity; the
    // CID rides through unchanged. STRONGER INVARIANT (the substituted
    // formula has the same free-var set minus the bound name plus
    // free-vars(arg_term)) is integration-tested.
    must(
        "obligation_property_cid_carries_through",
        forall(String_(), |resolved| {
            eq(
                ctor1(
                    "obligation_property_cid",
                    ctor1("instantiate", resolved.clone()),
                ),
                ctor1("resolved_property_cid", resolved),
            )
        }),
    );
}
