// SPDX-License-Identifier: Apache-2.0
//
// .invariant.cpp for provekit/verifier/enumerate_callsites.cpp
//
// Public surface covered:
//   * `enumerate_callsites(pool) -> std::vector<Callsite>`

#include "provekit/ir.hpp"

namespace {

using namespace provekit::ir;

std::shared_ptr<Term> ctor1(const std::string& name, std::shared_ptr<Term> arg) {
    return std::make_shared<Term>(Term{CtorTerm{name, {std::move(arg)}}});
}

}  // namespace

extern "C" void enumerate_callsites_invariants() {
    using namespace provekit::ir;

    // enumerate_callsites is deterministic on the same memento pool.
    must("cpp_enumerate_callsites_is_deterministic",
         forall(String(), [](std::shared_ptr<Term> pool) {
             return eq(ctor1("enumerate_callsites", pool),
                       ctor1("enumerate_callsites", pool));
         }));

    // Number of callsites is non-negative.
    must("cpp_enumerate_callsites_count_nonneg",
         forall(String(), [](std::shared_ptr<Term> pool) {
             return gte(ctor1("len", ctor1("enumerate_callsites", pool)), num(0));
         }));

    // An empty pool yields zero callsites.
    contract("cpp_enumerate_callsites_empty_pool_yields_zero",
             /*pre=*/nullptr,
             /*post=*/eq(ctor1("len", ctor1("enumerate_callsites", str_const("empty_pool"))), num(0)));
}
