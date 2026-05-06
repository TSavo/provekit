// SPDX-License-Identifier: Apache-2.0
//
// Kit-IR invariant for the Java null-boundary realizer.
//
// Public surface covered:
//   * JavaNullBoundaryRealizer native and Spring transform modes.
//
// Honest scope:
//   This invariant names the host-binding rule independent of Java syntax:
//   a closed realized contract over `proofVar` is admissible only if the
//   target method actually has a parameter named `proofVar`. The concrete
//   JavaParser enforcement lives in JavaNullBoundaryRealizer tests and code.

use std::rc::Rc;

use provekit_ir_symbolic::{atomic_, contract, forall, implies, ContractArgs, String_, Term};

fn ctor1(name: &str, arg: Rc<Term>) -> Rc<Term> {
    Rc::new(Term::Ctor {
        name: name.into(),
        args: vec![arg],
    })
}

pub fn invariants() {
    contract(
        "java_null_boundary_realizer_contract_var_is_bound_parameter",
        ContractArgs {
            post: Some(forall(String_(), |plan| {
                implies(
                    atomic_("closedNullBoundaryRealizerPlan", vec![plan.clone()]),
                    atomic_(
                        "methodHasParameter",
                        vec![ctor1("targetMethod", plan.clone()), ctor1("proofVar", plan)],
                    ),
                )
            })),
            ..Default::default()
        },
    );
}
