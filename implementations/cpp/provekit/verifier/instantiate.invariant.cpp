// SPDX-License-Identifier: Apache-2.0
//
// .invariant.cpp for provekit/verifier/instantiate.cpp
//
// Public surface covered:
//   * `instantiate(target, callsite_args) -> InstantiatedObligation`

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

extern "C" void instantiate_invariants() {
    using namespace provekit::ir;

    // instantiate is deterministic.
    must("cpp_instantiate_is_deterministic",
         forall(String(), [](std::shared_ptr<Term> target) {
             return eq(ctor2("instantiate", target, str_const("args")),
                       ctor2("instantiate", target, str_const("args")));
         }));

    // The instantiated obligation carries a non-empty body.
    must("cpp_instantiate_body_nonempty",
         forall(String(), [](std::shared_ptr<Term> target) {
             return gte(ctor1("len",
                              ctor1("instantiated_body",
                                     ctor2("instantiate", target, str_const("args")))),
                        num(1));
         }));
}
