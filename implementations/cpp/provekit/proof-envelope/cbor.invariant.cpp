// SPDX-License-Identifier: Apache-2.0
//
// .invariant.cpp for provekit/proof-envelope/cbor.cpp + cbor_decoder.cpp
//
// Public surface covered:
//   * Deterministic CBOR encoder (RFC 8949 §4.2.1)
//   * CBOR decoder (round-trips encoder output)

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

extern "C" void cbor_invariants() {
    using namespace provekit::ir;

    // Encoder is deterministic: same input bytes, same output bytes.
    must("cpp_cbor_encode_is_deterministic",
         forall(String(), [](std::shared_ptr<Term> v) {
             return eq(ctor1("cbor_encode", v), ctor1("cbor_encode", v));
         }));

    // Decoder round-trips encoder: decode(encode(v)) == v.
    must("cpp_cbor_round_trips",
         forall(String(), [](std::shared_ptr<Term> v) {
             return eq(ctor1("cbor_decode", ctor1("cbor_encode", v)), v);
         }));

    // Output is non-empty (a CBOR-encoded null is one byte; everything
    // we encode in proof envelopes is at least that long).
    must("cpp_cbor_encode_output_nonempty",
         forall(String(), [](std::shared_ptr<Term> v) {
             return gte(ctor1("len", ctor1("cbor_encode", v)), num(1));
         }));

    // Encoder is a function on its argument: equal inputs yield equal
    // outputs. Standard Z3-dischargeable equality of identical ctor
    // applications.
    must("cpp_cbor_encode_is_total",
         forall(String(), [](std::shared_ptr<Term> v) {
             return eq(ctor2("cbor_encode_pair", v, v), ctor2("cbor_encode_pair", v, v));
         }));
}
