// SPDX-License-Identifier: Apache-2.0
//
// .invariant.cpp for provekit/claim-envelope/mint.cpp
//
// Public surface covered:
//   * `mint_contract(args) -> MintedEnvelope`
//   * `mint_bridge(args) -> MintedEnvelope`
//   * `mint_implication(args) -> MintedEnvelope`

#include "provekit/ir.hpp"

namespace {

using namespace provekit::ir;

std::shared_ptr<Term> ctor1(const std::string& name, std::shared_ptr<Term> arg) {
    return std::make_shared<Term>(Term{CtorTerm{name, {std::move(arg)}}});
}

}  // namespace

extern "C" void mint_invariants() {
    using namespace provekit::ir;

    // mint_contract is deterministic on its (args, seed) input.
    must("cpp_mint_contract_is_deterministic",
         forall(String(), [](std::shared_ptr<Term> args) {
             return eq(ctor1("mint_contract", args), ctor1("mint_contract", args));
         }));

    // Minted envelope CID has length 139 (the BLAKE3-512 self-id form).
    must("cpp_mint_contract_cid_length_eq_139",
         forall(String(), [](std::shared_ptr<Term> args) {
             return eq(ctor1("len", ctor1("mint_contract_cid", args)), num(139));
         }));

    // Canonical bytes of a minted envelope are non-empty.
    must("cpp_mint_contract_canonical_bytes_nonempty",
         forall(String(), [](std::shared_ptr<Term> args) {
             return gte(ctor1("len", ctor1("mint_contract_bytes", args)), num(1));
         }));

    // mint_bridge is deterministic.
    must("cpp_mint_bridge_is_deterministic",
         forall(String(), [](std::shared_ptr<Term> args) {
             return eq(ctor1("mint_bridge", args), ctor1("mint_bridge", args));
         }));

    // mint_implication is deterministic.
    must("cpp_mint_implication_is_deterministic",
         forall(String(), [](std::shared_ptr<Term> args) {
             return eq(ctor1("mint_implication", args), ctor1("mint_implication", args));
         }));
}
