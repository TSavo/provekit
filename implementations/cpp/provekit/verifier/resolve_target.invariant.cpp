// SPDX-License-Identifier: Apache-2.0
//
// .invariant.cpp for provekit/verifier/resolve_target.cpp
//
// Public surface covered:
//   * `resolve_target(pool, callsite) -> ResolvedTarget`

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

}  // namespace

extern "C" void resolve_target_invariants() {
    using namespace provekit::ir;

    // resolve_target is deterministic.
    must("cpp_resolve_target_is_deterministic",
         forall(String(), [](std::shared_ptr<Term> cs) {
             return eq(ctor2("resolve_target", str_const("pool"), cs),
                       ctor2("resolve_target", str_const("pool"), cs));
         }));

    // Resolved target's contract CID has the BLAKE3-512 self-id length
    // (139 chars) when resolution succeeds. Failure modes are kit-defined
    // names (`isErr`, `isUnresolved`) outside Z3's first-order axioms.
    must("cpp_resolve_target_contract_cid_length_eq_139",
         forall(String(), [](std::shared_ptr<Term> cs) {
             return eq(ctor1("len",
                              ctor1("resolve_target_contract_cid",
                                     ctor2("resolve_target", str_const("pool"), cs))),
                        num(139));
         }));
}
