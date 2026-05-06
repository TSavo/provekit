// SPDX-License-Identifier: Apache-2.0
//
// .invariant.rs for provekit-verifier/src/proof_conformance.rs
//
// Public surface covered:
//   * `validate_proof_bytes(&Path, &[u8]) -> ProofFileConformanceReport`
//   * `.proof` catalog-level signature fields (`signer`, `declaredAt`,
//     `signature`)
//
// Honest scope:
//   The IR names the signature obligation and the unsigned-body binding.
//   Ed25519 verification over deterministic CBOR bytes is operationally
//   enforced by proof_conformance.rs tests and verifier code.

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
        "proof_conformance_requires_catalog_signature_fields",
        ContractArgs {
            post: Some(forall(String_(), |catalog| {
                implies(
                    atomic_("isProofCatalog", vec![catalog.clone()]),
                    atomic_("hasCatalogSignatureFields", vec![catalog]),
                )
            })),
            ..Default::default()
        },
    );

    contract(
        "proof_conformance_verifies_catalog_signature_over_unsigned_body",
        ContractArgs {
            post: Some(forall(String_(), |catalog| {
                implies(
                    atomic_("acceptedProofCatalog", vec![catalog.clone()]),
                    atomic_(
                        "ed25519Verifies",
                        vec![
                            ctor1("catalogSigner", catalog.clone()),
                            ctor1("catalogSignature", catalog.clone()),
                            ctor1("catalogUnsignedBody", catalog),
                        ],
                    ),
                )
            })),
            ..Default::default()
        },
    );
}
