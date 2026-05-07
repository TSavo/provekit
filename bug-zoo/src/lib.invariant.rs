// SPDX-License-Identifier: Apache-2.0
//
// .invariant.rs for bug-zoo/src/lib.rs
//
// Public surface covered:
//   * exhibit/fixed ProofIR CID verification in `check_specimen`
//   * fixed diagnostic cleanliness in `expect_green_diagnostic`
//
// Honest scope:
//   The IR names the byte-derived CID obligations. The concrete byte
//   derivation and red/green behavior are operationally enforced by the
//   self-contained Bug Zoo runner tests and smoke specimens.

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
        "zoo_exhibit_proofir_cid_is_derived",
        ContractArgs {
            post: Some(forall(String_(), |lift| {
                implies(
                    atomic_("acceptedExhibitLift", vec![lift.clone()]),
                    atomic_(
                        "cidEquals",
                        vec![
                            ctor1("exhibitProofIrCid", lift.clone()),
                            ctor1("json_document_cid", ctor1("exhibitProofIr", lift)),
                        ],
                    ),
                )
            })),
            ..Default::default()
        },
    );

    contract(
        "zoo_fixed_proofir_cid_is_derived",
        ContractArgs {
            post: Some(forall(String_(), |lift| {
                implies(
                    atomic_("acceptedFixedLift", vec![lift.clone()]),
                    atomic_(
                        "cidEquals",
                        vec![
                            ctor1("fixedProofIrCid", lift.clone()),
                            ctor1("json_document_cid", ctor1("fixedProofIr", lift)),
                        ],
                    ),
                )
            })),
            ..Default::default()
        },
    );

    contract(
        "zoo_fixed_diagnostic_has_no_missing_edge",
        ContractArgs {
            post: Some(forall(String_(), |diagnostic| {
                implies(
                    atomic_("acceptedFixedDiagnostic", vec![diagnostic.clone()]),
                    atomic_("missingEdgeAbsent", vec![diagnostic]),
                )
            })),
            ..Default::default()
        },
    );
}
