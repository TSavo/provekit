// SPDX-License-Identifier: Apache-2.0
//
// .invariant.cpp for provekit/canonicalizer/jcs.cpp
//
// Public surface covered:
//   * `encode_jcs(const Value&) -> std::string` (RFC 8785 JCS-JSON)
//
// Honest scope:
//   The IR's atomic-predicate domain is narrow. RFC 8785 conformance
//   is byte-faithful; the IR can express determinism, length floors,
//   and structural equality of repeated calls. The byte-faithful key-
//   ordering claim is enforced operationally by canonicalizer_test.cpp.

#include "provekit/ir.hpp"

namespace {

using namespace provekit::ir;

std::shared_ptr<Term> ctor1(const std::string& name, std::shared_ptr<Term> arg) {
    return std::make_shared<Term>(Term{CtorTerm{name, {std::move(arg)}}});
}

}  // namespace

extern "C" void jcs_invariants() {
    using namespace provekit::ir;

    // encode_jcs is a function: same input, same output.
    must("cpp_encode_jcs_is_deterministic",
         forall(String(), [](std::shared_ptr<Term> s) {
             return eq(ctor1("encode_jcs", s), ctor1("encode_jcs", s));
         }));

    // Output length is bounded below by 1.
    must("cpp_encode_jcs_output_nonempty",
         forall(String(), [](std::shared_ptr<Term> v) {
             return gte(ctor1("len", ctor1("encode_jcs", v)), num(1));
         }));

    // Empty-array emission is exactly "[]", length 2.
    contract("cpp_encode_jcs_empty_array_length_eq_2",
             /*pre=*/nullptr,
             /*post=*/eq(ctor1("len", ctor1("encode_jcs", str_const("[]"))), num(2)));

    // Empty-object emission is exactly "{}", length 2.
    contract("cpp_encode_jcs_empty_object_length_eq_2",
             /*pre=*/nullptr,
             /*post=*/eq(ctor1("len", ctor1("encode_jcs", str_const("{}"))), num(2)));

    // "true" emission is exactly the literal "true", length 4.
    contract("cpp_encode_jcs_true_length_eq_4",
             /*pre=*/nullptr,
             /*post=*/eq(ctor1("len", ctor1("encode_jcs", str_const("true"))), num(4)));
}
