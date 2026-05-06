// SPDX-License-Identifier: Apache-2.0
//
// .invariant.rs for provekit-cli/src/cmd_zoo.rs
//
// Public surface covered:
//   * dropper receipt verification in `verify_dropper`
//
// Honest scope:
//   The IR names the byte-derived CID obligations. The concrete byte
//   derivation is operationally enforced by cmd_zoo tests and the zoo
//   smoke specimen.

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
        "zoo_dropper_transformed_artifact_cid_is_derived",
        ContractArgs {
            post: Some(forall(String_(), |output| {
                implies(
                    atomic_("acceptedDropperOutput", vec![output.clone()]),
                    atomic_(
                        "cidEquals",
                        vec![
                            ctor1("transformedArtifactCid", output.clone()),
                            ctor1("blake3_512_of", ctor1("modifiedSource", output)),
                        ],
                    ),
                )
            })),
            ..Default::default()
        },
    );

    contract(
        "zoo_dropper_post_lift_cid_is_derived",
        ContractArgs {
            post: Some(forall(String_(), |output| {
                implies(
                    atomic_("acceptedDropperOutput", vec![output.clone()]),
                    atomic_(
                        "cidEquals",
                        vec![
                            ctor1("postLiftCid", output.clone()),
                            ctor1("json_document_cid", ctor1("postLift", output)),
                        ],
                    ),
                )
            })),
            ..Default::default()
        },
    );

    contract(
        "zoo_dropper_closure_witness_cid_is_derived",
        ContractArgs {
            post: Some(forall(String_(), |output| {
                implies(
                    atomic_("acceptedDropperOutput", vec![output.clone()]),
                    atomic_(
                        "cidEquals",
                        vec![
                            ctor1("closureWitnessCid", output.clone()),
                            ctor1("json_document_cid", ctor1("closureWitness", output)),
                        ],
                    ),
                )
            })),
            ..Default::default()
        },
    );
}
