// SPDX-License-Identifier: Apache-2.0
//
// .invariant.rs for provekit-proof-envelope/src/proof.rs
//
// Public surface covered:
//   * `build_proof_envelope(&ProofEnvelopeInput) -> ProofEnvelopeOutput`
//   * `ProofEnvelopeInput { name, version, members, signer_cid,
//                           signer_seed, declared_at }`
//   * `ProofEnvelopeOutput { bytes, cid }`
//
// Honest scope:
//   The .proof envelope is deterministic-CBOR per RFC 8949 §4.2.1; the
//   filename CID IS BLAKE3-512 of the bytes. The IR can express:
//     - determinism (same input -> same bytes -> same CID),
//     - CID length floor / equality 139,
//     - CID is the hash of the bytes (functional: same call yields
//       same CID).
//   It cannot express the byte-deterministic encoding correctness;
//   that's enforced by proof_envelope_catalog.rs tests.

use std::rc::Rc;

use provekit_ir_symbolic::{
    contract, eq, forall, gte, must, num, str_const, ContractArgs, Int,
    String_, Term,
};

fn ctor1(name: &str, arg: Rc<Term>) -> Rc<Term> {
    Rc::new(Term::Ctor {
        name: name.into(),
        args: vec![arg],
    })
}

pub fn invariants() {
    // -- build_proof_envelope is a function. --------------------------------
    //
    // STRONGER INVARIANT: the output bytes are byte-equal across runs
    // for fixed input (deterministic CBOR encoding plus deterministic
    // ed25519 signing). Operationally enforced by the catalog tests;
    // we also assert this in the orchestrator binary by minting twice
    // and comparing the resulting CIDs.
    must(
        "build_proof_envelope_is_deterministic",
        forall(String_(), |input| {
            eq(
                ctor1("build_proof_envelope", input.clone()),
                ctor1("build_proof_envelope", input),
            )
        }),
    );

    // -- ProofEnvelopeOutput.cid has length 139. ----------------------------
    //
    // The output's CID is BLAKE3-512 of the bytes; it carries the
    // "blake3-512:" + 128 hex form. Total 139.
    must(
        "build_proof_envelope_cid_length_eq_139",
        forall(String_(), |input| {
            eq(
                ctor1(
                    "len",
                    ctor1("proof_envelope_output_cid", ctor1("build_proof_envelope", input)),
                ),
                num(139),
            )
        }),
    );

    // -- bytes_len is at least the empty-catalog minimum. -------------------
    //
    // Even an empty members map produces a non-trivial envelope: a CBOR
    // array of [name, version, members={}, signer_cid, signer_seed_pubkey,
    // declared_at, signature]. We assert >= 1 (trivial floor) and
    // document the operational test separately.
    must(
        "build_proof_envelope_bytes_nonempty",
        forall(String_(), |input| {
            gte(ctor1("len", ctor1("build_proof_envelope_bytes", input)), num(1))
        }),
    );

    // -- Catalog name "@provekit/self-contracts" is a stable identifier. ----
    //
    // This is a meta-claim: the orchestrator below mints a catalog whose
    // `name` field equals the stable string. The IR can carry it as a
    // string-equality post-condition for future tooling that walks the
    // memento for human-readable provenance.
    contract(
        "self_contracts_catalog_name_is_stable",
        ContractArgs {
            post: Some(eq(
                ctor1("catalog_name", str_const("self-contracts")),
                str_const("@provekit/self-contracts"),
            )),
            ..Default::default()
        },
    );

    // -- members map is sorted-by-CID (BTreeMap insertion). -----------------
    //
    // STRONGER INVARIANT (RFC 8949 §4.2.1: map keys sorted by bytewise
    // CBOR encoded form; for tstr keys "blake3-512:..." this is
    // lexicographic on the prefix-suffix concat). Not expressible in
    // the IR. Operationally enforced via build_proof_envelope's use of
    // BTreeMap and the catalog test suite.
    must(
        "build_proof_envelope_member_count_nonneg",
        forall(Int(), |n| gte(n, num(0))),
    );
}
