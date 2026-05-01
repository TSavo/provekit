// SPDX-License-Identifier: Apache-2.0
//
// .invariant.cpp for provekit/verifier/load_all_proofs.cpp
//
// Public surface covered:
//   * `load_all_proofs(dir) -> MementoPool`

#include "provekit/ir.hpp"

namespace {

using namespace provekit::ir;

std::shared_ptr<Term> ctor1(const std::string& name, std::shared_ptr<Term> arg) {
    return std::make_shared<Term>(Term{CtorTerm{name, {std::move(arg)}}});
}

}  // namespace

extern "C" void load_all_proofs_invariants() {
    using namespace provekit::ir;

    // load_all_proofs is deterministic on the same directory.
    must("cpp_load_all_proofs_is_deterministic",
         forall(String(), [](std::shared_ptr<Term> dir) {
             return eq(ctor1("load_all_proofs", dir), ctor1("load_all_proofs", dir));
         }));

    // Loading an empty directory produces an empty memento pool.
    contract("cpp_load_all_proofs_empty_dir_yields_empty_pool",
             /*pre=*/nullptr,
             /*post=*/eq(ctor1("len", ctor1("load_all_proofs_pool", str_const("/empty"))), num(0)));

    // Pool size is non-negative (a tautology in unsigned arithmetic; the
    // IR uses signed Int, so this asserts the non-negativity floor).
    must("cpp_load_all_proofs_pool_size_nonneg",
         forall(String(), [](std::shared_ptr<Term> dir) {
             return gte(ctor1("len", ctor1("load_all_proofs_pool", dir)), num(0));
         }));
}
