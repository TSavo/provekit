// SPDX-License-Identifier: Apache-2.0
//
// Cross-kit conformance bridges (cpp peer): the `cross_kit_bridges_invariants`
// registrar. Authors the 10 cpp counterpart contracts, one per rust
// lift-plugin-protocol contract. Bridges live in cross_kit_bridges.hpp
// alongside the rust source-CID pins; the test
// (cross_kit_bridges_test.cpp) is the single point that pins the
// JCS-of-the-bridge-array bytes.
//
// Each counterpart asserts "cpp-kit's lift adapter satisfies rust's
// <contract name>" via a kit-defined named ctor whose paired-equality
// with `true_const` encodes the protocol-level claim. Z3 has no
// semantics for the ctor; the contract's value is the named-membership
// shape, which the bridge pins into a closed source-target pair.
//
// See cross_kit_bridges.hpp for the full design notes; this file is the
// extern-C entry point the cpp orchestrator (mint_cpp_self_contracts.cpp)
// links and calls during slab authoring.

#include "cross_kit_bridges.hpp"

#include "provekit/ir.hpp"

#include <memory>
#include <string>

extern "C" void cross_kit_bridges_invariants() {
    using namespace provekit::ir;
    using namespace provekit::cross_kit_bridges;

    auto ctor1 = [](std::string name, std::shared_ptr<Term> arg) {
        return std::make_shared<Term>(
            Term{CtorTerm{std::move(name), {std::move(arg)}}});
    };

    for (const auto& rust_name : lift_plugin_protocol_contract_names()) {
        const auto cp_name = counterpart_contract_name(rust_name);

        // Counterpart: the cpp lift adapter satisfies the rust rule.
        // Post is a paired-equality whose ctor name encodes the claim.
        contract(
            cp_name,
            /*pre=*/nullptr,
            /*post=*/eq(
                ctor1("cpp_lift_satisfies_rust_contract", str_const(rust_name)),
                ctor1("true_const", str_const(""))),
            /*inv=*/nullptr,
            /*outBinding=*/"out");
    }
}
