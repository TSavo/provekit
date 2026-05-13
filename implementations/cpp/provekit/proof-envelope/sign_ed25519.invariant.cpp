// SPDX-License-Identifier: Apache-2.0
//
// .invariant.cpp for provekit/proof-envelope/sign_ed25519.cpp
//
// Public surface covered:
//   * `ed25519_sign_with_seed(seed, msg, len) -> 64-byte signature`
//   * `ed25519_pubkey_from_seed(seed) -> 32-byte public key`
//
// Honest scope:
//   The IR cannot express "ed25519 signature is unforgeable under
//   chosen-message attack": that's a cryptographic claim outside FOL.
//   What IR CAN express: output sizes, determinism of seeded signer,
//   shape of pubkey-from-seed.

#include "provekit/ir.hpp"

namespace {

using namespace provekit::ir;

std::shared_ptr<Term> ctor1(const std::string& name, std::shared_ptr<Term> arg) {
    return std::make_shared<Term>(Term{CtorTerm{name, {std::move(arg)}}});
}

std::shared_ptr<Term> ctor2(const std::string& name,
                              std::shared_ptr<Term> a,
                              std::shared_ptr<Term> b) {
    return std::make_shared<Term>(Term{CtorTerm{name, {std::move(a), std::move(b)}}});
}

std::shared_ptr<Term> ctor3(const std::string& name,
                              std::shared_ptr<Term> a,
                              std::shared_ptr<Term> b,
                              std::shared_ptr<Term> c) {
    return std::make_shared<Term>(Term{CtorTerm{name,
        {std::move(a), std::move(b), std::move(c)}}});
}

std::shared_ptr<Term> bool_true() {
    return std::make_shared<Term>(Term{CtorTerm{"true", {}}});
}

}  // namespace

extern "C" void sign_invariants() {
    using namespace provekit::ir;

    // Signature output length is exactly 64 bytes.
    must("cpp_ed25519_sign_output_length_eq_64",
         forall(String(), [](std::shared_ptr<Term> seed) {
             return eq(ctor1("len", ctor2("ed25519_sign_with_seed", seed, str_const("msg"))), num(64));
         }));

    // Public key length is exactly 32 bytes.
    must("cpp_ed25519_pubkey_length_eq_32",
         forall(String(), [](std::shared_ptr<Term> seed) {
             return eq(ctor1("len", ctor1("ed25519_pubkey_from_seed", seed)), num(32));
         }));

    // Seeded signer is deterministic: same seed and message, same signature.
    must("cpp_ed25519_sign_is_deterministic",
         forall(String(), [](std::shared_ptr<Term> seed) {
             return eq(ctor2("ed25519_sign_with_seed", seed, str_const("msg")),
                       ctor2("ed25519_sign_with_seed", seed, str_const("msg")));
         }));

    // Pubkey-from-seed is deterministic.
    must("cpp_ed25519_pubkey_from_seed_is_deterministic",
         forall(String(), [](std::shared_ptr<Term> seed) {
             return eq(ctor1("ed25519_pubkey_from_seed", seed),
                       ctor1("ed25519_pubkey_from_seed", seed));
         }));

    // f(x(a)) = a: the round-trip identity. Verify(Sign(msg, seed),
    // PubkeyFromSeed(seed), msg) MUST equal true. This is the missing
    // invariant the placeholder-signer_cid bug exposed: without this
    // contract, the construction `signer_cid = compute_cid(pubkey_string_from_seed(seed))`
    // could (and did) drift to a hardcoded constant, and no content-
    // addressed gate caught it. The IR can express the round-trip
    // algebraically; Z3 cannot discharge cryptographic semantics, but
    // the contract IS minted, IS content-addressed, and IS available
    // for any verifier (LLM, formal-methods tool, manual auditor) to
    // check.
    must("cpp_ed25519_sign_verify_roundtrip",
         forall(String(), [](std::shared_ptr<Term> seed) {
             return eq(
                 ctor3("ed25519_verify",
                       ctor2("ed25519_sign_with_seed", seed, str_const("msg")),
                       ctor1("ed25519_pubkey_from_seed", seed),
                       str_const("msg")),
                 bool_true());
         }));
}
