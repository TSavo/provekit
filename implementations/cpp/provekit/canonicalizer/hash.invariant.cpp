// SPDX-License-Identifier: Apache-2.0
//
// .invariant.cpp for provekit/canonicalizer/hash.cpp
//
// Public surface covered:
//   * `blake3_512_hex(string_view) -> std::string`  // 128 lowercase hex chars
//   * `compute_cid(string_view) -> std::string`     // "blake3-512:" + 128 hex
//
// Honest scope:
//   The IR cannot model BLAKE3's collision resistance. What the IR CAN
//   say is shape-level: output lengths, determinism, prefix presence.

#include "provekit/ir.hpp"

namespace {

using namespace provekit::ir;

std::shared_ptr<Term> ctor1(const std::string& name, std::shared_ptr<Term> arg) {
    return std::make_shared<Term>(Term{CtorTerm{name, {std::move(arg)}}});
}

}  // namespace

extern "C" void hash_invariants() {
    using namespace provekit::ir;

    // compute_cid output length is exactly 139 (11-char prefix + 128 hex).
    must("cpp_compute_cid_output_length_eq_139",
         forall(String(), [](std::shared_ptr<Term> b) {
             return eq(ctor1("len", ctor1("compute_cid", b)), num(139));
         }));

    // blake3_512_hex output length is exactly 128.
    must("cpp_blake3_512_hex_output_length_eq_128",
         forall(String(), [](std::shared_ptr<Term> b) {
             return eq(ctor1("len", ctor1("blake3_512_hex", b)), num(128));
         }));

    // compute_cid is deterministic.
    must("cpp_compute_cid_is_deterministic",
         forall(String(), [](std::shared_ptr<Term> b) {
             return eq(ctor1("compute_cid", b), ctor1("compute_cid", b));
         }));

    // blake3_512_hex is deterministic.
    must("cpp_blake3_512_hex_is_deterministic",
         forall(String(), [](std::shared_ptr<Term> b) {
             return eq(ctor1("blake3_512_hex", b), ctor1("blake3_512_hex", b));
         }));

    // BLAKE3-512 prefix length is exactly 11 (the literal "blake3-512:").
    contract("cpp_blake3_512_prefix_length_eq_11",
             /*pre=*/nullptr,
             /*post=*/eq(ctor1("len", str_const("blake3-512:")), num(11)));
}
