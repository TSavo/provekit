// SPDX-License-Identifier: Apache-2.0
//
// .invariant.cpp for provekit/canonicalizer/property_hash.cpp
//
// Public surface covered:
//   * `property_hash(const Value&) -> std::string`
//     == "blake3-512:" + hex(BLAKE3-512(encode_jcs(v)))
//
// The IR can express determinism and length; the byte-faithful
// composition (`property_hash(v) == compute_cid(encode_jcs(v))`) is
// captured as a kit-defined predicate operationally enforced by tests.

#include "provekit/ir.hpp"

namespace {

using namespace provekit::ir;

std::shared_ptr<Term> ctor1(const std::string& name, std::shared_ptr<Term> arg) {
    return std::make_shared<Term>(Term{CtorTerm{name, {std::move(arg)}}});
}

}  // namespace

extern "C" void property_hash_invariants() {
    using namespace provekit::ir;

    // property_hash output length is exactly 139.
    must("cpp_property_hash_output_length_eq_139",
         forall(String(), [](std::shared_ptr<Term> v) {
             return eq(ctor1("len", ctor1("property_hash", v)), num(139));
         }));

    // property_hash is deterministic.
    must("cpp_property_hash_is_deterministic",
         forall(String(), [](std::shared_ptr<Term> v) {
             return eq(ctor1("property_hash", v), ctor1("property_hash", v));
         }));

    // property_hash equals compute_cid composed with encode_jcs.
    // Z3 has no semantics for this functional-equality claim across
    // distinct ctors, so the verifier marks it undecidable. Living-doc
    // value: every reader sees the exact composition the protocol
    // specifies.
    must("cpp_property_hash_equals_cid_of_jcs",
         forall(String(), [](std::shared_ptr<Term> v) {
             return eq(ctor1("property_hash", v),
                       ctor1("compute_cid", ctor1("encode_jcs", v)));
         }));
}
